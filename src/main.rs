use std::process::ExitCode;

use serde_json::json;
use workbuddy_auto_signin::app;
use workbuddy_auto_signin::output::Reporter;

#[tokio::main]
async fn main() -> ExitCode {
    let action = std::env::args().nth(1).unwrap_or_else(|| "auto".to_string());
    let action_for_task = action.clone();
    let result = tokio::spawn(async move { app::run(&action_for_task).await }).await;
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            Reporter::new(action.clone(), None).emit(json!({
                "result":"ERROR",
                "report":format!("程序运行异常（{error}）")
            }));
            2
        }
    };
    ExitCode::from(code.clamp(0, 255) as u8)
}
