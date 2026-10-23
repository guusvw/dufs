mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use reqwest::blocking::Response;
use rstest::rstest;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

const ARGS: &[&str] = &["--allow-upload", "--allow-delete"];
const ID: &str = "0123456789abcdef0123456789abcdef";

/// A PUT that sends only the start of its body and then loses the connection.
fn interrupted_put(server: &TestServer, path: &str, id: &str, total: usize, sent: &[u8]) {
    let mut stream = TcpStream::connect(("127.0.0.1", server.port())).unwrap();
    write!(
        stream,
        "PUT {path}?upload={id} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {total}\r\n\r\n"
    )
    .unwrap();
    stream.write_all(sent).unwrap();
    stream.flush().unwrap();
}

fn error_code(resp: &Response) -> &str {
    resp.headers()
        .get("x-dufs-error-code")
        .map(|v| v.to_str().unwrap())
        .unwrap_or_default()
}

fn received(resp: &Response) -> u64 {
    resp.headers()
        .get("x-dufs-upload-offset")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Bytes the server keeps for the upload once it has noticed the interrupt.
///
/// What was in flight when the connection dropped may or may not have been
/// written, so the length is only known once it stops changing.
fn settled_offset(url: &str) -> u64 {
    let mut last = None;
    let mut stable = 0;
    for _ in 0..200 {
        let resp = reqwest::blocking::Client::new().head(url).send().unwrap();
        let now = (resp.status() == 200).then(|| received(&resp));
        stable = if now.is_some() && now == last {
            stable + 1
        } else {
            0
        };
        if stable >= 6 {
            return now.unwrap();
        }
        last = now;
        sleep(Duration::from_millis(25));
    }
    panic!("upload never settled");
}

fn part_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|v| v.ok())
        .map(|v| v.file_name().to_string_lossy().into_owned())
        .filter(|v| v.contains(".dufs-") && v.ends_with(".part"))
        .collect()
}

/// Resumes at `offset`, waiting while the server still considers the
/// interrupted request to be writing.
fn resume(url: &str, offset: u64, body: Vec<u8>) -> Result<Response, Error> {
    for _ in 0..100 {
        let resp = fetch!(b"PATCH", url)
            .header("X-Update-Range", format!("bytes={offset}-"))
            .body(body.clone())
            .send()?;
        if error_code(&resp) != "upload-id-in-use" {
            return Ok(resp);
        }
        sleep(Duration::from_millis(50));
    }
    panic!("upload stayed busy");
}

#[rstest]
fn patch_append_to_existing_file_requires_allow_delete(
    #[with(&["--allow-upload"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let size = fs::metadata(server.path().join("test.txt"))?.len();
    for range in ["append", &format!("bytes={size}-"), "bytes=0-"] {
        let resp = fetch!(b"PATCH", &url)
            .header("X-Update-Range", range)
            .body("tail")
            .send()?;
        assert_eq!(resp.status(), 403, "{range}");
        assert_eq!(error_code(&resp), "delete-disabled", "{range}");
    }
    assert_eq!(
        fs::read_to_string(server.path().join("test.txt"))?,
        "This is test.txt"
    );
    Ok(())
}

#[rstest]
fn patch_accepts_exact_end_offset(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let size = fs::metadata(server.path().join("test.txt"))?.len();
    let resp = fetch!(b"PATCH", &url)
        .header("X-Update-Range", format!("bytes={size}-"))
        .body("-tail")
        .send()?;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        fs::read_to_string(server.path().join("test.txt"))?,
        "This is test.txt-tail"
    );
    let resp = fetch!(b"PATCH", &url)
        .header("X-Update-Range", format!("bytes={}-", size + 100))
        .body("x")
        .send()?;
    assert_eq!(resp.status(), 400);
    Ok(())
}

