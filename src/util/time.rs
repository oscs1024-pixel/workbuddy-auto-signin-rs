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

    // 使用服务端 arrive_at - server_now，避免本机时钟漂移；终端轮询时 HH:MM:SS
    // 比“小数小时”更容易直观看出每一轮的剩余时间变化。
    let total_seconds = left.round().max(1.0) as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    format!("，旅行倒计时 {hours:02}:{minutes:02}:{seconds:02}")
}
