mod digest_auth_util;
mod fixtures;
mod utils;

use digest_auth_util::send_with_digest_auth;
use fixtures::{port, tmpdir, wait_for_port, Error, Port};

use assert_fs::fixture::TempDir;
use rstest::rstest;
use std::io::Read;
use std::process::{Command, Stdio};

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