#[rstest]
fn interrupted_put_resumes_through_upload_id(
    #[with(ARGS)] server: TestServer,
) -> Result<(), Error> {
    let data: Vec<u8> = (0..200_000u32).map(|v| (v % 251) as u8).collect();
    let url = format!("{}test.txt?upload={ID}", server.url());
    let target = server.path().join("test.txt");

    interrupted_put(&server, "/test.txt", ID, data.len(), &data[..50_000]);
    let received_len = settled_offset(&url);
    assert!(received_len > 0 && received_len <= 50_000);
    // The existing file is untouched until the upload completes.
    assert_eq!(fs::read_to_string(&target)?, "This is test.txt");

    let resp = resume(&url, received_len, data[received_len as usize..].to_vec())?;
    assert_eq!(resp.status(), 204);
    assert_eq!(fs::read(&target)?, data);
    assert!(part_files(server.path()).is_empty());
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(error_code(&resp), "upload-not-found");
    Ok(())
}

#[rstest]
fn interrupted_put_of_new_file_resumes(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let data = vec![b'z'; 100_000];
    let url = format!("{}dir9/new.bin?upload={ID}", server.url());
    interrupted_put(&server, "/dir9/new.bin", ID, data.len(), &data[..10_000]);
    let received_len = settled_offset(&url);
    assert!(received_len > 0);
    assert!(!server.path().join("dir9/new.bin").exists());
    let resp = resume(&url, received_len, data[received_len as usize..].to_vec())?;
    assert_eq!(resp.status(), 204);
    assert_eq!(fs::read(server.path().join("dir9/new.bin"))?, data);
    Ok(())
}

#[rstest]
fn resume_with_wrong_offset_conflicts(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let data = vec![b'q'; 20_000];
    let url = format!("{}test.txt?upload={ID}", server.url());
    interrupted_put(&server, "/test.txt", ID, data.len(), &data[..15_000]);
    let n = settled_offset(&url);
    assert!(n > 0);

    for wrong in [0, n - 1, n + 1, n + 5_000] {
        let resp = resume(&url, wrong, b"x".to_vec())?;
        assert_eq!(resp.status(), 409, "{wrong}");
        assert_eq!(error_code(&resp), "upload-offset-mismatch", "{wrong}");
        assert_eq!(received(&resp), n);
    }
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(received(&resp), n);
    assert_eq!(
        fs::read_to_string(server.path().join("test.txt"))?,
        "This is test.txt"
    );

    let resp = resume(&url, n, data[n as usize..].to_vec())?;
    assert_eq!(resp.status(), 204);
    assert_eq!(fs::read(server.path().join("test.txt"))?, data);
    Ok(())
}

#[rstest]
fn resume_requires_an_exact_offset_header(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt?upload={ID}", server.url());
    interrupted_put(&server, "/test.txt", ID, 100, b"abc");
    let n = settled_offset(&url);

    let resp = fetch!(b"PATCH", &url).body("x").send()?;
    assert_eq!(error_code(&resp), "missing-upload-offset");
    for value in [
        "append".to_string(),
        format!("bytes={n}-9"),
        format!("bytes=-{n}"),
        format!("bytes=+{n}-"),
        "junk".to_string(),
    ] {
        let resp = resume_with_header(&url, &value)?;
        assert_eq!(resp.status(), 400, "{value}");
        assert_eq!(error_code(&resp), "invalid-upload-offset", "{value}");
    }
    Ok(())
}

fn resume_with_header(url: &str, value: &str) -> Result<Response, Error> {
    Ok(fetch!(b"PATCH", url)
        .header("X-Update-Range", value)
        .body("x")
        .send()?)
}

#[rstest]
fn unknown_or_invalid_upload_id(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt?upload={ID}", server.url());
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(error_code(&resp), "upload-not-found");
    let resp = resume_with_header(&url, "bytes=0-")?;
    assert_eq!(resp.status(), 404);
    assert_eq!(error_code(&resp), "upload-not-found");
    let bad = format!("{}test.txt?upload=../x", server.url());
    for resp in [
        fetch!(b"HEAD", &bad).send()?,
        fetch!(b"PUT", &bad).send()?,
        fetch!(b"PATCH", &bad).send()?,
    ] {
        assert_eq!(resp.status(), 400);
        assert_eq!(error_code(&resp), "invalid-upload-id");
    }
    Ok(())
}

#[rstest]
fn upload_id_is_exclusive(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt?upload={ID}", server.url());
    let mut busy = TcpStream::connect(("127.0.0.1", server.port()))?;
    write!(
        busy,
        "PUT /test.txt?upload={ID} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\nabc"
    )?;
    settled_offset(&url);

    let resp = fetch!(b"PUT", &url).body("x").send()?;
    assert_eq!(resp.status(), 409);
    assert_eq!(error_code(&resp), "upload-id-in-use");
    let resp = resume_with_header(&url, "bytes=0-")?;
    assert_eq!(resp.status(), 409);
    assert_eq!(error_code(&resp), "upload-id-in-use");
    Ok(())
}

