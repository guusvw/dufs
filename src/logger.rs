use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use log::{Level, LevelFilter, Metadata, Record};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Target of access lines. Their text is final, as the access log format wants.
pub const TARGET_ACCESS: &str = "http_access";
/// Target of application events, final like the access lines.
const TARGET_EVENT: &str = "dufs_event";

static JSON: AtomicBool = AtomicBool::new(false);

struct SimpleLogger {
    file: Option<Mutex<File>>,
    json: bool,
}

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut text = record.args().to_string();
            // Free text from `info!` and friends becomes a JSON line too.
            if self.json && !matches!(record.target(), TARGET_ACCESS | TARGET_EVENT) {
                text = json_line(record.level(), &text, "", Vec::new());
            }
            match &self.file {
                Some(file) => {
                    if let Ok(mut file) = file.lock() {
                        let _ = writeln!(file, "{text}");
                    }
                }
                None => {
                    if record.level() < Level::Info {
                        eprintln!("{text}");
                    } else {
                        println!("{text}");
                    }
                }
            }
        }
    }

    fn flush(&self) {}
}

pub fn init(log_file: Option<PathBuf>, json: bool) -> Result<()> {
    let file = match log_file {
        None => None,
        Some(log_file) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
                .with_context(|| {
                    format!("Failed to open the log file at '{}'", log_file.display())
                })?;
            Some(Mutex::new(file))
        }
    };
    let logger = SimpleLogger { file, json };
    log::set_boxed_logger(Box::new(logger))
        .map(|_| log::set_max_level(LevelFilter::Info))
        .with_context(|| "Failed to init logger")?;
    JSON.store(json, Ordering::Relaxed);
    Ok(())
}

pub fn is_json() -> bool {
    JSON.load(Ordering::Relaxed)
}

/// One JSON object per line: `ts`, `level`, `msg`, then `event` (if any) and
/// the remaining fields in the order given.
pub fn json_line(level: Level, msg: &str, event: &str, fields: Vec<(&str, Value)>) -> String {
    let mut out = format!(
        "{{\"ts\":\"{}\",\"level\":\"{}\",\"msg\":{}",
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        level_name(level),
        Value::from(msg),
    );
    let event = (!event.is_empty()).then(|| ("event", Value::from(event)));
    for (key, value) in event.into_iter().chain(fields) {
        out.push_str(&format!(",{}:{value}", Value::from(key)));
    }
    out.push('}');
    out
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}

/// Emit an access line that is already formatted.
pub fn access(level: Level, line: &str) {
    emit(level, TARGET_ACCESS, line);
}

/// Log an application event. `msg` is the line in the plain format; with
/// `--log-json` the line is an object with `event` and `fields` instead.
pub fn event(level: Level, name: &str, msg: &str, fields: Vec<(&str, Value)>) {
    if is_json() {
        emit(level, TARGET_EVENT, &json_line(level, msg, name, fields));
    } else {
        emit(level, TARGET_EVENT, msg);
    }
}

fn emit(level: Level, target: &str, line: &str) {
    log::logger().log(
        &Record::builder()
            .args(format_args!("{line}"))
            .level(level)
            .target(target)
            .build(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_line_keeps_order_and_escapes() {
        let line = json_line(
            Level::Warn,
            "a \"b\"\n",
            "walk",
            vec![("kind", Value::from("search")), ("entries", Value::from(3))],
        );
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["level"], "WARN");
        assert_eq!(value["msg"], "a \"b\"\n");
        assert_eq!(value["event"], "walk");
        assert_eq!(value["entries"], 3);
        assert!(line.starts_with("{\"ts\":\""));
        assert!(line.find("\"msg\"").unwrap() < line.find("\"event\"").unwrap());
    }
}
