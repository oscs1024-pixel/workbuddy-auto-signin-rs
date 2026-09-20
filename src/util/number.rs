use serde_json::Value;

pub fn try_i64(value: Option<&Value>) -> Option<i64> {
    let value = value?;

    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Some(value)
            } else if let Some(value) = number.as_u64() {
                i64::try_from(value).ok()
            } else if let Some(value) = number.as_f64() {
                (value.is_finite() && value >= i64::MIN as f64 && value <= i64::MAX as f64)
                    .then_some(value.trunc() as i64)
            } else {
                None
            }
        }
        Value::String(value) => value.parse::<i64>().ok().or_else(|| {
            value.parse::<f64>().ok().and_then(|number| {
                (number.is_finite() && number >= i64::MIN as f64 && number <= i64::MAX as f64)
                    .then_some(number.trunc() as i64)
            })
        }),
        Value::Bool(value) => Some(i64::from(*value)),
        _ => None,
    }
}

pub fn as_i64(value: Option<&Value>, default: i64) -> i64 {
    try_i64(value).unwrap_or(default)
}

pub fn format_credit(value: &Value) -> String {
    match value {
        Value::String(value) => {
            if let Ok(number) = value.parse::<i64>() {
                number.to_string()
            } else if let Ok(number) = value.parse::<f64>() {
                if number.is_finite() && number >= i64::MIN as f64 && number <= i64::MAX as f64 {
                    (number.trunc() as i64).to_string()
                } else {
                    value.clone()
                }
            } else {
                value.clone()
            }
        }
        Value::Number(_) | Value::Bool(_) => as_i64(Some(value), 0).to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}
