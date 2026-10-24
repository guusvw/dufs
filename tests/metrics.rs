mod digest_auth_util;
mod fixtures;
mod utils;

use assert_fs::fixture::TempDir;
use digest_auth_util::send_with_digest_auth;
use fixtures::{port, tmpdir, wait_for_port, Error, Port};
use regex::Regex;
use rstest::rstest;
use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const ID: &str = "0123456789abcdef0123456789abcdef";

/// A dufs with the metrics listener on its own port.
struct Dufs {
    child: Child,
    dir: TempDir,
    port: Port,
    metrics_port: Port,
}

impl Dufs {
    fn start(dir: TempDir, args: &[&str]) -> Dufs {
        let (port, metrics_port) = (port(), port());
        let child = Command::new(assert_cmd::cargo::cargo_bin!())
            .arg(dir.path())
            .args(["-b", "127.0.0.1", "-p", &port.to_string()])
            .args(["--metrics-bind", &format!("127.0.0.1:{metrics_port}")])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Couldn't run test binary");
        wait_for_port(&port);
        wait_for_port(&metrics_port);
        Dufs {
            child,
            dir,
            port,
            metrics_port,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn scrape_text(&self) -> String {
        let url = format!("http://127.0.0.1:{}/metrics", self.metrics_port);
        let resp = reqwest::blocking::get(url).unwrap();
        assert_eq!(resp.status(), 200);
        resp.text().unwrap()
    }

    fn scrape(&self) -> Metrics {
        Metrics::parse(&self.scrape_text())
    }

    /// Scrape until `check` accepts the samples; fail after a few seconds.
    fn wait_for(&self, what: &str, check: impl Fn(&Metrics) -> bool) -> Metrics {
        let start = Instant::now();
        loop {
            let metrics = self.scrape();
            if check(&metrics) {
                return metrics;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "timed out waiting for {what}:\n{}",
                metrics.text
            );
            sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Dufs {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The samples of one scrape, keyed by `name{labels}`.
struct Metrics {
    text: String,
    samples: BTreeMap<String, f64>,
}

impl Metrics {
    /// Parses the text with a strict check of the Prometheus 0.0.4 format.
    fn parse(text: &str) -> Metrics {
        let name = "[a-zA-Z_:][a-zA-Z0-9_:]*";
        let label = r#"[a-zA-Z_][a-zA-Z0-9_]*="([^"\\\n]|\\.)*""#;
        let comment = Regex::new(&format!(r"^# (HELP|TYPE) {name} .+$")).unwrap();
        let sample = Regex::new(&format!(
            r"^({name})(\{{({label}(,{label})*)?\}})? (-?[0-9]+(\.[0-9]+)?(e[+-]?[0-9]+)?|NaN|[+-]Inf)$"
        ))
        .unwrap();
        let mut samples = BTreeMap::new();
        let mut typed = std::collections::HashSet::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("# TYPE ") {
                assert!(comment.is_match(line), "bad comment: {line}");
                let kind = rest.rsplit(' ').next().unwrap();
                assert!(
                    ["counter", "gauge", "histogram"].contains(&kind),
                    "bad type: {line}"
                );
                typed.insert(rest.split(' ').next().unwrap().to_string());
            } else if line.starts_with('#') {
                assert!(comment.is_match(line), "bad comment: {line}");
            } else {
                assert!(sample.is_match(line), "bad sample: {line}");
                let (series, value) = line.rsplit_once(' ').unwrap();
                let family = series.split(['{', ' ']).next().unwrap();
                let family = family
                    .strip_suffix("_bucket")
                    .or_else(|| family.strip_suffix("_sum"))
                    .or_else(|| family.strip_suffix("_count"))
                    .filter(|f| typed.contains(*f))
                    .unwrap_or(family);
                assert!(typed.contains(family), "no # TYPE before: {line}");
                let previous = samples.insert(series.to_string(), value.parse().unwrap());
                assert!(previous.is_none(), "duplicate series: {series}");
            }
        }
        assert!(text.ends_with('\n'));
        Metrics {
            text: text.to_string(),
            samples,
        }
    }

    /// Value of series `name{labels}`; 0 for a series that is not exposed.
    fn get(&self, series: &str) -> f64 {
        self.samples.get(series).copied().unwrap_or(0.0)
    }

    fn has(&self, series: &str) -> bool {
        self.samples.contains_key(series)
    }
}

fn upload(result: &str) -> String {
    format!("dufs_uploads_total{{result=\"{result}\"}}")
}

fn walk(kind: &str, result: &str) -> String {
    format!("dufs_walks_total{{kind=\"{kind}\",result=\"{result}\"}}")
}

/// A PUT that sends only the start of its body and then loses the connection.
fn interrupted_put(dufs: &Dufs, path: &str, total: usize, sent: &[u8], extra_headers: &str) {
    let mut stream =
        TcpStream::connect(("127.0.0.1", dufs.port.to_string().parse().unwrap())).expect("connect");
    write!(
        stream,
        "PUT {path} HTTP/1.1\r\nHost: localhost\r\n{extra_headers}Content-Length: {total}\r\n\r\n"
    )
    .unwrap();
    stream.write_all(sent).unwrap();
    stream.flush().unwrap();
    // Let the server read what was sent before the connection goes away.
    sleep(Duration::from_millis(200));
}

fn big_tree(dir: &std::path::Path, dirs: usize, files: usize) {
    for d in 0..dirs {
        let dir = dir.join(format!("big/d{d}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..files {
            std::fs::File::create(dir.join(format!("f{f}.dat"))).unwrap();
        }
    }
}

fn send_search(dufs: &Dufs, query: &str) -> TcpStream {
    let mut stream =
        TcpStream::connect(("127.0.0.1", dufs.port.to_string().parse().unwrap())).expect("connect");
    write!(
        stream,
        "GET /big/?q={query} HTTP/1.1\r\nHost: localhost\r\n\r\n"
    )
    .unwrap();
    stream
}

#[cfg(target_os = "linux")]
fn listening_sockets(pid: u32) -> usize {
    let inodes: Vec<String> = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .unwrap()
        .filter_map(|v| std::fs::read_link(v.ok()?.path()).ok())
        .filter_map(|v| {
            v.to_string_lossy()
                .strip_prefix("socket:[")
                .and_then(|v| v.strip_suffix(']'))
                .map(str::to_string)
        })
        .collect();
    let tables: Vec<String> = ["tcp", "tcp6"]
        .iter()
        .map(|table| {
            std::fs::read_to_string(format!("/proc/{pid}/net/{table}")).unwrap_or_default()
        })
        .collect();
    tables
        .iter()
        .flat_map(|table| table.lines().skip(1))
        .filter(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // State 0A is LISTEN; the inode is the tenth field.
            fields.get(3) == Some(&"0A")
                && inodes.iter().any(|i| Some(&i.as_str()) == fields.get(9))
        })
        .count()
}

#[rstest]
fn without_the_flag_nothing_listens_on_a_metrics_port(
    tmpdir: TempDir,
    port: Port,
    #[from(port)] other: Port,
) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .args(["-b", "127.0.0.1", "-p", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    wait_for_port(&port);
    #[cfg(target_os = "linux")]
    assert_eq!(listening_sockets(child.id()), 1);
    assert!(!port_check::is_port_reachable(format!("127.0.0.1:{other}")));
    // The main port serves no metrics either.
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{port}/metrics"))?;
    assert_eq!(resp.status(), 404);
    child.kill()?;
    child.wait()?;
    Ok(())
}

#[rstest]
fn metrics_parse_and_stay_off_the_main_port(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &[]);
    #[cfg(target_os = "linux")]
    assert_eq!(listening_sockets(dufs.child.id()), 2);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", dufs.metrics_port))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()["content-type"],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    let metrics = Metrics::parse(&resp.text()?);
    let build_info = format!(
        "dufs_build_info{{version=\"{}\",commit=\"",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        metrics
            .samples
            .iter()
            .any(|(k, v)| k.starts_with(&build_info) && *v == 1.0),
        "{}",
        metrics.text
    );

    // The file server does not know `/metrics`...
    let resp = reqwest::blocking::get(dufs.url("/metrics"))?;
    assert_eq!(resp.status(), 404);
    // ...and the metrics listener serves nothing else.
    let base = format!("http://127.0.0.1:{}", dufs.metrics_port);
    for path in ["/", "/metrics/", "/__dufs__/health", "/test.txt"] {
        assert_eq!(
            reqwest::blocking::get(format!("{base}{path}"))?.status(),
            404
        );
    }
    let resp = reqwest::blocking::Client::new()
        .post(format!("{base}/metrics"))
        .send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn health_checks_and_scrapes_are_not_counted(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &[]);
    for _ in 0..3 {
        assert_eq!(
            reqwest::blocking::get(dufs.url("/__dufs__/health"))?.status(),
            200
        );
        dufs.scrape();
    }
    let metrics = dufs.scrape();
    assert!(!metrics.text.contains("dufs_http_requests_total{"));
    assert_eq!(metrics.get("dufs_http_request_duration_seconds_count"), 0.0);

    assert_eq!(reqwest::blocking::get(dufs.url("/test.txt"))?.status(), 200);
    assert_eq!(reqwest::blocking::get(dufs.url("/nope"))?.status(), 404);
    let metrics = dufs.scrape();
    assert_eq!(
        metrics.get("dufs_http_requests_total{method=\"GET\",class=\"2xx\"}"),
        1.0
    );
    assert_eq!(
        metrics.get("dufs_http_requests_total{method=\"GET\",class=\"4xx\"}"),
        1.0
    );
    assert_eq!(metrics.get("dufs_http_request_duration_seconds_count"), 2.0);
    assert_eq!(
        metrics.get("dufs_http_request_duration_seconds_bucket{le=\"+Inf\"}"),
        2.0
    );
    assert!(metrics.get("dufs_http_request_duration_seconds_sum") > 0.0);
    Ok(())
}

#[rstest]
fn put_counts_an_ok_upload_and_its_bytes(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["--allow-upload", "--allow-delete"]);
    let before = dufs.scrape();
    let body = "x".repeat(12_345);
    let resp = fetch!(b"PUT", dufs.url("/new.txt")).body(body).send()?;
    assert_eq!(resp.status(), 201);
    let after = dufs.scrape();
    assert_eq!(after.get(&upload("ok")) - before.get(&upload("ok")), 1.0);
    assert_eq!(
        after.get("dufs_upload_bytes_total") - before.get("dufs_upload_bytes_total"),
        12_345.0
    );
    assert_eq!(after.get("dufs_uploads_active"), 0.0);
    assert_eq!(after.get(&upload("failed")), 0.0);
    assert_eq!(after.get(&upload("aborted")), 0.0);
    assert_eq!(
        after.get("dufs_http_requests_total{method=\"PUT\",class=\"2xx\"}"),
        1.0
    );
    Ok(())
}

#[rstest]
fn aborted_put_counts_as_aborted(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["--allow-upload", "--allow-delete"]);
    interrupted_put(&dufs, "/half.bin", 100_000, &[7u8; 1000], "");
    let metrics = dufs.wait_for("the aborted upload", |m| m.get(&upload("aborted")) == 1.0);
    assert_eq!(metrics.get(&upload("ok")), 0.0);
    assert_eq!(metrics.get("dufs_upload_bytes_total"), 1000.0);
    assert_eq!(metrics.get("dufs_uploads_active"), 0.0);
    assert_eq!(metrics.get("dufs_http_requests_in_flight"), 0.0);
    // The client leaving is not a server error.
    assert_eq!(
        metrics.get("dufs_http_requests_total{method=\"PUT\",class=\"4xx\"}"),
        1.0
    );
    assert_eq!(
        metrics.get("dufs_http_requests_total{method=\"PUT\",class=\"5xx\"}"),
        0.0
    );
    Ok(())
}

#[rstest]
fn resumed_patch_counts_as_resumed(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["--allow-upload", "--allow-delete"]);
    let data: Vec<u8> = (0..20_000u32).map(|v| (v % 251) as u8).collect();
    let url = dufs.url(&format!("/resume.bin?upload={ID}"));
    interrupted_put(
        &dufs,
        &format!("/resume.bin?upload={ID}"),
        data.len(),
        &data[..5000],
        "",
    );
    dufs.wait_for("the interrupted upload", |m| {
        m.get(&upload("aborted")) == 1.0
    });

    let client = reqwest::blocking::Client::new();
    let received: usize = client.head(&url).send()?.headers()["x-dufs-upload-offset"]
        .to_str()?
        .parse()?;
    assert!(received > 0 && received <= 5000);
    let resp = fetch!(b"PATCH", &url)
        .header("X-Update-Range", format!("bytes={received}-"))
        .body(data[received..].to_vec())
        .send()?;
    assert_eq!(resp.status(), 204);

    let metrics = dufs.scrape();
    assert_eq!(metrics.get(&upload("resumed")), 1.0);
    assert_eq!(metrics.get(&upload("ok")), 0.0);
    assert_eq!(metrics.get("dufs_upload_bytes_total"), data.len() as f64);
    assert_eq!(std::fs::read(dufs.dir.path().join("resume.bin"))?, data);
    Ok(())
}

#[rstest]
fn refused_and_failed_uploads_are_told_apart(tmpdir: TempDir) -> Result<(), Error> {
    // Uploads are not allowed: a PUT is refused before any data is taken.
    let dufs = Dufs::start(tmpdir, &[]);
    let resp = fetch!(b"PUT", dufs.url("/new.txt")).body("x").send()?;
    assert_eq!(resp.status(), 403);
    let metrics = dufs.scrape();
    assert_eq!(metrics.get(&upload("refused")), 1.0);
    assert_eq!(metrics.get(&upload("ok")), 0.0);
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn upload_into_a_read_only_directory_counts_as_failed(tmpdir: TempDir) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    // A privileged process ignores the mode bits.
    if Command::new("id").arg("-u").output()?.stdout == b"0\n" {
        return Ok(());
    }
    let dufs = Dufs::start(tmpdir, &["--allow-upload", "--allow-delete"]);
    let ro = dufs.dir.path().join("ro");
    std::fs::create_dir(&ro)?;
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555))?;
    let resp = fetch!(b"PUT", dufs.url("/ro/new.txt")).body("x").send()?;
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755))?;
    assert_eq!(resp.status(), 403);
    assert_eq!(resp.headers()["x-dufs-error-code"], "permission-denied");
    let metrics = dufs.scrape();
    assert_eq!(metrics.get(&upload("failed")), 1.0);
    assert_eq!(metrics.get(&upload("refused")), 0.0);
    Ok(())
}

