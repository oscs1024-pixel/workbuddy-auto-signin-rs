use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::api::{BillingApi, GrowthApi};
use crate::auth::{build_session_context, discover_auth_file, load_session_retry};
use crate::budget::Budget;
use crate::cli::Action;
use crate::error::AuthError;
use crate::http::WorkBuddyClient;
use crate::output::Reporter;
use crate::service::{run_daily, GrowthService, SigninService};
use crate::util::env_flag;

pub async fn run(action_name: &str) -> i32 {
    let parsed = Action::parse(action_name);
    let (budget, warning) = Budget::for_action(parsed);
    let budget = Arc::new(budget);
    let reporter = Reporter::new(action_name.to_string(), warning);

    let Some(action) = parsed else {
        reporter.emit(json!({
            "result": "ERROR",
            "report": format!(
                "未知命令：{}（可用：{}）",
                action_name,
                Action::NAMES.join(" / ")
            )
        }));
        return 2;
    };

    let discovery = discover_auth_file();
    let Some(auth_file) = discovery.found else {
        let env_override = std::env::var("WORKBUDDY_AUTH_FILE")
            .ok()
            .filter(|value| !value.is_empty());

        let report = if let Some(path) = env_override {
            format!("WORKBUDDY_AUTH_FILE 指向的文件不存在：{path}")
        } else {
            "未找到 WorkBuddy 登录凭据。请先在本机登录 WorkBuddy 桌面端；或设置环境变量 WORKBUDDY_AUTH_FILE 指向 workbuddy-desktop.info。".to_string()
        };

        reporter.emit(json!({
            "result": "NO_AUTH",
            "report": report,
            "looked_in": discovery
                .looked_in
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>()
        }));
        return 2;
    };

    let session =
        match load_session_retry(&auth_file, 3, Duration::from_secs(2)).await {
            Ok(session) => session,
            Err(AuthError::Json(error)) => {
                reporter.emit(json!({
                    "result": "ERROR",
                    "report": format!(
                        "登录凭据文件不是合法 JSON（{error}），请重新登录 WorkBuddy 桌面端"
                    )
                }));
                return 2;
            }
            Err(AuthError::Io(error))
                if error.kind() == ErrorKind::InvalidData =>
            {
                reporter.emit(json!({
                    "result": "ERROR",
                    "report": format!(
                        "登录凭据文件内容损坏（InvalidData: {error}），请重新登录 WorkBuddy 桌面端"
                    )
                }));
                return 2;
            }
            Err(error) => {
                reporter.emit(json!({
                    "result": "ERROR",
                    "report": format!(
                        "读取登录凭据失败（{error}），请重新登录 WorkBuddy 桌面端"
                    )
                }));
                return 2;
            }
        };

    let context = match build_session_context(&session) {
        Ok(context) => context,
        Err(AuthError::NoSession) => {
            reporter.emit(json!({
                "result": "NO_SESSION",
                "report": "NO_SESSION: 本地未找到有效登录会话"
            }));
            return 1;
        }
        Err(error) => {
            reporter.emit(json!({
                "result": "ERROR",
                "report": error.to_string()
            }));
            return 2;
        }
    };

    let client = match WorkBuddyClient::new(
        context.endpoint,
        context.headers,
        budget.clone(),
    ) {
        Ok(client) => client,
        Err(error) => {
            reporter.emit(json!({
                "result": "ERROR",
                "report": format!("初始化 HTTP 客户端失败：{error}")
            }));
            return 2;
        }
    };

    let signin = SigninService::new(BillingApi::new(client.clone()));
    let growth = GrowthService::new(GrowthApi::new(client), budget);

    match action {
        Action::Auto | Action::Silent => {
            let (code, out, _) = run_daily(&signin, &growth).await;
            reporter.emit(out);
            code
        }
        Action::SilentPoll | Action::SilentGrowth => {
            let (code, mut out, quiet) = run_daily(&signin, &growth).await;
            insert(&mut out, "trigger", json!("poll"));

            if !quiet || env_flag("WORKBUDDY_GROWTH_LOG_EMPTY") {
                reporter.emit(out);
            }

            code
        }
        Action::Growth => {
            let run = growth.run().await;
            reporter.emit(run.out);
            run.code
        }
        Action::Status => {
            let response = signin.raw_status().await;
            reporter.emit(json!({
                "step": "status",
                "http": response.code,
                "body": response.body
            }));
            0
        }
        Action::Claim => {
            let response = signin.raw_claim().await;
            reporter.emit(json!({
                "step": "claim",
                "http": response.code,
                "body": response.body
            }));
            0
        }
        Action::All => {
            let status = signin.raw_status().await;
            reporter.emit(json!({
                "step": "status",
                "http": status.code,
                "body": status.body
            }));

            let claim = signin.raw_claim().await;
            reporter.emit(json!({
                "step": "claim",
                "http": claim.code,
                "body": claim.body
            }));

            0
        }
    }
}

fn insert(out: &mut Value, key: &str, value: Value) {
    if let Some(object) = out.as_object_mut() {
        object.insert(key.to_string(), value);
    }
}
