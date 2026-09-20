use serde_json::Value;

use super::number::as_i64;

const ENVELOPES: &[&str] = &["data", "result", "resp", "response"];

pub fn dig<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let object = value.as_object()?;
    if let Some(found) = object.get(key).filter(|value| !value.is_null()) {
        return Some(found);
    }

    for wrapper in ENVELOPES {
        if let Some(child) = object.get(*wrapper).filter(|value| value.is_object()) {
            if let Some(found) = dig(child, key) {
                return Some(found);
            }
        }
    }

    None
}

pub fn first_i64(body: &Value, key: &str, fallback: Option<&Value>) -> i64 {
    dig(body, key)
        .map(|value| as_i64(Some(value), 0))
        .unwrap_or_else(|| as_i64(fallback, 0))
}

pub fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

pub fn value_truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|value| value != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(value)) => !value.is_empty(),
        Some(Value::Object(value)) => !value.is_empty(),
        _ => false,
    }
}

pub fn is_true_or_one(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(true))) || as_i64(value, 0) == 1
}
