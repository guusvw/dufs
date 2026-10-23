//! Share tokens (`?tokengen` / `?token=`) are signed with a key derived from
//! a server secret, never from the user's password.

mod fixtures;
mod utils;

use assert_fs::fixture::TempDir;
use assert_fs::prelude::*;
use ed25519_dalek::{ed25519::signature::Signer, SigningKey};
use fixtures::{port, tmpdir, Error, TestServer};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Stdio};

const SECRET: [u8; 32] = [0x42; 32];
const HASHED_AUTH: &str = "user:$6$gQxZwKyWn/ZmWEA2$4uV7KKMnSUnET2BtWTj/9T5.Jq3h/MdkOlnIl5hdlTxDZ4MZKmJ.kl6C.NL9xnNPqC4lVHC1vuI0E5cLpTJX81@/:rw";
const HASH: &str = "$6$gQxZwKyWn/ZmWEA2$4uV7KKMnSUnET2BtWTj/9T5.Jq3h/MdkOlnIl5hdlTxDZ4MZKmJ.kl6C.NL9xnNPqC4lVHC1vuI0E5cLpTJX81";

/// Start dufs on a free port with `args` appended. Unlike the `server`
/// fixture, arguments are never inspected, so temp paths cannot be mistaken
/// for TLS options.
fn spawn(args: &[&str]) -> TestServer {
    let tmpdir = tmpdir();
    let port = port();
    let child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .stdout(Stdio::null())
        .spawn()
        .expect("Couldn't run test binary");
    fixtures::wait_for_port(&port);
    TestServer::new(port, tmpdir, child, false)
}

