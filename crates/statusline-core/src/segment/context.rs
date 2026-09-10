use crate::constants::{CYAN, DOWN_ARROW, GRAY, GREEN, ORANGE, PURPLE, UP_ARROW, YELLOW};
use crate::format::{ColoredPercentage, Percentage, countdown_to, elapsed_since};
use crate::input::PromptCache;
use chrono::Utc;
use owo_colors::OwoColorize;

use super::{Icon, RenderContext, SegmentConfig, apply_style, format_icon, paint};

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
	if segment.colors() {
		format!(" {}", text.dimmed())
	} else {
		format!(" {text}")
	}
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
		.and_then(|at| countdown_to(at, Utc::now()))
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
	let misses = observed_cache(ctx)?.misses;
	let noun = if misses == 1 { "miss" } else { "misses" };
	let color = if misses == 0 { GRAY } else { YELLOW };
	let text = paint(segment, &format!("{misses} {noun}"), color);

	Some(apply_style(&text, segment.style()))
}

pub(super) fn cache_last_miss(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let cache = observed_cache(ctx)?;
	let cause = cache
		.last_miss_cause
		.as_ref()
		.map(|c| c.causes.join("+"))
		.filter(|c| !c.is_empty());
	if cause.is_none() && cache.last_miss_at.is_none() {
		return None;
	}

	// Claude Code reports a miss it could not diagnose with a null cause; it is still a miss.
	let label = paint(segment, cause.as_deref().unwrap_or("miss"), ORANGE);
	let age = cache
		.last_miss_at
		.and_then(|at| elapsed_since(at, Utc::now()))
		.map(|ago| dim_suffix(segment, &format!("{ago} ago")))
		.unwrap_or_default();

	Some(apply_style(&format!("{label}{age}"), segment.style()))
}
