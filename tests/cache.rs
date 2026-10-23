mod fixtures;
mod utils;

use chrono::{DateTime, Duration};
use fixtures::{server, Error, TestServer};
use reqwest::header::{
    HeaderName, CACHE_CONTROL, ETAG, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH,
    IF_UNMODIFIED_SINCE, LAST_MODIFIED,
};
use reqwest::StatusCode;
use rstest::rstest;
use std::time::{Duration as StdDuration, SystemTime};

#[rstest]
#[case(IF_UNMODIFIED_SINCE, Duration::days(1), StatusCode::OK)]
#[case(IF_UNMODIFIED_SINCE, Duration::days(0), StatusCode::OK)]
#[case(IF_UNMODIFIED_SINCE, Duration::days(-1), StatusCode::PRECONDITION_FAILED)]
#[case(IF_MODIFIED_SINCE, Duration::days(1), StatusCode::NOT_MODIFIED)]
#[case(IF_MODIFIED_SINCE, Duration::days(0), StatusCode::NOT_MODIFIED)]
#[case(IF_MODIFIED_SINCE, Duration::days(-1), StatusCode::OK)]
fn get_file_with_if_modified_since_condition(
    #[case] header_condition: HeaderName,
    #[case] duration_after_file_modified: Duration,
    #[case] expected_code: StatusCode,
    server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url())).send()?;

    let last_modified = resp
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| DateTime::parse_from_rfc2822(s).ok())
        .expect("Received no valid last modified header");

    let req_modified_time = (last_modified + duration_after_file_modified)
        .format("%a, %d %b %Y %T GMT")
        .to_string();

    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header(header_condition, req_modified_time)
        .send()?;

    assert_eq!(resp.status(), expected_code);
    Ok(())
}

fn same_etag(etag: &str) -> String {
    etag.to_owned()
}

fn different_etag(etag: &str) -> String {
    format!("{etag}1234")
}

#[rstest]
#[case(IF_MATCH, same_etag, StatusCode::OK)]
#[case(IF_MATCH, different_etag, StatusCode::PRECONDITION_FAILED)]
#[case(IF_NONE_MATCH, same_etag, StatusCode::NOT_MODIFIED)]
#[case(IF_NONE_MATCH, different_etag, StatusCode::OK)]
fn get_file_with_etag_match(
    #[case] header_condition: HeaderName,
    #[case] etag_modifier: fn(&str) -> String,
    #[case] expected_code: StatusCode,
    server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url())).send()?;

    let etag = resp
        .headers()
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .expect("Received no valid etag header");

    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header(header_condition, etag_modifier(etag))
        .send()?;

    assert_eq!(resp.status(), expected_code);
    Ok(())
}

const UPLOAD_ARGS: &[&str] = &["-A"];

fn header(resp: &reqwest::blocking::Response, name: HeaderName) -> String {
    resp.headers()
        .get(name)
        .and_then(|h| h.to_str().ok())
        .expect("missing header")
        .to_owned()
}

/// Give `file` the whole-second mtime `secs` plus `millis` milliseconds.
fn set_mtime(file: &std::path::Path, secs: u64, millis: u64) {
    let mtime =
        SystemTime::UNIX_EPOCH + StdDuration::from_secs(secs) + StdDuration::from_millis(millis);
    std::fs::OpenOptions::new()
        .write(true)
        .open(file)
        .and_then(|f| f.set_modified(mtime))
        .unwrap();
}

#[rstest]
fn same_second_rewrite_is_not_served_as_not_modified(server: TestServer) -> Result<(), Error> {
    let file = server.path().join("index.html");
    set_mtime(&file, 1_700_000_000, 0);
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"HEAD", &url).send()?;
    let (etag, last_modified) = (header(&resp, ETAG), header(&resp, LAST_MODIFIED));

    // Rewritten within the same HTTP-date second: only the ETag tells.
    std::fs::write(&file, "changed")?;
    set_mtime(&file, 1_700_000_000, 500);

    let resp = fetch!(b"GET", &url)
        .header(IF_NONE_MATCH, &etag)
        .header(IF_MODIFIED_SINCE, &last_modified)
        .send()?;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.text()?, "changed");
    Ok(())
}