#[rstest]
fn searches_and_zips_are_counted_by_how_they_end(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["-A", "--search-limit", "3"]);
    for i in 0..10 {
        std::fs::write(dufs.dir.path().join(format!("limit-{i}.txt")), "x")?;
    }
    let resp = reqwest::blocking::get(dufs.url("/?q=limit-&json"))?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(dufs.url("/?q=zzzz-no-match&json"))?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(dufs.url("/dir1/?zip"))?;
    assert_eq!(resp.status(), 200);
    resp.bytes()?;

    let metrics = dufs.wait_for("the walks", |m| m.get(&walk("zip", "complete")) == 1.0);
    assert_eq!(metrics.get(&walk("search", "truncated")), 1.0);
    assert_eq!(metrics.get(&walk("search", "complete")), 1.0);
    assert_eq!(metrics.get(&walk("search", "cancelled")), 0.0);
    assert_eq!(metrics.get("dufs_walks_active"), 0.0);
    assert_eq!(metrics.get("dufs_walks_waiting"), 0.0);
    Ok(())
}

#[rstest]
fn a_search_the_client_left_is_cancelled(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["--allow-search"]);
    big_tree(dufs.dir.path(), 100, 300);
    let stream = send_search(&dufs, "zzzz");
    dufs.wait_for("the walk to start", |m| m.get("dufs_walks_active") == 1.0);
    drop(stream);
    let metrics = dufs.wait_for("the cancelled walk", |m| {
        m.get(&walk("search", "cancelled")) == 1.0 && m.get("dufs_walks_active") == 0.0
    });
    assert_eq!(metrics.get(&walk("search", "complete")), 0.0);
    Ok(())
}

