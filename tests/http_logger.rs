mod digest_auth_util;
mod fixtures;
mod utils;

use digest_auth_util::send_with_digest_auth;
use fixtures::{port, tmpdir, wait_for_port, Error, Port};

use assert_fs::fixture::TempDir;
use regex::Regex;
use rstest::rstest;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

#[rstest]
#[case(&["-a", "user:pass@/:rw", "--log-format", "$remote_user"], false)]
#[case(&["-a", "user:pass@/:rw", "--log-format", "$remote_user"], true)]
fn log_remote_user(
    tmpdir: TempDir,
    port: Port,
    #[case] args: &[&str],
    #[case] is_basic: bool,
) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    let stdout = child.stdout.as_mut().expect("Failed to get stdout");

    let req_builder = fetch!(b"GET", &format!("http://localhost:{port}"));

    let resp = if is_basic {
        req_builder.basic_auth("user", Some("pass")).send()?
    } else {
        send_with_digest_auth(req_builder, "user", "pass")?
    };

    assert_eq!(resp.status(), 200);

    let mut buf = [0; 2048];
    let buf_len = stdout.read(&mut buf)?;
    let output = std::str::from_utf8(&buf[0..buf_len])?;

    assert!(output.lines().last().unwrap().ends_with("user"));

    child.kill()?;
    Ok(())
}

#[rstest]
#[case(&["--log-format", ""])]
fn no_log(tmpdir: TempDir, port: Port, #[case] args: &[&str]) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    let stdout = child.stdout.as_mut().expect("Failed to get stdout");

    let resp = fetch!(b"GET", &format!("http://localhost:{port}")).send()?;
    assert_eq!(resp.status(), 200);

    let mut buf = [0; 2048];
    let buf_len = stdout.read(&mut buf)?;
    let output = std::str::from_utf8(&buf[0..buf_len])?;

    assert_eq!(output.lines().last().unwrap(), "");

    child.kill()?;
    Ok(())
}

fn read_log_line(child: &mut std::process::Child) -> Result<String, Error> {
    let stdout = child.stdout.as_mut().expect("Failed to get stdout");
    let mut buf = [0; 2048];
    let buf_len = stdout.read(&mut buf)?;
    let output = std::str::from_utf8(&buf[0..buf_len])?;
    Ok(output.lines().last().unwrap_or_default().to_string())
}

#[rstest]
#[case("/?token=x&a=b", "/?token=***&a=b")]
#[case("/?a=b&token=x", "/?a=b&token=***")]
#[case("/?%74oken=x&a=b", "/?token=***&a=b")]
#[case("/?token=a%26b&a=b", "/?token=***&a=b")]
#[case("/?token=x&token=y", "/?token=***&token=***")]
#[case("/?tokengen&mytoken=x", "/?tokengen&mytoken=x")]
fn log_redacts_token(
    tmpdir: TempDir,
    port: Port,
    #[case] path: &str,
    #[case] expected: &str,
) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(["--log-format", "$request_uri|$request"])
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    fetch!(b"GET", &format!("http://localhost:{port}{path}")).send()?;

    let line = read_log_line(&mut child)?;
    child.kill()?;

    assert_eq!(line, format!("{expected}|GET {expected}"));
    Ok(())
}

#[rstest]
fn log_redacts_credential_headers(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(["-a", "user:pass@/:rw"])
        .args([
            "--log-format",
            "$http_authorization|$http_proxy_authorization|$http_cookie",
        ])
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    fetch!(b"GET", &format!("http://localhost:{port}"))
        .basic_auth("user", Some("pass"))
        .header("proxy-authorization", "Bearer SECRETTOKEN")
        .header("cookie", "sid=SECRETSID; theme=dark")
        .send()?;

    let line = read_log_line(&mut child)?;
    child.kill()?;

    assert_eq!(line, "Basic ***|Bearer ***|sid=***; theme=***");
    Ok(())
}

