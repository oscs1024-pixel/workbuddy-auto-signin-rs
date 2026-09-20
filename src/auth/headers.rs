use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use url::Url;

use crate::config::DEFAULT_ENDPOINT;
use crate::error::AuthError;
use crate::model::auth::{SessionContext, SessionFile};

pub fn build_session_context(session: &SessionFile) -> Result<SessionContext, AuthError> {
    let token = session
        .auth
        .access_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(AuthError::NoSession)?;
    let uid = session
        .account
        .uid
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(AuthError::NoSession)?;

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("WorkBuddy"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| AuthError::InvalidHeader)?,
    );
    headers.insert(
        "X-User-Id",
        HeaderValue::from_str(uid).map_err(|_| AuthError::InvalidHeader)?,
    );

    if let Some(enterprise) = session
        .account
        .enterprise_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let value = HeaderValue::from_str(enterprise).map_err(|_| AuthError::InvalidHeader)?;
        headers.insert("X-Enterprise-Id", value.clone());
        headers.insert("X-Tenant-Id", value);
    }

    if let Some(domain) = session
        .auth
        .domain
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        headers.insert(
            "X-Domain",
            HeaderValue::from_str(domain).map_err(|_| AuthError::InvalidHeader)?,
        );
    }

    let raw_endpoint = session
        .auth
        .endpoint
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ENDPOINT);
    let endpoint = Url::parse(raw_endpoint.trim_end_matches('/'))?;

    Ok(SessionContext { endpoint, headers })
}
