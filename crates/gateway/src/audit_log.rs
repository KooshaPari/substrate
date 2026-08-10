//! Structured JSONL audit log for all gateway requests.
//!
//! Enable by setting `SUBSTRATE_AUDIT_LOG` to the desired log file path.
//! Each request produces one JSON line after the response is sent.
//! When the file exceeds 50 MB it is rotated to `<path>.1` and a fresh file
//! is opened.

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// One structured audit record per request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unix epoch in milliseconds.
    pub timestamp_ms: u64,
    /// Provider name (e.g. `"openai"`).
    pub provider: String,
    /// Model identifier (e.g. `"openai/gpt-4o"`).
    pub model: String,
    /// Opaque request identifier (UUID or caller-supplied).
    pub request_id: String,
    /// HTTP status code returned to the caller.
    pub status: u16,
    /// Wall-clock latency in milliseconds.
    pub latency_ms: u64,
    /// Input token count when known.
    pub input_tokens: Option<u32>,
    /// Output token count when known.
    pub output_tokens: Option<u32>,
    /// Error message when the request failed.
    pub error: Option<String>,
}

const MAX_BYTES: u64 = 50 * 1024 * 1024; // 50 MB

/// Thread-safe append-only JSONL log writer with 50 MB rotation.
#[derive(Clone)]
pub struct AuditLogger {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    writer: BufWriter<File>,
    path: PathBuf,
    bytes_written: u64,
}

impl AuditLogger {
    /// Open (or create) the log file at `path`.  Parent directories are created
    /// automatically.
    pub fn new(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let bytes_written = file.metadata().map(|m| m.len()).unwrap_or(0);
        let writer = BufWriter::new(file);
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                writer,
                path: path.to_owned(),
                bytes_written,
            })),
        })
    }

    /// Serialize `entry` as a single JSON line and flush.
    ///
    /// If the file has grown past 50 MB *before* this write, it is renamed to
    /// `<path>.1` and a fresh file is opened first.
    pub fn write(&self, entry: &AuditEntry) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().expect("audit log lock poisoned");
        // Rotate if needed.
        if inner.bytes_written >= MAX_BYTES {
            inner.writer.flush()?;
            let rotated = inner.path.with_extension("log.1");
            fs::rename(&inner.path, &rotated)?;
            let fresh = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&inner.path)?;
            inner.writer = BufWriter::new(fresh);
            inner.bytes_written = 0;
        }

        let mut line = serde_json::to_string(entry)?;
        line.push('\n');
        inner.bytes_written += line.len() as u64;
        inner.writer.write_all(line.as_bytes())?;
        inner.writer.flush()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    fn sample_entry(ts: u64) -> AuditEntry {
        AuditEntry {
            timestamp_ms: ts,
            provider: "openai".to_string(),
            model: "openai/gpt-4o".to_string(),
            request_id: "req-abc".to_string(),
            status: 200,
            latency_ms: 42,
            input_tokens: Some(10),
            output_tokens: Some(20),
            error: None,
        }
    }

    /// Write an entry and read it back; confirm round-trip fidelity.
    #[test]
    fn write_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let logger = AuditLogger::new(&path).unwrap();

        let entry = sample_entry(1_700_000_000_000);
        logger.write(&entry).unwrap();

        let file = File::open(&path).unwrap();
        let mut lines = std::io::BufReader::new(file).lines();
        let line = lines.next().unwrap().unwrap();
        let got: AuditEntry = serde_json::from_str(&line).unwrap();
        assert_eq!(got.timestamp_ms, 1_700_000_000_000);
        assert_eq!(got.provider, "openai");
        assert_eq!(got.status, 200);
    }

    /// Each serialized line must be valid JSON.
    #[test]
    fn json_validity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let logger = AuditLogger::new(&path).unwrap();

        for ts in [1u64, 2, 3] {
            logger.write(&sample_entry(ts)).unwrap();
        }

        let file = File::open(&path).unwrap();
        for line in std::io::BufReader::new(file).lines() {
            let line = line.unwrap();
            let v: serde_json::Value =
                serde_json::from_str(&line).expect("each line must be valid JSON");
            assert!(v.get("timestamp_ms").is_some());
        }
    }

    /// Timestamps must be written in ascending (non-decreasing) order.
    #[test]
    fn timestamp_ordering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let logger = AuditLogger::new(&path).unwrap();

        for ts in [100u64, 200, 300] {
            logger.write(&sample_entry(ts)).unwrap();
        }

        let file = File::open(&path).unwrap();
        let timestamps: Vec<u64> = std::io::BufReader::new(file)
            .lines()
            .map(|l| {
                let l = l.unwrap();
                let v: serde_json::Value = serde_json::from_str(&l).unwrap();
                v["timestamp_ms"].as_u64().unwrap()
            })
            .collect();
        assert_eq!(timestamps, vec![100, 200, 300]);
    }

    /// Rotation: force a tiny max size by writing many entries into a file that
    /// was pre-grown past the limit, then verify the rotated file exists and the
    /// fresh file is non-empty after the next write.
    #[test]
    fn rotation_trigger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");

        // Pre-create a file that is already past MAX_BYTES.
        {
            let mut f = File::create(&path).unwrap();
            // Write a dummy 51 MB blob.
            let chunk = vec![b'x'; 1024];
            for _ in 0..(51 * 1024) {
                f.write_all(&chunk).unwrap();
            }
            f.flush().unwrap();
        }
        assert!(path.metadata().unwrap().len() >= MAX_BYTES);

        let logger = AuditLogger::new(&path).unwrap();
        logger.write(&sample_entry(999)).unwrap();

        // Original file should have been rotated.
        let rotated = path.with_extension("log.1");
        assert!(rotated.exists(), "rotated file must exist");

        // Fresh log has exactly one entry.
        let file = File::open(&path).unwrap();
        let count = std::io::BufReader::new(file).lines().count();
        assert_eq!(count, 1);
    }

    /// When logger is None the caller does nothing — simulate with an Option.
    #[test]
    fn noop_when_logger_is_none() {
        let logger: Option<AuditLogger> = None;
        // Must not panic or do anything.
        if let Some(l) = &logger {
            l.write(&sample_entry(0)).unwrap();
        }
        // If we reach here, the no-op path works.
    }
}
