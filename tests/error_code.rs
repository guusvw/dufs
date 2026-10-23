mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

fn error_code(resp: &reqwest::blocking::Response) -> Option<&str> {
    resp.headers()
        .get("x-dufs-error-code")
        .and_then(|v| v.to_str().ok())
}

#[rstest]
fn invalid_path(#[with(&["--path-prefix", "/foo"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}bar", server.url())).send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-path"));
    Ok(())
}

#[rstest]
fn invalid_destination(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MOVE", format!("{}test.txt", server.url())).send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-destination"));
    Ok(())
}

#[rstest]
fn invalid_depth(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dir1/", server.url()))
        .header("Depth", "2")
        .send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-depth"));
    Ok(())
}

#[rstest]
fn invalid_upload_offset(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let resp = fetch!(b"PATCH", &url)
        .header("X-Update-Range", "bogus")
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-upload-offset"));

    let resp = fetch!(b"PATCH", &url).body("abc").send()?;
    assert_eq!(resp.status(), 405);
    assert_eq!(error_code(&resp), Some("missing-upload-offset"));
    Ok(())
}

#[rstest]
fn upload_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PUT", format!("{}new.txt", server.url()))
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("upload-disabled"));
    Ok(())
}

#[rstest]
fn delete_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"DELETE", format!("{}test.txt", server.url())).send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("delete-disabled"));
    Ok(())
}

#[rstest]
fn hash_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}test.txt?hash", server.url())).send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("hash-disabled"));
    Ok(())
}

#[rstest]
fn put_directory(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PUT", format!("{}dir1/", server.url()))
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 409);
    assert_eq!(error_code(&resp), Some("is-a-directory"));
    Ok(())
}

#[rstest]
fn copy_directory(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"COPY", format!("{}dir1/", server.url()))
        .header("Destination", format!("{}copied", server.url()))
        .send()?;
    assert_eq!(resp.status(), 409);
    assert_eq!(error_code(&resp), Some("is-a-directory"));
    Ok(())
}

#[rstest]
fn mkcol_already_exists(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MKCOL", format!("{}dir1/", server.url())).send()?;
    assert_eq!(resp.status(), 405);
    assert_eq!(error_code(&resp), Some("already-exists"));
    Ok(())
}

#[rstest]
fn method_not_allowed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"FOO", server.url()).send()?;
    assert_eq!(resp.status(), 405);
    assert_eq!(error_code(&resp), Some("method-not-allowed"));
    Ok(())
}

#[rstest]
fn precondition_failed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}test.txt", server.url()))
        .header("If-Match", "\"bogus\"")
        .send()?;
    assert_eq!(resp.status(), 412);
    assert_eq!(error_code(&resp), Some("precondition-failed"));
    Ok(())
}

#[rstest]
fn copy_destination_exists(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"COPY", format!("{}test.txt", server.url()))
        .header("Destination", format!("{}index.html", server.url()))
        .header("Overwrite", "F")
        .send()?;
    assert_eq!(resp.status(), 412);
    assert_eq!(error_code(&resp), Some("destination-exists"));
    Ok(())
}

#[rstest]
fn archive_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}?zip", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(error_code(&resp), Some("archive-disabled"));
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[rstest]
fn io_permission_denied(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let dir = server.path().join("readonly");
    std::fs::create_dir(&dir)?;
    set_mode(&dir, 0o555);
    // Root ignores the mode bits, so there is nothing to test.
    if std::fs::File::create(dir.join("probe")).is_ok() {
        set_mode(&dir, 0o755);
        return Ok(());
    }

    let resp = fetch!(b"PUT", format!("{}readonly/new.txt", server.url()))
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("permission-denied"));
    assert!(!resp.text()?.is_empty());

    set_mode(&dir, 0o000);
    let resp = fetch!(b"PROPFIND", format!("{}readonly/", server.url()))
        .header("Depth", "1")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("permission-denied"));

    set_mode(&dir, 0o755);
    Ok(())
}

#[rstest]
fn io_not_a_directory(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PUT", format!("{}test.txt/new.txt", server.url()))
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 409);
    assert_eq!(error_code(&resp), Some("conflict"));
    assert!(!resp.text()?.is_empty());
    Ok(())
}

#[rstest]
fn io_is_a_directory(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MOVE", format!("{}test.txt", server.url()))
        .header("Destination", format!("{}dir1", server.url()))
        .send()?;
    assert_eq!(resp.status(), 409);
    assert_eq!(error_code(&resp), Some("is-a-directory"));
    assert!(!resp.text()?.is_empty());
    Ok(())
}

#[rstest]
fn io_not_found(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MOVE", format!("{}missing.txt", server.url()))
        .header("Destination", format!("{}moved.txt", server.url()))
        .send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(error_code(&resp), None);
    assert!(!resp.text()?.is_empty());
    Ok(())
}
