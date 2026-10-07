//! Live session activity from the plugin's `mod` object, which Claude Code's own input never carries.

use crate::constants::{CYAN, GRAY, GREEN, RED, YELLOW};
use crate::format::{format_duration_ms, format_duration_secs};
use chrono::{DateTime, Utc};

use super::{Icon, RenderContext, SegmentConfig, apply_style, dim, format_icon, paint};

const TOOL_ICON: Icon = Icon {
	unicode: "\u{2699}",
	nerd: "\u{f0ad}",
};

const TURN_ICON: Icon = Icon {
	unicode: "\u{23f1}",
	nerd: "\u{f252}",
};

const PERMISSION_ICON: Icon = Icon {
	unicode: "\u{23f8}",
	nerd: "\u{f04c}",
};

const ERROR_ICON: Icon = Icon {
	unicode: "\u{26a0}",
	nerd: "\u{f071}",
};

const TODO_ICON: Icon = Icon {
	unicode: "\u{2611}",
	nerd: "\u{f0ae}",
};

const AGENTS_ICON: Icon = Icon {
	unicode: "\u{2042}",
	nerd: "\u{f0c0}",
};

const BACKGROUND_ICON: Icon = Icon {
	unicode: "\u{29d7}",
	nerd: "\u{f085}",
};

const COMPACTION_ICON: Icon = Icon {
	unicode: "\u{27f3}",
	nerd: "\u{f01e}",
};

const AUTOCOMPACT_ICON: Icon = Icon {
	unicode: "\u{21a7}",
	nerd: "\u{f066}",
};

const SEPARATOR: &str = " \u{b7} ";

/// Time since the epoch millisecond `at_ms`. A clock that runs slightly behind the plugin's reads as zero rather
/// than hiding the segment.
fn elapsed_since_ms(at_ms: i64, now: DateTime<Utc>) -> String {
	let elapsed = now.timestamp_millis().saturating_sub(at_ms) / 1000;
	format_duration_secs(u64::try_from(elapsed).unwrap_or(0))
}

fn words<const N: usize>(parts: [String; N]) -> String {
	parts
		.into_iter()
		.filter(|p| !p.is_empty())
		.collect::<Vec<_>>()
		.join(" ")
}

pub(super) fn current_tool(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let tools = &ctx.input.mod_info.as_ref()?.tools;
	let first = tools.first()?;

	let icon = format_icon(segment, TOOL_ICON, CYAN, ctx.nerd_font);
	let name = if first.tool.is_empty() {
		String::new()
	} else {
		paint(segment, &first.tool, CYAN)
	};
	let detail = dim(segment, &first.detail);
	let elapsed = first
		.started_at_ms
		.map(|at| dim(segment, &elapsed_since_ms(at, Utc::now())))
		.unwrap_or_default();
	let more = if tools.len() > 1 {
		dim(segment, &format!("+{}", tools.len() - 1))
	} else {
		String::new()
	};

	Some(apply_style(
		&format!("{icon}{}", words([name, detail, elapsed, more])),
		segment.style(),
	))
}

pub(super) fn turn_elapsed(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let turn = ctx.input.mod_info.as_ref()?.turn.as_ref()?;

	if let Some(started) = turn.started_at_ms {
		let icon = format_icon(segment, TURN_ICON, CYAN, ctx.nerd_font);
		return Some(apply_style(
			&format!("{icon}{}", elapsed_since_ms(started, Utc::now())),
			segment.style(),
		));
	}

	// Between turns the last one's length is the useful number, dimmed so it does not read as a live clock.
	let last = turn.last_duration_ms?;
	let icon = format_icon(segment, TURN_ICON, GRAY, ctx.nerd_font);
	let text = dim(segment, &format!("last {}", format_duration_ms(last)));

	Some(apply_style(&format!("{icon}{text}"), segment.style()))
}

pub(super) fn permission_pending(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let permission = ctx.input.mod_info.as_ref()?.permission.as_ref()?;

	// The plugin sees the prompt open but not the answer, so an approved tool keeps this up until it finishes. The
	// wording and neutral colour cover both states.
	let icon = format_icon(segment, PERMISSION_ICON, GRAY, ctx.nerd_font);
	let what = words([permission.tool.clone(), "waiting or running".to_owned()]);
	let waited = permission
		.since_ms
		.map(|at| dim(segment, &elapsed_since_ms(at, Utc::now())))
		.unwrap_or_default();

	Some(apply_style(
		&format!("{icon}{}", words([what, waited])),
		segment.style(),
	))
}

