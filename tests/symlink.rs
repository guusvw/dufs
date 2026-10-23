mod fixtures;
mod utils;

use assert_fs::fixture::TempDir;
use fixtures::{server, tmpdir, Error, TestServer};
use rstest::rstest;

#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;

#[rstest]
fn default_not_allow_symlink(server: TestServer, tmpdir: TempDir) -> Result<(), Error> {
    // Create symlink directory "foo" to point outside the root
    let dir = "foo";
    symlink_dir(tmpdir.path(), server.path().join(dir)).expect("Couldn't create symlink");
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), dir))?;
    assert_eq!(resp.status(), 404);
    let resp = reqwest::blocking::get(format!("{}{}/index.html", server.url(), dir))?;
    assert_eq!(resp.status(), 404);
    let resp = reqwest::blocking::get(server.url())?;
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    assert!(!paths.contains(&format!("{dir}/")));
    Ok(())
}

#[rstest]
fn allow_symlink(
    #[with(&["--allow-symlink"])] server: TestServer,
    tmpdir: TempDir,
) -> Result<(), Error> {
    // Create symlink directory "foo" to point outside the root
    let dir = "foo";
    symlink_dir(tmpdir.path(), server.path().join(dir)).expect("Couldn't create symlink");
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), dir))?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(format!("{}{}/index.html", server.url(), dir))?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(server.url())?;
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    assert!(paths.contains(&format!("{dir}/")));
    Ok(())
}

#[cfg(unix)]
fn link_outside_index(server: &TestServer, tmpdir: &TempDir) {
    let secret = tmpdir.path().join("secret.txt");
    std::fs::write(&secret, "OUTSIDE-SECRET").unwrap();
    std::fs::create_dir(server.path().join("sub")).unwrap();
    symlink_dir(&secret, server.path().join("sub").join("index.html")).unwrap();
}

#[cfg(unix)]
#[rstest]
fn render_index_ignores_outside_symlink(
    #[with(&["--render-index", "--render-try-index"])] server: TestServer,
    tmpdir: TempDir,
) -> Result<(), Error> {
    link_outside_index(&server, &tmpdir);
    let resp = reqwest::blocking::get(format!("{}sub/", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert!(!resp.text()?.contains("OUTSIDE-SECRET"));
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn render_index_without_try_index_outside_symlink_not_found(
    #[with(&["--render-index"])] server: TestServer,
    tmpdir: TempDir,
) -> Result<(), Error> {
    link_outside_index(&server, &tmpdir);
    let resp = reqwest::blocking::get(format!("{}sub/", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn render_index_allow_symlink_serves_target(
    #[with(&["--render-index", "--allow-symlink"])] server: TestServer,
    tmpdir: TempDir,
) -> Result<(), Error> {
    link_outside_index(&server, &tmpdir);
    let resp = reqwest::blocking::get(format!("{}sub/", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "OUTSIDE-SECRET");
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn render_spa_ignores_outside_symlink(
    #[with(&["--render-spa"])] server: TestServer,
    tmpdir: TempDir,
) -> Result<(), Error> {
    let secret = tmpdir.path().join("secret.txt");
    std::fs::write(&secret, "OUTSIDE-SECRET").unwrap();
    let index = server.path().join("index.html");
    let _ = std::fs::remove_file(&index);
    symlink_dir(&secret, &index).unwrap();
    let resp = reqwest::blocking::get(format!("{}some/route", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn render_spa_allow_symlink_serves_target(
    #[with(&["--render-spa", "--allow-symlink"])] server: TestServer,
    tmpdir: TempDir,
) -> Result<(), Error> {
    let secret = tmpdir.path().join("secret.txt");
    std::fs::write(&secret, "OUTSIDE-SECRET").unwrap();
    let index = server.path().join("index.html");
    let _ = std::fs::remove_file(&index);
    symlink_dir(&secret, &index).unwrap();
    let resp = reqwest::blocking::get(format!("{}some/route", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "OUTSIDE-SECRET");
    Ok(())
}
