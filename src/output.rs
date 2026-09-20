use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct Reporter {
    action: String,
    config_warning: Option<String>,
    default_log: PathBuf,
}

impl Reporter {
    pub fn new(action: impl Into<String>, config_warning: Option<String>) -> Self {
        let default_log = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("signin.log");
        Self { action: action.into(), config_warning, default_log }
    }

    #[cfg(test)]
    pub fn with_default_log(action: impl Into<String>, config_warning: Option<String>, default_log: PathBuf) -> Self {
        Self { action: action.into(), config_warning, default_log }
    }

    pub fn emit(&self, mut out: Value) {
        if let (Some(warning), Some(object)) = (self.config_warning.as_ref(), out.as_object_mut()) {
            object.insert("config_warning".into(), json!(warning));
        }
        let payload = serde_json::to_string(&out).unwrap_or_else(|_| format!("{out:?}"));
        let is_error = out.get("result").and_then(Value::as_str) == Some("ERROR");

        if !self.action.starts_with("silent") {
            let stdout_ok = writeln!(io::stdout().lock(), "{payload}").is_ok();
            if stdout_ok && !is_error {
                return;
            }
        }

        let line = format!("[{}] {payload}\n", Local::now().format("%Y-%m-%d %H:%M:%S"));
        let requested = std::env::var_os("WORKBUDDY_SIGNIN_LOG")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_log.clone());

        for path in unique_paths(requested, self.default_log.clone()) {
            if append(&path, line.as_bytes()).is_ok() {
                return;
            }
        }
    }
}

fn unique_paths(first: PathBuf, second: PathBuf) -> Vec<PathBuf> {
    if first == second { vec![first] } else { vec![first, second] }
}

fn append(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn silent_output_is_appended_to_log() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("signin.log");
        let reporter = Reporter::with_default_log("silent", Some("warn".into()), log.clone());
        reporter.emit(json!({"result":"ALREADY","report":"ok"}));
        let text = std::fs::read_to_string(log).unwrap();
        assert!(text.contains("ALREADY"));
        assert!(text.contains("config_warning"));
    }
}
