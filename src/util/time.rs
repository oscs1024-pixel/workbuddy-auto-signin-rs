use serde_json::Value;

pub fn format_eta(arrive_at: Option<&Value>, server_now: Option<&Value>) -> String {
    fn as_f64(value: &Value) -> Option<f64> {
        match value {
            Value::Number(number) => number.as_f64(),
            Value::String(value) => value.parse::<f64>().ok(),
            _ => None,
        }
    }

    let Some(arrive) = arrive_at.and_then(as_f64) else {
        return String::new();
    };
    let Some(now) = server_now.and_then(as_f64) else {
        return String::new();
    };

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
