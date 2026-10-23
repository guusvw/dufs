mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use predicates::str::contains;
use reqwest::blocking::ClientBuilder;
use rstest::rstest;

use crate::fixtures::port;

/// Can start the server with TLS and receive encrypted responses.
#[rstest]
#[case(server(&[
        "--tls-cert", "tests/data/cert.pem",
        "--tls-key", "tests/data/key_pkcs8.pem",
]))]
#[case(server(&[
        "--tls-cert", "tests/data/cert.pem",
        "--tls-key", "tests/data/key_pkcs1.pem",
]))]
#[case(server(&[
        "--tls-cert", "tests/data/cert_ecdsa.pem",
        "--tls-key", "tests/data/key_ecdsa.pem",
]))]
fn tls_works(#[case] server: TestServer) -> Result<(), Error> {
    let client = ClientBuilder::new()
        .tls_danger_accept_invalid_certs(true)
        .build()?;
    let resp = client.get(server.url()).send()?.error_for_status()?;
    assert_resp_paths!(resp);
    Ok(())
}

/// Wrong path for cert throws error.
#[rstest]
fn wrong_path_cert() -> Result<(), Error> {
    // Hold the port reservation until the child process has run: dufs binds
    // before it loads the cert, so the port must not be handed to another test.
    let port = port();
    assert_cmd::cargo::cargo_bin_cmd!()
        .args([
            "--tls-cert",
            "wrong",
            "--tls-key",
            "tests/data/key.pem",
            "--port",
            &port.to_string(),
        ])
        .assert()
        .failure()
        .stderr(contains("Failed to load cert file at `wrong`"));

    Ok(())
}

/// Wrong paths for key throws errors.
#[rstest]
fn wrong_path_key() -> Result<(), Error> {
    let port = port();
    assert_cmd::cargo::cargo_bin_cmd!()
        .args([
            "--tls-cert",
            "tests/data/cert.pem",
            "--tls-key",
            "wrong",
            "--port",
            &port.to_string(),
        ])
        .assert()
        .failure()
        .stderr(contains("Failed to load key file at `wrong`"));

    Ok(())
}

/// A connection that never sends a ClientHello must not delay other clients.
#[rstest]
fn stalled_handshake_does_not_block_accept(
    // Single listener, so the stalled socket and the request share one accept loop.
    #[with(&[
        "-b", "127.0.0.1",
        "--tls-cert", "tests/data/cert.pem",
        "--tls-key", "tests/data/key_pkcs8.pem",
    ])]
    server: TestServer,
) -> Result<(), Error> {
    let _stalled = std::net::TcpStream::connect(("127.0.0.1", server.port()))?;
    let client = ClientBuilder::new()
        .tls_danger_accept_invalid_certs(true)
        .build()?;
    let start = std::time::Instant::now();
    client
        .get(format!("https://127.0.0.1:{}", server.port()))
        .send()?
        .error_for_status()?;
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "HTTPS request took {:?} behind a stalled handshake",
        start.elapsed()
    );
    Ok(())
}
