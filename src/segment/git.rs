use crate::constants::{GREEN, RED, YELLOW};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};

use super::{Icon, RenderContext, SegmentConfig, apply_style, format_icon};

const GIT_CACHE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct GitCache {
	pub(crate) branch: Option<String>,
	pub(crate) dirty: bool,
	pub(crate) ahead: u64,
	pub(crate) behind: u64,
	pub(crate) stash_count: u64,
}

fn git_cache_path(cwd: &str) -> Option<std::path::PathBuf> {
	use std::hash::{Hash, Hasher};

	let mut hasher = std::collections::hash_map::DefaultHasher::new();

	cwd.hash(&mut hasher);

	let hash = hasher.finish();
	let dir = crate::util::app_data_dir().ok()?.join("cache");

	Some(dir.join(format!("{hash:x}")))
}

fn is_cache_fresh(path: &std::path::Path) -> bool {
	std::fs::metadata(path)
		.and_then(|m| m.modified())
		.map(|t| t.elapsed().unwrap_or(std::time::Duration::MAX) < GIT_CACHE_MAX_AGE)
		.unwrap_or(false)
}

pub(crate) fn load_git_cache(cwd: &str) -> Option<GitCache> {
	if cwd.is_empty() {
		return None;
	}

	let path = git_cache_path(cwd)?;

	if is_cache_fresh(&path) {
		let cached = std::fs::read_to_string(&path)
			.ok()
			.and_then(|data| serde_json::from_str::<GitCache>(&data).ok());

		if let Some(cache) = cached {
			return Some(cache);
		}
	}

	let cache = refresh_git_cache(cwd);

	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	if let Ok(json) = serde_json::to_string(&cache) {
		let _ = std::fs::write(&path, json);
	}

	Some(cache)
}

fn refresh_git_cache(cwd: &str) -> GitCache {
	let (branch, dirty) = fetch_git_info(cwd)
		.map(|info| (Some(info.branch), info.dirty))
		.unwrap_or_default();

	let (ahead, behind) = fetch_git_ahead_behind(cwd).unwrap_or((0, 0));
	let stash_count = fetch_git_stash_count(cwd).unwrap_or(0);

	GitCache {
		branch,
		dirty,
		ahead,
		behind,
		stash_count,
	}
}

struct GitInfo {
	branch: String,
	dirty: bool,
}

fn git_output(cwd: &str, args: &[&str]) -> Option<String> {
	let dir = if cwd.is_empty() { "." } else { cwd };
	let output = std::process::Command::new("git")
		.arg("--no-optional-locks")
		.args(args)
		.current_dir(dir)
		.stderr(std::process::Stdio::null())
		.output()
		.ok()?;

	if !output.status.success() {
		return None;
	}

	String::from_utf8(output.stdout).ok()
}

fn fetch_git_info(cwd: &str) -> Option<GitInfo> {
	let branch = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;
	let branch = branch.trim().to_owned();
	if branch.is_empty() {
		return None;
	}

	let dirty = git_output(cwd, &["status", "--porcelain", "--ignore-submodules=dirty"])
		.is_some_and(|o| !o.is_empty());

	Some(GitInfo { branch, dirty })
}

fn fetch_git_ahead_behind(cwd: &str) -> Option<(u64, u64)> {
	let text = git_output(
		cwd,
		&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
	)?;
	let mut parts = text.trim().split('\t');
	let ahead = parts.next()?.parse().ok()?;
	let behind = parts.next()?.parse().ok()?;

	Some((ahead, behind))
}

fn fetch_git_stash_count(cwd: &str) -> Option<u64> {
	let text = git_output(cwd, &["stash", "list"])?;
	let count = text.lines().count() as u64;

	Some(count)
}

pub(super) fn git_branch(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let git = ctx.git?;
	let branch = git.branch.as_deref()?;
	let dirty_suffix = if git.dirty {
		if let Some(indicator) = segment.dirty_indicator() {
			if segment.colors() {
				if let Some(color) = segment.dirty_color() {
					format!("{}", indicator.color(color))
				} else {
					format!("{}", indicator.red())
				}
			} else {
				indicator.to_owned()
			}
		} else {
			String::new()
		}
	} else {
		String::new()
	};
	let text = format!("{branch}{dirty_suffix}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn git_ahead_behind(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let git = ctx.git?;
	let (ahead, behind) = (git.ahead, git.behind);
	if ahead == 0 && behind == 0 {
		return None;
	}

	let up = if ctx.nerd_font {
		"\u{f062}"
	} else {
		"\u{2191}"
	};
	let down = if ctx.nerd_font {
		"\u{f063}"
	} else {
		"\u{2193}"
	};
	let mut parts = Vec::new();

	if ahead > 0 {
		if segment.colors() {
			parts.push(format!("{}", format_args!("{up} {ahead}").color(GREEN)));
		} else {
			parts.push(format!("{up} {ahead}"));
		}
	}

	if behind > 0 {
		if segment.colors() {
			parts.push(format!("{}", format_args!("{down} {behind}").color(RED)));
		} else {
			parts.push(format!("{down} {behind}"));
		}
	}

	let text = parts.join(" ");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn git_stash(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let count = ctx.git?.stash_count;
	if count == 0 {
		return None;
	}

	let icon = format_icon(
		segment,
		Icon {
			unicode: "\u{2691}",
			nerd: "\u{f1b2}",
		},
		YELLOW,
		ctx.nerd_font,
	);
	let text = format!("{icon}{count}");

	Some(apply_style(&text, segment.style()))
}
