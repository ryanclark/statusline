use super::{
	RenderContext, SegmentConfig, SegmentType, account, context, cost, credits, env, git,
	rate_limit,
};

#[must_use]
pub fn render_segment(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let result = match segment.segment_type() {
		SegmentType::ContextPercentage => context::context_percentage(segment, ctx),
		SegmentType::TotalInputTokens => context::total_input_tokens(segment, ctx),
		SegmentType::InputTokens => context::input_tokens(segment, ctx),
		SegmentType::OutputTokens => context::output_tokens(segment, ctx),
		SegmentType::CacheReadTokens => context::cache_read_tokens(segment, ctx),
		SegmentType::CacheHitRatio => context::cache_hit_ratio(segment, ctx),
		SegmentType::ContextRemaining => context::context_remaining(segment, ctx),
		SegmentType::ContextWindowSize => context::context_window_size(segment, ctx),
		SegmentType::Exceeds200k => context::exceeds_200k(segment, ctx),
		SegmentType::CacheWarm => context::cache_warm(segment, ctx),
		SegmentType::SessionCacheHitRatio => context::session_cache_hit_ratio(segment, ctx),
		SegmentType::CacheMisses => context::cache_misses(segment, ctx),
		SegmentType::CacheLastMiss => context::cache_last_miss(segment, ctx),

		SegmentType::FiveHour => rate_limit::five_hour(segment, ctx),
		SegmentType::SevenDay => rate_limit::seven_day(segment, ctx),
		SegmentType::SpendLimit => rate_limit::spend_limit(segment, ctx),
		SegmentType::FableUsage => rate_limit::fable_usage(segment, ctx),
		SegmentType::ExtraUsage => rate_limit::extra_usage(segment, ctx),
		SegmentType::Credits => credits::credits(segment, ctx),

		SegmentType::Cost => cost::cost(segment, ctx),
		SegmentType::CostRate => cost::cost_rate(segment, ctx),
		SegmentType::Duration => cost::duration(segment, ctx),
		SegmentType::ApiDuration => cost::api_duration(segment, ctx),
		SegmentType::TokensPerSecond => cost::tokens_per_second(segment, ctx),
		SegmentType::LinesAdded => cost::lines_added(segment, ctx),
		SegmentType::LinesRemoved => cost::lines_removed(segment, ctx),

		SegmentType::GitBranch => git::git_branch(segment, ctx),
		SegmentType::GitAheadBehind => git::git_ahead_behind(segment, ctx),
		SegmentType::GitStash => git::git_stash(segment, ctx),
		SegmentType::Pr => git::pr(segment, ctx),
		SegmentType::Repo => git::repo(segment, ctx),

		SegmentType::Divider => env::divider(segment, ctx),
		SegmentType::Newline => env::newline(segment, ctx),
		SegmentType::Cwd => env::cwd(segment, ctx),
		SegmentType::ProjectDir => env::project_dir(segment, ctx),
		SegmentType::Model => env::model(segment, ctx),
		SegmentType::ModelId => env::model_id(segment, ctx),
		SegmentType::Version => env::version(segment, ctx),
		SegmentType::SessionId => env::session_id(segment, ctx),
		SegmentType::SessionName => env::session_name(segment, ctx),
		SegmentType::VimMode => env::vim_mode(segment, ctx),
		SegmentType::AgentName => env::agent_name(segment, ctx),
		SegmentType::Worktree => env::worktree(segment, ctx),
		SegmentType::Effort => env::effort(segment, ctx),
		SegmentType::Thinking => env::thinking(segment, ctx),
		SegmentType::FastMode => env::fast_mode(segment, ctx),

		SegmentType::Account => account::account(segment, ctx),
	};

	result.filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::constants::DIVIDER;
	use crate::context_window::ContextWindow;
	use crate::input::{CostInfo, InputData, RateLimits};
	use crate::segment::SegmentLine;
	use chrono::Utc;

	fn default_input() -> InputData {
		InputData::default()
	}

	fn default_ctx(input: &InputData) -> RenderContext<'_> {
		RenderContext {
			input,
			usage: None,
			credits: None,
			git: None,
			five_threshold: 70.0.into(),
			seven_threshold: 100.0.into(),
			divider: DIVIDER,
			nerd_font: false,
			account: None,
		}
	}

	fn strip_ansi(s: &str) -> String {
		String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
	}

	#[test]
	fn render_context_percentage() {
		let mut input = default_input();
		input.context_window = ContextWindow::from_reader(
			r#"{"context_window": {"used_percentage": 42, "total_output_tokens": 0, "current_usage": {"input_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#
				.as_bytes(),
		)
		.unwrap()
		.context_window;
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::ContextPercentage);
		let output = render_segment(&seg, &ctx).unwrap();
		assert_eq!(strip_ansi(&output), "42%");
	}

	#[test]
	fn render_context_percentage_no_colors() {
		let mut input = default_input();
		input.context_window = ContextWindow::from_reader(
			r#"{"context_window": {"used_percentage": 42, "total_output_tokens": 0, "current_usage": {"input_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#
				.as_bytes(),
		)
		.unwrap()
		.context_window;
		let ctx = default_ctx(&input);
		let seg: SegmentConfig =
			serde_json::from_str(r#"{"type": "context_percentage", "colors": false}"#).unwrap();
		let output = render_segment(&seg, &ctx).unwrap();
		assert_eq!(output, "42%");
	}

	#[test]
	fn render_input_tokens_with_icon() {
		let mut input = default_input();
		input.context_window = ContextWindow::from_reader(
			r#"{"context_window": {"used_percentage": 0, "total_input_tokens": 1500, "total_output_tokens": 0, "current_usage": {"input_tokens": 1500, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#
				.as_bytes(),
		)
		.unwrap()
		.context_window;
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::InputTokens);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "\u{2191} 1.5k");
	}

	#[test]
	fn render_input_tokens_no_icon() {
		let mut input = default_input();
		input.context_window = ContextWindow::from_reader(
			r#"{"context_window": {"used_percentage": 0, "total_input_tokens": 1500, "total_output_tokens": 0, "current_usage": {"input_tokens": 1500, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#
				.as_bytes(),
		)
		.unwrap()
		.context_window;
		let ctx = default_ctx(&input);
		let seg: SegmentConfig =
			serde_json::from_str(r#"{"type": "input_tokens", "icon": false}"#).unwrap();
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "1.5k");
	}

	#[test]
	fn render_divider() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Divider);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, DIVIDER);
	}

	#[test]
	fn render_cwd_empty_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Cwd);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_cwd_with_value() {
		let mut input = default_input();
		input.cwd = "/tmp/test".to_owned();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Cwd);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "/tmp/test");
	}

	#[test]
	fn render_model() {
		let mut input = default_input();
		input.model.display_name = "Opus".to_owned();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Model);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "Opus");
	}

	#[test]
	fn render_model_empty_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Model);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_five_hour_no_data_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::FiveHour);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_five_hour_with_data() {
		use crate::input::RateLimitPeriod;
		let mut input = default_input();
		let future_epoch = Utc::now().timestamp() + 7200;
		input.rate_limits = RateLimits {
			five_hour: Some(RateLimitPeriod {
				used_percentage: 42.5.into(),
				resets_at: future_epoch,
			}),
			seven_day: None,
			spend_limit: None,
		};
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::FiveHour);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert!(output.contains("42%"), "got: {output}");
	}

	#[test]
	fn render_spend_limit_no_data_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::SpendLimit);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_spend_limit_shows_countdown_at_any_usage() {
		use crate::input::RateLimitPeriod;
		let mut input = default_input();
		input.rate_limits = RateLimits {
			five_hour: None,
			seven_day: None,
			spend_limit: Some(RateLimitPeriod {
				used_percentage: 5.0.into(),
				resets_at: Utc::now().timestamp() + 3 * 86_400,
			}),
		};
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::SpendLimit);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert!(output.contains("5%"), "got: {output}");
		// A spend limit has no threshold setting: the countdown is always useful.
		assert!(
			output.contains('d'),
			"expected a days countdown, got: {output}"
		);
	}

	#[test]
	fn render_spend_limit_over_100_keeps_the_real_value() {
		use crate::input::RateLimitPeriod;
		let mut input = default_input();
		input.rate_limits = RateLimits {
			five_hour: None,
			seven_day: None,
			spend_limit: Some(RateLimitPeriod {
				used_percentage: 120.0.into(),
				resets_at: 0,
			}),
		};
		let ctx = default_ctx(&input);
		let seg: SegmentConfig =
			serde_json::from_str(r#"{"type": "spend_limit", "icon": false}"#).unwrap();
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "120%", "no countdown once resets_at has passed");
	}

	#[test]
	fn render_newline_emits_a_line_break() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Newline);
		assert_eq!(render_segment(&seg, &ctx).as_deref(), Some("\n"));
	}

	#[test]
	fn render_effort_shows_the_level() {
		let mut input = default_input();
		input.effort.level = "xhigh".to_owned();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Effort);
		assert_eq!(strip_ansi(&render_segment(&seg, &ctx).unwrap()), "xhigh");
	}

	#[test]
	fn render_effort_absent_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Effort);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_thinking_only_when_enabled() {
		let mut input = default_input();
		let seg = SegmentConfig::Simple(SegmentType::Thinking);
		assert!(render_segment(&seg, &default_ctx(&input)).is_none());
		input.thinking.enabled = true;
		let ctx = default_ctx(&input);
		assert_eq!(strip_ansi(&render_segment(&seg, &ctx).unwrap()), "thinking");
	}

	#[test]
	fn render_fast_mode_only_when_on() {
		let mut input = default_input();
		let seg = SegmentConfig::Simple(SegmentType::FastMode);
		assert!(render_segment(&seg, &default_ctx(&input)).is_none());
		input.fast_mode = true;
		let ctx = default_ctx(&input);
		assert_eq!(strip_ansi(&render_segment(&seg, &ctx).unwrap()), "fast");
	}

	fn pr_input(number: u64, kind: &str, state: &str) -> InputData {
		let mut input = default_input();
		input.pr.number = Some(number);
		input.pr.url = format!("https://example.com/pull/{number}");
		input.pr.kind = kind.to_owned();
		input.pr.review_state = state.to_owned();
		input
	}

	#[test]
	fn render_pr_absent_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Pr);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_pr_links_the_number_to_the_url() {
		let input = pr_input(1234, "", "approved");
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Pr);
		let output = render_segment(&seg, &ctx).unwrap();
		assert!(
			output.contains("\u{1b}]8;;https://example.com/pull/1234\u{07}"),
			"expected an OSC 8 link, got: {output:?}"
		);
		assert!(strip_ansi(&output).ends_with("#1234"), "got: {output:?}");
	}

	#[test]
	fn render_pr_merge_request_uses_bang() {
		let input = pr_input(12, "mr", "draft");
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Pr);
		assert!(strip_ansi(&render_segment(&seg, &ctx).unwrap()).ends_with("!12"));
	}

	#[test]
	fn render_pr_without_colors_is_plain_text() {
		let input = pr_input(7, "", "changes_requested");
		let ctx = default_ctx(&input);
		let seg: SegmentConfig =
			serde_json::from_str(r#"{"type": "pr", "colors": false, "icon": false}"#).unwrap();
		assert_eq!(render_segment(&seg, &ctx).unwrap(), "#7");
	}

	fn cache_input(warm: bool, observed: bool) -> InputData {
		let mut input = default_input();
		input.prompt_cache = Some(crate::input::PromptCache {
			warm,
			caching_observed: observed,
			expires_at: warm.then(|| Utc::now().timestamp() + 2 * 3600),
			requests: 14,
			misses: 2,
			hit_ratio: Some(0.91),
			last_miss_at: Some(Utc::now().timestamp() - 180),
			last_miss_cause: Some(crate::input::MissCause {
				causes: vec!["tools_changed".to_owned()],
				..Default::default()
			}),
			..Default::default()
		});
		input
	}

	fn rendered(ty: SegmentType, input: &InputData) -> Option<String> {
		render_segment(&SegmentConfig::Simple(ty), &default_ctx(input)).map(|s| strip_ansi(&s))
	}

	#[test]
	fn render_cache_segments_are_silent_without_prompt_cache() {
		let input = default_input();
		for ty in [
			SegmentType::CacheWarm,
			SegmentType::SessionCacheHitRatio,
			SegmentType::CacheMisses,
			SegmentType::CacheLastMiss,
		] {
			assert!(rendered(ty.clone(), &input).is_none(), "{ty:?}");
		}
	}

	#[test]
	fn render_cache_warm_shows_time_until_cold() {
		let out = rendered(SegmentType::CacheWarm, &cache_input(true, true)).unwrap();
		assert!(
			out.ends_with("warm 2h0m") || out.ends_with("warm 1h59m"),
			"got: {out}"
		);
	}

	#[test]
	fn render_cache_warm_says_cold_when_not_warm() {
		let out = rendered(SegmentType::CacheWarm, &cache_input(false, true)).unwrap();
		assert!(out.ends_with("cold"), "got: {out}");
	}

	#[test]
	fn render_cache_warm_is_silent_when_caching_is_not_observed() {
		assert!(rendered(SegmentType::CacheWarm, &cache_input(false, false)).is_none());
	}

	#[test]
	fn render_session_cache_hit_ratio_as_percent() {
		let input = cache_input(true, true);
		assert_eq!(
			rendered(SegmentType::SessionCacheHitRatio, &input).as_deref(),
			Some("91%")
		);
		let mut no_ratio = cache_input(true, true);
		no_ratio.prompt_cache.as_mut().unwrap().hit_ratio = None;
		assert!(rendered(SegmentType::SessionCacheHitRatio, &no_ratio).is_none());
	}

	#[test]
	fn render_cache_misses_counts_with_plural() {
		let input = cache_input(true, true);
		assert!(
			rendered(SegmentType::CacheMisses, &input)
				.unwrap()
				.ends_with("2 misses")
		);
		let mut one = cache_input(true, true);
		one.prompt_cache.as_mut().unwrap().misses = 1;
		assert!(
			rendered(SegmentType::CacheMisses, &one)
				.unwrap()
				.ends_with("1 miss")
		);
	}

	#[test]
	fn render_cache_last_miss_shows_cause_and_age() {
		let out = rendered(SegmentType::CacheLastMiss, &cache_input(true, true)).unwrap();
		assert!(
			out.contains("tools_changed") && out.contains("3m"),
			"got: {out}"
		);
	}

	#[test]
	fn render_session_name_when_present() {
		let mut input = default_input();
		assert!(rendered(SegmentType::SessionName, &input).is_none());
		input.session_name = "my-session".to_owned();
		assert_eq!(
			rendered(SegmentType::SessionName, &input).as_deref(),
			Some("my-session")
		);
	}

	fn repo_input() -> InputData {
		let mut input = default_input();
		input.workspace.repo = Some(crate::input::RepoInfo {
			host: "github.com".to_owned(),
			owner: "anthropics".to_owned(),
			name: "claude-code".to_owned(),
		});
		input
	}

	#[test]
	fn render_repo_links_owner_and_name() {
		let input = repo_input();
		let ctx = default_ctx(&input);
		let output = render_segment(&SegmentConfig::Simple(SegmentType::Repo), &ctx).unwrap();
		assert!(
			output.contains("\u{1b}]8;;https://github.com/anthropics/claude-code\u{07}"),
			"got: {output:?}"
		);
		assert!(strip_ansi(&output).ends_with("anthropics/claude-code"));
		assert!(rendered(SegmentType::Repo, &default_input()).is_none());
	}

	#[test]
	fn render_repo_without_colors_is_plain_text() {
		let input = repo_input();
		let ctx = default_ctx(&input);
		let seg: SegmentConfig =
			serde_json::from_str(r#"{"type": "repo", "colors": false}"#).unwrap();
		assert_eq!(
			render_segment(&seg, &ctx).unwrap(),
			"anthropics/claude-code"
		);
	}

	#[test]
	fn render_worktree_falls_back_to_any_linked_worktree() {
		let mut input = default_input();
		input.workspace.git_worktree = "feature-xyz".to_owned();
		assert_eq!(
			rendered(SegmentType::Worktree, &input).as_deref(),
			Some("feature-xyz")
		);
		input.worktree.name = "my-feature".to_owned();
		assert_eq!(
			rendered(SegmentType::Worktree, &input).as_deref(),
			Some("my-feature"),
			"a worktree session wins"
		);
	}

	#[test]
	fn render_cache_last_miss_without_a_cause_still_shows_the_age() {
		let mut input = cache_input(true, true);
		input.prompt_cache.as_mut().unwrap().last_miss_cause = None;
		let out = rendered(SegmentType::CacheLastMiss, &input).unwrap();
		assert!(
			out.starts_with("miss ") && out.contains("ago"),
			"got: {out}"
		);
		input.prompt_cache.as_mut().unwrap().last_miss_at = None;
		assert!(
			rendered(SegmentType::CacheLastMiss, &input).is_none(),
			"no miss at all"
		);
	}

	#[test]
	fn render_cache_last_miss_joins_multiple_causes() {
		let mut input = cache_input(true, true);
		input.prompt_cache.as_mut().unwrap().last_miss_cause = Some(crate::input::MissCause {
			causes: vec![
				"tools_changed".to_owned(),
				"system_prompt_changed".to_owned(),
			],
			..Default::default()
		});
		let out = rendered(SegmentType::CacheLastMiss, &input).unwrap();
		assert!(
			out.starts_with("tools_changed+system_prompt_changed "),
			"got: {out}"
		);
	}

	#[test]
	fn render_pr_colors_by_review_state() {
		let input = pr_input(1, "", "approved");
		let ctx = default_ctx(&input);
		let out = render_segment(&SegmentConfig::Simple(SegmentType::Pr), &ctx).unwrap();
		assert!(
			out.contains("\u{1b}[38;2;80;200;120m#1"),
			"approved should be green: {out:?}"
		);
	}

	#[test]
	fn render_effort_colors_by_level() {
		let mut input = default_input();
		input.effort.level = "xhigh".to_owned();
		let ctx = default_ctx(&input);
		let out = render_segment(&SegmentConfig::Simple(SegmentType::Effort), &ctx).unwrap();
		assert!(
			out.contains("\u{1b}[38;2;240;160;60mxhigh"),
			"xhigh should be orange: {out:?}"
		);
	}

	#[test]
	fn render_repo_percent_encodes_unsafe_url_bytes() {
		let mut input = repo_input();
		input.workspace.repo.as_mut().unwrap().name = "a b\u{07}c".to_owned();
		let ctx = default_ctx(&input);
		let out = render_segment(&SegmentConfig::Simple(SegmentType::Repo), &ctx).unwrap();
		assert!(
			out.contains("]8;;https://github.com/anthropics/a%20b%07c\u{07}"),
			"got: {out:?}"
		);
	}

	#[test]
	fn render_cache_warm_colors_icon_and_word_by_state() {
		let warm = cache_input(true, true);
		let seg = SegmentConfig::Simple(SegmentType::CacheWarm);
		let out = render_segment(&seg, &default_ctx(&warm)).unwrap();
		assert!(
			out.contains("\u{1b}[38;2;80;200;120m\u{2668}"),
			"green icon: {out:?}"
		);
		assert!(
			out.contains("\u{1b}[38;2;80;200;120mwarm"),
			"green word: {out:?}"
		);
		let cold = cache_input(false, true);
		let out = render_segment(&seg, &default_ctx(&cold)).unwrap();
		assert!(
			out.contains("\u{1b}[38;2;240;200;80m\u{2668}"),
			"yellow icon: {out:?}"
		);
		assert!(
			out.contains("\u{1b}[38;2;240;200;80mcold"),
			"yellow word: {out:?}"
		);
	}

	#[test]
	fn render_cache_warm_honors_configured_state_colors() {
		let seg: SegmentConfig = serde_json::from_str(
			r##"{"type":"cache_warm","warm_color":"#0000FF","cold_color":"#FF00FF"}"##,
		)
		.unwrap();
		let warm = cache_input(true, true);
		let out = render_segment(&seg, &default_ctx(&warm)).unwrap();
		assert!(out.contains("\u{1b}[38;2;0;0;255m\u{2668}"), "{out:?}");
		assert!(out.contains("\u{1b}[38;2;0;0;255mwarm"), "{out:?}");
		let cold = cache_input(false, true);
		let out = render_segment(&seg, &default_ctx(&cold)).unwrap();
		assert!(out.contains("\u{1b}[38;2;255;0;255mcold"), "{out:?}");

		let seg: SegmentConfig = serde_json::from_str(
			r##"{"type":"cache_warm","warm_color":"#0000FF","icon_color":"#FF0000"}"##,
		)
		.unwrap();
		let out = render_segment(&seg, &default_ctx(&warm)).unwrap();
		assert!(
			out.contains("\u{1b}[38;2;255;0;0m\u{2668}"),
			"icon_color wins for the icon: {out:?}"
		);
		assert!(out.contains("\u{1b}[38;2;0;0;255mwarm"), "{out:?}");
	}

	#[test]
	fn render_fable_usage_no_api_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::FableUsage);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_fable_usage_absent_limit_returns_none() {
		let input = default_input();
		let usage: crate::usage::UsageResponse =
			serde_json::from_str(r#"{"limits": [{"kind": "session", "percent": 5}]}"#).unwrap();
		let mut ctx = default_ctx(&input);
		ctx.usage = Some(Ok(&usage));
		let seg = SegmentConfig::Simple(SegmentType::FableUsage);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_fable_usage_with_data_shows_percent() {
		let input = default_input();
		let future = (Utc::now() + chrono::Duration::hours(5)).to_rfc3339();
		let usage: crate::usage::UsageResponse = serde_json::from_str(&format!(
			r#"{{"limits": [{{"kind": "weekly_scoped", "percent": 42, "resets_at": "{future}",
				"scope": {{"model": {{"display_name": "Fable"}}}}}}]}}"#
		))
		.unwrap();
		let mut ctx = default_ctx(&input);
		ctx.usage = Some(Ok(&usage));
		let seg = SegmentConfig::Simple(SegmentType::FableUsage);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert!(output.contains("42%"), "got: {output}");
	}

	#[test]
	fn render_extra_usage_no_api_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::ExtraUsage);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_extra_usage_not_logged_in_shows_login() {
		let input = default_input();
		let err = crate::usage::UsageError::NotLoggedIn;
		let mut ctx = default_ctx(&input);
		ctx.usage = Some(Err(&err));
		let seg = SegmentConfig::Simple(SegmentType::ExtraUsage);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "\u{2205} log in to claude.ai");
	}

	#[test]
	fn render_extra_usage_other_error_shows_message() {
		let input = default_input();
		let err = crate::usage::UsageError::Other("network timeout".to_owned());
		let mut ctx = default_ctx(&input);
		ctx.usage = Some(Err(&err));
		let seg = SegmentConfig::Simple(SegmentType::ExtraUsage);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "\u{2a2f} network timeout");
	}

	#[test]
	fn render_cost() {
		let mut input = default_input();
		input.cost = CostInfo {
			total_cost_usd: 1.23,
			total_duration_ms: 0,
			total_api_duration_ms: 0,
			total_lines_added: 0,
			total_lines_removed: 0,
		};
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Cost);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "$1.23");
	}

	#[test]
	fn render_cost_zero_returns_none() {
		let input = default_input();
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Cost);
		assert!(render_segment(&seg, &ctx).is_none());
	}

	#[test]
	fn render_lines_added() {
		let mut input = default_input();
		input.cost.total_lines_added = 156;
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::LinesAdded);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "+156");
	}

	#[test]
	fn render_duration() {
		let mut input = default_input();
		input.cost.total_duration_ms = 45000;
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Duration);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "45s");
	}

	#[test]
	fn render_duration_minutes() {
		let mut input = default_input();
		input.cost.total_duration_ms = 125000;
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::Duration);
		let output = strip_ansi(&render_segment(&seg, &ctx).unwrap());
		assert_eq!(output, "2m5s");
	}

	#[test]
	fn segment_line_spaces_between_segments() {
		let mut input = default_input();
		input.context_window = ContextWindow::from_reader(
			r#"{"context_window": {"used_percentage": 50, "total_input_tokens": 500, "total_output_tokens": 1000, "current_usage": {"input_tokens": 500, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#
				.as_bytes(),
		)
		.unwrap()
		.context_window;
		let segments = vec![
			SegmentConfig::Simple(SegmentType::ContextPercentage),
			SegmentConfig::Simple(SegmentType::InputTokens),
			SegmentConfig::Simple(SegmentType::Divider),
		];
		let ctx = default_ctx(&input);
		let line = SegmentLine {
			segments: &segments,
			ctx,
		};
		let output = strip_ansi(&format!("{line}"));
		assert_eq!(output, "50% \u{2191} 500");
	}

	#[test]
	fn segment_line_skips_none_segments() {
		let input = default_input();
		let segments = vec![
			SegmentConfig::Simple(SegmentType::Divider),
			SegmentConfig::Simple(SegmentType::Model),
			SegmentConfig::Simple(SegmentType::Divider),
		];
		let ctx = default_ctx(&input);
		let line = SegmentLine {
			segments: &segments,
			ctx,
		};
		let output = strip_ansi(&format!("{line}"));
		assert_eq!(output, "");
	}

	#[test]
	fn segment_line_empty_segments() {
		let input = default_input();
		let segments = vec![
			SegmentConfig::Simple(SegmentType::Model),
			SegmentConfig::Simple(SegmentType::Cwd),
		];
		let ctx = default_ctx(&input);
		let line = SegmentLine {
			segments: &segments,
			ctx,
		};
		let output = format!("{line}");
		assert_eq!(output, "");
	}

	#[test]
	fn is_extra_usage_check() {
		assert!(SegmentConfig::Simple(SegmentType::ExtraUsage).is_extra_usage());
		assert!(!SegmentConfig::Simple(SegmentType::Model).is_extra_usage());
	}
}
