use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use tokio::time::sleep;

use crate::error::AuthError;
use crate::model::auth::SessionFile;

pub async fn load_session(path: &Path) -> Result<SessionFile, AuthError> {
    let text = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str(&text)?)
}

pub async fn load_session_retry(
    path: &Path,
    attempts: usize,
    delay: Duration,
) -> Result<SessionFile, AuthError> {
    let attempts = attempts.max(1);
    for index in 0..attempts {
        match load_session(path).await {
            Ok(session) => return Ok(session),
            Err(AuthError::Io(err))
                if err.kind() == ErrorKind::PermissionDenied && index + 1 < attempts =>
            {
                sleep(delay).await;
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("loop either returns or retries")
}
