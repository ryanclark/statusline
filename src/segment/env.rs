use owo_colors::OwoColorize;

use crate::constants::GRAY;

use super::{RenderContext, SegmentConfig, apply_style};

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
	if ctx.input.model.display_name.is_empty() {
		return None;
	}

	let raw = ctx.input.model.display_name.replace("1M context", "1M");
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

pub(super) fn worktree(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	if ctx.input.worktree.name.is_empty() {
		return None;
	}

	Some(apply_style(&ctx.input.worktree.name, segment.style()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn shorten_path_outside_home() {
		assert_eq!(shorten_path("/tmp/test"), "/tmp/test");
	}
}
