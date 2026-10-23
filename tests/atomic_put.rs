mod fixtures;
mod utils;

use fixtures::{port, server, tmpdir, wait_for_port, Error, Port, TestServer};
use rstest::rstest;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

const ARGS: &[&str] = &[
    "--allow-upload",
    "--allow-delete",
    "--allow-search",
    "--allow-archive",
];

/// A PUT whose body is sent in pieces, so the test controls when it completes.
struct SlowPut {
    stream: TcpStream,
}

impl SlowPut {
    fn start(server: &TestServer, path: &str, total: usize, first: &[u8]) -> Self {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port())).unwrap();
        write!(
            stream,
            "PUT {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {total}\r\n\r\n"
        )
        .unwrap();
        stream.write_all(first).unwrap();
        stream.flush().unwrap();
        Self { stream }
    }

    fn send(&mut self, rest: &[u8]) {
        self.stream.write_all(rest).unwrap();
        self.stream.flush().unwrap();
    }

    fn status(&mut self) -> String {
        let mut buf = [0u8; 12];
        self.stream.read_exact(&mut buf).unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }
}

fn part_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|v| v.ok())
        .map(|v| v.file_name().to_string_lossy().into_owned())
        .filter(|v| v.contains(".dufs-") && v.ends_with(".part"))
        .collect()
}