#[rstest]
fn log_redacts_digest_authorization(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(["-a", "user:pass@/:rw"])
        .args(["--log-format", "$http_authorization"])
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    let req_builder = fetch!(b"GET", &format!("http://localhost:{port}"));
    let resp = send_with_digest_auth(req_builder, "user", "pass")?;
    assert_eq!(resp.status(), 200);

    let line = read_log_line(&mut child)?;
    child.kill()?;

    assert_eq!(line, r#"Digest username="user" …***"#);
    Ok(())
}

const ID: &str = "0123456789abcdef0123456789abcdef";

/// A dufs whose stdout and stderr are read line by line.
struct Logged {
    child: Child,
    lines: Receiver<String>,
    port: Port,
}

impl Logged {
    fn start(tmpdir: &TempDir, port: Port, args: &[&str]) -> Logged {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
            .arg(tmpdir.path())
            .args(["-b", "127.0.0.1", "-p", &port.to_string()])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Couldn't run test binary");
        let (tx, lines) = channel();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        for stream in [
            Box::new(stdout) as Box<dyn Read + Send>,
            Box::new(stderr) as Box<dyn Read + Send>,
        ] {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        wait_for_port(&port);
        Logged { child, lines, port }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// The next line that is not the startup banner.
    fn next_line(&self) -> String {
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(10))
                .expect("no log line");
            if !line.starts_with("Listening on") && !line.contains("\"event\":\"listening\"") {
                return line;
            }
        }
    }

    fn next_json(&self) -> Value {
        let line = self.next_line();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    /// The next JSON line that satisfies `pred`.
    fn json_where(&self, pred: impl Fn(&Value) -> bool) -> Value {
        loop {
            let value = self.next_json();
            if pred(&value) {
                return value;
            }
        }
    }

    fn send_raw(&self, request: &[u8]) -> TcpStream {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port.to_string().parse().unwrap()))
            .expect("connect");
        stream.write_all(request).unwrap();
        stream.flush().unwrap();
        stream
    }
}

