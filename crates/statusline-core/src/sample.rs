use crate::constants::DIVIDER;
use crate::context_window::{ContextWindow, CurrentUsage};
use crate::format::{Percentage, Tokens};
use crate::input::{AgentInfo, CostInfo, InputData, ModelInfo, RateLimitPeriod, RateLimits, VimInfo, Workspace, WorktreeInfo};
use crate::segment::{AccountDisplay, GitCache, RenderContext};
use crate::usage::{PrepaidCredits, UsageError, UsageResponse};

pub struct SampleData {
	pub input: InputData,
	pub git: GitCache,
	pub usage: Result<UsageResponse, UsageError>,
	pub credits: Result<PrepaidCredits, UsageError>,
	pub account: AccountDisplay,
	pub divider: String,
	pub nerd_font: bool,
}

impl SampleData {
	#[must_use]
	pub fn representative() -> Self {
		let five_reset = chrono::Utc::now().timestamp() + 7200; // +2h
		let seven_reset = chrono::Utc::now().timestamp() + 86_400 * 5; // +5d

		let input = InputData {
			cwd: "/home/user/project".to_owned(),
			session_id: "0a1b2c3d-4e5f-6789-abcd-ef0123456789".to_owned(),
			model: ModelInfo {
				id: "claude-fable-5".to_owned(),
				display_name: "Fable".to_owned(),
			},
			workspace: Workspace {
				current_dir: "/home/user/project".to_owned(),
				project_dir: "/home/user/project".to_owned(),
			},
			version: "1.0.80".to_owned(),
			cost: CostInfo {
				total_cost_usd: 1.23,
				total_duration_ms: 125000,
				total_api_duration_ms: 2300,
				total_lines_added: 156,
				total_lines_removed: 23,
			},
			context_window: ContextWindow {
				used_percentage: Percentage::from(8.0),
				remaining_percentage: Percentage::from(92.0),
				total_input_tokens: Tokens::from(16000),
				total_output_tokens: Tokens::from(4521),
				context_window_size: Tokens::from(200000),
				current_usage: CurrentUsage {
					input_tokens: Tokens::from(8500),
					cache_creation_input_tokens: Tokens::from(5000),
					cache_read_input_tokens: Tokens::from(2000),
				},
			},
			rate_limits: RateLimits {
				five_hour: Some(RateLimitPeriod {
					used_percentage: Percentage::from(23.5),
					resets_at: five_reset,
				}),
				seven_day: Some(RateLimitPeriod {
					used_percentage: Percentage::from(41.2),
					resets_at: seven_reset,
				}),
			},
			vim: VimInfo {
				mode: "NORMAL".to_owned(),
			},
			agent: AgentInfo {
				name: "security-reviewer".to_owned(),
			},
			worktree: WorktreeInfo {
				name: "my-feature".to_owned(),
				branch: "worktree-my-feature".to_owned(),
				original_branch: "main".to_owned(),
			},
			exceeds_200k_tokens: true,
		};

		let git = GitCache {
			branch: Some("main".to_owned()),
			dirty: true,
			ahead: 2,
			behind: 0,
			stash_count: 1,
		};

		let usage: UsageResponse = serde_json::from_str(
			r#"{"extra_usage": {"monthly_limit": 10000.0, "used_credits": 2500.0}}"#,
		)
		.expect("representative sample usage JSON should parse");

		let credits: PrepaidCredits = serde_json::from_str(r#"{"amount": 3304}"#)
			.expect("representative sample credits JSON should parse");

		let account = AccountDisplay {
			nickname: "work".to_owned(),
			color: Some("cyan".to_owned()),
		};

		Self {
			input,
			git,
			usage: Ok(usage),
			credits: Ok(credits),
			account,
			divider: DIVIDER.to_owned(),
			nerd_font: false,
		}
	}

	#[must_use]
	pub fn render_context(&self) -> RenderContext<'_> {
		self.render_context_with(&self.divider, self.nerd_font, 70.0.into(), 100.0.into())
	}

	#[must_use]
	pub fn render_context_with<'a>(
		&'a self,
		divider: &'a str,
		nerd_font: bool,
		five_threshold: Percentage,
		seven_threshold: Percentage,
	) -> RenderContext<'a> {
		RenderContext {
			input: &self.input,
			usage: Some(self.usage.as_ref()),
			credits: Some(self.credits.as_ref()),
			git: Some(&self.git),
			five_threshold,
			seven_threshold,
			divider,
			nerd_font,
			account: Some(self.account.clone()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::catalog::catalog;
	use crate::segment::{SegmentConfig, render_segment};

	#[test]
	fn sample_renders_every_segment() {
		let data = SampleData::representative();
		let ctx = data.render_context();

		for m in catalog() {
			let seg = SegmentConfig::Simple(m.ty.clone());
			let out = render_segment(&seg, &ctx);
			assert!(
				out.as_deref().is_some_and(|s| !s.is_empty()),
				"{} rendered None/empty over sample data",
				m.id
			);
		}
	}

	#[test]
	fn render_context_borrows_sample() {
		let data = SampleData::representative();
		let ctx = data.render_context();
		assert_eq!(ctx.divider, DIVIDER);
		assert!(ctx.git.is_some());
		assert!(ctx.usage.is_some());
		assert!(ctx.credits.is_some());
		assert!(ctx.account.is_some());
	}
}
