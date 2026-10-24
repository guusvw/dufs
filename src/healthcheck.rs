//! `dufs --healthcheck [URL]`: probe a running server from the same image,
//! which has no `curl` to do it.

use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use http_body_util::Empty;
use hyper::{header::HOST, Request, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

/// The probe has this long, connecting and waiting for the answer included.
const TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_PORT: u16 = 5000;
const HEALTH_PATH: &str = "/__dufs__/health";

/// The URL given on the command line after `--healthcheck`, if the command
/// line is a health check at all. `Some(None)` means "use the default URL".
///
/// This looks at the raw arguments so that a health check reads nothing else:
/// no configuration file, and no other option or environment variable that
/// could make the regular parser fail.
pub fn requested(args: impl IntoIterator<Item = String>) -> Option<Option<String>> {
    let mut args = args.into_iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--healthcheck" {
            return Some(args.next().filter(|v| !v.starts_with('-')));
        }
        if let Some(url) = arg.strip_prefix("--healthcheck=") {
            return Some(Some(url.to_string()));
        }
    }
    None
}

/// `http://127.0.0.1:${DUFS_PORT:-5000}/__dufs__/health`
fn default_url() -> String {
    let port = std::env::var("DUFS_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    format!("http://127.0.0.1:{port}{HEALTH_PATH}")
}

/// Exit status of the process: 0 if the server answered 200, else 1 after one
/// line on stderr with the reason.
pub async fn run(url: Option<String>) -> i32 {
    let url = url.unwrap_or_else(default_url);
    match timeout(TIMEOUT, probe(&url)).await {
        Ok(Ok(())) => 0,
        Ok(Err(err)) => fail(&url, &format!("{err:#}")),
        Err(_) => fail(&url, &format!("no answer within {}s", TIMEOUT.as_secs())),
    }
}

fn fail(url: &str, reason: &str) -> i32 {
    eprintln!("healthcheck failed: {url}: {reason}");
    1
}

async fn probe(url: &str) -> Result<()> {
    let uri: Uri = url.parse().with_context(|| "invalid URL")?;
    match uri.scheme_str() {
        Some("http") => {}
        Some(other) => bail!("only http URLs are supported, not {other}"),
        None => bail!("the URL must start with http://"),
    }
    let authority = uri.authority().ok_or_else(|| anyhow!("missing host"))?;
    let address = if authority.port().is_some() {
        authority.to_string()
    } else {
        format!("{}:80", authority.host())
    };
    let stream = TcpStream::connect(&address)
        .await
        .with_context(|| format!("cannot connect to {address}"))?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    // Ends with the exchange; nothing to report if it ends badly after that.
    tokio::spawn(conn);
    let request = Request::get(uri.path_and_query().map_or("/", |v| v.as_str()))
        .header(HOST, authority.as_str())
        .body(Empty::<Bytes>::new())?;
    let response = sender.send_request(request).await?;
    match response.status() {
        StatusCode::OK => Ok(()),
        status => bail!("unexpected status {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn requested_finds_the_flag_and_its_url() {
        assert_eq!(requested(args(&["dufs"])), None);
        assert_eq!(requested(args(&["dufs", "--healthcheck"])), Some(None));
        assert_eq!(
            requested(args(&["dufs", "--healthcheck", "http://h:1/x"])),
            Some(Some("http://h:1/x".to_string()))
        );
        assert_eq!(
            requested(args(&["dufs", "--healthcheck=http://h:1/x"])),
            Some(Some("http://h:1/x".to_string()))
        );
        assert_eq!(
            requested(args(&["dufs", "--healthcheck", "-p", "1"])),
            Some(None)
        );
    }
}