/// Phrases for the `SDKAssistantMessageError` codes and the plugin's own stop reasons. Unknown codes read as words.
fn describe_error(kind: &str) -> String {
	let phrase = match kind {
		"authentication_failed" => "login failed",
		"oauth_org_not_allowed" => "organization not allowed",
		"account_on_hold" => "account on hold",
		"verification_required" => "verification required",
		"billing_error" => "billing error",
		"rate_limit" => "rate limited",
		"overloaded" => "overloaded",
		"invalid_request" => "invalid request",
		"model_not_found" => "model not found",
		"server_error" => "server error",
		"cloud_credential_error" => "cloud credentials failed",
		"max_output_tokens" | "max_tokens" => "hit max tokens",
		"refusal" => "refused",
		"aborted" => "interrupted",
		"unknown" | "error" | "" => "error",
		other => return other.replace('_', " "),
	};

	phrase.to_owned()
}

pub(super) fn last_api_error(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let error = ctx.input.mod_info.as_ref()?.last_error.as_ref()?;

	// An interrupt is the user's own doing, so it is not coloured as a failure.
	let color = if error.kind == "aborted" { GRAY } else { RED };
	let icon = format_icon(segment, ERROR_ICON, color, ctx.nerd_font);
	let mut what = describe_error(&error.kind);
	// A generic code says nothing on its own, so the detail is the message worth reading.
	if what == "error" && !error.detail.is_empty() {
		what.clone_from(&error.detail);
	}
	let what = paint(segment, &what, color);
	let ago = error
		.at_ms
		.map(|at| {
			dim(
				segment,
				&format!("{} ago", elapsed_since_ms(at, Utc::now())),
			)
		})
		.unwrap_or_default();

	Some(apply_style(
		&format!("{icon}{}", words([what, ago])),
		segment.style(),
	))
}

pub(super) fn todo_progress(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let todos = ctx.input.mod_info.as_ref()?.todos.as_ref()?;
	if todos.total == 0 {
		return None;
	}

	let color = if todos.done >= todos.total {
		GREEN
	} else {
		CYAN
	};
	let icon = format_icon(segment, TODO_ICON, color, ctx.nerd_font);
	let count = paint(segment, &format!("{}/{}", todos.done, todos.total), color);
	let active = if todos.active.is_empty() {
		String::new()
	} else {
		dim(segment, &format!("{SEPARATOR}{}", todos.active))
	};

	Some(apply_style(
		&format!("{icon}{count}{active}"),
		segment.style(),
	))
}

pub(super) fn agents(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let agents = ctx.input.mod_info.as_ref()?.agents.as_ref()?;

	let mut parts = Vec::new();
	if agents.running > 0 {
		parts.push(paint(segment, &format!("{} running", agents.running), CYAN));
	}
	if agents.idle > 0 {
		parts.push(dim(segment, &format!("{} idle", agents.idle)));
	}
	if parts.is_empty() {
		return None;
	}

	let icon = format_icon(segment, AGENTS_ICON, CYAN, ctx.nerd_font);
	Some(apply_style(
		&format!("{icon}{}", parts.join(SEPARATOR)),
		segment.style(),
	))
}

/// Claude Code's task type as a word, `remote_agent` reading as `remote agent`.
fn task_kind(kind: &str, count: usize) -> String {
	let word = if kind.is_empty() {
		"task".to_owned()
	} else {
		kind.replace('_', " ")
	};
	if count > 1 { format!("{word}s") } else { word }
}

