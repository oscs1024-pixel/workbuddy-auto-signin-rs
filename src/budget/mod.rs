use std::env;
use std::time::{Duration, Instant};

use crate::cli::Action;
use crate::config::{
    DEFAULT_BUDGET_SECONDS, MAX_BUDGET_SECONDS, POLL_BUDGET_SECONDS, POLL_MAX_BUDGET_SECONDS,
};

#[derive(Debug, Clone)]
pub struct BudgetConfig {
    pub limit: Duration,
    pub warning: Option<String>,
}

impl BudgetConfig {
    pub fn from_env(default: f64, maximum: f64) -> Self {
        Self::from_raw(
            env::var("WORKBUDDY_BUDGET_SECONDS").ok().as_deref(),
            default,
            maximum,
        )
    }

    pub fn from_raw(raw: Option<&str>, default: f64, maximum: f64) -> Self {
        let Some(raw) = raw.filter(|s| !s.is_empty()) else {
            return Self {
                limit: Duration::from_secs_f64(default),
                warning: None,
            };
        };

        let value = match raw.parse::<f64>() {
            Ok(v) if v.is_finite() => v,
            _ => {
                return Self {
                    limit: Duration::from_secs_f64(default),
                    warning: Some(format!(
                        "WORKBUDDY_BUDGET_SECONDS={raw:?} 不是数字，已回落 {} 秒",
                        default as i64
                    )),
                };
            }
        };

        if value <= 0.0 {
            return Self {
                limit: Duration::from_secs_f64(default),
                warning: Some(format!(
                    "WORKBUDDY_BUDGET_SECONDS={raw} 必须为正数，已回落 {} 秒",
                    default as i64
                )),
            };
        }

        if value > maximum {
            return Self {
                limit: Duration::from_secs_f64(maximum),
                warning: Some(format!(
                    "WORKBUDDY_BUDGET_SECONDS={raw} 超过本命令上限，已夹到 {} 秒（须小于该定时任务的 ExecutionTimeLimit）",
                    maximum as i64
                )),
            };
        }

        Self {
            limit: Duration::from_secs_f64(value),
            warning: None,
        }
    }
}

#[derive(Debug)]
pub struct Budget {
    started_at: Instant,
    limit: Duration,
}

impl Budget {
    pub fn for_action(action: Option<Action>) -> (Self, Option<String>) {
        let (default, maximum) = if action.map(Action::is_poll).unwrap_or(false) {
            (POLL_BUDGET_SECONDS, POLL_MAX_BUDGET_SECONDS)
        } else {
            (DEFAULT_BUDGET_SECONDS, MAX_BUDGET_SECONDS)
        };
        let cfg = BudgetConfig::from_env(default, maximum);
        (
            Self {
                started_at: Instant::now(),
                limit: cfg.limit,
            },
            cfg.warning,
        )
    }

    pub fn remaining(&self) -> Duration {
        self.limit.saturating_sub(self.started_at.elapsed())
    }

    pub fn remaining_secs_f64(&self) -> f64 {
        self.remaining().as_secs_f64()
    }

    pub fn exhausted(&self) -> bool {
        self.remaining().is_zero()
    }
}
