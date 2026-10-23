mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};

/// Create `dirs` directories holding `files` empty files each.
fn big_tree(root: &Path, dirs: usize, files: usize) {
    for d in 0..dirs {
        let dir = root.join(format!("big/d{d}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..files {
            std::fs::File::create(dir.join(format!("f{f}.dat"))).unwrap();
        }
    }
}

fn send_search(server: &TestServer, query: &str) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", server.port())).unwrap();
    write!(
        stream,
        "GET /big/?q={query} HTTP/1.1\r\nHost: localhost\r\n\r\n"
    )
    .unwrap();
    stream
}

/// CPU time (user + system, in clock ticks) used by a process so far.
#[cfg(target_os = "linux")]
fn cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    // The command name may contain spaces, so count fields after the `)`.
    let rest = &stat[stat.rfind(')').unwrap() + 2..];
    let fields: Vec<&str> = rest.split(' ').collect();
    // utime and stime are fields 14 and 15 of the full line, 12 and 13 here.
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

#[rstest]
fn search_limit_truncates(
    #[with(&["--allow-search", "--search-limit", "5"])] server: TestServer,
) -> Result<(), Error> {
    for i in 0..20 {
        std::fs::write(server.path().join(format!("limit-{i:02}.txt")), "x")?;
    }
    let resp = reqwest::blocking::get(format!("{}?q=limit-&json", server.url()))?;
    assert_eq!(resp.status(), 200);
    let value = serde_json::from_str::<serde_json::Value>(&resp.text()?)?;
    assert_eq!(value["paths"].as_array().unwrap().len(), 5);
    assert_eq!(value["truncated"], true);
    Ok(())
}

#[rstest]
fn search_within_limit_is_not_truncated(
    #[with(&["--allow-search", "--search-limit", "5"])] server: TestServer,
) -> Result<(), Error> {
    for i in 0..5 {
        std::fs::write(server.path().join(format!("limit-{i:02}.txt")), "x")?;
    }
    let resp = reqwest::blocking::get(format!("{}?q=limit-&json", server.url()))?;
    let value = serde_json::from_str::<serde_json::Value>(&resp.text()?)?;
    assert_eq!(value["paths"].as_array().unwrap().len(), 5);
    assert!(value.get("truncated").is_none());
    Ok(())
}

#[rstest]
fn search_limit_default(#[with(&["--allow-search"])] server: TestServer) -> Result<(), Error> {
    let dir = server.path().join("many");
    std::fs::create_dir(&dir)?;
    for i in 0..1005 {
        std::fs::File::create(dir.join(format!("m{i}.txt")))?;
    }
    let resp = reqwest::blocking::get(format!("{}many/?q=m&json", server.url()))?;
    let value = serde_json::from_str::<serde_json::Value>(&resp.text()?)?;
    assert_eq!(value["paths"].as_array().unwrap().len(), 1000);
    assert_eq!(value["truncated"], true);
    Ok(())
}

#[cfg(target_os = "linux")]
#[rstest]
fn search_stops_when_client_disconnects(
    #[with(&["--allow-search"])] server: TestServer,
) -> Result<(), Error> {
    big_tree(server.path(), 200, 500);
    let pid = server.pid();

    // Warm the dentry cache and measure what a complete walk costs.
    let resp = reqwest::blocking::get(format!("{}big/?q=zzzz&json", server.url()))?;
    assert_eq!(resp.status(), 200);
    let before = cpu_ticks(pid);
    let resp = reqwest::blocking::get(format!("{}big/?q=zzzz&json", server.url()))?;
    assert_eq!(resp.status(), 200);
    let full = cpu_ticks(pid) - before;
    assert!(full >= 20, "walk too cheap to measure: {full} ticks");

    let before = cpu_ticks(pid);
    let stream = send_search(&server, "zzzz");
    std::thread::sleep(Duration::from_millis(100));
    drop(stream);
    std::thread::sleep(Duration::from_secs(2));
    let aborted = cpu_ticks(pid) - before;
    assert!(
        aborted * 2 < full,
        "aborted search used {aborted} ticks, a full walk {full}"
    );
    Ok(())
}

#[rstest]
fn health_stays_responsive_during_searches(
    #[with(&["--allow-search"])] server: TestServer,
) -> Result<(), Error> {
    big_tree(server.path(), 100, 500);
    let streams: Vec<_> = (0..32).map(|_| send_search(&server, "zzzz")).collect();
    std::thread::sleep(Duration::from_millis(200));

    let client = reqwest::blocking::Client::new();
    let url = format!("{}__dufs__/health", server.url());
    let mut samples = vec![];
    for _ in 0..11 {
        let start = Instant::now();
        let resp = client.get(&url).send()?;
        assert_eq!(resp.status(), 200);
        samples.push(start.elapsed());
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(streams);
    // The median keeps the test steady on a loaded machine; blocked runtime
    // workers used to delay health checks by seconds.
    samples.sort();
    assert!(
        samples[samples.len() / 2] < Duration::from_millis(100),
        "health took {samples:?}"
    );
    Ok(())
}

#[rstest]
fn zip_first_byte_does_not_wait_for_the_walk(
    #[with(&["-A"])] server: TestServer,
) -> Result<(), Error> {
    big_tree(server.path(), 200, 1000);
    let start = Instant::now();
    let mut resp = reqwest::blocking::get(format!("{}big/?zip", server.url()))?;
    assert_eq!(resp.status(), 200);
    let mut first = [0u8; 4];
    resp.read_exact(&mut first)?;
    assert_eq!(&first, b"PK\x03\x04");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "first zip byte after {:?}",
        start.elapsed()
    );
    Ok(())
}