impl Drop for Logged {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[rstest]
fn log_renders_the_new_variables(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let format = "$request_time|$bytes_sent|$bytes_received|$error_code|$upload_id|$status";
    let dufs = Logged::start(&tmpdir, port, &["-A", "--log-format", format]);
    let line_re = Regex::new(r"^(\d+\.\d{3})\|([^|]*)\|([^|]*)\|([^|]*)\|([^|]*)\|(\d+)$")?;
    let fields = |line: &str| -> Vec<String> {
        let caps = line_re.captures(line).unwrap_or_else(|| panic!("{line}"));
        (1..=6).map(|i| caps[i].to_string()).collect()
    };

    // A file of 16 bytes, no request body.
    fetch!(b"GET", dufs.url("/test.txt")).send()?;
    let got = fields(&dufs.next_line());
    assert_eq!(&got[1..], ["16", "0", "-", "-", "200"]);

    // HEAD announces the length but sends nothing.
    fetch!(b"HEAD", dufs.url("/test.txt")).send()?;
    assert_eq!(&fields(&dufs.next_line())[1..3], ["0", "0"]);

    // An upload under an id: the body is counted and the id is shown.
    fetch!(b"PUT", dufs.url(&format!("/up.txt?upload={ID}")))
        .body("hello")
        .send()?;
    let got = fields(&dufs.next_line());
    assert_eq!(&got[1..], ["0", "5", "-", ID, "201"]);

    // A failure names its code.
    fetch!(b"PUT", dufs.url("/test.txt?upload=nope"))
        .body("x")
        .send()?;
    let got = fields(&dufs.next_line());
    assert_eq!(&got[3..], ["invalid-upload-id", "-", "400"]);
    Ok(())
}

#[rstest]
fn log_error_code_for_disabled_upload(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--log-format", "$status $error_code"]);
    fetch!(b"PUT", dufs.url("/x.txt")).body("x").send()?;
    assert_eq!(dufs.next_line(), "403 upload-disabled");
    Ok(())
}

#[rstest]
fn log_json_lines_carry_the_access_fields(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--allow-upload", "--log-json"]);
    let keys = [
        "ts",
        "level",
        "msg",
        "remote",
        "user",
        "method",
        "uri",
        "status",
        "duration_ms",
        "bytes_sent",
        "bytes_received",
        "error_code",
        "upload_id",
    ];
    let check_keys = |line: &Value| {
        for key in keys {
            assert!(line.get(key).is_some(), "{key} missing in {line}");
        }
    };

    fetch!(b"GET", dufs.url("/test.txt")).send()?;
    let get = dufs.next_json();
    check_keys(&get);
    assert_eq!(get["level"], "INFO");
    assert_eq!(get["method"], "GET");
    assert_eq!(get["uri"], "/test.txt");
    assert_eq!(get["status"], 200);
    assert_eq!(get["remote"], "127.0.0.1");
    assert_eq!(get["bytes_sent"], 16);
    assert_eq!(get["bytes_received"], 0);
    assert!(get["duration_ms"].as_f64().unwrap() >= 0.0);
    assert!(get["error_code"].is_null() && get["upload_id"].is_null() && get["user"].is_null());
    assert_eq!(get["msg"], "GET /test.txt 200");
    assert!(Regex::new(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$")?
        .is_match(get["ts"].as_str().unwrap()));

    // A failed PUT: deleting is off, so replacing a file is refused.
    fetch!(b"PUT", dufs.url("/test.txt")).body("x").send()?;
    let put = dufs.next_json();
    check_keys(&put);
    assert_eq!(put["status"], 403);
    assert_eq!(put["error_code"], "delete-disabled");
    Ok(())
}

#[rstest]
fn log_json_marks_aborted_uploads(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--allow-upload", "--log-json"]);
    let stream = dufs.send_raw(
        format!(
            "PUT /half.bin?upload={ID} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100000\r\n\r\n{}",
            "a".repeat(1000)
        )
        .as_bytes(),
    );
    std::thread::sleep(Duration::from_millis(300));
    drop(stream);

    let event = dufs.json_where(|v| v["event"] == "upload_failed");
    assert_eq!(event["level"], "WARN");
    assert_eq!(event["upload_id"], ID);
    assert_eq!(event["reason"], "aborted");
    assert_eq!(event["bytes_received"], 1000);
    // The access line of that request names the upload too.
    let access = dufs.json_where(|v| v["method"] == "PUT");
    assert_eq!(access["upload_id"], ID);
    assert_eq!(access["bytes_received"], 1000);
    assert_eq!(access["status"], 499);
    Ok(())
}

#[rstest]
fn log_json_covers_free_text_and_events(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(
        &tmpdir,
        port,
        &["--allow-search", "--search-limit", "2", "--log-json"],
    );
    for i in 0..5 {
        std::fs::write(tmpdir.path().join(format!("hit-{i}.txt")), "x")?;
    }
    fetch!(b"GET", dufs.url("/?q=hit-&json")).send()?;
    let walk = dufs.json_where(|v| v["event"] == "walk");
    assert_eq!(walk["kind"], "search");
    assert_eq!(walk["result"], "truncated");
    assert_eq!(walk["entries"], 2);
    assert!(walk["duration_ms"].is_u64());

    // Every line is a JSON object, whatever produced it.
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-TERM", &dufs.child.id().to_string()])
            .status()?;
        assert!(status.success());
        let shutdown = dufs.json_where(|v| v["event"] == "shutdown");
        assert_eq!(shutdown["phase"], "draining");
        assert!(shutdown["timeout_s"].is_u64());
        assert!(shutdown["ts"].is_string() && shutdown["level"] == "INFO");
    }
    Ok(())
}

#[rstest]
fn log_json_redacts_the_token(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--log-json"]);
    fetch!(b"GET", dufs.url("/?token=SECRET&a=b")).send()?;
    let line = dufs.next_line();
    assert!(!line.contains("SECRET"), "{line}");
    let value: Value = serde_json::from_str(&line)?;
    assert_eq!(value["uri"], "/?token=***&a=b");
    assert_eq!(value["msg"], "GET /?token=***&a=b 200");
    Ok(())
}

#[rstest]
fn log_json_escapes_awkward_uris(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--log-json"]);
    fetch!(b"GET", dufs.url("/a%22b%5Cc%0Ad")).send()?;
    let value = dufs.next_json();
    assert_eq!(value["uri"], "/a\"b\\c\nd");
    Ok(())
}

#[rstest]
fn log_json_names_the_user(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--log-json", "-a", "user:pass@/:rw"]);
    fetch!(b"GET", dufs.url("/"))
        .basic_auth("user", Some("pass"))
        .send()?;
    let value = dufs.json_where(|v| v["status"] == 200);
    assert_eq!(value["user"], "user");
    Ok(())
}

#[rstest]
fn log_skip_health_drops_successful_checks(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--log-skip-health", "--log-json"]);
    for _ in 0..3 {
        assert_eq!(
            fetch!(b"GET", dufs.url("/__dufs__/health"))
                .send()?
                .status(),
            200
        );
    }
    // The first line that shows up is the one after the health checks.
    fetch!(b"GET", dufs.url("/test.txt")).send()?;
    let value = dufs.next_json();
    assert_eq!(value["uri"], "/test.txt");
    Ok(())
}

#[rstest]
fn health_lines_stay_without_the_flag(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--log-json"]);
    fetch!(b"GET", dufs.url("/__dufs__/health")).send()?;
    assert_eq!(dufs.next_json()["uri"], "/__dufs__/health");
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn log_skip_health_keeps_a_draining_503(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let dufs = Logged::start(&tmpdir, port, &["--log-skip-health", "--log-json"]);
    // A request that is only half sent keeps its connection alive during the drain.
    let mut stream = dufs.send_raw(b"GET /__dufs__/health HTTP/1.1\r\nHost: localhost\r\n");
    std::thread::sleep(Duration::from_millis(300));
    let status = Command::new("kill")
        .args(["-TERM", &dufs.child.id().to_string()])
        .status()?;
    assert!(status.success());
    dufs.json_where(|v| v["event"] == "shutdown");

    stream.write_all(b"\r\n")?;
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    assert!(response.starts_with("HTTP/1.1 503"), "{response}");

    // The 200 of the earlier probes is not logged, the 503 is.
    let line = dufs.json_where(|v| v["uri"] == "/__dufs__/health");
    assert_eq!(line["status"], 503);
    assert_eq!(line["level"], "INFO");
    Ok(())
}

#[rstest]
fn log_format_and_log_json_conflict(tmpdir: TempDir) -> Result<(), Error> {
    for envs in [(false, false), (true, false), (false, true), (true, true)] {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!());
        cmd.arg(tmpdir.path());
        match envs {
            (false, false) => cmd.args(["--log-format", "$status", "--log-json"]),
            (true, false) => cmd.args(["--log-json"]).env("DUFS_LOG_FORMAT", "$status"),
            (false, true) => cmd
                .args(["--log-format", "$status"])
                .env("DUFS_LOG_JSON", "true"),
            (true, true) => cmd
                .env("DUFS_LOG_FORMAT", "$status")
                .env("DUFS_LOG_JSON", "true"),
        };
        let out = cmd.output()?;
        assert!(!out.status.success(), "{envs:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("--log-json") && stderr.contains("--log-format"),
            "{stderr}"
        );
    }
    Ok(())
}

#[rstest]
fn log_format_and_log_json_conflict_in_the_config(tmpdir: TempDir) -> Result<(), Error> {
    let config = tmpdir.path().join("dufs.yaml");
    std::fs::write(&config, "log-json: true\nlog-format: '$status'\n")?;
    let out = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-c")
        .arg(&config)
        .output()?;
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("log-json"));
    // The default format alone is no conflict.
    std::fs::write(&config, "log-json: true\n")?;
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .args(["-p", "0"])
        .arg("-c")
        .arg(&config)
        .stdout(Stdio::null())
        .spawn()?;
    std::thread::sleep(Duration::from_millis(500));
    assert!(child.try_wait()?.is_none());
    child.kill()?;
    child.wait()?;
    Ok(())
}