#[rstest]
fn walks_beyond_the_slots_are_seen_waiting(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["--allow-search"]);
    big_tree(dufs.dir.path(), 100, 300);
    let slots = std::thread::available_parallelism()?.get();
    let streams: Vec<_> = (0..slots + 4).map(|_| send_search(&dufs, "zzzz")).collect();
    let metrics = dufs.wait_for("walks to wait", |m| m.get("dufs_walks_waiting") >= 1.0);
    assert!(metrics.get("dufs_walks_active") <= slots as f64);
    drop(streams);
    dufs.wait_for("the walks to end", |m| {
        m.get("dufs_walks_active") == 0.0 && m.get("dufs_walks_waiting") == 0.0
    });
    Ok(())
}

#[rstest]
fn part_files_are_gauged_and_swept(tmpdir: TempDir) -> Result<(), Error> {
    let old = format!(".old.txt.dufs-{ID}.part");
    let fresh = format!(".fresh.txt.dufs-{}.part", "f".repeat(32));
    std::fs::write(tmpdir.path().join(&old), "12345")?;
    std::fs::write(tmpdir.path().join(&fresh), "1234567")?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(tmpdir.path().join(&old))?;
    file.set_modified(std::time::SystemTime::now() - Duration::from_secs(7200))?;
    drop(file);

    let dufs = Dufs::start(tmpdir, &["--allow-upload", "--upload-part-ttl", "1h"]);
    let metrics = dufs.wait_for("the first sweep", |m| {
        m.get("dufs_part_files_swept_total") == 1.0
    });
    assert_eq!(metrics.get("dufs_part_files"), 1.0);
    assert_eq!(metrics.get("dufs_part_bytes"), 7.0);
    assert!(!dufs.dir.path().join(&old).exists());
    assert!(dufs.dir.path().join(&fresh).exists());
    Ok(())
}

