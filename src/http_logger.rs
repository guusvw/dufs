use std::{
    collections::HashMap,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Local, SecondsFormat};

use hyper::header::HeaderValue;

use crate::{auth::get_auth_user, server::Request, utils::decode_uri};

pub const DEFAULT_LOG_FORMAT: &str =
    r#"$time_iso8601 $log_level - $remote_addr "$request" $status"#;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpLogger {
    elements: Vec<LogElement>,
}

impl Default for HttpLogger {
    fn default() -> Self {
        DEFAULT_LOG_FORMAT
            .parse()
            .expect("default log format is always valid")
    }
}

#[derive(Debug, Clone, PartialEq)]
enum LogElement {
    Variable(String),
    Header(String),
    Literal(String),
}

impl HttpLogger {
    pub fn data(&self, req: &Request) -> HashMap<String, String> {
        let mut data = HashMap::default();
        for element in self.elements.iter() {
            match element {
                LogElement::Variable(name) => match name.as_str() {
                    "request" | "request_method" | "request_uri" => {
                        let uri = redact_query(&req.uri().to_string());
                        let decoded_uri = decode_uri(&uri)
                            .map(|s| sanitize_log_value(&s))
                            .unwrap_or_else(|| uri.clone());
                        data.entry("request".to_string())
                            .or_insert_with(|| format!("{} {decoded_uri}", req.method()));
                        data.entry("request_method".to_string())
                            .or_insert_with(|| req.method().to_string());
                        data.entry("request_uri".to_string())
                            .or_insert_with(|| decoded_uri);
                    }
                    "remote_user" => {
                        if let Some(user) =
                            req.headers().get("authorization").and_then(get_auth_user)
                        {
                            data.insert(name.to_string(), user);
                        }
                    }
                    _ => {}
                },
                LogElement::Header(name) => {
                    if let Some(value) = req.headers().get(name) {
                        let logged = if is_credential_header(name) {
                            redact_credential(name, value)
                        } else {
                            value.to_str().ok().map(sanitize_log_value)
                        };
                        if let Some(logged) = logged {
                            data.insert(name.to_string(), logged);
                        }
                    }
                }
                LogElement::Literal(_) => {}
            }
        }
        data
    }

    pub fn log(&self, data: &HashMap<String, String>, err: Option<String>) {
        if self.elements.is_empty() {
            return;
        }
        let is_error = err.is_some();
        let now = Local::now();
        let time_local = now.to_rfc3339_opts(SecondsFormat::Secs, false);
        let time_iso8601 = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let msec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| format!("{:.3}", d.as_secs_f64()))
            .unwrap_or_default();
        let log_level = if is_error { "ERROR" } else { "INFO" };

        let mut output = String::new();
        for element in self.elements.iter() {
            match element {
                LogElement::Literal(value) => output.push_str(value.as_str()),
                LogElement::Variable(name) => {
                    let resolved = match name.as_str() {
                        "time_local" => Some(time_local.as_str()),
                        "time_iso8601" => Some(time_iso8601.as_str()),
                        "msec" => Some(msec.as_str()),
                        "log_level" => Some(log_level),
                        _ => None,
                    };
                    let val = resolved
                        .or_else(|| data.get(name.as_str()).map(|v| v.as_str()))
                        .unwrap_or("-");
                    output.push_str(val);
                }
                LogElement::Header(name) => {
                    output.push_str(data.get(name.as_str()).map(|v| v.as_str()).unwrap_or("-"))
                }
            }
        }
        match err {
            Some(err) => emit_http_access(&format!("{output} {err}"), true),
            None => emit_http_access(&output, false),
        }
    }
}

/// Emit via the `log` crate with target `http_access` so the system logger
/// prints the line verbatim (no extra timestamp/level prefix).
fn emit_http_access(msg: &str, is_error: bool) {
    let level = if is_error {
        log::Level::Error
    } else {
        log::Level::Info
    };
    log::logger().log(
        &log::Record::builder()
            .args(format_args!("{}", msg))
            .level(level)
            .target("http_access")
            .build(),
    );
}

impl FromStr for HttpLogger {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut elements = vec![];
        let mut is_var = false;
        let mut cache = String::new();
        for c in format!("{s} ").chars() {
            if c == '$' {
                if !cache.is_empty() {
                    elements.push(LogElement::Literal(cache.to_string()));
                }
                cache.clear();
                is_var = true;
            } else if is_var && !(c.is_alphanumeric() || c == '_') {
                if let Some(value) = cache.strip_prefix("$http_") {
                    elements.push(LogElement::Header(value.replace('_', "-").to_string()));
                } else if let Some(value) = cache.strip_prefix('$') {
                    elements.push(LogElement::Variable(value.to_string()));
                }
                cache.clear();
                is_var = false;
            }
            cache.push(c);
        }
        let cache = cache.trim();
        if !cache.is_empty() {
            elements.push(LogElement::Literal(cache.to_string()));
        }
        Ok(Self { elements })
    }
}

fn sanitize_log_value(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            c if c.is_control() => format!("\\x{:02x}", c as u32).chars().collect::<Vec<_>>(),
            c => vec![c],
        })
        .collect()
}

const REDACTED: &str = "***";

/// Replace the value of every `token` query parameter so share tokens never
/// reach the access log. Works on the raw (still percent-encoded) URI so an
/// encoded `&` inside a value cannot leak the tail of the token.
fn redact_query(uri: &str) -> String {
    let Some((path, rest)) = uri.split_once('?') else {
        return uri.to_string();
    };
    let (query, fragment) = match rest.split_once('#') {
        Some((query, fragment)) => (query, Some(fragment)),
        None => (rest, None),
    };
    let query = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if is_token_key(key) => format!("{key}={REDACTED}"),
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&");
    match fragment {
        Some(fragment) => format!("{path}?{query}#{fragment}"),
        None => format!("{path}?{query}"),
    }
}

/// Query keys are matched the way the server parses them (form-urlencoded).
fn is_token_key(raw_key: &str) -> bool {
    form_urlencoded::parse(raw_key.as_bytes())
        .next()
        .is_some_and(|(key, _)| key == "token")
}

fn is_credential_header(name: &str) -> bool {
    ["authorization", "proxy-authorization", "cookie"]
        .iter()
        .any(|h| name.eq_ignore_ascii_case(h))
}

/// Keep only what identifies the kind of credential: the auth scheme (plus the
/// claimed username for Digest) or the cookie names. Secrets become `***`.
fn redact_credential(name: &str, value: &HeaderValue) -> Option<String> {
    let text = value.to_str().ok()?;
    if name.eq_ignore_ascii_case("cookie") {
        let cookies = text
            .split(';')
            .map(|cookie| {
                let cookie_name = cookie.split_once('=').map_or(cookie, |(n, _)| n).trim();
                format!("{}={REDACTED}", sanitize_log_value(cookie_name))
            })
            .collect::<Vec<_>>();
        return Some(cookies.join("; "));
    }
    let scheme = text.split_whitespace().next()?;
    if scheme.eq_ignore_ascii_case("digest") {
        if let Some(user) = get_auth_user(value) {
            return Some(format!(
                "{} username=\"{}\" …{REDACTED}",
                sanitize_log_value(scheme),
                sanitize_log_value(&user)
            ));
        }
    }
    if text.trim() == scheme {
        return Some(REDACTED.to_string());
    }
    Some(format!("{} {REDACTED}", sanitize_log_value(scheme)))
}
