mod buddy;
mod context;
mod lottery;
mod makeup;
mod redeem;
mod summary;
mod tasks;
mod travel;

use std::sync::Arc;

use crate::api::GrowthApi;
use crate::budget::Budget;
use crate::model::common::ServiceRun;

use context::{GrowthAccumulator, GrowthContext};

#[derive(Clone)]
pub struct GrowthService {
    api: GrowthApi,
    budget: Arc<Budget>,
}

impl GrowthService {
    pub fn new(api: GrowthApi, budget: Arc<Budget>) -> Self {
        Self { api, budget }
    }

    pub async fn run(&self) -> ServiceRun {
        let ctx = GrowthContext {
            api: &self.api,
            budget: &self.budget,
        };
        let mut acc = GrowthAccumulator::default();

        if let Some(run) = travel::run(&ctx, &mut acc).await {
            return run;
        }

        if let Some(run) = tasks::run(&ctx, &mut acc).await {
            return run;
        }

        let makeup = match makeup::run(&ctx, &mut acc).await {
            Ok(state) => state,
            Err(run) => return run,
        };

        if let Some(run) = redeem::run(&ctx, &mut acc).await {
            return run;
        }

        if let Some(run) = lottery::run(&ctx, &mut acc).await {
            return run;
        }

        if let Some(run) = buddy::run(&ctx, &mut acc).await {
            return run;
        }

        let (energy, streak_days) = summary::load_values(
            &ctx,
            makeup.streak_body,
            makeup.streak_stale,
        )
        .await;

        summary::finalize(acc, energy, streak_days)
    }
}

pub use buddy::compute_open_count;
pub use lottery::is_no_chance;
pub use makeup::parse_makeup_cards;
pub use redeem::{is_tier_locked, is_unknown_tier, redeem_reward_desc};
