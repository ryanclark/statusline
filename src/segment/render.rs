use super::{context, cost, env, git, rate_limit, RenderContext, SegmentConfig, SegmentType};

pub(crate) fn render_segment(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
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

		SegmentType::FiveHour => rate_limit::five_hour(segment, ctx),
		SegmentType::SevenDay => rate_limit::seven_day(segment, ctx),
		SegmentType::ExtraUsage => rate_limit::extra_usage(segment, ctx),

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

		SegmentType::Divider => env::divider(segment, ctx),
		SegmentType::Cwd => env::cwd(segment, ctx),
		SegmentType::ProjectDir => env::project_dir(segment, ctx),
		SegmentType::Model => env::model(segment, ctx),
		SegmentType::ModelId => env::model_id(segment, ctx),
		SegmentType::Version => env::version(segment, ctx),
		SegmentType::SessionId => env::session_id(segment, ctx),
		SegmentType::VimMode => env::vim_mode(segment, ctx),
		SegmentType::AgentName => env::agent_name(segment, ctx),
		SegmentType::Worktree => env::worktree(segment, ctx),
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
			git: None,
			five_threshold: 70.0.into(),
			seven_threshold: 100.0.into(),
			divider: DIVIDER,
			nerd_font: false,
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
		};
		let ctx = default_ctx(&input);
		let seg = SegmentConfig::Simple(SegmentType::FiveHour);
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