pub(super) fn background_tasks(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let tasks = &ctx.input.mod_info.as_ref()?.background_tasks;
	let icon = format_icon(segment, BACKGROUND_ICON, CYAN, ctx.nerd_font);

	if let [only] = tasks.as_slice() {
		let kind = paint(segment, &task_kind(&only.kind, 1), CYAN);
		let description = if only.description.is_empty() {
			String::new()
		} else {
			dim(segment, &format!("{SEPARATOR}{}", only.description))
		};
		return Some(apply_style(
			&format!("{icon}{kind}{description}"),
			segment.style(),
		));
	}

	// Grouped in the order each kind first started, so the line keeps its shape as tasks come and go.
	let mut counts: Vec<(&str, usize)> = Vec::new();
	for task in tasks {
		match counts.iter_mut().find(|(kind, _)| *kind == task.kind) {
			Some((_, n)) => *n += 1,
			None => counts.push((&task.kind, 1)),
		}
	}
	if counts.is_empty() {
		return None;
	}
	let parts: Vec<String> = counts
		.into_iter()
		.map(|(kind, n)| paint(segment, &format!("{n} {}", task_kind(kind, n)), CYAN))
		.collect();

	Some(apply_style(
		&format!("{icon}{}", parts.join(SEPARATOR)),
		segment.style(),
	))
}

pub(super) fn compaction(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let compaction = ctx.input.mod_info.as_ref()?.compaction.as_ref()?;
	let now = Utc::now();

	if let Some(since) = compaction.running_since_ms {
		let icon = format_icon(segment, COMPACTION_ICON, YELLOW, ctx.nerd_font);
		let text = words([
			paint(segment, "compacting", YELLOW),
			dim(segment, &elapsed_since_ms(since, now)),
		]);
		return Some(apply_style(&format!("{icon}{text}"), segment.style()));
	}
	if compaction.count == 0 {
		return None;
	}

	let icon = format_icon(segment, COMPACTION_ICON, GRAY, ctx.nerd_font);
	let mut parts = vec![if compaction.count > 1 {
		format!("compacted \u{d7}{}", compaction.count)
	} else {
		"compacted".to_owned()
	}];
	if let Some(at) = compaction.last_at_ms {
		parts.push(format!("{} ago", elapsed_since_ms(at, now)));
	}
	if let (Some(before), Some(after)) = (compaction.tokens_before, compaction.tokens_after) {
		parts.push(format!("{before}\u{2192}{after}"));
	}

	Some(apply_style(
		&format!("{icon}{}", dim(segment, &parts.join(SEPARATOR))),
		segment.style(),
	))
}

