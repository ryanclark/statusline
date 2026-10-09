use crate::constants::{CYAN, DOWN_ARROW, GRAY, GREEN, ORANGE, PURPLE, UP_ARROW, YELLOW};
use crate::format::{
	ColoredPercentage, Percentage, Tokens, elapsed, format_duration_secs, format_window,
};
use crate::input::{Code, MissCause, MissCauseCode, PromptCache};
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use std::borrow::Cow;
use std::time::Duration;

use super::{
	Icon, RenderContext, SegmentConfig, apply_style, dim, format_icon, paint, unknown_code,
};

pub(super) fn context_percentage(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let pct = ctx.input.context_window.used_percentage;
	let text = if segment.colors() {
		format!("{}", ColoredPercentage(pct))
	} else {
		format!("{pct}")
	};

	Some(apply_style(&text, segment.style()))
}

pub(super) fn total_input_tokens(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let icon = format_icon(
		segment,
		Icon {
			unicode: UP_ARROW,
			nerd: "\u{f062}",
		},
		CYAN,
		ctx.nerd_font,
	);
	let tokens = ctx.input.context_window.current_usage.input_tokens
		+ ctx
			.input
			.context_window
			.current_usage
			.cache_creation_input_tokens
		+ ctx
			.input
			.context_window
			.current_usage
			.cache_read_input_tokens;
	let text = format!("{icon}{tokens}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn input_tokens(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let icon = format_icon(
		segment,
		Icon {
			unicode: UP_ARROW,
			nerd: "\u{f062}",
		},
		ORANGE,
		ctx.nerd_font,
	);
	let tokens = ctx.input.context_window.total_input_tokens;
	let text = format!("{icon}{tokens}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn output_tokens(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let icon = format_icon(
		segment,
		Icon {
			unicode: DOWN_ARROW,
			nerd: "\u{f063}",
		},
		PURPLE,
		ctx.nerd_font,
	);
	let tokens = ctx.input.context_window.total_output_tokens;
	let text = format!("{icon}{tokens}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn cache_read_tokens(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let tokens = ctx
		.input
		.context_window
		.current_usage
		.cache_read_input_tokens;
	let icon = format_icon(
		segment,
		Icon {
			unicode: "\u{21BB}",
			nerd: "\u{f021}",
		},
		CYAN,
		ctx.nerd_font,
	);
	let text = format!("{icon}{tokens}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn cache_hit_ratio(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let total = ctx.input.context_window.total_input_tokens;
	if total == 0.into() {
		return None;
	}

	let cache_read = ctx
		.input
		.context_window
		.current_usage
		.cache_read_input_tokens;
	let ratio = cache_read.ratio_of(total);
	let text = format!("{ratio:.0}%");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn context_remaining(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let pct = ctx.input.context_window.remaining_percentage;
	let text = if segment.colors() {
		let color_pct = Percentage::from(100.0) - pct;
		format!("{}", format_args!("{pct}").color(color_pct.color()).bold())
	} else {
		format!("{pct}")
	};

	Some(apply_style(&text, segment.style()))
}

pub(super) fn context_window_size(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let size = ctx.input.context_window.context_window_size;
	if size == 0.into() {
		return None;
	}
	let text = format!("{size}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn exceeds_200k(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if !ctx.input.exceeds_200k_tokens {
		return None;
	}

	let text = if segment.colors() {
		format!("{}", ">200k".color(crate::constants::RED).bold())
	} else {
		">200k".to_owned()
	};

	Some(apply_style(&text, segment.style()))
}

/// Prompt cache segments stay silent until the session has seen a cache-reporting response, so a
/// provider that never reports cache tokens does not show a permanently cold cache.
fn observed_cache<'a>(ctx: &'a RenderContext<'_>) -> Option<&'a PromptCache> {
	ctx.input
		.prompt_cache
		.as_ref()
		.filter(|c| c.caching_observed)
}

fn dim_suffix(segment: &SegmentConfig, text: &str) -> String {
	format!(" {}", dim(segment, text))
}

pub(super) fn cache_warm(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let cache = observed_cache(ctx)?;

	// The state colour covers the icon too, so a glance at either tells the story; an explicit
	// icon_color still wins inside format_icon.
	let (state, color) = if cache.warm {
		("warm", segment.warm_color().unwrap_or(GREEN))
	} else {
		("cold", segment.cold_color().unwrap_or(YELLOW))
	};
	let icon_str = format_icon(
		segment,
		Icon {
			unicode: "\u{2668}",
			nerd: "\u{f2c7}",
		},
		color,
		ctx.nerd_font,
	);
	let state = paint(segment, state, color);
	let until_cold = cache
		.expires_at
		.filter(|_| cache.warm)
		.and_then(|at| super::timing::reset_hint(segment, at, Utc::now()))
		.map(|left| dim_suffix(segment, &left))
		.unwrap_or_default();

	Some(apply_style(
		&format!("{icon_str}{state}{until_cold}"),
		segment.style(),
	))
}

pub(super) fn session_cache_hit_ratio(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let ratio = observed_cache(ctx)?.hit_ratio?;
	let text = format!("{}", Percentage::from(ratio * 100.0));

	Some(apply_style(&text, segment.style()))
}

pub(super) fn cache_misses(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let cache = observed_cache(ctx)?;
	// Claude Code only sends the session total, so a window can only be applied to the plugin's miss times.
	let (misses, window) = match (&cache.miss_times, segment.within()) {
		(Some(times), Some(window)) => (
			misses_within(times, window, Utc::now()) as u64,
			Some(window),
		),
		_ => (cache.misses, None),
	};
	if misses == 0 && window.is_some() {
		return None;
	}

	let noun = if misses == 1 { "miss" } else { "misses" };
	let color = if misses == 0 { GRAY } else { YELLOW };
	let text = match window {
		Some(window) => format!("{misses} {noun} in {}", format_window(window)),
		None => format!("{misses} {noun}"),
	};
	let text = paint(segment, &text, color);

	Some(apply_style(&text, segment.style()))
}

fn misses_within(times: &[DateTime<Utc>], window: Duration, now: DateTime<Utc>) -> usize {
	times
		.iter()
		.filter(|at| !is_older_than(**at, window, now))
		.count()
}

fn is_older_than(at: DateTime<Utc>, window: Duration, now: DateTime<Utc>) -> bool {
	elapsed(at, now).is_some_and(|age| age.as_secs() > window.as_secs())
}

pub(super) fn cache_last_miss(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let cache = observed_cache(ctx)?;
	let cause = cache
		.last_miss_cause
		.as_ref()
		.filter(|c| !c.causes.is_empty());
	if cause.is_none() && cache.last_miss_at.is_none() {
		return None;
	}
	let now = Utc::now();
	if let (Some(at), Some(window)) = (cache.last_miss_at, segment.within())
		&& is_older_than(at, window, now)
	{
		return None;
	}

	let label = paint(segment, &describe_miss(cause, segment.details()), ORANGE);
	let age = cache
		.last_miss_at
		.and_then(|at| elapsed(at, now))
		.map(|ago| format_duration_secs(ago.as_secs()))
		.map(|ago| dim_suffix(segment, &format!("{ago} ago")))
		.unwrap_or_default();

	Some(apply_style(&format!("{label}{age}"), segment.style()))
}

/// Claude Code reports a miss it could not diagnose with a null cause or `unknown`.
const UNEXPLAINED: &str = "unexplained miss";

fn describe_miss(cause: Option<&MissCause>, details: bool) -> Cow<'_, str> {
	let Some(cause) = cause else {
		return UNEXPLAINED.into();
	};

	cause
		.causes
		.iter()
		.map(|code| describe_cause(code, cause, details))
		.collect::<Vec<_>>()
		.join(", ")
		.into()
}

fn describe_cause<'a>(
	code: &'a Code<MissCauseCode>,
	cause: &MissCause,
	details: bool,
) -> Cow<'a, str> {
	let code = match code {
		Code::Known(code) => code,
		Code::Other(code) => return unknown_code(code),
	};
	let phrase = match code {
		MissCauseCode::TtlExpired => "expired after idle",
		MissCauseCode::TtlExpired5m => "expired after 5m idle",
		MissCauseCode::TtlExpired1h => "expired after 1h idle",
		MissCauseCode::ToolsChanged => {
			let delta = details
				.then(|| tools_delta(cause.tools_added, cause.tools_removed))
				.flatten();
			return with_detail("tools changed", delta);
		}
		MissCauseCode::SystemChanged => {
			let delta = cause
				.system_char_delta
				.filter(|d| details && *d != 0)
				.map(|d| format!("{}{} chars", sign(d), Tokens::from(d.unsigned_abs())));
			return with_detail("system prompt changed", delta);
		}
		MissCauseCode::ModelChanged => "model switched",
		MissCauseCode::Compacted => "compacted",
		MissCauseCode::FastModeChanged => "fast mode toggled",
		MissCauseCode::CacheScopeOrTtlChanged => "cache scope or TTL changed",
		MissCauseCode::BetasChanged => "beta headers changed",
		MissCauseCode::EffortChanged => "effort changed",
		MissCauseCode::ThinkingModeChanged => "thinking toggled",
		MissCauseCode::ThinkingDisplayChanged => "thinking display changed",
		MissCauseCode::AutoModeChanged => "auto mode toggled",
		MissCauseCode::OverageChanged => "usage limit state changed",
		MissCauseCode::ExtraBodyChanged => "extra request fields changed",
		MissCauseCode::DeferLoadingChanged => "deferred tool loading changed",
		MissCauseCode::MessagesRewritten => "earlier messages changed",
		MissCauseCode::LikelyServerSide => "prompt unchanged, likely server side",
		MissCauseCode::Unknown => UNEXPLAINED,
	};

	phrase.into()
}