#[rstest]
fn if_match_takes_precedence_over_if_unmodified_since(server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"HEAD", &url).send()?;
    let etag = header(&resp, ETAG);
    let stale = "Thu, 01 Jan 1970 00:00:00 GMT";

    let resp = fetch!(b"GET", &url)
        .header(IF_MATCH, &etag)
        .header(IF_UNMODIFIED_SINCE, stale)
        .send()?;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = fetch!(b"GET", &url)
        .header(IF_UNMODIFIED_SINCE, stale)
        .send()?;
    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
    Ok(())
}

#[rstest]
fn if_none_match_takes_precedence_over_if_modified_since(server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"HEAD", &url).send()?;
    let etag = header(&resp, ETAG);
    let future = "Fri, 01 Jan 2100 00:00:00 GMT";

    let resp = fetch!(b"GET", &url)
        .header(IF_NONE_MATCH, format!("{etag}1234"))
        .header(IF_MODIFIED_SINCE, future)
        .send()?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

#[rstest]
fn not_modified_carries_validators(server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"HEAD", &url).send()?;
    let etag = header(&resp, ETAG);
    let last_modified = header(&resp, LAST_MODIFIED);

    for (name, value) in [
        (IF_NONE_MATCH, etag.as_str()),
        (IF_MODIFIED_SINCE, last_modified.as_str()),
    ] {
        let resp = fetch!(b"GET", &url).header(name, value).send()?;
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(header(&resp, ETAG), etag);
        assert_eq!(header(&resp, LAST_MODIFIED), last_modified);
        assert_eq!(header(&resp, CACHE_CONTROL), "no-cache");
    }
    Ok(())
}

#[rstest]
fn put_with_stale_if_match_is_refused(
    #[with(UPLOAD_ARGS)] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let original = std::fs::read(server.path().join("index.html"))?;
    let resp = fetch!(b"HEAD", &url).send()?;
    let etag = header(&resp, ETAG);

    let resp = fetch!(b"PUT", &url)
        .header(IF_MATCH, format!("{etag}1234"))
        .body("overwritten")
        .send()?;
    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(std::fs::read(server.path().join("index.html"))?, original);

    let resp = fetch!(b"PUT", &url)
        .header(IF_MATCH, &etag)
        .body("updated")
        .send()?;
    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(
        std::fs::read_to_string(server.path().join("index.html"))?,
        "updated"
    );

    // The ETag changed with the write, so the same validator is stale now.
    let resp = fetch!(b"PUT", &url)
        .header(IF_MATCH, &etag)
        .body("again")
        .send()?;
    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(
        std::fs::read_to_string(server.path().join("index.html"))?,
        "updated"
    );
    Ok(())
}

#[rstest]
fn put_with_if_unmodified_since(#[with(UPLOAD_ARGS)] server: TestServer) -> Result<(), Error> {
    let file = server.path().join("index.html");
    set_mtime(&file, 1_700_000_000, 0);
    let url = format!("{}index.html", server.url());
    let original = std::fs::read(&file)?;

    let resp = fetch!(b"PUT", &url)
        .header(IF_UNMODIFIED_SINCE, "Tue, 14 Nov 2023 22:13:19 GMT")
        .body("overwritten")
        .send()?;
    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(std::fs::read(&file)?, original);

    let resp = fetch!(b"PUT", &url)
        .header(IF_UNMODIFIED_SINCE, "Tue, 14 Nov 2023 22:13:20 GMT")
        .body("updated")
        .send()?;
    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(std::fs::read_to_string(&file)?, "updated");
    Ok(())
}

#[rstest]
fn put_if_match_on_missing_file_is_refused(
    #[with(UPLOAD_ARGS)] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}new.txt", server.url());
    for value in ["*", "\"1-1\""] {
        let resp = fetch!(b"PUT", &url)
            .header(IF_MATCH, value)
            .body("x")
            .send()?;
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
        assert!(!server.path().join("new.txt").exists());
    }
    let resp = fetch!(b"PUT", &url).body("x").send()?;
    assert_eq!(resp.status(), StatusCode::CREATED);
    Ok(())
}