fn wait_until(what: &str, cond: impl Fn() -> bool) {
    for _ in 0..100 {
        if cond() {
            return;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("timeout waiting for {what}");
}

#[rstest]
fn interrupted_overwrite_keeps_original(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let target = server.path().join("test.txt");
    let original = fs::read(&target)?;
    let put = SlowPut::start(&server, "/test.txt", 1000, b"partial");
    wait_until("part file", || part_files(server.path()).len() == 1);
    assert_eq!(fs::read(&target)?, original);
    drop(put);
    wait_until("part file removal", || part_files(server.path()).is_empty());
    assert_eq!(fs::read(&target)?, original);
    Ok(())
}

#[rstest]
fn interrupted_create_leaves_nothing(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let put = SlowPut::start(&server, "/new.txt", 1000, b"partial");
    wait_until("part file", || part_files(server.path()).len() == 1);
    assert!(!server.path().join("new.txt").exists());
    drop(put);
    wait_until("part file removal", || part_files(server.path()).is_empty());
    assert!(!server.path().join("new.txt").exists());
    Ok(())
}

#[rstest]
fn concurrent_puts_never_mix(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let a = vec![b'A'; 4096];
    let b = vec![b'B'; 4096];
    let mut put_a = SlowPut::start(&server, "/test.txt", a.len(), &a[..2048]);
    let mut put_b = SlowPut::start(&server, "/test.txt", b.len(), &b[..2048]);
    wait_until("two part files", || part_files(server.path()).len() == 2);
    put_b.send(&b[2048..]);
    assert_eq!(put_b.status(), "HTTP/1.1 201");
    put_a.send(&a[2048..]);
    assert_eq!(put_a.status(), "HTTP/1.1 201");
    assert_eq!(fs::read(server.path().join("test.txt"))?, a);
    assert!(part_files(server.path()).is_empty());
    Ok(())
}

#[rstest]
fn aborted_put_does_not_remove_other_upload(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let b = vec![b'B'; 4096];
    let aborted = SlowPut::start(&server, "/test.txt", 4096, b"AAAA");
    let mut put_b = SlowPut::start(&server, "/test.txt", b.len(), &b[..2048]);
    wait_until("two part files", || part_files(server.path()).len() == 2);
    drop(aborted);
    wait_until("one part file", || part_files(server.path()).len() == 1);
    put_b.send(&b[2048..]);
    assert_eq!(put_b.status(), "HTTP/1.1 201");
    assert_eq!(fs::read(server.path().join("test.txt"))?, b);
    Ok(())
}

#[rstest]
fn get_during_upload_returns_old_content(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let mut put = SlowPut::start(&server, "/test.txt", 12, b"new");
    wait_until("part file", || part_files(server.path()).len() == 1);
    let resp = reqwest::blocking::get(format!("{}test.txt", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is test.txt");
    put.send(b"-complete");
    assert_eq!(put.status(), "HTTP/1.1 201");
    let resp = reqwest::blocking::get(format!("{}test.txt", server.url()))?;
    assert_eq!(resp.text()?, "new-complete");
    Ok(())
}

#[rstest]
fn part_files_are_invisible(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let _put = SlowPut::start(&server, "/dir1/test.txt", 1000, b"partial");
    wait_until("part file", || {
        part_files(&server.path().join("dir1")).len() == 1
    });
    let part = part_files(&server.path().join("dir1")).remove(0);
    let url = server.url();

    for query in ["?json", "", "?q=dufs", "?q=part"] {
        let resp = reqwest::blocking::get(format!("{url}dir1/{query}"))?;
        assert_eq!(resp.status(), 200, "{query}");
        assert!(!resp.text()?.contains(".dufs-"), "{query}");
    }
    let resp = reqwest::blocking::get(format!("{url}?q=test.txt&json"))?;
    assert!(!resp.text()?.contains(".dufs-"));

    let resp = fetch!(b"PROPFIND", format!("{url}dir1/")).send()?;
    assert_eq!(resp.status(), 207);
    assert!(!resp.text()?.contains(".dufs-"));

    let resp = reqwest::blocking::get(format!("{url}dir1/?zip"))?;
    assert_eq!(resp.status(), 200);
    assert!(!String::from_utf8_lossy(&resp.bytes()?).contains(".dufs-"));

    for suffix in ["", "?json", "?edit", "?hash"] {
        let resp = reqwest::blocking::get(format!("{url}dir1/{part}{suffix}"))?;
        assert_eq!(resp.status(), 404, "{suffix}");
    }
    let resp = fetch!(b"PUT", format!("{url}dir1/{part}"))
        .body("x")
        .send()?;
    assert_eq!(resp.status(), 404);
    let resp = fetch!(b"DELETE", format!("{url}dir1/{part}")).send()?;
    assert_eq!(resp.status(), 404);
    let resp = fetch!(b"MOVE", format!("{url}dir1/test.txt"))
        .header("Destination", format!("{url}dir1/{part}"))
        .send()?;
    assert_eq!(resp.status(), 400);
    Ok(())
}

#[rstest]
fn overwrite_still_requires_allow_delete(
    #[with(&["--allow-upload"])] server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"PUT", format!("{}test.txt", server.url()))
        .body("new")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(
        fs::read_to_string(server.path().join("test.txt"))?,
        "This is test.txt"
    );
    assert!(part_files(server.path()).is_empty());
    Ok(())
}

#[rstest]
fn put_keeps_long_file_names(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let name = "n".repeat(250);
    let resp = fetch!(b"PUT", format!("{}{name}", server.url()))
        .body("data")
        .send()?;
    assert_eq!(resp.status(), 201);
    assert_eq!(fs::read_to_string(server.path().join(name))?, "data");
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn overwrite_keeps_permissions(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let target = server.path().join("test.txt");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640))?;
    let resp = fetch!(b"PUT", format!("{}test.txt", server.url()))
        .body("new")
        .send()?;
    assert_eq!(resp.status(), 201);
    assert_eq!(fs::metadata(&target)?.permissions().mode() & 0o777, 0o640);
    Ok(())
}

#[rstest]
fn stale_part_files_removed_at_startup(
    tmpdir: assert_fs::TempDir,
    port: Port,
) -> Result<(), Error> {
    let id = "0123456789abcdef0123456789abcdef";
    let stale = tmpdir
        .path()
        .join("dir1")
        .join(format!(".a.dufs-{id}.part"));
    let fresh = tmpdir.path().join(format!(".b.dufs-{id}.part"));
    let lookalike = tmpdir.path().join(".c.part");
    for (path, age) in [(&stale, 25 * 3600), (&fresh, 3600), (&lookalike, 25 * 3600)] {
        let file = fs::File::create(path)?;
        file.set_modified(SystemTime::now() - Duration::from_secs(age))?;
    }
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .spawn()?;
    wait_for_port(&port);
    wait_until("stale removal", || !stale.exists());
    child.kill()?;
    child.wait()?;
    assert!(fresh.exists());
    assert!(lookalike.exists());
    Ok(())
}
