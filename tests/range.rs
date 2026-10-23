mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rstest::rstest;

#[rstest]
fn get_file_range(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-6"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes 0-6/18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "7");
    assert_eq!(resp.text()?, "This is");
    Ok(())
}

#[rstest]
fn get_file_range_beyond(server: TestServer) -> Result<(), Error> {
    // last-pos beyond EOF is clamped to size - 1 (RFC 9110 §14.1.2)
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=12-20"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 12-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "6");
    assert_eq!(resp.text()?, "x.html");
    Ok(())
}

#[rstest]
fn get_file_range_invalid(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=20-"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    Ok(())
}

#[rstest]
fn get_file_range_last_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    // RFC 9110 §14.1.2: last-pos >= size must be clamped to size - 1, not 416
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-19"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 0-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "18");
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn get_file_range_last_pos_at_eof(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-18"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 0-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "18");
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn get_file_range_last_pos_beyond_eof_middle(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=6-99"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 6-17/18"
    );
    assert_eq!(resp.headers().get("content-length").unwrap(), "12");
    assert_eq!(resp.text()?, "s index.html");
    Ok(())
}

#[rstest]
fn head_file_range_last_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-19"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 0-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "18");
    Ok(())
}

#[rstest]
fn get_file_multipart_range_last_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-5, 9-19"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()?
        .to_string();
    assert!(content_type.starts_with("multipart/byteranges; boundary="));

    let boundary = content_type.split_once('=').unwrap().1.trim_ascii_start();
    assert!(!boundary.is_empty());

    let body = resp.text()?;
    let parts = parse_multipart_body(&body, boundary);
    assert_eq!(parts.len(), 2);

    let (headers, body) = &parts[0];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 0-5/18");
    assert_eq!(*body, "This i");

    let (headers, body) = &parts[1];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 9-17/18");
    assert_eq!(*body, "ndex.html");

    Ok(())
}

#[rstest]
fn get_file_multipart_range_first_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    // Mixed satisfiable + unsatisfiable (first-pos >= size) is still 416
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-19, 20-"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    Ok(())
}

fn parse_multipart_body<'a>(body: &'a str, boundary: &str) -> Vec<(HeaderMap, &'a str)> {
    body.split(&format!("--{boundary}"))
        .filter(|part| !part.is_empty() && *part != "--\r\n")
        .map(|part| {
            let (head, body) = part.trim_ascii().split_once("\r\n\r\n").unwrap();
            let headers = head
                .split("\r\n")
                .fold(HeaderMap::new(), |mut headers, header| {
                    let (key, value) = header.split_once(":").unwrap();
                    let key = HeaderName::from_bytes(key.as_bytes()).unwrap();
                    let value = HeaderValue::from_str(value.trim_ascii_start()).unwrap();
                    headers.insert(key, value);
                    headers
                });
            (headers, body)
        })
        .collect()
}

#[rstest]
fn get_file_multipart_range(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-5, 9-17"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()?
        .to_string();
    assert!(content_type.starts_with("multipart/byteranges; boundary="));

    let boundary = content_type.split_once('=').unwrap().1.trim_ascii_start();
    assert!(!boundary.is_empty());

    let body = resp.text()?;
    let parts = parse_multipart_body(&body, boundary);
    assert_eq!(parts.len(), 2);

    let (headers, body) = &parts[0];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 0-5/18");
    assert_eq!(*body, "This i");

    let (headers, body) = &parts[1];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 9-17/18");
    assert_eq!(*body, "ndex.html");

    Ok(())
}

#[rstest]
fn get_file_range_suffix_zero(server: TestServer) -> Result<(), Error> {
    // a zero-length suffix range is unsatisfiable (and used to underflow)
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=-0"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    Ok(())
}

#[rstest]
fn get_file_multipart_range_too_many(server: TestServer) -> Result<(), Error> {
    // more than 32 ranges would let a client amplify a small request
    let ranges = (0..18)
        .map(|end| format!("0-{end}"))
        .chain((0..16).map(|end| format!("1-{end}")))
        .collect::<Vec<_>>()
        .join(",");
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", format!("bytes={ranges}"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    Ok(())
}