#[rstest]
fn rejected_credentials_are_counted_by_scheme(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["-a", "user:pass@/:rw"]);
    let url = dufs.url("/");

    // Asking for credentials, or giving valid ones, is not a failure.
    let resp = send_with_digest_auth(fetch!(b"GET", &url), "user", "pass")?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        dufs.scrape()
            .get("dufs_auth_failures_total{scheme=\"digest\"}"),
        0.0
    );

    let resp = send_with_digest_auth(fetch!(b"GET", &url), "user", "wrong")?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("wrong"))
        .send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", &url)
        .basic_auth("nobody", Some("x"))
        .send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", dufs.url("/?token=00")).send()?;
    assert_eq!(resp.status(), 401);

    let metrics = dufs.scrape();
    assert_eq!(
        metrics.get("dufs_auth_failures_total{scheme=\"digest\"}"),
        1.0
    );
    assert_eq!(
        metrics.get("dufs_auth_failures_total{scheme=\"basic\"}"),
        2.0
    );
    assert_eq!(
        metrics.get("dufs_auth_failures_total{scheme=\"token\"}"),
        1.0
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[rstest]
fn process_gauges_come_from_proc(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &[]);
    let metrics = dufs.scrape();
    assert!(metrics.get("process_threads") > 0.0);
    assert!(metrics.get("process_open_fds") >= 3.0);
    assert!(metrics.get("process_resident_memory_bytes") > 100_000.0);
    let start = metrics.get("process_start_time_seconds");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs_f64();
    assert!(
        start > now - 3600.0 && start <= now + 1.0,
        "start {start}, now {now}"
    );
    Ok(())
}

