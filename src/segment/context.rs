use crate::constants::{CYAN, DOWN_ARROW, ORANGE, PURPLE, UP_ARROW};
use crate::format::{ColoredPercentage, Percentage};
use owo_colors::OwoColorize;

use super::{apply_style, format_icon, Icon, RenderContext, SegmentConfig};

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
