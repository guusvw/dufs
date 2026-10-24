//! Prometheus metrics: relaxed atomics plus a hand-written text exposition
//! (format 0.0.4). Every label is a bounded enum, so the number of series is
//! fixed no matter what clients send.

use hyper::{Method, StatusCode};
use std::fmt::Write;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};
use std::time::Duration;

pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// The only metrics instance. Counting is a few relaxed atomic adds, so it is
/// always on; the exposition is served only when `--metrics-bind` is set.
pub static METRICS: Metrics = Metrics::new();

const METHODS: [&str; 9] = [
    "GET", "HEAD", "PUT", "PATCH", "DELETE", "MOVE", "COPY", "PROPFIND", "other",
];
const CLASSES: [&str; 4] = ["2xx", "3xx", "4xx", "5xx"];
/// Upper bounds of the request duration buckets, in seconds.
const BUCKETS: [f64; 7] = [0.005, 0.025, 0.1, 0.5, 2.5, 10.0, 60.0];
const BUCKET_LABELS: [&str; 7] = ["0.005", "0.025", "0.1", "0.5", "2.5", "10", "60"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadResult {
    Ok,
    Failed,
    Aborted,
    Resumed,
    Refused,
}

const UPLOAD_RESULTS: [&str; 5] = ["ok", "failed", "aborted", "resumed", "refused"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkKind {
    Search,
    Zip,
}

impl WalkKind {
    pub fn as_str(self) -> &'static str {
        WALK_KINDS[self as usize]
    }
}

const WALK_KINDS: [&str; 2] = ["search", "zip"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkResult {
    Complete,
    Truncated,
    Cancelled,
}

impl WalkResult {
    pub fn as_str(self) -> &'static str {
        WALK_RESULTS[self as usize]
    }
}

const WALK_RESULTS: [&str; 3] = ["complete", "truncated", "cancelled"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthScheme {
    Basic,
    Digest,
    Token,
}

const AUTH_SCHEMES: [&str; 3] = ["basic", "digest", "token"];

pub struct Metrics {
    requests: [[AtomicU64; CLASSES.len()]; METHODS.len()],
    /// Requests per bucket (not cumulative); the last slot is `+Inf`.
    duration_buckets: [AtomicU64; BUCKETS.len() + 1],
    duration_sum_nanos: AtomicU64,
    in_flight: AtomicI64,
    upload_bytes: AtomicU64,
    uploads: [AtomicU64; UPLOAD_RESULTS.len()],
    uploads_active: AtomicI64,
    part_files: AtomicU64,
    part_bytes: AtomicU64,
    part_files_swept: AtomicU64,
    walks: [[AtomicU64; WALK_RESULTS.len()]; WALK_KINDS.len()],
    walks_active: AtomicI64,
    walks_waiting: AtomicI64,
    auth_failures: [AtomicU64; AUTH_SCHEMES.len()],
    draining: AtomicU64,
}

impl Metrics {
    const fn new() -> Self {
        Self {
            requests: [const { [const { AtomicU64::new(0) }; CLASSES.len()] }; METHODS.len()],
            duration_buckets: [const { AtomicU64::new(0) }; BUCKETS.len() + 1],
            duration_sum_nanos: AtomicU64::new(0),
            in_flight: AtomicI64::new(0),
            upload_bytes: AtomicU64::new(0),
            uploads: [const { AtomicU64::new(0) }; UPLOAD_RESULTS.len()],
            uploads_active: AtomicI64::new(0),
            part_files: AtomicU64::new(0),
            part_bytes: AtomicU64::new(0),
            part_files_swept: AtomicU64::new(0),
            walks: [const { [const { AtomicU64::new(0) }; WALK_RESULTS.len()] }; WALK_KINDS.len()],
            walks_active: AtomicI64::new(0),
            walks_waiting: AtomicI64::new(0),
            auth_failures: [const { AtomicU64::new(0) }; AUTH_SCHEMES.len()],
            draining: AtomicU64::new(0),
        }
    }

    /// Counts a request that is being handled until the guard is dropped.
    pub fn track_in_flight(&self) -> Gauge<'_> {
        Gauge::new(&self.in_flight)
    }

    pub fn observe_request(&self, method: &Method, status: StatusCode, elapsed: Duration) {
        let method = match method.as_str() {
            "GET" => 0,
            "HEAD" => 1,
            "PUT" => 2,
            "PATCH" => 3,
            "DELETE" => 4,
            "MOVE" => 5,
            "COPY" => 6,
            "PROPFIND" => 7,
            _ => 8,
        };
        let class = match status.as_u16() {
            200..=299 => 0,
            300..=399 => 1,
            400..=499 => 2,
            _ => 3,
        };
        self.requests[method][class].fetch_add(1, Relaxed);
        let secs = elapsed.as_secs_f64();
        let bucket = BUCKETS
            .iter()
            .position(|le| secs <= *le)
            .unwrap_or(BUCKETS.len());
        self.duration_buckets[bucket].fetch_add(1, Relaxed);
        let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        self.duration_sum_nanos.fetch_add(nanos, Relaxed);
    }

    pub fn add_upload_bytes(&self, bytes: u64) {
        self.upload_bytes.fetch_add(bytes, Relaxed);
    }

    pub fn count_upload(&self, result: UploadResult) {
        self.uploads[result as usize].fetch_add(1, Relaxed);
    }

    pub fn track_upload(&self) -> Gauge<'_> {
        Gauge::new(&self.uploads_active)
    }

    /// Records what the last part file sweep found left on disk.
    pub fn set_part_files(&self, files: u64, bytes: u64) {
        self.part_files.store(files, Relaxed);
        self.part_bytes.store(bytes, Relaxed);
    }

    pub fn add_part_files_swept(&self, files: u64) {
        self.part_files_swept.fetch_add(files, Relaxed);
    }

    pub fn count_walk(&self, kind: WalkKind, result: WalkResult) {
        self.walks[kind as usize][result as usize].fetch_add(1, Relaxed);
    }

    pub fn track_walk(&self) -> Gauge<'_> {
        Gauge::new(&self.walks_active)
    }

    pub fn track_walk_waiting(&self) -> Gauge<'_> {
        Gauge::new(&self.walks_waiting)
    }

    pub fn count_auth_failure(&self, scheme: AuthScheme) {
        self.auth_failures[scheme as usize].fetch_add(1, Relaxed);
    }

    pub fn set_draining(&self) {
        self.draining.store(1, Relaxed);
    }

    /// The exposition: one `# HELP`/`# TYPE` pair per metric, then its samples.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(4096);
        header(
            &mut out,
            "dufs_build_info",
            "gauge",
            "Build information; the value is always 1.",
        );
        let _ = writeln!(
            out,
            "dufs_build_info{{version=\"{}\",commit=\"{}\"}} 1",
            escape_label(env!("CARGO_PKG_VERSION")),
            escape_label(env!("DUFS_COMMIT")),
        );

        header(
            &mut out,
            "dufs_http_requests_total",
            "counter",
            "Requests handled, by method and status class (health checks excluded).",
        );
        for (m, row) in self.requests.iter().enumerate() {
            for (c, count) in row.iter().enumerate() {
                let count = count.load(Relaxed);
                if count > 0 {
                    let _ = writeln!(
                        out,
                        "dufs_http_requests_total{{method=\"{}\",class=\"{}\"}} {count}",
                        METHODS[m], CLASSES[c]
                    );
                }
            }
        }

        header(
            &mut out,
            "dufs_http_request_duration_seconds",
            "histogram",
            "Time from receiving a request to having its response ready.",
        );
        let mut cumulative = 0;
        for (i, bucket) in self.duration_buckets.iter().enumerate() {
            cumulative += bucket.load(Relaxed);
            let le = BUCKET_LABELS.get(i).copied().unwrap_or("+Inf");
            let _ = writeln!(
                out,
                "dufs_http_request_duration_seconds_bucket{{le=\"{le}\"}} {cumulative}"
            );
        }
        let _ = writeln!(
            out,
            "dufs_http_request_duration_seconds_sum {}",
            self.duration_sum_nanos.load(Relaxed) as f64 / 1e9
        );
        let _ = writeln!(out, "dufs_http_request_duration_seconds_count {cumulative}");

        gauge(
            &mut out,
            "dufs_http_requests_in_flight",
            "Requests being handled now.",
            self.in_flight.load(Relaxed),
        );
        counter(
            &mut out,
            "dufs_upload_bytes_total",
            "Bytes received in upload request bodies.",
            self.upload_bytes.load(Relaxed),
        );

        header(
            &mut out,
            "dufs_uploads_total",
            "counter",
            "Upload requests by result.",
        );
        for (i, count) in self.uploads.iter().enumerate() {
            let _ = writeln!(
                out,
                "dufs_uploads_total{{result=\"{}\"}} {}",
                UPLOAD_RESULTS[i],
                count.load(Relaxed)
            );
        }

        gauge(
            &mut out,
            "dufs_uploads_active",
            "Uploads receiving data now.",
            self.uploads_active.load(Relaxed),
        );
        gauge(
            &mut out,
            "dufs_part_files",
            "Upload temp files found by the last sweep.",
            self.part_files.load(Relaxed),
        );
        gauge(
            &mut out,
            "dufs_part_bytes",
            "Size of the upload temp files found by the last sweep.",
            self.part_bytes.load(Relaxed),
        );
        counter(
            &mut out,
            "dufs_part_files_swept_total",
            "Stale upload temp files removed by the sweep.",
            self.part_files_swept.load(Relaxed),
        );

        header(
            &mut out,
            "dufs_walks_total",
            "counter",
            "Directory walks (search, zip) by how they ended.",
        );
        for (k, row) in self.walks.iter().enumerate() {
            for (r, count) in row.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "dufs_walks_total{{kind=\"{}\",result=\"{}\"}} {}",
                    WALK_KINDS[k],
                    WALK_RESULTS[r],
                    count.load(Relaxed)
                );
            }
        }
        gauge(
            &mut out,
            "dufs_walks_active",
            "Directory walks running now.",
            self.walks_active.load(Relaxed),
        );
        gauge(
            &mut out,
            "dufs_walks_waiting",
            "Requests waiting for a free walk slot.",
            self.walks_waiting.load(Relaxed),
        );

        header(
            &mut out,
            "dufs_auth_failures_total",
            "counter",
            "Credentials that were presented and rejected, by scheme.",
        );
        for (i, count) in self.auth_failures.iter().enumerate() {
            let _ = writeln!(
                out,
                "dufs_auth_failures_total{{scheme=\"{}\"}} {}",
                AUTH_SCHEMES[i],
                count.load(Relaxed)
            );
        }

        gauge(
            &mut out,
            "dufs_draining",
            "1 once the server was asked to stop and is draining.",
            self.draining.load(Relaxed),
        );

        process::render(&mut out);
        out
    }
}