#[cfg(unix)]
#[rstest]
fn draining_is_visible_while_the_server_stops(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(tmpdir, &["--allow-upload", "--shutdown-timeout", "20"]);
    assert_eq!(dufs.scrape().get("dufs_draining"), 0.0);
    // A request in flight keeps the server draining instead of exiting.
    let mut stream = TcpStream::connect(("127.0.0.1", dufs.port.to_string().parse().unwrap()))?;
    write!(
        stream,
        "PUT /slow.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 10\r\n\r\n01234"
    )?;
    stream.flush()?;
    sleep(Duration::from_millis(300));
    let status = Command::new("kill")
        .args(["-TERM", &dufs.child.id().to_string()])
        .status()?;
    assert!(status.success());
    let metrics = dufs.wait_for("draining", |m| m.get("dufs_draining") == 1.0);
    assert_eq!(metrics.get("dufs_uploads_active"), 1.0);
    assert_eq!(metrics.get("dufs_http_requests_in_flight"), 1.0);
    Ok(())
}

#[rstest]
fn series_stay_within_the_budget(tmpdir: TempDir) -> Result<(), Error> {
    let dufs = Dufs::start(
        tmpdir,
        &[
            "-A",
            "--search-limit",
            "3",
            "-a",
            "user:pass@/:rw",
            "-a",
            "@/:ro",
        ],
    );
    let client = reqwest::blocking::Client::new();
    let auth = |b: reqwest::blocking::RequestBuilder| b.basic_auth("user", Some("pass"));
    let url = |p: &str| dufs.url(p);

    // Every method the metrics tell apart, with successful and failing outcomes.
    for method in [
        "GET", "HEAD", "PUT", "PATCH", "DELETE", "MOVE", "COPY", "PROPFIND", "OPTIONS", "MKCOL",
        "LOCK", "BREW",
    ] {
        let method = reqwest::Method::from_bytes(method.as_bytes())?;
        for path in ["/test.txt", "/new-dir", "/missing/none.txt"] {
            let mut req = auth(client.request(method.clone(), url(path)));
            if method == "MOVE" || method == "COPY" {
                req = req.header("Destination", url("/moved.txt"));
            }
            if method == "PATCH" {
                req = req.header("X-Update-Range", "append");
            }
            let _ = req.body("data").send();
        }
    }
    // A redirect class and a server-error class.
    let _ = client
        .get(url("/dir1"))
        .basic_auth("user", Some("pass"))
        .send();
    for i in 0..10 {
        std::fs::write(dufs.dir.path().join(format!("limit-{i}.txt")), "x")?;
    }
    // Uploads of every outcome.
    let _ = auth(client.put(url("/ok.txt"))).body("ok").send()?;
    // Basic credentials for user:pass.
    interrupted_put(
        &dufs,
        "/half.bin",
        100_000,
        &[1u8; 100],
        "Authorization: Basic dXNlcjpwYXNz\r\n",
    );
    let _ = client.put(url("/anon.txt")).body("x").send()?;
    // Walks, auth failures, draining excluded (needs a signal).
    let _ = client
        .get(url("/?q=limit-&json"))
        .basic_auth("user", Some("pass"))
        .send()?;
    let _ = client
        .get(url("/dir1/?zip"))
        .basic_auth("user", Some("pass"))
        .send()?
        .bytes()?;
    let _ = client
        .get(url("/"))
        .basic_auth("user", Some("bad"))
        .send()?;
    let _ = client.get(url("/?token=00")).send()?;

    let metrics = dufs.wait_for("the aborted upload", |m| m.get(&upload("aborted")) >= 1.0);
    let samples = metrics
        .text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .count();
    assert!(samples <= 70, "{samples} samples:\n{}", metrics.text);
    assert_eq!(samples, metrics.samples.len());
    // Labels never carry paths, ids or users.
    for series in metrics.samples.keys() {
        for forbidden in ["test.txt", ID, "user", "limit"] {
            assert!(!series.contains(forbidden), "{series}");
        }
    }
    assert!(metrics.has("dufs_http_requests_total{method=\"other\",class=\"4xx\"}"));
    Ok(())
}
