use crate::constants::{GREEN, RED};
use crate::format::format_duration_secs;
use owo_colors::OwoColorize;

use super::{RenderContext, SegmentConfig, apply_style};

fn format_duration_ms(ms: u64) -> String {
	format_duration_secs(ms / 1000)
}

fn format_cost_usd(cost: f64) -> String {
	if cost < 0.01 {
		format!("${cost:.4}")
	} else {
		format!("${cost:.2}")
	}
}

pub(super) fn cost(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let c = ctx.input.cost.total_cost_usd;
	if c == 0.0 {
		return None;
	}

	let text = format_cost_usd(c);

	Some(apply_style(&text, segment.style()))
}

pub(super) fn cost_rate(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let c = ctx.input.cost.total_cost_usd;
	let duration_ms = ctx.input.cost.total_duration_ms;
	if c == 0.0 || duration_ms == 0 {
		return None;
	}

	let cost_per_min = c / (duration_ms as f64 / 60_000.0);
	let text = format!("${cost_per_min:.2}/m");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn duration(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let ms = ctx.input.cost.total_duration_ms;
	if ms == 0 {
		return None;
	}

	let text = format_duration_ms(ms);

	Some(apply_style(&text, segment.style()))
}

pub(super) fn api_duration(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let ms = ctx.input.cost.total_api_duration_ms;
	if ms == 0 {
		return None;
	}

	let text = format_duration_ms(ms);

	Some(apply_style(&text, segment.style()))
}

pub(super) fn tokens_per_second(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let api_ms = ctx.input.cost.total_api_duration_ms;
	let output = ctx.input.context_window.total_output_tokens.as_u64();
	if api_ms == 0 || output == 0 {
		return None;
	}

	let tps = (output as f64) / (api_ms as f64 / 1000.0);
	let text = format!("{tps:.0}t/s");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn lines_added(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let lines = ctx.input.cost.total_lines_added;
	if lines == 0 {
		return None;
	}

	let text = if segment.colors() {
		format!("{}{lines}", "+".color(GREEN))
	} else {
		format!("+{lines}")
	};

	Some(apply_style(&text, segment.style()))
}

pub(super) fn lines_removed(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let lines = ctx.input.cost.total_lines_removed;
	if lines == 0 {
		return None;
	}

	let text = if segment.colors() {
		format!("{}{lines}", "-".color(RED))
	} else {
		format!("-{lines}")
	};

	Some(apply_style(&text, segment.style()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn format_duration_ms_seconds() {
		assert_eq!(format_duration_ms(5000), "5s");
	}

	#[test]
	fn format_duration_ms_minutes() {
		assert_eq!(format_duration_ms(125000), "2m5s");
	}

	#[test]
	fn format_duration_ms_hours() {
		assert_eq!(format_duration_ms(3_661_000), "1h1m");
	}

	#[test]
	fn format_cost_small() {
		assert_eq!(format_cost_usd(0.001), "$0.0010");
	}

	#[test]
	fn format_cost_normal() {
		assert_eq!(format_cost_usd(1.23), "$1.23");
	}
}
