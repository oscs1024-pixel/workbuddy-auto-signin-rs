use serde_json::Value;
use uuid::Uuid;

pub const ENVELOPES: &[&str] = &["data", "result", "resp", "response"];

pub fn dig<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let object = value.as_object()?;
    if let Some(found) = object.get(key).filter(|v| !v.is_null()) {
        return Some(found);
    }
    for wrapper in ENVELOPES {
        if let Some(child) = object.get(*wrapper).filter(|v| v.is_object()) {
            if let Some(found) = dig(child, key) {
                return Some(found);
            }
        }
    }
    None
}

pub fn as_i64(value: Option<&Value>, default: i64) -> i64 {
    let Some(value) = value else { return default; };
    match value {
        Value::Number(n) => {
            if let Some(v) = n.as_i64() {
                v
            } else if let Some(v) = n.as_u64() {
                i64::try_from(v).unwrap_or(default)
            } else if let Some(v) = n.as_f64() {
                if v.is_finite() && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                    v.trunc() as i64
                } else {
                    default
                }
            } else {
                default
            }
        }
        Value::String(s) => s.parse::<i64>().ok().or_else(|| {
            s.parse::<f64>().ok().and_then(|v| {
                (v.is_finite() && v >= i64::MIN as f64 && v <= i64::MAX as f64)
                    .then_some(v.trunc() as i64)
            })
        }).unwrap_or(default),
        Value::Bool(v) => i64::from(*v),
        _ => default,
    }
}

pub fn first_i64(body: &Value, key: &str, fallback: Option<&Value>) -> i64 {
    dig(body, key)
        .map(|v| as_i64(Some(v), 0))
        .unwrap_or_else(|| as_i64(fallback, 0))
}

pub fn format_credit(value: &Value) -> String {
    match value {
        Value::String(s) => {
            if let Ok(v) = s.parse::<i64>() {
                v.to_string()
            } else if let Ok(v) = s.parse::<f64>() {
                if v.is_finite() && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                    (v.trunc() as i64).to_string()
                } else {
                    s.clone()
                }
            } else {
                s.clone()
            }
        }
        Value::Number(_) | Value::Bool(_) => as_i64(Some(value), 0).to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

pub fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

pub fn value_truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(v)) => *v,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        _ => false,
    }
}

pub fn is_true_or_one(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(true))) || as_i64(value, 0) == 1
}

pub fn format_eta(arrive_at: Option<&Value>, server_now: Option<&Value>) -> String {
    fn as_f64(v: &Value) -> Option<f64> {
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    let Some(arrive) = arrive_at.and_then(as_f64) else { return String::new(); };
    let Some(now) = server_now.and_then(as_f64) else { return String::new(); };
    let left = arrive - now;
    if !left.is_finite() {
        return String::new();
    }
    if left <= 0.0 {
        return "，已到达待领取".to_string();
    }
    let minutes = (left / 60.0).round() as i64;
    if minutes < 60 {
        format!("，约 {} 分钟后回", minutes.max(1))
    } else {
        format!("，约 {:.1} 小时后回", left / 3600.0)
    }
}

pub fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

pub fn client_token(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dig_only_follows_known_envelopes() {
        let v = json!({"data":{"result":{"x":7}}, "other":{"y":9}});
        assert_eq!(dig(&v, "x"), Some(&json!(7)));
        assert!(dig(&v, "y").is_none());
    }

    #[test]
    fn as_i64_accepts_numeric_strings() {
        assert_eq!(as_i64(Some(&json!("1.5")), 0), 1);
        assert_eq!(as_i64(Some(&json!("1e3")), 0), 1000);
        assert_eq!(as_i64(Some(&json!("bad")), 7), 7);
    }

    #[test]
    fn eta_switches_to_hours_after_rounding() {
        assert_eq!(format_eta(Some(&json!(3599)), Some(&json!(0))), "，约 1.0 小时后回");
        assert_eq!(format_eta(Some(&json!(60)), Some(&json!(0))), "，约 1 分钟后回");
        assert_eq!(format_eta(Some(&json!(0)), Some(&json!(1))), "，已到达待领取");
    }

    #[test]
    fn client_token_has_expected_prefix() {
        assert!(client_token("u").starts_with("u-"));
    }
}