fn write_secret(dir: &TempDir, secret: &[u8; 32]) -> String {
    let file = dir.child("token.secret");
    file.write_str(&format!("{}\n", hex::encode(secret)))
        .unwrap();
    restrict(file.path());
    file.path().to_str().unwrap().to_string()
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn restrict(_: &Path) {}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    block[..key.len()].copy_from_slice(key);
    let inner = Sha256::new()
        .chain_update(block.map(|b| b ^ 0x36))
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(block.map(|b| b ^ 0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Build a token by hand: `version | signature | expiry | user`.
fn mint(key: SigningKey, version: u8, path: &str, exp: u64, user: &str) -> String {
    let sig = key.sign(format!("{path}:{exp}").as_bytes()).to_bytes();
    let mut raw = vec![version];
    raw.extend_from_slice(&sig);
    raw.extend_from_slice(&exp.to_be_bytes());
    raw.extend_from_slice(user.as_bytes());
    hex::encode(raw)
}

fn current_key(secret: &[u8; 32], user: &str) -> SigningKey {
    SigningKey::from_bytes(&hmac_sha256(secret, user.as_bytes()))
}

fn status_with_token(server: &TestServer, token: &str) -> Result<u16, Error> {
    let url = format!("{}index.html?token={token}", server.url());
    Ok(fetch!(b"GET", &url).send()?.status().as_u16())
}

fn tokengen(server: &TestServer) -> Result<String, Error> {
    let url = format!("{}index.html?tokengen", server.url());
    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);
    Ok(resp.text()?)
}

#[test]
fn token_is_signed_with_hmac_of_server_secret() -> Result<(), Error> {
    let dir = TempDir::new()?;
    let secret = write_secret(&dir, &SECRET);
    let server = spawn(&["-a", "user:pass@/", "--token-secret-file", &secret]);

    // The issued token carries the version byte and verifies under the
    // documented key derivation.
    let token = tokengen(&server)?;
    assert_eq!(&token[..2], "01");
    assert_eq!(status_with_token(&server, &token)?, 200);

    // A hand-built token with the documented derivation is accepted...
    let exp = now_millis() + 60_000;
    let ok = mint(current_key(&SECRET, "user"), 1, "index.html", exp, "user");
    assert_eq!(status_with_token(&server, &ok)?, 200);

    // ...but only for the path it was signed for.
    let url = format!("{}test.txt?token={ok}", server.url());
    assert_eq!(fetch!(b"GET", &url).send()?.status(), 401);
    Ok(())
}

#[test]
fn expired_token_is_unauthorized() -> Result<(), Error> {
    let dir = TempDir::new()?;
    let secret = write_secret(&dir, &SECRET);
    let server = spawn(&["-a", "user:pass@/", "--token-secret-file", &secret]);

    let key = || current_key(&SECRET, "user");
    let expired = mint(key(), 1, "index.html", now_millis() - 1000, "user");
    assert_eq!(status_with_token(&server, &expired)?, 401);
    Ok(())
}

#[test]
fn token_with_other_version_is_unauthorized() -> Result<(), Error> {
    let dir = TempDir::new()?;
    let secret = write_secret(&dir, &SECRET);
    let server = spawn(&["-a", "user:pass@/", "--token-secret-file", &secret]);

    let exp = now_millis() + 60_000;
    for version in [0, 2] {
        let key = current_key(&SECRET, "user");
        let token = mint(key, version, "index.html", exp, "user");
        assert_eq!(status_with_token(&server, &token)?, 401);
    }
    Ok(())
}

/// Previously the key was `SHA256("{user}:{pass}")`, i.e. derivable from the
/// configured `$6$` hash alone.
#[test]
fn token_from_password_derived_key_is_rejected() -> Result<(), Error> {
    let exp = now_millis() + 60_000;
    let old_key = |pass: &str| {
        let seed: [u8; 32] = Sha256::digest(format!("user:{pass}")).into();
        SigningKey::from_bytes(&seed)
    };

    for (auth, pass) in [(HASHED_AUTH, HASH), ("user:pass@/:rw", "pass")] {
        let server = spawn(&["-a", auth]);
        // Old layout: no version byte.
        let sig = old_key(pass).sign(format!("index.html:{exp}").as_bytes());
        let mut raw = sig.to_bytes().to_vec();
        raw.extend_from_slice(&exp.to_be_bytes());
        raw.extend_from_slice(b"user");
        assert_eq!(status_with_token(&server, &hex::encode(&raw))?, 401);

        // Old key under the new layout.
        let token = mint(old_key(pass), 1, "index.html", exp, "user");
        assert_eq!(status_with_token(&server, &token)?, 401);
    }
    Ok(())
}

#[test]
fn tokens_survive_restart_only_with_secret_file() -> Result<(), Error> {
    // Without a secret file every start picks a new random secret.
    let server = spawn(&["-a", "user:pass@/"]);
    let token = tokengen(&server)?;
    assert_eq!(status_with_token(&server, &token)?, 200);
    drop(server);
    let server = spawn(&["-a", "user:pass@/"]);
    assert_eq!(status_with_token(&server, &token)?, 401);
    drop(server);

    // With a secret file (here created by dufs itself) they stay valid.
    let dir = TempDir::new()?;
    let secret = dir.child("token.secret");
    let secret_arg = secret.path().to_str().unwrap().to_string();
    let args = ["-a", "user:pass@/", "--token-secret-file", &secret_arg];
    let server = spawn(&args);
    let token = tokengen(&server)?;
    assert_eq!(status_with_token(&server, &token)?, 200);
    drop(server);
    assert!(secret.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(secret.path())?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let server = spawn(&args);
    assert_eq!(status_with_token(&server, &token)?, 200);
    drop(server);

    // Rotating the secret invalidates every token.
    write_secret(&dir, &[7; 32]);
    let server = spawn(&args);
    assert_eq!(status_with_token(&server, &token)?, 401);
    Ok(())
}

#[test]
fn invalid_secret_file_is_rejected() -> Result<(), Error> {
    let dir = TempDir::new()?;
    let file = dir.child("token.secret");
    let run = || -> Result<_, Error> {
        Ok(Command::new(assert_cmd::cargo::cargo_bin!())
            .arg(dir.path())
            .args(["-a", "user:pass@/", "--token-secret-file"])
            .arg(file.path())
            .output()?)
    };

    // Wrong length.
    file.write_str("abcd\n")?;
    restrict(file.path());
    let out = run()?;
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("must hold 32 bytes"));

    // Readable by others.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.write_str(&hex::encode(SECRET))?;
        std::fs::set_permissions(file.path(), std::fs::Permissions::from_mode(0o644))?;
        let out = run()?;
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("accessible by other users"));
    }
    Ok(())
}
