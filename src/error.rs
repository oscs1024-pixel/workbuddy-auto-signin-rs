use thiserror::Error;

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
