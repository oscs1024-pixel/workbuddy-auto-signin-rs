use reqwest::header::HeaderMap;
use serde::Deserialize;
use url::Url;

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
