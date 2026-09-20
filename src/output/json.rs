use serde::Serialize;

pub fn serialize_safe<T: Serialize + std::fmt::Debug>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}