#[rstest]
fn head_file_multipart_range(server: TestServer) -> Result<(), Error> {
    // HEAD must report the same headers as GET without transferring a body
    let get = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-5, 9-17"))
        .send()?;
    assert_eq!(get.status(), 206);
    let content_type = get
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()?
        .to_string();
    assert!(content_type.starts_with("multipart/byteranges; boundary="));
    let body_len = get.text()?.len();

    let resp = fetch!(b"HEAD", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-5, 9-17"))
        .send()?;
    assert_eq!(resp.status(), 206);
    let head_content_type = resp.headers().get("content-type").unwrap().to_str()?;
    assert!(head_content_type.starts_with("multipart/byteranges; boundary="));
    assert_eq!(
        resp.headers().get("content-length").unwrap(),
        &body_len.to_string()
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn get_file_multipart_range_large(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    // a range larger than the read buffer must be streamed back intact
    let data: String = (0..200_000u32)
        .map(|i| (b'a' + (i % 26) as u8) as char)
        .collect();
    let url = format!("{}big.txt", server.url());
    let resp = fetch!(b"PUT", &url).body(data.clone()).send()?;
    assert_eq!(resp.status(), 201);

    let resp = fetch!(b"GET", &url)
        .header(
            "range",
            HeaderValue::from_static("bytes=0-99999, 150000-199999"),
        )
        .send()?;
    assert_eq!(resp.status(), 206);

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()?
        .to_string();
    let boundary = content_type.split_once('=').unwrap().1.trim_ascii_start();
    let body = resp.text()?;
    let parts = parse_multipart_body(&body, boundary);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].1, &data[0..100_000]);
    assert_eq!(parts[1].1, &data[150_000..200_000]);
    Ok(())
}

#[rstest]
fn get_file_multipart_range_invalid(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-6, 20-30"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "0");
    Ok(())
}

#[rstest]
fn get_file_range_reversed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=10-1"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    Ok(())
}

#[rstest]
fn get_file_multipart_range_reversed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=10-1,20-2"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    Ok(())
}

#[rstest]
fn get_file_overlapping_ranges_covering_file(server: TestServer) -> Result<(), Error> {
    // 0-,1-,2- merge into one range covering the whole file: plain 200
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-,1-,2-"))
        .send()?;
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().get("content-range").is_none());
    assert_eq!(resp.headers().get("content-length").unwrap(), "18");
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn get_file_adjacent_ranges_covering_file(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=9-17, 0-8"))
        .send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn get_file_overlapping_ranges_merged(server: TestServer) -> Result<(), Error> {
    // 0-5 and 3-8 overlap -> one 206 part, not multipart
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-5, 3-8"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes 0-8/18");
    assert_eq!(resp.headers().get("content-length").unwrap(), "9");
    assert_eq!(resp.text()?, "This is i");
    Ok(())
}

#[rstest]
fn get_file_overlapping_ranges_keep_disjoint(server: TestServer) -> Result<(), Error> {
    // 0-3 and 2-5 merge to 0-5; 10-12 stays separate
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=10-12, 0-3, 2-5"))
        .send()?;
    assert_eq!(resp.status(), 206);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()?
        .to_string();
    let boundary = content_type.split_once('=').unwrap().1.trim_ascii_start();
    let body = resp.text()?;
    let parts = parse_multipart_body(&body, boundary);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].0.get("content-range").unwrap(), "bytes 0-5/18");
    assert_eq!(parts[0].1, "This i");
    assert_eq!(parts[1].0.get("content-range").unwrap(), "bytes 10-12/18");
    assert_eq!(parts[1].1, "dex");
    Ok(())
}

#[rstest]
fn get_large_file_range_streams_fully(server: TestServer) -> Result<(), Error> {
    // spans several 64 KiB chunks of the range stream
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(server.path().join("big.bin"), &data)?;
    let resp = fetch!(b"GET", format!("{}big.bin", server.url()))
        .header("range", HeaderValue::from_static("bytes=1000-250999"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.bytes()?.as_ref(), &data[1000..251000]);
    Ok(())
}