pub(super) fn autocompact_headroom(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
) -> Option<String> {
	let autocompact = ctx.input.mod_info.as_ref()?.autocompact.as_ref()?;

	let (color, text) = if autocompact.enabled {
		(CYAN, format!("compact in {}", autocompact.headroom_tokens?))
	} else {
		(GRAY, dim(segment, "autocompact off"))
	};
	let icon = format_icon(segment, AUTOCOMPACT_ICON, color, ctx.nerd_font);

	Some(apply_style(&format!("{icon}{text}"), segment.style()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::input::InputData;
	use crate::segment::{SegmentType, render_segment};

	const ALL: [SegmentType; 9] = [
		SegmentType::CurrentTool,
		SegmentType::TurnElapsed,
		SegmentType::PermissionPending,
		SegmentType::LastApiError,
		SegmentType::TodoProgress,
		SegmentType::Agents,
		SegmentType::BackgroundTasks,
		SegmentType::Compaction,
		SegmentType::AutocompactHeadroom,
	];

	fn render_with(segment: &SegmentConfig, input_json: &str, nerd_font: bool) -> Option<String> {
		let input = InputData::from_reader(input_json.as_bytes()).unwrap();
		let ctx = RenderContext {
			input: &input,
			usage: None,
			credits: None,
			usage_stale: false,
			git: None,
			five_threshold: 70.0.into(),
			seven_threshold: 100.0.into(),
			divider: crate::constants::DIVIDER,
			nerd_font,
			account: None,
			task: None,
		};
		render_segment(segment, &ctx)
	}

	fn render(ty: SegmentType, body: &str) -> Option<String> {
		render_with(&SegmentConfig::Simple(ty), &mod_json(body), false)
			.map(|s| String::from_utf8(strip_ansi_escapes::strip(s)).unwrap())
	}

	/// The plugin's JSON with `{now-N}` placeholders swapped for epoch milliseconds N seconds ago, so the rendered
	/// elapsed times are stable.
	fn mod_json(body: &str) -> String {
		let now = Utc::now().timestamp_millis();
		let mut out = body.to_owned();
		while let Some(start) = out.find("{now-") {
			let end = start + out[start..].find('}').unwrap();
			let secs: i64 = out[start + 5..end].parse().unwrap();
			out.replace_range(start..=end, &(now - secs * 1000).to_string());
		}
		format!(r#"{{"mod": {out}}}"#)
	}

	#[test]
	fn every_activity_segment_is_hidden_on_the_cli_path() {
		let claude_code_input = r#"{"cwd": "/x", "model": {"id": "claude-opus-5-5", "display_name": "Opus 5.5"},
			"prompt_cache": {"warm": true, "caching_observed": true, "misses": 1}}"#;
		for input in [claude_code_input, r#"{"mod": null}"#, r#"{"mod": {}}"#] {
			for ty in ALL {
				let seg = SegmentConfig::Simple(ty.clone());
				assert_eq!(render_with(&seg, input, false), None, "{ty:?} over {input}");
			}
		}
	}

	#[test]
	fn a_null_sub_object_hides_its_segment() {
		for (ty, body) in [
			(SegmentType::CurrentTool, r#"{"tools": null}"#),
			(SegmentType::TurnElapsed, r#"{"turn": null}"#),
			(SegmentType::PermissionPending, r#"{"permission": null}"#),
			(SegmentType::LastApiError, r#"{"last_error": null}"#),
			(SegmentType::TodoProgress, r#"{"todos": null}"#),
			(SegmentType::Agents, r#"{"agents": null}"#),
			(
				SegmentType::BackgroundTasks,
				r#"{"background_tasks": null}"#,
			),
			(SegmentType::Compaction, r#"{"compaction": null}"#),
			(SegmentType::AutocompactHeadroom, r#"{"autocompact": null}"#),
		] {
			assert_eq!(render(ty, body), None, "{body}");
		}
	}

	#[test]
	fn elapsed_since_ms_reads_a_clock_behind_the_plugin_as_zero() {
		let now = DateTime::from_timestamp(1_791_280_000, 0).unwrap();
		assert_eq!(elapsed_since_ms(1_791_279_897_500, now), "1m42s");
		assert_eq!(elapsed_since_ms(1_791_280_002_000, now), "0s");
	}

	#[test]
	fn current_tool_shows_the_oldest_call_and_counts_the_rest() {
		assert_eq!(
			render(
				SegmentType::CurrentTool,
				r#"{"tools": [{"tool": "Bash", "detail": "cargo test", "started_at_ms": {now-12}},
					{"tool": "Read", "detail": "a.rs", "started_at_ms": {now-2}},
					{"tool": "Grep", "detail": null, "started_at_ms": {now-1}}]}"#
			)
			.as_deref(),
			Some("\u{2699} Bash cargo test 12s +2")
		);
		assert_eq!(
			render(
				SegmentType::CurrentTool,
				r#"{"tools": [{"tool": "Task", "detail": null, "started_at_ms": null}]}"#
			)
			.as_deref(),
			Some("\u{2699} Task")
		);
		assert_eq!(render(SegmentType::CurrentTool, r#"{"tools": []}"#), None);
	}

	#[test]
	fn turn_elapsed_dims_the_last_turn_between_turns() {
		assert_eq!(
			render(
				SegmentType::TurnElapsed,
				r#"{"turn": {"started_at_ms": {now-102}, "last_duration_ms": 130000}}"#
			)
			.as_deref(),
			Some("\u{23f1} 1m42s")
		);
		assert_eq!(
			render(
				SegmentType::TurnElapsed,
				r#"{"turn": {"started_at_ms": null, "last_duration_ms": 130000}}"#
			)
			.as_deref(),
			Some("\u{23f1} last 2m10s")
		);
		assert_eq!(
			render(
				SegmentType::TurnElapsed,
				r#"{"turn": {"started_at_ms": null, "last_duration_ms": null}}"#
			),
			None
		);
	}

	#[test]
	fn permission_pending_shows_the_tool_and_the_wait() {
		assert_eq!(
			render(
				SegmentType::PermissionPending,
				r#"{"permission": {"tool": "Bash", "since_ms": {now-45}}}"#
			)
			.as_deref(),
			Some("\u{23f8} Bash waiting or running 45s")
		);
		assert_eq!(
			render(
				SegmentType::PermissionPending,
				r#"{"permission": {"tool": null, "since_ms": null}}"#
			)
			.as_deref(),
			Some("\u{23f8} waiting or running")
		);
	}

	#[test]
	fn last_api_error_reads_the_code_as_words() {
		for (body, want) in [
			(
				r#"{"last_error": {"kind": "overloaded", "detail": "529 Overloaded", "at_ms": {now-120}}}"#,
				"\u{26a0} overloaded 2m ago",
			),
			(
				r#"{"last_error": {"kind": "rate_limit", "detail": null, "at_ms": {now-30}}}"#,
				"\u{26a0} rate limited 30s ago",
			),
			(
				r#"{"last_error": {"kind": "max_tokens", "detail": null, "at_ms": null}}"#,
				"\u{26a0} hit max tokens",
			),
			(
				r#"{"last_error": {"kind": "aborted", "detail": null, "at_ms": {now-3}}}"#,
				"\u{26a0} interrupted 3s ago",
			),
			(
				r#"{"last_error": {"kind": "quota_exploded", "detail": null, "at_ms": null}}"#,
				"\u{26a0} quota exploded",
			),
		] {
			assert_eq!(
				render(SegmentType::LastApiError, body).as_deref(),
				Some(want),
				"{body}"
			);
		}
	}

	#[test]
	fn last_api_error_falls_back_to_the_detail_for_a_generic_code() {
		assert_eq!(
			render(
				SegmentType::LastApiError,
				r#"{"last_error": {"kind": "error", "detail": "socket hang up", "at_ms": null}}"#
			)
			.as_deref(),
			Some("\u{26a0} socket hang up")
		);
	}

	#[test]
	fn todo_progress_shows_the_count_and_the_active_item() {
		assert_eq!(
			render(
				SegmentType::TodoProgress,
				r#"{"todos": {"done": 3, "total": 7, "active": "Running tests"}}"#
			)
			.as_deref(),
			Some("\u{2611} 3/7 \u{b7} Running tests")
		);
		assert_eq!(
			render(
				SegmentType::TodoProgress,
				r#"{"todos": {"done": 7, "total": 7, "active": null}}"#
			)
			.as_deref(),
			Some("\u{2611} 7/7")
		);
		assert_eq!(
			render(
				SegmentType::TodoProgress,
				r#"{"todos": {"done": 0, "total": 0, "active": null}}"#
			),
			None
		);
	}

	#[test]
	fn agents_hide_when_none_run_or_idle() {
		for (body, want) in [
			(
				r#"{"agents": {"running": 3, "idle": 1}}"#,
				Some("\u{2042} 3 running \u{b7} 1 idle"),
			),
			(
				r#"{"agents": {"running": 2, "idle": 0}}"#,
				Some("\u{2042} 2 running"),
			),
			(
				r#"{"agents": {"running": 0, "idle": 4}}"#,
				Some("\u{2042} 4 idle"),
			),
			(r#"{"agents": {"running": 0, "idle": 0}}"#, None),
		] {
			assert_eq!(render(SegmentType::Agents, body).as_deref(), want, "{body}");
		}
	}

	#[test]
	fn background_tasks_describe_one_and_count_several_by_kind() {
		for (body, want) in [
			(
				r#"{"background_tasks": [{"type": "shell", "description": "npm run dev"}]}"#,
				Some("\u{29d7} shell \u{b7} npm run dev"),
			),
			(
				r#"{"background_tasks": [{"type": "workflow", "description": null}]}"#,
				Some("\u{29d7} workflow"),
			),
			(
				r#"{"background_tasks": [{"type": "shell", "description": "npm run dev"},
					{"type": "monitor", "description": "CI on #42"},
					{"type": "shell", "description": "tail -f log"}]}"#,
				Some("\u{29d7} 2 shells \u{b7} 1 monitor"),
			),
			(
				r#"{"background_tasks": [{"type": "remote_agent", "description": "Fix the flake"},
					{"type": "remote_agent", "description": "Bump deps"}, {"type": null, "description": null}]}"#,
				Some("\u{29d7} 2 remote agents \u{b7} 1 task"),
			),
			(r#"{"background_tasks": []}"#, None),
		] {
			assert_eq!(
				render(SegmentType::BackgroundTasks, body).as_deref(),
				want,
				"{body}"
			);
		}
	}

	#[test]
	fn compaction_shows_running_then_the_history() {
		assert_eq!(
			render(
				SegmentType::Compaction,
				r#"{"compaction": {"count": 1, "last_at_ms": null, "running_since_ms": {now-18}}}"#
			)
			.as_deref(),
			Some("\u{27f3} compacting 18s")
		);
		assert_eq!(
			render(
				SegmentType::Compaction,
				r#"{"compaction": {"count": 2, "last_at_ms": {now-840}, "tokens_before": 182000,
					"tokens_after": 21000, "running_since_ms": null}}"#
			)
			.as_deref(),
			Some("\u{27f3} compacted \u{d7}2 \u{b7} 14m ago \u{b7} 182.0k\u{2192}21.0k")
		);
		assert_eq!(
			render(
				SegmentType::Compaction,
				r#"{"compaction": {"count": 1, "last_at_ms": null, "tokens_before": null,
					"tokens_after": null, "running_since_ms": null}}"#
			)
			.as_deref(),
			Some("\u{27f3} compacted")
		);
		assert_eq!(
			render(
				SegmentType::Compaction,
				r#"{"compaction": {"count": 0, "running_since_ms": null}}"#
			),
			None
		);
	}

	#[test]
	fn autocompact_headroom_shows_the_tokens_left_or_that_it_is_off() {
		assert_eq!(
			render(
				SegmentType::AutocompactHeadroom,
				r#"{"autocompact": {"enabled": true, "headroom_tokens": 38000}}"#
			)
			.as_deref(),
			Some("\u{21a7} compact in 38.0k")
		);
		assert_eq!(
			render(
				SegmentType::AutocompactHeadroom,
				r#"{"autocompact": {"enabled": false, "headroom_tokens": null}}"#
			)
			.as_deref(),
			Some("\u{21a7} autocompact off")
		);
		assert_eq!(
			render(
				SegmentType::AutocompactHeadroom,
				r#"{"autocompact": {"enabled": true, "headroom_tokens": null}}"#
			),
			None
		);
	}

	#[test]
	fn activity_segments_follow_the_icon_label_colour_and_style_options() {
		let input = mod_json(r#"{"agents": {"running": 3, "idle": 1}}"#);
		let seg = |json: &str| -> SegmentConfig { serde_json::from_str(json).unwrap() };

		let plain = render_with(
			&seg(r#"{"type": "agents", "colors": false}"#),
			&input,
			false,
		);
		assert_eq!(
			plain.as_deref(),
			Some("\u{2042} 3 running \u{b7} 1 idle"),
			"no escapes at all"
		);

		let no_icon = render_with(
			&seg(r#"{"type": "agents", "colors": false, "icon": false}"#),
			&input,
			false,
		);
		assert_eq!(no_icon.as_deref(), Some("3 running \u{b7} 1 idle"));

		let label = render_with(
			&seg(r#"{"type": "agents", "colors": false, "label": "bg:"}"#),
			&input,
			false,
		);
		assert_eq!(label.as_deref(), Some("bg: 3 running \u{b7} 1 idle"));

		let nerd = render_with(&seg(r#"{"type": "agents", "colors": false}"#), &input, true);
		assert_eq!(nerd.as_deref(), Some("\u{f0c0} 3 running \u{b7} 1 idle"));

		let coloured =
			render_with(&SegmentConfig::Simple(SegmentType::Agents), &input, false).unwrap();
		assert!(
			coloured.contains('\u{1b}'),
			"colours on by default: {coloured:?}"
		);

		let bold = render_with(
			&seg(r#"{"type": "agents", "colors": false, "style": "bold"}"#),
			&input,
			false,
		)
		.unwrap();
		assert!(bold.starts_with("\u{1b}[1m"), "{bold:?}");
	}

	#[test]
	fn every_activity_segment_has_its_own_icon() {
		let icons = [
			TOOL_ICON,
			TURN_ICON,
			PERMISSION_ICON,
			ERROR_ICON,
			TODO_ICON,
			AGENTS_ICON,
			BACKGROUND_ICON,
			COMPACTION_ICON,
			AUTOCOMPACT_ICON,
		];
		for (i, a) in icons.iter().enumerate() {
			for b in &icons[i + 1..] {
				assert_ne!(a.unicode, b.unicode);
				assert_ne!(a.nerd, b.nerd);
			}
		}
	}
}
