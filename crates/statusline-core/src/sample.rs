use crate::constants::DIVIDER;
use crate::context_window::{ContextWindow, CurrentUsage};
use crate::format::{Percentage, Tokens};
use crate::input::{
	AgentInfo, CostInfo, EffortInfo, InputData, MissCause, ModelInfo, PrInfo, PromptCache,
	RateLimitPeriod, RateLimits, RepoInfo, ThinkingInfo, VimInfo, Workspace, WorktreeInfo,
};
use crate::segment::{AccountDisplay, GitCache, RenderContext};
use crate::subagent::{Effort, Task};
use crate::usage::{PrepaidCredits, UsageError, UsageResponse};

pub struct SampleData {
	pub input: InputData,
	pub tasks: Vec<Task>,
	/// Each task viewed as a status line input, borrowed by [`Self::task_context`].
	pub task_inputs: Vec<InputData>,
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
		let spend_reset = chrono::Utc::now().timestamp() + 86_400 * 19; // +19d
		let cache_expires = chrono::Utc::now().timestamp() + 1800; // +30m
		let last_miss = chrono::Utc::now().timestamp() - 300; // -5m

		let input = InputData {
			cwd: "/home/user/project".to_owned(),
			session_id: "0a1b2c3d-4e5f-6789-abcd-ef0123456789".to_owned(),
			session_name: "statusline".to_owned(),
			model: ModelInfo {
				id: "claude-fable-5".to_owned(),
				display_name: "Fable".to_owned(),
			},
			workspace: Workspace {
				current_dir: "/home/user/project".to_owned(),
				project_dir: "/home/user/project".to_owned(),
				git_worktree: String::new(),
				repo: Some(RepoInfo {
					host: "github.com".to_owned(),
					owner: "ryanclark".to_owned(),
					name: "statusline".to_owned(),
				}),
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
				spend_limit: Some(RateLimitPeriod {
					used_percentage: Percentage::from(62.8),
					resets_at: spend_reset,
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
			fast_mode: true,
			effort: EffortInfo {
				level: "high".to_owned(),
			},
			thinking: ThinkingInfo { enabled: true },
			prompt_cache: Some(PromptCache {
				warm: true,
				caching_observed: true,
				ttl: "1h".to_owned(),
				expires_at: Some(cache_expires),
				requests: 14,
				misses: 2,
				expected_rebuilds: 1,
				hit_ratio: Some(0.91),
				cache_write_tokens: Tokens::from(352_000),
				miss_recache_tokens: Tokens::from(310_200),
				last_miss_at: Some(last_miss),
				last_miss_cause: Some(MissCause {
					causes: vec!["tools_changed".to_owned()],
					tools_added: Some(2),
					tools_removed: Some(0),
					system_char_delta: None,
				}),
				miss_causes: [("tools_changed".to_owned(), 2)].into_iter().collect(),
				recache_tokens_if_cold: Some(Tokens::from(45_000)),
			}),
			pr: PrInfo {
				number: Some(1234),
				url: "https://github.com/ryanclark/statusline/pull/1234".to_owned(),
				review_state: "approved".to_owned(),
				kind: String::new(),
			},
		};

		let git = GitCache {
			branch: Some("main".to_owned()),
			dirty: true,
			ahead: 2,
			behind: 0,
			stash_count: 1,
		};

		let fable_reset = (chrono::Utc::now() + chrono::Duration::days(3)).to_rfc3339();
		let usage: UsageResponse = serde_json::from_str(&format!(
			r#"{{
				"extra_usage": {{"monthly_limit": 10000.0, "used_credits": 2500.0}},
				"limits": [
					{{"kind": "weekly_scoped", "percent": 37, "resets_at": "{fable_reset}",
					  "scope": {{"model": {{"display_name": "Fable"}}}}}}
				]
			}}"#
		))
		.expect("representative sample usage JSON should parse");

		let credits: PrepaidCredits = serde_json::from_str(r#"{"amount": 3304}"#)
			.expect("representative sample credits JSON should parse");

		let account = AccountDisplay {
			nickname: "work".to_owned(),
			color: Some("cyan".to_owned()),
		};

		let task_started = (chrono::Utc::now().timestamp() - 95) * 1000; // -1m35s, in millis
		let tasks = vec![
			Task {
				id: "task-1".to_owned(),
				name: "security-reviewer".to_owned(),
				kind: "agent".to_owned(),
				status: "running".to_owned(),
				description: "Review the auth flow for injection risks".to_owned(),
				label: "security-reviewer".to_owned(),
				start_time: Some(task_started),
				model: "claude-opus-5".to_owned(),
				effort: Some(Effort::Level("high".to_owned())),
				context_window_size: Some(Tokens::from(200_000)),
				token_count: Some(Tokens::from(45_321)),
				cwd: "/home/user/project".to_owned(),
			},
			Task {
				id: "task-2".to_owned(),
				name: "Explore".to_owned(),
				kind: "agent".to_owned(),
				status: "completed".to_owned(),
				description: "Find call sites of parse_color".to_owned(),
				start_time: Some(task_started - 40_000),
				model: "claude-sonnet-5".to_owned(),
				context_window_size: Some(Tokens::from(200_000)),
				token_count: Some(Tokens::from(8_000)),
				cwd: "/home/user/project".to_owned(),
				..Task::default()
			},
		];
		let task_inputs = tasks.iter().map(Task::to_input).collect();

		Self {
			input,
			tasks,
			task_inputs,
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

	/// A render context for sample task `index`, or `None` past the last task.
	#[must_use]
	pub fn task_context(&self, index: usize) -> Option<RenderContext<'_>> {
		self.task_context_with(
			index,
			&self.divider,
			self.nerd_font,
			70.0.into(),
			100.0.into(),
		)
	}

	#[must_use]
	pub fn task_context_with<'a>(
		&'a self,
		index: usize,
		divider: &'a str,
		nerd_font: bool,
		five_threshold: Percentage,
		seven_threshold: Percentage,
	) -> Option<RenderContext<'a>> {
		Some(RenderContext {
			input: self.task_inputs.get(index)?,
			usage: None,
			credits: None,
			git: None,
			five_threshold,
			seven_threshold,
			divider,
			nerd_font,
			account: None,
			task: self.tasks.get(index),
		})
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
			task: self.tasks.first(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::catalog::catalog;
	use crate::segment::{SegmentConfig, SegmentType, render_segment};

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
	fn every_sample_task_renders_the_task_segments() {
		let data = SampleData::representative();
		assert!(
			data.tasks.len() >= 2,
			"the subagent preview should show more than one row"
		);
		for i in 0..data.tasks.len() {
			let ctx = data.task_context(i).unwrap();
			let name = render_segment(&SegmentConfig::Simple(SegmentType::TaskName), &ctx).unwrap();
			assert_eq!(
				String::from_utf8(strip_ansi_escapes::strip(&name)).unwrap(),
				data.tasks[i].name
			);
			let model = render_segment(&SegmentConfig::Simple(SegmentType::Model), &ctx);
			assert_eq!(model.is_some(), !data.tasks[i].model.is_empty());
		}
		assert!(data.task_context(data.tasks.len()).is_none());
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
