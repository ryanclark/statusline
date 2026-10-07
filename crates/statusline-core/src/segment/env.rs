use owo_colors::{DynColors, OwoColorize};
use std::borrow::Cow;

use crate::constants::{CYAN, GRAY, ORANGE, PURPLE, RED, YELLOW};

use super::{RenderContext, SegmentConfig, apply_style, paint};

fn shorten_path(path: &str) -> String {
	let Ok(home) = crate::util::home_dir() else {
		return path.to_owned();
	};
	let Some(rest) = home.to_str().and_then(|h| path.strip_prefix(h)) else {
		return path.to_owned();
	};

	format!("~{rest}")
}

pub(super) fn divider(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let div = ctx.divider;

	if segment.colors() {
		Some(format!("{}", div.color(GRAY)))
	} else {
		Some(div.to_owned())
	}
}

pub(super) fn newline(_segment: &SegmentConfig, _ctx: &RenderContext<'_>) -> Option<String> {
	Some("\n".to_owned())
}

pub(super) fn cwd(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.cwd.is_empty() {
		return None;
	}

	let text = shorten_path(&ctx.input.cwd);

	Some(apply_style(&text, segment.style()))
}

pub(super) fn project_dir(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.workspace.project_dir.is_empty() {
		return None;
	}

	let text = shorten_path(&ctx.input.workspace.project_dir);

	Some(apply_style(&text, segment.style()))
}

pub(super) fn model(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let name: Cow<'_, str> = if ctx.input.model.display_name.is_empty() {
		display_name_from_id(&ctx.input.model.id)?.into()
	} else {
		ctx.input.model.display_name.as_str().into()
	};

	let raw = name.replace("1M context", "1M");
	let text = if segment.colors() {
		if let Some((base, suffix)) = raw.split_once(" (") {
			format!("{base} {}", format!("({suffix}").dimmed())
		} else {
			raw
		}
	} else {
		raw
	};

	Some(apply_style(&text, segment.style()))
}

/// Builds Claude Code's label for a model from its id, for inputs that carry only the id. Handles
/// `claude-<family>-<major>-<minor>[-date]`, the older `claude-<major>-<minor>-<family>`, and wrappers like Bedrock's
/// `us.anthropic.` and `-v1:0`.
fn display_name_from_id(id: &str) -> Option<String> {
	let (id, long_context) = match id.strip_suffix("[1m]") {
		Some(base) => (base, true),
		None => (id, false),
	};
	let id = id.split_once('@').map_or(id, |(base, _)| base);
	let rest = &id[id.find("claude-")? + "claude-".len()..];
	let rest = rest.split_once("-v").map_or(rest, |(base, _)| base);

	let mut family = None;
	let mut version = Vec::new();
	for part in rest.split('-').filter(|part| !part.is_empty()) {
		if part.chars().all(|c| c.is_ascii_digit()) {
			// Dates are eight digits, version parts one or two.
			if part.len() <= 2 {
				version.push(part);
			}
		} else if family.is_none() && part.chars().all(|c| c.is_ascii_alphabetic()) {
			family = Some(part);
		}
	}

	let family = family?;
	if version.is_empty() {
		return None;
	}
	let mut chars = family.chars();
	let first = chars.next()?.to_ascii_uppercase();
	let mut name = format!("{first}{} {}", chars.as_str(), version.join("."));
	if long_context {
		name.push_str(" (1M context)");
	}

	Some(name)
}

pub(super) fn model_id(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.model.id.is_empty() {
		return None;
	}

	Some(apply_style(&ctx.input.model.id, segment.style()))
}

pub(super) fn version(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.version.is_empty() {
		return None;
	}

	Some(apply_style(&ctx.input.version, segment.style()))
}

pub(super) fn session_id(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.session_id.is_empty() {
		return None;
	}

	Some(apply_style(&ctx.input.session_id, segment.style()))
}

pub(super) fn vim_mode(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.vim.mode.is_empty() {
		return None;
	}

	Some(apply_style(&ctx.input.vim.mode, segment.style()))
}

pub(super) fn agent_name(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.agent.name.is_empty() {
		return None;
	}

	Some(apply_style(&ctx.input.agent.name, segment.style()))
}

pub(super) fn session_name(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.session_name.is_empty() {
		return None;
	}

	Some(apply_style(&ctx.input.session_name, segment.style()))
}

pub(super) fn worktree(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	// A worktree session names itself; outside one, any linked git worktree still has a name.
	let name = [&ctx.input.worktree.name, &ctx.input.workspace.git_worktree]
		.into_iter()
		.find(|name| !name.is_empty())?;

	Some(apply_style(name, segment.style()))
}

/// Effort colours climb with the level so a high setting stands out the way a high context
/// percentage does; unknown levels stay uncoloured.
fn effort_color(level: &str) -> Option<DynColors> {
	Some(match level {
		"low" => GRAY,
		"medium" => CYAN,
		"high" => YELLOW,
		"xhigh" => ORANGE,
		"max" => RED,
		_ => return None,
	})
}

pub(super) fn effort(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let level = ctx.input.effort.level.as_str();
	if level.is_empty() {
		return None;
	}

	let text = match effort_color(level) {
		Some(color) => paint(segment, level, color),
		None => level.to_owned(),
	};

	Some(apply_style(&text, segment.style()))
}

fn flag(segment: &SegmentConfig, on: bool, text: &str, color: DynColors) -> Option<String> {
	if !on {
		return None;
	}

	Some(apply_style(&paint(segment, text, color), segment.style()))
}

pub(super) fn thinking(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	flag(segment, ctx.input.thinking.enabled, "thinking", PURPLE)
}

pub(super) fn fast_mode(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	flag(segment, ctx.input.fast_mode, "fast", YELLOW)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn shorten_path_outside_home() {
		assert_eq!(shorten_path("/tmp/test"), "/tmp/test");
	}

	#[test]
	fn display_name_is_derived_from_model_ids() {
		for (id, want) in [
			("claude-opus-5-5", Some("Opus 5.5")),
			("claude-fable-5-1", Some("Fable 5.1")),
			("claude-haiku-4-5-20251001", Some("Haiku 4.5")),
			("claude-opus-4-1", Some("Opus 4.1")),
			("claude-sonnet-4-20250514", Some("Sonnet 4")),
			("claude-3-5-sonnet-20241022", Some("Sonnet 3.5")),
			("claude-opus-5-5[1m]", Some("Opus 5.5 (1M context)")),
			("us.anthropic.claude-sonnet-5-5-v1:0", Some("Sonnet 5.5")),
			("claude-opus-5-5@20260101", Some("Opus 5.5")),
			("claude-opus-5-5-", Some("Opus 5.5")),
			("gpt-5", None),
			("", None),
		] {
			assert_eq!(display_name_from_id(id).as_deref(), want, "{id}");
		}
	}
}