#[rstest]
fn resume_follows_upload_and_delete_permissions(
    #[with(&["--allow-upload"])] server: TestServer,
) -> Result<(), Error> {
    // The final rename would overwrite the existing file.
    let resp = resume_with_header(&format!("{}test.txt?upload={ID}", server.url()), "bytes=0-")?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), "delete-disabled");

    // A new file only needs upload permission.
    let url = format!("{}fresh.bin?upload={ID}", server.url());
    interrupted_put(&server, "/fresh.bin", ID, 10, b"abc");
    let n = settled_offset(&url);
    let resp = resume(&url, n, b"abcdefghij"[n as usize..].to_vec())?;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        fs::read_to_string(server.path().join("fresh.bin"))?,
        "abcdefghij"
    );
    Ok(())
}

#[rstest]
fn upload_status_requires_allow_upload(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}test.txt?upload={ID}", server.url())).send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), "upload-disabled");
    Ok(())
}

/// Permission bits are not enforced for root, so those tests are skipped there.
#[cfg(unix)]
fn running_as_root(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let probe = dir.join("probe-owner");
    fs::write(&probe, "").unwrap();
    let uid = fs::metadata(&probe).unwrap().uid();
    fs::remove_file(&probe).unwrap();
    uid == 0
}

#[cfg(unix)]
#[rstest]
fn resumed_upload_drops_setuid_and_keeps_mode(
    #[with(ARGS)] server: TestServer,
) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let target = server.path().join("test.txt");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o4750))?;
    let data: Vec<u8> = (0..200_000u32).map(|v| (v % 251) as u8).collect();
    let url = format!("{}test.txt?upload={ID}", server.url());
    interrupted_put(&server, "/test.txt", ID, data.len(), &data[..50_000]);
    let received_len = settled_offset(&url);
    let resp = resume(&url, received_len, data[received_len as usize..].to_vec())?;
    assert_eq!(resp.status(), 204);
    assert_eq!(fs::read(&target)?, data);
    assert_eq!(fs::metadata(&target)?.permissions().mode() & 0o7777, 0o750);
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn upload_id_put_keeps_read_only_file(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root(server.path()) {
        return Ok(());
    }
    let target = server.path().join("test.txt");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444))?;
    let resp = fetch!(b"PUT", format!("{}test.txt?upload={ID}", server.url()))
        .body("new")
        .send()?;
    assert_ne!(resp.status(), 201);
    assert_eq!(fs::read_to_string(&target)?, "This is test.txt");
    assert!(part_files(server.path()).is_empty());
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn resume_keeps_file_made_read_only(#[with(ARGS)] server: TestServer) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root(server.path()) {
        return Ok(());
    }
    let target = server.path().join("test.txt");
    let data: Vec<u8> = (0..200_000u32).map(|v| (v % 251) as u8).collect();
    let url = format!("{}test.txt?upload={ID}", server.url());
    interrupted_put(&server, "/test.txt", ID, data.len(), &data[..50_000]);
    let received_len = settled_offset(&url);
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444))?;
    let resp = resume(&url, received_len, data[received_len as usize..].to_vec())?;
    assert_ne!(resp.status(), 204);
    assert_eq!(fs::read_to_string(&target)?, "This is test.txt");
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn upload_id_put_in_read_only_dir_writes_in_place(
    #[with(ARGS)] server: TestServer,
) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root(server.path()) {
        return Ok(());
    }
    let dir = server.path().join("locked");
    fs::create_dir(&dir)?;
    let target = dir.join("file.txt");
    fs::write(&target, "old")?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o666))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555))?;
    let resp = fetch!(
        b"PUT",
        format!("{}locked/file.txt?upload={ID}", server.url())
    )
    .body("new")
    .send()?;
    let status = resp.status();
    let content = fs::read_to_string(&target)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))?;
    assert_eq!(status, 201);
    assert_eq!(content, "new");
    Ok(())
}
