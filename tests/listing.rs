mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;
use rstest::rstest;
use serde_json::Value;

#[cfg(unix)]
#[rstest]
fn unreadable_dir_is_listed_with_null_size(server: TestServer) -> Result<(), Error> {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;

    let locked = server.path().join("locked");
    fs::create_dir(&locked)?;
    fs::set_permissions(&locked, Permissions::from_mode(0o000))?;
    // root ignores permission bits, nothing to observe then
    let readable = fs::read_dir(&locked).is_ok();

    let resp = reqwest::blocking::get(format!("{}?json", server.url()));
    let json: Result<Value, Error> = resp
        .map_err(Error::from)
        .and_then(|v| Ok(serde_json::from_str(&v.text()?)?));
    fs::set_permissions(&locked, Permissions::from_mode(0o755))?;
    if readable {
        return Ok(());
    }

    let json = json?;
    let paths = json["paths"].as_array().unwrap();
    let item = paths
        .iter()
        .find(|v| v["name"] == "locked")
        .expect("unreadable dir must still be listed");
    assert_eq!(item["path_type"], "Dir");
    assert!(item["size"].is_null());
    let other = paths.iter().find(|v| v["name"] == "dir1").unwrap();
    assert!(other["size"].is_u64());
    Ok(())
}

#[rstest]
fn file_size_is_a_number(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?json", server.url()))?;
    let json: Value = serde_json::from_str(&resp.text()?)?;
    let item = json["paths"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "index.html")
        .unwrap();
    assert!(item["size"].is_u64());
    Ok(())
}

#[rstest]
fn dir_json_etag_gives_304(server: TestServer) -> Result<(), Error> {
    let url = format!("{}?json", server.url());
    let resp = reqwest::blocking::get(&url)?;
    assert_eq!(resp.status(), StatusCode::OK);
    let etag = resp.headers().get(ETAG).expect("etag").clone();

    let client = reqwest::blocking::Client::new();
    let resp = client.get(&url).header(IF_NONE_MATCH, &etag).send()?;
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(resp.headers().get(ETAG), Some(&etag));

    let resp = client.get(&url).header(IF_NONE_MATCH, "\"other\"").send()?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

#[rstest]
fn dir_json_etag_changes_with_entries(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}?json", server.url());
    let resp = reqwest::blocking::get(&url)?;
    let etag = resp.headers().get(ETAG).expect("etag").clone();

    let resp = fetch!(b"PUT", format!("{}new-file", server.url()))
        .body("x")
        .send()?;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = reqwest::blocking::Client::new()
        .get(&url)
        .header(IF_NONE_MATCH, &etag)
        .send()?;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_ne!(resp.headers().get(ETAG), Some(&etag));
    Ok(())
}
