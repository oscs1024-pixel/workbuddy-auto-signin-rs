use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{json, Value};

use super::json::serialize_safe;

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
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("signin.log");

        Self {
            action: action.into(),
            config_warning,
            default_log,
        }
    }

    pub fn with_default_log(
        action: impl Into<String>,
        config_warning: Option<String>,
        default_log: PathBuf,
    ) -> Self {
        Self {
            action: action.into(),
            config_warning,
            default_log,
        }
    }

    pub fn emit(&self, mut out: Value) {
        if let (Some(warning), Some(object)) = (self.config_warning.as_ref(), out.as_object_mut()) {
            object.insert("config_warning".into(), json!(warning));
        }

        let payload = serialize_safe(&out);
        let is_error = out.get("result").and_then(Value::as_str) == Some("ERROR");

        // silent* 运行于无控制台场景，直接写日志；交互命令优先 stdout。
        if !self.action.starts_with("silent") {
            let stdout_ok = writeln!(io::stdout().lock(), "{payload}").is_ok();
            if stdout_ok && !is_error {
                return;
            }
        }

        let line = format!("[{}] {payload}\n", Local::now().format("%Y-%m-%d %H:%M:%S"));

        let requested = std::env::var_os("WORKBUDDY_SIGNIN_LOG")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_log.clone());

        // 自定义日志路径不可写时回退到默认路径，尽量保证计划任务的失败信息不会无声丢失。
        for path in unique_paths(requested, self.default_log.clone()) {
            if append(&path, line.as_bytes()).is_ok() {
                return;
            }
        }
    }
}

fn unique_paths(first: PathBuf, second: PathBuf) -> Vec<PathBuf> {
    if first == second {
        vec![first]
    } else {
        vec![first, second]
    }
}

fn append(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(bytes)
}
