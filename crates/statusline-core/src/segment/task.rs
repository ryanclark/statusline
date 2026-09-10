use crate::constants::{CYAN, GRAY, GREEN, RED};
use crate::format::elapsed_since;
use chrono::Utc;
use owo_colors::{DynColors, OwoColorize};

use super::{RenderContext, SegmentConfig, apply_style, paint};

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

	Some(apply_style(name, segment.style()))
}

pub(super) fn task_status(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let status = &ctx.task?.status;
	if status.is_empty() {
		return None;
	}

	let text = match status_color(status) {
		Some(color) => paint(segment, status, color),
		None => status.clone(),
	};

	Some(apply_style(&text, segment.style()))
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

	Some(apply_style(&elapsed, segment.style()))
}

pub(super) fn task_tokens(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let tokens = ctx.task?.token_count?;

	Some(apply_style(&tokens.to_string(), segment.style()))
}
