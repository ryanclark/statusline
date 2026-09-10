use crate::constants::{CYAN, GRAY, GREEN, RED, UP_ARROW};
use crate::format::elapsed_since;
use chrono::Utc;
use owo_colors::{DynColors, OwoColorize};

use super::{Icon, RenderContext, SegmentConfig, apply_style, format_icon, paint};

const AGENT_ICON: Icon = Icon {
	unicode: "\u{2699}",
	nerd: "\u{f013}",
};

const STATE_ICON: Icon = Icon {
	unicode: "\u{25cf}",
	nerd: "\u{f111}",
};

const ELAPSED_ICON: Icon = Icon {
	unicode: "\u{23f1}",
	nerd: "\u{f252}",
};

const TOKENS_ICON: Icon = Icon {
	unicode: UP_ARROW,
	nerd: "\u{f062}",
};

/// Epoch values this large can only be milliseconds; the docs do not name the unit.
const MILLIS_THRESHOLD: i64 = 100_000_000_000;

fn status_color(status: &str) -> Option<DynColors> {
	Some(match status {
		"running" | "in_progress" => CYAN,
		"completed" | "done" => GREEN,
		"failed" | "error" => RED,
		"pending" | "queued" => GRAY,
		_ => return None,
	})
}

pub(super) fn task_name(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let name = &ctx.task?.name;
	if name.is_empty() {
		return None;
	}

	let icon = format_icon(segment, AGENT_ICON, GRAY, ctx.nerd_font);
	Some(apply_style(&format!("{icon}{name}"), segment.style()))
}

pub(super) fn task_status(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let status = &ctx.task?.status;
	if status.is_empty() {
		return None;
	}

	// The dot carries the state colour too, so it reads at a glance even when the word is cut off.
	let color = status_color(status);
	let icon = format_icon(segment, STATE_ICON, color.unwrap_or(GRAY), ctx.nerd_font);
	let text = match color {
		Some(color) => paint(segment, status, color),
		None => status.clone(),
	};

	Some(apply_style(&format!("{icon}{text}"), segment.style()))
}

pub(super) fn task_description(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let description = &ctx.task?.description;
	if description.is_empty() {
		return None;
	}

	let text = if segment.colors() {
		format!("{}", description.dimmed())
	} else {
		description.clone()
	};

	Some(apply_style(&text, segment.style()))
}

pub(super) fn task_elapsed(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let start = ctx.task?.start_time?;
	let start_secs = if start >= MILLIS_THRESHOLD {
		start / 1000
	} else {
		start
	};
	let elapsed = elapsed_since(start_secs, Utc::now())?;

	let icon = format_icon(segment, ELAPSED_ICON, GRAY, ctx.nerd_font);
	Some(apply_style(&format!("{icon}{elapsed}"), segment.style()))
}

pub(super) fn task_tokens(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let tokens = ctx.task?.token_count?;

	let icon = format_icon(segment, TOKENS_ICON, CYAN, ctx.nerd_font);
	Some(apply_style(&format!("{icon}{tokens}"), segment.style()))
}

/// The task's live activity, which Claude Code updates as the agent works.
pub(super) fn task_label(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let label = &ctx.task?.label;
	if label.is_empty() {
		return None;
	}

	Some(apply_style(label, segment.style()))
}
