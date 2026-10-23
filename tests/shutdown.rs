#![cfg(unix)]

mod fixtures;

use fixtures::{port, tmpdir, wait_for_port, Error, Port};

use assert_fs::fixture::TempDir;
use rstest::rstest;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn spawn_dufs(tmpdir: &TempDir, port: &Port, args: &[&str]) -> Child {
    let child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("-A")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Couldn't run test binary");
    wait_for_port(port);
    child
}

fn sigterm(child: &Child) {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .unwrap();
    assert!(status.success());
}

/// Wait for the child to exit, killing it when it doesn't within `limit`.
fn wait_exit(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        if start.elapsed() > limit {
            child.kill().unwrap();
            child.wait().unwrap();
            return None;
        }
        sleep(Duration::from_millis(20));
    }
}

fn connect(port: &Port) -> TcpStream {
    let stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}

/// Send the head of a PUT of `len` bytes plus `first` bytes of the body.
fn start_put(port: &Port, name: &str, len: usize, first: &[u8]) -> TcpStream {
    let mut stream = connect(port);
    write!(
        stream,
        "PUT /{name} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {len}\r\n\r\n"
    )
    .unwrap();
    stream.write_all(first).unwrap();
    stream.flush().unwrap();
    // Let the server see the request before the signal arrives.
    sleep(Duration::from_millis(300));
    stream
}

fn read_response(mut stream: TcpStream) -> String {
    let mut buf = String::new();
    // The server closes the connection after the response while draining.
    let _ = stream.read_to_string(&mut buf);
    buf
}

fn part_files(tmpdir: &TempDir) -> Vec<String> {
    std::fs::read_dir(tmpdir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".part"))
        .collect()
}

#[rstest]
fn upload_in_flight_completes_on_sigterm(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = spawn_dufs(&tmpdir, &port, &[]);

    let mut stream = start_put(&port, "inflight.txt", 10, b"01234");
    sigterm(&child);
    sleep(Duration::from_millis(300));
    // Still running: the upload is in flight.
    assert!(child.try_wait().unwrap().is_none());

    stream.write_all(b"56789").unwrap();
    let resp = read_response(stream);
    assert!(resp.starts_with("HTTP/1.1 201"), "{resp}");

    let status = wait_exit(&mut child, Duration::from_secs(5)).expect("server did not exit");
    assert!(status.success());
    assert_eq!(
        std::fs::read_to_string(tmpdir.path().join("inflight.txt")).unwrap(),
        "0123456789"
    );
    assert!(part_files(&tmpdir).is_empty());
    Ok(())
}

#[rstest]
fn stalled_upload_is_aborted_after_timeout(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = spawn_dufs(&tmpdir, &port, &["--shutdown-timeout", "1"]);

    let _stream = start_put(&port, "stalled.txt", 10, b"01234");
    let start = Instant::now();
    sigterm(&child);

    let status = wait_exit(&mut child, Duration::from_secs(5)).expect("server did not exit");
    assert!(status.success());
    assert!(start.elapsed() >= Duration::from_millis(900));
    assert!(part_files(&tmpdir).is_empty());
    Ok(())
}

#[rstest]
fn idle_connections_do_not_delay_shutdown(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = spawn_dufs(&tmpdir, &port, &[]);

    let _idle = connect(&port);
    sleep(Duration::from_millis(200));
    let start = Instant::now();
    sigterm(&child);

    let status = wait_exit(&mut child, Duration::from_secs(5)).expect("server did not exit");
    assert!(status.success());
    assert!(start.elapsed() < Duration::from_secs(2));
    Ok(())
}

#[rstest]
fn health_is_503_while_draining(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = spawn_dufs(&tmpdir, &port, &[]);

    // A request that is only half sent keeps its connection alive during the drain.
    let mut stream = connect(&port);
    stream
        .write_all(b"GET /__dufs__/health HTTP/1.1\r\nHost: localhost\r\n")
        .unwrap();
    stream.flush().unwrap();
    sleep(Duration::from_millis(300));
    sigterm(&child);
    sleep(Duration::from_millis(300));

    stream.write_all(b"\r\n").unwrap();
    let resp = read_response(stream);
    assert!(resp.starts_with("HTTP/1.1 503"), "{resp}");
    assert!(resp.contains("shutting down"), "{resp}");

    let status = wait_exit(&mut child, Duration::from_secs(5)).expect("server did not exit");
    assert!(status.success());
    Ok(())
}

#[rstest]
fn new_connections_are_refused_while_draining(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = spawn_dufs(&tmpdir, &port, &[]);

    let stream = start_put(&port, "busy.txt", 10, b"01234");
    sigterm(&child);
    sleep(Duration::from_millis(300));
    assert!(TcpStream::connect(format!("127.0.0.1:{port}")).is_err());

    drop(stream);
    wait_exit(&mut child, Duration::from_secs(5)).expect("server did not exit");
    Ok(())
}

/// CPU time (user + system, in clock ticks) used by the process so far.
#[cfg(target_os = "linux")]
fn cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    // Skip `pid (comm)`; comm may contain spaces.
    let rest = &stat[stat.rfind(')').unwrap() + 2..];
    let fields: Vec<&str> = rest.split(' ').collect();
    // utime and stime are fields 14 and 15 of the full line, 12 and 13 here.
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

#[cfg(target_os = "linux")]
#[rstest]
fn out_of_file_descriptors_does_not_spin(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("ulimit -n 24 && exec \"$0\" \"$@\"")
        .arg(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for_port(&port);

    let conns: Vec<_> = (0..40)
        .filter_map(|_| TcpStream::connect(format!("127.0.0.1:{port}")).ok())
        .collect();
    assert_eq!(conns.len(), 40);
    sleep(Duration::from_millis(500));

    let before = cpu_ticks(child.id());
    sleep(Duration::from_secs(2));
    let used = cpu_ticks(child.id()) - before;
    // 100 ticks per second: 5 % of 2 s is 10 ticks. A spinning accept loop uses ~200.
    assert!(used < 10, "used {used} ticks of cpu in 2s");

    child.kill().unwrap();
    child.wait().unwrap();
    Ok(())
}