fn with_detail(phrase: &'static str, detail: Option<String>) -> Cow<'static, str> {
	match detail {
		Some(detail) => format!("{phrase} ({detail})").into(),
		None => phrase.into(),
	}
}

const MINUS: char = '\u{2212}';

fn sign(delta: i64) -> char {
	if delta < 0 { MINUS } else { '+' }
}

fn tools_delta(added: Option<u64>, removed: Option<u64>) -> Option<String> {
	let parts: Vec<String> = [(added, '+'), (removed, MINUS)]
		.into_iter()
		.filter_map(|(n, sign)| n.filter(|n| *n > 0).map(|n| format!("{sign}{n}")))
		.collect();

	(!parts.is_empty()).then(|| parts.join(" "))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::input::InputData;
	use crate::segment::render_segment;
	use chrono::TimeDelta;

	fn cause(json: &str) -> MissCause {
		serde_json::from_str(json).unwrap()
	}

	#[test]
	fn causes_read_as_natural_language() {
		for (json, details, want) in [
			(r#"{"causes": ["ttl_expired"]}"#, true, "expired after idle"),
			(
				r#"{"causes": ["tools_changed"], "tools_added": 2, "tools_removed": 1}"#,
				true,
				"tools changed (+2 \u{2212}1)",
			),
			(
				r#"{"causes": ["tools_changed"], "tools_added": 2, "tools_removed": 0}"#,
				true,
				"tools changed (+2)",
			),
			(
				r#"{"causes": ["tools_changed"], "tools_added": 2, "tools_removed": 1}"#,
				false,
				"tools changed",
			),
			(r#"{"causes": ["tools_changed"]}"#, true, "tools changed"),
			(
				r#"{"causes": ["system_changed"], "system_char_delta": 1200}"#,
				true,
				"system prompt changed (+1.2k chars)",
			),
			(
				r#"{"causes": ["system_prompt_changed"], "system_char_delta": -340}"#,
				true,
				"system prompt changed (\u{2212}340 chars)",
			),
			(
				r#"{"causes": ["system_prompt_changed"], "system_char_delta": 0}"#,
				true,
				"system prompt changed",
			),
			(
				r#"{"causes": ["system_changed"], "system_char_delta": 1200}"#,
				false,
				"system prompt changed",
			),
			(r#"{"causes": ["model_changed"]}"#, true, "model switched"),
			(r#"{"causes": ["compacted"]}"#, true, "compacted"),
			(
				r#"{"causes": ["ttl_expired_5m"]}"#,
				true,
				"expired after 5m idle",
			),
			(
				r#"{"causes": ["ttl_expired_1h"]}"#,
				true,
				"expired after 1h idle",
			),
			(
				r#"{"causes": ["messages_rewritten"]}"#,
				true,
				"earlier messages changed",
			),
			(
				r#"{"causes": ["likely_server_side"]}"#,
				true,
				"prompt unchanged, likely server side",
			),
			(
				r#"{"causes": ["fast_mode_changed", "effort_changed"]}"#,
				true,
				"fast mode toggled, effort changed",
			),
			(r#"{"causes": ["unknown"]}"#, true, "unexplained miss"),
			(r#"{"causes": [""]}"#, true, "unexplained miss"),
			(
				r#"{"causes": ["brand_new_reason"]}"#,
				true,
				"brand new reason",
			),
		] {
			assert_eq!(
				describe_miss(Some(&cause(json)), details),
				want,
				"{json} details={details}"
			);
		}
		assert_eq!(describe_miss(None, true), "unexplained miss");
	}

	fn render(segment_json: &str, input_json: &str) -> Option<String> {
		let input = InputData::from_reader(input_json.as_bytes()).unwrap();
		let segment: SegmentConfig = serde_json::from_str(segment_json).unwrap();
		let ctx = RenderContext {
			path_format: None,
			input: &input,
			usage: None,
			credits: None,
			usage_stale: false,
			git: None,
			five_threshold: 70.0.into(),
			seven_threshold: 100.0.into(),
			divider: crate::constants::DIVIDER,
			nerd_font: false,
			account: None,
			task: None,
		};
		render_segment(&segment, &ctx)
			.map(|s| String::from_utf8(strip_ansi_escapes::strip(s)).unwrap())
	}

	/// A prompt cache as Claude Code sends it, plus the plugin's `miss_times` when `times` is given.
	fn cache_json(
		misses: u64,
		last_miss_ago: Option<i64>,
		times_ago: Option<&[i64]>,
		cause: &str,
	) -> String {
		let now = Utc::now().timestamp();
		let last = last_miss_ago.map_or_else(|| "null".to_owned(), |ago| (now - ago).to_string());
		let times = times_ago.map_or_else(String::new, |ago| {
			let list: Vec<String> = ago.iter().map(|a| (now - a).to_string()).collect();
			format!(r#", "miss_times": [{}]"#, list.join(", "))
		});
		format!(
			r#"{{"prompt_cache": {{"warm": true, "caching_observed": true, "misses": {misses},
				"last_miss_at": {last}, "last_miss_cause": {cause}{times}}}}}"#
		)
	}

	#[test]
	fn misses_within_counts_only_the_window() {
		let now = DateTime::from_timestamp(1_791_280_000, 0).unwrap();
		let times = [3600, 1800, 600, 60].map(|ago| now - TimeDelta::seconds(ago));
		assert_eq!(misses_within(&times, Duration::from_secs(1800), now), 3);
		assert_eq!(misses_within(&times, Duration::from_secs(90), now), 1);
		assert_eq!(misses_within(&[], Duration::from_secs(1800), now), 0);
	}

	#[test]
	fn is_older_than_keeps_a_miss_at_the_edge_of_the_window() {
		let now = DateTime::from_timestamp(1_791_280_000, 0).unwrap();
		let window = Duration::from_secs(1800);
		assert!(!is_older_than(now - TimeDelta::seconds(1800), window, now));
		assert!(is_older_than(now - TimeDelta::seconds(1801), window, now));
		assert!(
			!is_older_than(now + TimeDelta::seconds(5), window, now),
			"a clock behind Claude Code's"
		);
	}

	#[test]
	fn cache_misses_counts_within_the_window() {
		let tools = r#"{"causes": ["tools_changed"]}"#;
		for (name, segment, input, want) in [
			(
				"default 30m window over the plugin's miss times",
				r#""cache_misses""#,
				cache_json(3, Some(60), Some(&[3600, 600, 60]), tools),
				Some("2 misses in 30m"),
			),
			(
				"one recent miss",
				r#""cache_misses""#,
				cache_json(3, Some(60), Some(&[3600, 2400, 60]), tools),
				Some("1 miss in 30m"),
			),
			(
				"none in the window hides",
				r#""cache_misses""#,
				cache_json(2, Some(3600), Some(&[9000, 3600]), tools),
				None,
			),
			(
				"a wider window",
				r#"{"type": "cache_misses", "within": "2h"}"#,
				cache_json(2, Some(3600), Some(&[9000, 3600]), tools),
				Some("1 miss in 2h"),
			),
			(
				"seconds",
				r#"{"type": "cache_misses", "within": "90s"}"#,
				cache_json(2, Some(30), Some(&[600, 30]), tools),
				Some("1 miss in 90s"),
			),
			(
				"session counts the total",
				r#"{"type": "cache_misses", "within": "session"}"#,
				cache_json(2, Some(3600), Some(&[9000, 3600]), tools),
				Some("2 misses"),
			),
			(
				"null is the whole session",
				r#"{"type": "cache_misses", "within": null}"#,
				cache_json(2, Some(3600), Some(&[9000, 3600]), tools),
				Some("2 misses"),
			),
			(
				"an unparseable window renders as the default",
				r#"{"type": "cache_misses", "within": "soon"}"#,
				cache_json(3, Some(60), Some(&[3600, 600, 60]), tools),
				Some("2 misses in 30m"),
			),
			(
				"Claude Code input has no miss times, so the total shows unwindowed",
				r#""cache_misses""#,
				cache_json(2, Some(3600), None, tools),
				Some("2 misses"),
			),
			(
				"a zero total still shows without miss times",
				r#""cache_misses""#,
				cache_json(0, None, None, "null"),
				Some("0 misses"),
			),
		] {
			assert_eq!(render(segment, &input).as_deref(), want, "{name}");
		}
	}

	#[test]
	fn cache_last_miss_hides_past_the_window() {
		let tools = r#"{"causes": ["tools_changed"]}"#;
		let hour_old = cache_json(1, Some(3600), None, tools);
		assert_eq!(render(r#""cache_last_miss""#, &hour_old), None);
		assert_eq!(
			render(
				r#"{"type": "cache_last_miss", "within": "soon"}"#,
				&hour_old
			),
			None
		);
		for segment in [
			r#"{"type": "cache_last_miss", "within": "2h"}"#,
			r#"{"type": "cache_last_miss", "within": "session"}"#,
			r#"{"type": "cache_last_miss", "within": null}"#,
		] {
			let got = render(segment, &hour_old).unwrap();
			assert!(got.starts_with("tools changed 1h0m"), "{segment}: {got}");
		}
		assert_eq!(
			render(r#""cache_last_miss""#, &cache_json(0, None, None, "null")),
			None
		);
	}

	#[test]
	fn cache_last_miss_details_add_the_deltas() {
		let tools = r#"{"causes": ["tools_changed"], "tools_added": 2, "tools_removed": 1}"#;
		for (segment, cause, want) in [
			(
				r#""cache_last_miss""#,
				tools,
				"tools changed (+2 \u{2212}1) ",
			),
			(
				r#"{"type": "cache_last_miss", "details": false}"#,
				tools,
				"tools changed ",
			),
			(r#""cache_last_miss""#, "null", "unexplained miss "),
			(
				r#""cache_last_miss""#,
				r#"{"causes": []}"#,
				"unexplained miss ",
			),
			(
				r#""cache_last_miss""#,
				r#"{"causes": ["system_changed"], "system_char_delta": 1200}"#,
				"system prompt changed (+1.2k chars) ",
			),
			(
				r#""cache_last_miss""#,
				r#"{"causes": ["ttl_expired_5m"]}"#,
				"expired after 5m idle ",
			),
		] {
			let got = render(segment, &cache_json(1, Some(180), None, cause)).unwrap();
			assert!(
				got.starts_with(want) && got.ends_with(" ago"),
				"{segment} over {cause}: {got}"
			);
		}
	}
}
