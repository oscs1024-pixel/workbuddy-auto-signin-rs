use std::env;
use std::path::PathBuf;

use crate::config::{AUTH_BASENAME, CLI_AUTH_BASENAME};

#[derive(Debug, Clone)]
pub struct AuthDiscovery {
    pub found: Option<PathBuf>,
    pub looked_in: Vec<PathBuf>,
}

fn push_segments(mut base: PathBuf, segments: &[&str]) -> PathBuf {
    for segment in segments {
        base.push(segment);
    }
    base
}

pub fn discover_from_candidates(
    override_path: Option<PathBuf>,
    candidates: Vec<PathBuf>,
) -> AuthDiscovery {
    if let Some(path) = override_path {
        return AuthDiscovery {
            found: path.exists().then_some(path.clone()),
            looked_in: vec![path],
        };
    }
    let found = candidates.iter().find(|path| path.exists()).cloned();
    AuthDiscovery {
        found,
        looked_in: candidates,
    }
}

pub fn discover_auth_file() -> AuthDiscovery {
    let override_path = env::var_os("WORKBUDDY_AUTH_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let local = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Local"));
    let xdg = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("share"));

    let candidates = vec![
        push_segments(local, AUTH_BASENAME),
        push_segments(
            home.join("Library").join("Application Support"),
            AUTH_BASENAME,
        ),
        push_segments(xdg, CLI_AUTH_BASENAME),
        push_segments(home.join(".config"), AUTH_BASENAME),
        home.join(".workbuddy")
            .join("auth")
            .join("workbuddy-desktop.info"),
    ];

    discover_from_candidates(override_path, candidates)
}
