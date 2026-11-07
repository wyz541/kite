use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

const TRACE_PATH_ENV: &str = "KITE_PERF_TRACE_PATH";

static START: OnceLock<Instant> = OnceLock::new();
static EMITTED: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();

#[derive(Debug, Serialize)]
struct TraceEvent<'a> {
    schema_version: u32,
    platform: &'static str,
    version: &'static str,
    pid: u32,
    event: &'a str,
    timestamp_unix_ms: u64,
    elapsed_ms: u64,
}

pub fn start() {
    let started = *START.get_or_init(Instant::now);
    emit_at(started, "process_entry");
}

pub fn mark(event: &str) {
    emit_at(*START.get_or_init(Instant::now), event);
}

pub fn mark_once(event: &str) {
    let emitted = EMITTED.get_or_init(|| Mutex::new(BTreeSet::new()));
    let should_emit = emitted
        .lock()
        .map(|mut values| values.insert(event.to_string()))
        .unwrap_or(false);
    if should_emit {
        mark(event);
    }
}

fn emit_at(started: Instant, event: &str) {
    let Some(path) = trace_path() else {
        return;
    };
    if let Err(error) = write_event(&path, started, Instant::now(), event) {
        eprintln!("Kite performance trace unavailable: {error}");
    }
}

fn trace_path() -> Option<PathBuf> {
    std::env::var_os(TRACE_PATH_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn write_event(path: &Path, started: Instant, now: Instant, event: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create trace directory: {error}"))?;
    }
    let trace_event = TraceEvent {
        schema_version: 1,
        platform: std::env::consts::OS,
        version: env!("CARGO_PKG_VERSION"),
        pid: std::process::id(),
        event,
        timestamp_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        elapsed_ms: now.saturating_duration_since(started).as_millis() as u64,
    };
    let mut encoded = serde_json::to_vec(&trace_event)
        .map_err(|error| format!("cannot serialize trace event: {error}"))?;
    encoded.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot open trace file: {error}"))?;
    file.write_all(&encoded)
        .map_err(|error| format!("cannot write trace event: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_valid_json_line_with_elapsed_time() {
        let path = std::env::temp_dir().join(format!(
            "kite-perf-trace-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let started = Instant::now();
        write_event(&path, started, started, "first_content").expect("write trace event");

        let line = std::fs::read_to_string(&path).expect("read trace event");
        let value: serde_json::Value =
            serde_json::from_str(line.trim()).expect("parse trace event");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["event"], "first_content");
        assert_eq!(value["elapsed_ms"], 0);
        std::fs::remove_file(path).expect("remove trace event");
    }
}
