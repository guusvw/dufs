mod fixtures;

use assert_fs::fixture::TempDir;
use fixtures::{port, tmpdir, wait_for_port, Error, Port};
use rstest::rstest;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn healthcheck(args: &[&str], envs: &[(&str, &str)]) -> (Output, Duration) {
    let start = Instant::now();
    let output = Command::new(assert_cmd::cargo::cargo_bin!())
        .args(args)
        .envs(envs.iter().copied())
        .output()
        .expect("Couldn't run test binary");
    (output, start.elapsed())
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A running server, stopped when dropped, also when a test fails.
struct Server(std::process::Child);

impl Server {
    fn id(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_server(tmpdir: &TempDir, port: &Port, args: &[&str]) -> Server {
    let child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .args(["-b", "127.0.0.1", "-p", &port.to_string()])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Couldn't run test binary");
    wait_for_port(port);
    Server(child)
}

/// A server that answers every request with `response`, or never answers.
fn fake_server(response: Option<&'static str>) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0; 1024];
        let _ = stream.read(&mut buf);
        match response {
            Some(response) => {
                stream.write_all(response.as_bytes()).unwrap();
            }
            // Hold the connection open past the probe's deadline.
            None => sleep(Duration::from_secs(5)),
        }
    });
    (port, handle)
}

#[rstest]
fn exits_0_against_a_running_server(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let _server = spawn_server(&tmpdir, &port, &[]);
    let url = format!("http://127.0.0.1:{port}/__dufs__/health");
    let (output, _) = healthcheck(&["--healthcheck", &url], &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(output.stderr.is_empty() && output.stdout.is_empty());

    // Without a URL the port comes from DUFS_PORT.
    let (output, _) = healthcheck(&["--healthcheck"], &[("DUFS_PORT", &port.to_string())]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let (output, _) = healthcheck(&["--healthcheck="], &[]);
    assert_ne!(output.status.code(), Some(0));
    Ok(())
}

#[rstest]
fn follows_the_path_prefix_in_the_url(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let _server = spawn_server(&tmpdir, &port, &["--path-prefix", "drop"]);
    let (output, _) = healthcheck(
        &[
            "--healthcheck",
            &format!("http://127.0.0.1:{port}/drop/__dufs__/health"),
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    // The default URL has no prefix, which the server refuses.
    let (output, _) = healthcheck(&["--healthcheck"], &[("DUFS_PORT", &port.to_string())]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("400"), "{}", stderr(&output));
    Ok(())
}

#[rstest]
fn reads_no_other_configuration(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let _server = spawn_server(&tmpdir, &port, &[]);
    let url = format!("http://127.0.0.1:{port}/__dufs__/health");
    // A missing config file and options that exclude each other would stop
    // the regular start; the health check does not look at them.
    let (output, _) = healthcheck(
        &[
            "-c",
            "/no/such/config.yaml",
            "--healthcheck",
            &url,
            "--log-format",
            "x",
        ],
        &[("DUFS_LOG_JSON", "true"), ("DUFS_PORT", "not-a-port")],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    Ok(())
}

#[rstest]
fn exits_1_against_a_closed_port(port: Port) -> Result<(), Error> {
    let url = format!("http://127.0.0.1:{port}/__dufs__/health");
    let (output, elapsed) = healthcheck(&["--healthcheck", &url], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(elapsed < Duration::from_millis(3500), "{elapsed:?}");
    let message = stderr(&output);
    assert_eq!(message.lines().count(), 1, "{message}");
    assert!(message.contains("cannot connect"), "{message}");
    Ok(())
}

#[test]
fn exits_1_when_the_server_does_not_answer() -> Result<(), Error> {
    let (port, handle) = fake_server(None);
    let url = format!("http://127.0.0.1:{port}/__dufs__/health");
    let (output, elapsed) = healthcheck(&["--healthcheck", &url], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(elapsed >= Duration::from_millis(2900), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(3500), "{elapsed:?}");
    let message = stderr(&output);
    assert_eq!(message.lines().count(), 1, "{message}");
    assert!(message.contains("no answer within 3s"), "{message}");
    handle.join().unwrap();
    Ok(())
}

#[rstest]
#[case("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n", "503")]
#[case("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n", "404")]
#[case(
    "HTTP/1.1 301 Moved Permanently\r\nLocation: /\r\nContent-Length: 0\r\n\r\n",
    "301"
)]
fn exits_1_unless_the_status_is_200(#[case] response: &'static str, #[case] code: &str) {
    let (port, handle) = fake_server(Some(response));
    let url = format!("http://127.0.0.1:{port}/");
    let (output, _) = healthcheck(&["--healthcheck", &url], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains(code), "{}", stderr(&output));
    handle.join().unwrap();
}

#[rstest]
#[case("https://127.0.0.1:1/", "http")]
#[case("127.0.0.1:1", "http://")]
#[case("not a url", "invalid URL")]
fn exits_1_for_urls_it_cannot_use(#[case] url: &str, #[case] reason: &str) {
    let (output, _) = healthcheck(&["--healthcheck", url], &[]);
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert_eq!(message.lines().count(), 1, "{message}");
    assert!(message.contains(reason), "{message}");
}

#[cfg(unix)]
#[rstest]
fn exits_1_against_a_draining_server(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let server = spawn_server(&tmpdir, &port, &["-A", "--shutdown-timeout", "20"]);
    let url = format!("http://127.0.0.1:{port}/__dufs__/health");
    // An unfinished upload keeps the server draining.
    let mut slow = std::net::TcpStream::connect(("127.0.0.1", port.to_string().parse::<u16>()?))?;
    write!(
        slow,
        "PUT /slow.txt HTTP/1.1\r\nHost: x\r\nContent-Length: 10\r\n\r\n01234"
    )?;
    sleep(Duration::from_millis(300));
    let (output, _) = healthcheck(&["--healthcheck", &url], &[]);
    assert_eq!(output.status.code(), Some(0));

    let status = Command::new("kill")
        .args(["-TERM", &server.id().to_string()])
        .status()?;
    assert!(status.success());
    sleep(Duration::from_millis(300));
    let (output, elapsed) = healthcheck(&["--healthcheck", &url], &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(elapsed < Duration::from_millis(3500));
    Ok(())
}
