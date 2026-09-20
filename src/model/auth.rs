use reqwest::header::HeaderMap;
use serde::{Deserialize, Deserializer};
use url::Url;

#[derive(Debug, Default, Deserialize)]
pub struct SessionFile {
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub auth: AuthInfo,
    #[serde(default, deserialize_with = "deserialize_null_default")]
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


fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
