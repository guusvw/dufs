use std::{
    collections::HashMap,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::{Local, SecondsFormat};

use hyper::header::HeaderValue;
use serde_json::Value;

use crate::{auth::get_auth_user, logger, server::Request, utils::decode_uri};

pub const DEFAULT_LOG_FORMAT: &str =
    r#"$time_iso8601 $log_level - $remote_addr "$request" $status"#;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpLogger {
    elements: Vec<LogElement>,
    /// Whether the format was given by the user instead of being the default.
    custom: bool,
    /// One JSON object per line instead of the format.
    json: bool,
}

impl Default for HttpLogger {
    fn default() -> Self {
        let mut logger: Self = DEFAULT_LOG_FORMAT
            .parse()
            .expect("default log format is always valid");
        logger.custom = false;
        logger
    }
}

/// What is known about a request once its response is ready.
#[derive(Debug, Default)]
pub struct Access {
    pub remote_addr: Option<String>,
    pub status: u16,
    pub duration: Duration,
    /// Length of the response body, `None` if it is streamed without one.
    pub bytes_sent: Option<u64>,
    pub bytes_received: u64,
    pub error_code: Option<String>,
    pub upload_id: Option<String>,
    /// Kind of the io error that failed the request, if one did.
    pub io_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum LogElement {
    Variable(String),
    Header(String),
    Literal(String),
}

impl HttpLogger {
    /// The logger of `--log-json`: it needs the request method, uri and user.
    pub fn json() -> Self {
        let elements = ["request_method", "request_uri", "remote_user"]
            .map(|name| LogElement::Variable(name.to_string()))
            .to_vec();
        Self {
            elements,
            custom: false,
            json: true,
        }
    }

    /// Whether `--log-format` (or the config file) set the format.
    pub fn is_custom(&self) -> bool {
        self.custom
    }

    /// Make a value safe to put between the quotes of a log format. JSON
    /// escapes it on its own.
    fn escape(&self, value: &str) -> String {
        if self.json {
            value.to_string()
        } else {
            sanitize_log_value(value)
        }
    }

    pub fn data(&self, req: &Request) -> HashMap<String, String> {
        let mut data = HashMap::default();
        for element in self.elements.iter() {
            match element {
                LogElement::Variable(name) => match name.as_str() {
                    "request" | "request_method" | "request_uri" => {
                        let uri = redact_query(&req.uri().to_string());
                        let decoded_uri = decode_uri(&uri)
                            .map(|s| self.escape(&s))
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
                            value.to_str().ok().map(|v| self.escape(v))
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

    pub fn log(&self, data: &HashMap<String, String>, access: &Access, err: Option<String>) {
        if self.elements.is_empty() {
            return;
        }
        if self.json {
            return self.log_json(data, access, err);
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
        let status = access.status.to_string();
        let request_time = format!("{:.3}", access.duration.as_secs_f64());
        let bytes_sent = access.bytes_sent.map(|v| v.to_string());
        let bytes_received = access.bytes_received.to_string();

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
                        "remote_addr" => access.remote_addr.as_deref(),
                        "status" => Some(status.as_str()),
                        "request_time" => Some(request_time.as_str()),
                        "bytes_sent" => bytes_sent.as_deref(),
                        "bytes_received" => Some(bytes_received.as_str()),
                        "error_code" => access.error_code.as_deref(),
                        "upload_id" => access.upload_id.as_deref(),
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
            Some(err) => logger::access(log::Level::Error, &format!("{output} {err}")),
            None => logger::access(log::Level::Info, &output),
        }
    }

    fn log_json(&self, data: &HashMap<String, String>, access: &Access, err: Option<String>) {
        let text = |key: &str| data.get(key).cloned().unwrap_or_default();
        let optional = |value: &Option<String>| value.clone().map_or(Value::Null, Value::from);
        let (method, uri) = (text("request_method"), text("request_uri"));
        let msg = format!("{method} {uri} {}", access.status);
        let mut fields = vec![
            ("remote", optional(&access.remote_addr)),
            ("user", optional(&data.get("remote_user").cloned())),
            ("method", Value::from(method)),
            ("uri", Value::from(uri)),
            ("status", Value::from(access.status)),
            (
                "duration_ms",
                Value::from((access.duration.as_secs_f64() * 1e6).round() / 1e3),
            ),
            (
                "bytes_sent",
                access.bytes_sent.map_or(Value::Null, Value::from),
            ),
            ("bytes_received", Value::from(access.bytes_received)),
            ("error_code", optional(&access.error_code)),
            ("upload_id", optional(&access.upload_id)),
        ];
        if let Some(kind) = &access.io_kind {
            fields.push(("io_kind", Value::from(kind.as_str())));
        }
        let level = if err.is_some() {
            log::Level::Error
        } else {
            log::Level::Info
        };
        if let Some(err) = err {
            fields.push(("error", Value::from(err)));
        }
        logger::access(level, &logger::json_line(level, &msg, "", fields));
    }
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
        Ok(Self {
            elements,
            custom: true,
            json: false,
        })
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