/// A gauge that goes up now and down again when the guard is dropped, also
/// when the future holding it is dropped mid-request.
pub struct Gauge<'a>(&'a AtomicI64);

impl<'a> Gauge<'a> {
    fn new(value: &'a AtomicI64) -> Self {
        value.fetch_add(1, Relaxed);
        Self(value)
    }
}

impl Drop for Gauge<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Relaxed);
    }
}

fn header(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
}

fn gauge(out: &mut String, name: &str, help: &str, value: impl std::fmt::Display) {
    header(out, name, "gauge", help);
    let _ = writeln!(out, "{name} {value}");
}

fn counter(out: &mut String, name: &str, help: &str, value: u64) {
    header(out, name, "counter", help);
    let _ = writeln!(out, "{name} {value}");
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Process gauges, read from `/proc/self` at scrape time. A value that cannot
/// be read is left out, and so is everything off Linux.
#[cfg(target_os = "linux")]
mod process {
    use super::gauge;
    use std::sync::LazyLock;

    /// `USER_HZ`, the unit of `starttime` in `/proc/<pid>/stat`. The kernel
    /// reports it as 100 on every architecture userspace sees.
    const CLOCK_TICKS_PER_SEC: u64 = 100;

    pub fn render(out: &mut String) {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        if let Some(threads) = status_value(&status, "Threads:") {
            gauge(
                out,
                "process_threads",
                "Number of OS threads in the process.",
                threads,
            );
        }
        // The directory handle used to list the descriptors is one of them.
        if let Ok(dir) = std::fs::read_dir("/proc/self/fd") {
            gauge(
                out,
                "process_open_fds",
                "Number of open file descriptors.",
                dir.count().saturating_sub(1),
            );
        }
        if let Some(kib) = status_value(&status, "VmRSS:") {
            gauge(
                out,
                "process_resident_memory_bytes",
                "Resident memory size in bytes.",
                kib * 1024,
            );
        }
        if let Some(start) = start_time() {
            gauge(
                out,
                "process_start_time_seconds",
                "Start time of the process since the Unix epoch in seconds.",
                start,
            );
        }
    }

    /// The number in the `/proc/self/status` line starting with `key`.
    fn status_value(status: &str, key: &str) -> Option<u64> {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    }

    fn start_time() -> Option<u64> {
        *START
    }

    static START: LazyLock<Option<u64>> = LazyLock::new(|| {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        // The command name may contain spaces, so count the fields after the
        // closing parenthesis; `starttime` is field 22, index 19 there.
        let rest = &stat[stat.rfind(')')? + 2..];
        let ticks: u64 = rest.split(' ').nth(19)?.parse().ok()?;
        let boot = std::fs::read_to_string("/proc/stat")
            .ok()?
            .lines()
            .find_map(|line| line.strip_prefix("btime "))?
            .trim()
            .parse::<u64>()
            .ok()?;
        Some(boot + ticks / CLOCK_TICKS_PER_SEC)
    });
}

#[cfg(not(target_os = "linux"))]
mod process {
    pub fn render(_out: &mut String) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_is_cumulative_and_consistent() {
        let metrics = Metrics::new();
        metrics.observe_request(&Method::GET, StatusCode::OK, Duration::from_millis(1));
        metrics.observe_request(&Method::GET, StatusCode::OK, Duration::from_millis(50));
        metrics.observe_request(&Method::GET, StatusCode::OK, Duration::from_secs(100));
        let text = metrics.render();
        assert!(text.contains("dufs_http_request_duration_seconds_bucket{le=\"0.005\"} 1\n"));
        assert!(text.contains("dufs_http_request_duration_seconds_bucket{le=\"0.1\"} 2\n"));
        assert!(text.contains("dufs_http_request_duration_seconds_bucket{le=\"60\"} 2\n"));
        assert!(text.contains("dufs_http_request_duration_seconds_bucket{le=\"+Inf\"} 3\n"));
        assert!(text.contains("dufs_http_request_duration_seconds_count 3\n"));
    }

    #[test]
    fn unknown_methods_share_one_label() {
        let metrics = Metrics::new();
        metrics.observe_request(
            &Method::from_bytes(b"BREW").unwrap(),
            StatusCode::IM_A_TEAPOT,
            Duration::ZERO,
        );
        metrics.observe_request(&Method::OPTIONS, StatusCode::NO_CONTENT, Duration::ZERO);
        let text = metrics.render();
        assert!(text.contains("dufs_http_requests_total{method=\"other\",class=\"4xx\"} 1\n"));
        assert!(text.contains("dufs_http_requests_total{method=\"other\",class=\"2xx\"} 1\n"));
    }

    #[test]
    fn gauge_guard_restores_the_value() {
        let metrics = Metrics::new();
        {
            let _a = metrics.track_in_flight();
            let _b = metrics.track_in_flight();
            assert!(metrics
                .render()
                .contains("dufs_http_requests_in_flight 2\n"));
        }
        assert!(metrics
            .render()
            .contains("dufs_http_requests_in_flight 0\n"));
    }

    #[test]
    fn label_values_are_escaped() {
        assert_eq!(escape_label("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }
}
