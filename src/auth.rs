use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde::Deserialize;
use thiserror::Error;
use tokio::time::sleep;
use url::Url;

pub const DEFAULT_ENDPOINT: &str = "https://copilot.tencent.com";
const AUTH_BASENAME: &[&str] = &["CodeBuddyExtension", "Data", "Public", "auth", "workbuddy-desktop.info"];
const CLI_AUTH_BASENAME: &[&str] = &["CodeBuddyExtension", "Data", "Public", "auth", "Tencent-Cloud.coding-copilot.info"];

#[derive(Debug, Clone)]
pub struct AuthDiscovery {
    pub found: Option<PathBuf>,
    pub looked_in: Vec<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SessionFile {
    #[serde(default)]
    pub auth: AuthInfo,
    #[serde(default)]
    pub account: AccountInfo,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthInfo {
    pub access_token: Option<String>,
    pub endpoint: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub uid: Option<String>,
    pub enterprise_id: Option<String>,
}

#[derive(Debug)]
pub struct SessionContext {
    pub endpoint: Url,
    pub headers: HeaderMap,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("读取登录凭据失败（{0}）")]
    Io(#[from] std::io::Error),
    #[error("登录凭据文件不是合法 JSON（{0}）")]
    Json(#[from] serde_json::Error),
    #[error("NO_SESSION: 本地未找到有效登录会话")]
    NoSession,
    #[error("登录 endpoint 无效（{0}）")]
    InvalidEndpoint(#[from] url::ParseError),
    #[error("登录凭据包含非法 HTTP header 值")]
    InvalidHeader,
}

fn push_segments(mut base: PathBuf, segments: &[&str]) -> PathBuf {
    for segment in segments {
        base.push(segment);
    }
    base
}

pub fn discover_auth_file() -> AuthDiscovery {
    if let Ok(override_path) = env::var("WORKBUDDY_AUTH_FILE") {
        if !override_path.is_empty() {
            let path = PathBuf::from(&override_path);
            return AuthDiscovery {
                found: path.exists().then_some(path.clone()),
                looked_in: vec![path],
            };
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let local = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Local"));
    let xdg = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("share"));

    let candidates = vec![
        push_segments(local, AUTH_BASENAME),
        push_segments(home.join("Library").join("Application Support"), AUTH_BASENAME),
        push_segments(xdg, CLI_AUTH_BASENAME),
        push_segments(home.join(".config"), AUTH_BASENAME),
        home.join(".workbuddy").join("auth").join("workbuddy-desktop.info"),
    ];
    let found = candidates.iter().find(|p| p.exists()).cloned();
    AuthDiscovery { found, looked_in: candidates }
}

pub async fn load_session(path: &Path) -> Result<SessionFile, AuthError> {
    let text = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str(&text)?)
}

pub async fn load_session_retry(path: &Path, attempts: usize, delay: Duration) -> Result<SessionFile, AuthError> {
    let attempts = attempts.max(1);
    for index in 0..attempts {
        match load_session(path).await {
            Ok(session) => return Ok(session),
            Err(AuthError::Io(err)) if err.kind() == ErrorKind::PermissionDenied && index + 1 < attempts => {
                sleep(delay).await;
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("loop either returns or retries")
}

pub fn build_session_context(session: &SessionFile) -> Result<SessionContext, AuthError> {
    let token = session.auth.access_token.as_deref().filter(|s| !s.is_empty()).ok_or(AuthError::NoSession)?;
    let uid = session.account.uid.as_deref().filter(|s| !s.is_empty()).ok_or(AuthError::NoSession)?;

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("WorkBuddy"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| AuthError::InvalidHeader)?,
    );
    headers.insert("X-User-Id", HeaderValue::from_str(uid).map_err(|_| AuthError::InvalidHeader)?);

    if let Some(enterprise) = session.account.enterprise_id.as_deref().filter(|s| !s.is_empty()) {
        let value = HeaderValue::from_str(enterprise).map_err(|_| AuthError::InvalidHeader)?;
        headers.insert("X-Enterprise-Id", value.clone());
        headers.insert("X-Tenant-Id", value);
    }
    if let Some(domain) = session.auth.domain.as_deref().filter(|s| !s.is_empty()) {
        headers.insert("X-Domain", HeaderValue::from_str(domain).map_err(|_| AuthError::InvalidHeader)?);
    }

    let raw_endpoint = session.auth.endpoint.as_deref().filter(|s| !s.is_empty()).unwrap_or(DEFAULT_ENDPOINT);
    let endpoint = Url::parse(raw_endpoint.trim_end_matches('/'))?;
    Ok(SessionContext { endpoint, headers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_required_and_optional_headers() {
        let session = SessionFile {
            auth: AuthInfo {
                access_token: Some("secret".into()),
                endpoint: None,
                domain: Some("example".into()),
            },
            account: AccountInfo {
                uid: Some("uid-1".into()),
                enterprise_id: Some("ent-1".into()),
            },
        };
        let ctx = build_session_context(&session).unwrap();
        assert_eq!(ctx.endpoint.as_str(), "https://copilot.tencent.com/");
        assert_eq!(ctx.headers["X-User-Id"], "uid-1");
        assert_eq!(ctx.headers["X-Enterprise-Id"], "ent-1");
        assert_eq!(ctx.headers["X-Tenant-Id"], "ent-1");
        assert_eq!(ctx.headers["X-Domain"], "example");
    }

    #[test]
    fn missing_session_fields_are_not_json_errors() {
        let session: SessionFile = serde_json::from_str(r#"{"auth":{},"account":{}}"#).unwrap();
        assert!(matches!(build_session_context(&session), Err(AuthError::NoSession)));
    }
}
