//! Claude Code's subagent status line: the agent panel pipes every visible task as one JSON object
//! and takes back one `{"id", "content"}` line per row to override.

use crate::context_window::{ContextWindow, CurrentUsage};
use crate::format::{Percentage, Tokens};
use crate::input::{AgentInfo, EffortInfo, InputData, ModelInfo};
use crate::segment::{RenderContext, SegmentConfig, SegmentLine, SegmentType, align_rows};
use crate::text::truncate_visible;
use crate::util::null_as_default;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
pub struct SubagentInput {
	/// Usable row width in the agent panel.
	#[serde(default)]
	pub columns: Option<usize>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub tasks: Vec<Task>,
}

impl SubagentInput {
	pub fn from_reader(reader: impl std::io::Read) -> Result<Self, serde_json::Error> {
		serde_json::from_reader(reader)
	}
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
	#[serde(default, deserialize_with = "null_as_default")]
	pub id: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub name: String,
	#[serde(default, rename = "type", deserialize_with = "null_as_default")]
	pub kind: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub status: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub description: String,
	#[serde(default, deserialize_with = "null_as_default")]
	pub label: String,
	/// Epoch timestamp. The docs do not name the unit, so values large enough to be milliseconds
	/// are treated as such.
	#[serde(default)]
	pub start_time: Option<i64>,
	/// Resolved model ID, absent until the task's model is known.
	#[serde(default, deserialize_with = "null_as_default")]
	pub model: String,
	#[serde(default)]
	pub effort: Option<Effort>,
	#[serde(default)]
	pub context_window_size: Option<Tokens>,
	#[serde(default)]
	pub token_count: Option<Tokens>,
	#[serde(default, deserialize_with = "null_as_default")]
	pub cwd: String,
}

/// A subagent's effort is either a level name or a numeric token budget.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Effort {
	Level(String),
	Budget(u64),
}

impl Effort {
	#[must_use]
	pub fn label(&self) -> String {
		match self {
			Self::Level(level) => level.clone(),
			Self::Budget(tokens) => Tokens::from(*tokens).to_string(),
		}
	}
}

impl Task {
	/// The task viewed as a status line input, so the model, effort, cwd, and context segments render
	/// per task without any code of their own.
	#[must_use]
	pub fn to_input(&self) -> InputData {
		let tokens = self.token_count.unwrap_or_default();
		let window = self.context_window_size.unwrap_or_default();
		let used = if window == Tokens::default() {
			Percentage::default()
		} else {
			Percentage::from(tokens.ratio_of(window))
		};

		InputData {
			cwd: self.cwd.clone(),
			model: ModelInfo {
				id: self.model.clone(),
				display_name: short_model_name(&self.model),
			},
			effort: EffortInfo {
				level: self.effort.as_ref().map(Effort::label).unwrap_or_default(),
			},
			agent: AgentInfo {
				name: self.name.clone(),
			},
			context_window: ContextWindow {
				used_percentage: used,
				remaining_percentage: Percentage::from(100.0) - used,
				total_input_tokens: tokens,
				total_output_tokens: Tokens::default(),
				context_window_size: window,
				current_usage: CurrentUsage::default(),
			},
			..InputData::default()
		}
	}
}

/// The model family from a resolved model ID, since tasks carry no display name.
#[must_use]
pub fn short_model_name(id: &str) -> String {
	const FAMILIES: [&str; 5] = ["fable", "mythos", "opus", "sonnet", "haiku"];
	let lower = id.to_ascii_lowercase();
	FAMILIES
		.iter()
		.find(|family| lower.contains(*family))
		.map_or_else(
			|| id.to_owned(),
			|family| {
				let mut chars = family.chars();
				chars
					.next()
					.map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
					.unwrap_or_default()
			},
		)
}

/// One agent-panel row override, serialised as the JSON line Claude Code reads back.
#[derive(Debug, Serialize)]
pub struct Row {
	pub id: String,
	pub content: String,
}

#[must_use]
pub fn default_subagent_segments() -> Vec<SegmentConfig> {
	[
		SegmentType::TaskName,
		SegmentType::TaskStatus,
		SegmentType::Divider,
		SegmentType::Model,
		SegmentType::TaskTokens,
		SegmentType::Divider,
		SegmentType::TaskDescription,
		SegmentType::Divider,
		SegmentType::TaskLabel,
	]
	.into_iter()
	.map(SegmentConfig::Simple)
	.collect()
}

/// Renders every task; a task whose row comes out empty is left out so Claude Code keeps its
/// default rendering for it. With `grid` on, the rows are laid out as aligned columns.
#[must_use]
pub fn render_rows(
	input: &SubagentInput,
	segments: &[SegmentConfig],
	divider: &str,
	nerd_font: bool,
	grid: bool,
) -> Vec<Row> {
	let inputs: Vec<InputData> = input.tasks.iter().map(Task::to_input).collect();
	let lines: Vec<SegmentLine<'_>> = input
		.tasks
		.iter()
		.zip(&inputs)
		.map(|(task, data)| SegmentLine {
			segments,
			ctx: RenderContext {
				input: data,
				usage: None,
				credits: None,
				git: None,
				five_threshold: Percentage::default(),
				seven_threshold: Percentage::default(),
				divider,
				nerd_font,
				account: None,
				task: Some(task),
			},
		})
		.collect();
	// Claude Code truncates or wraps rows wider than the panel; cutting them here keeps the
	// grid's columns intact and marks the cut.
	let width = input.columns.filter(|columns| *columns > 0);
	let contents: Vec<String> = if grid {
		let parts: Vec<_> = lines.iter().map(SegmentLine::parts_with_indices).collect();
		align_rows(&parts, width)
	} else {
		lines.iter().map(ToString::to_string).collect()
	};

	input
		.tasks
		.iter()
		.zip(contents)
		.filter(|(_, content)| !content.is_empty())
		.map(|(task, mut content)| {
			if let Some(width) = width {
				truncate_visible(&mut content, width);
			}
			Row {
				id: task.id.clone(),
				content,
			}
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::format::{Percentage, Tokens};
	use crate::segment::{SegmentConfig, SegmentType};

	const FIXTURE: &str = include_str!("../tests/fixtures/subagent.json");

	fn strip_ansi(s: &str) -> String {
		String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
	}

	#[test]
	fn fixture_parses_every_task_field() {
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		assert_eq!(input.columns, Some(100));
		assert_eq!(input.tasks.len(), 3);
		let t = &input.tasks[0];
		assert_eq!(t.id, "task-1");
		assert_eq!(t.name, "security-reviewer");
		assert_eq!(t.status, "running");
		assert_eq!(t.description, "Review the auth flow for injection risks");
		assert_eq!(t.start_time, Some(1_738_425_600_000));
		assert_eq!(t.model, "claude-opus-5");
		assert_eq!(t.effort, Some(Effort::Level("high".to_owned())));
		assert_eq!(t.context_window_size, Some(Tokens::from(200_000)));
		assert_eq!(t.token_count, Some(Tokens::from(45_321)));
		assert_eq!(t.cwd, "/home/user/project");
		assert!(input.tasks[1].context_window_size.is_none());
		assert_eq!(input.tasks[2].effort, Some(Effort::Budget(12_000)));
		assert_eq!(input.tasks[2].description, "");
		assert_eq!(input.tasks[2].model, "");
	}

	#[test]
	fn empty_and_null_task_lists_parse() {
		assert!(
			SubagentInput::from_reader(b"{}" as &[u8])
				.unwrap()
				.tasks
				.is_empty()
		);
		let null =
			SubagentInput::from_reader(r#"{"columns": null, "tasks": null}"#.as_bytes()).unwrap();
		assert!(null.tasks.is_empty());
		assert!(null.columns.is_none());
	}

	#[test]
	fn task_becomes_an_input_for_the_shared_segments() {
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		let data = input.tasks[0].to_input();
		assert_eq!(data.model.id, "claude-opus-5");
		assert_eq!(data.model.display_name, "Opus");
		assert_eq!(data.effort.level, "high");
		assert_eq!(data.cwd, "/home/user/project");
		assert_eq!(data.context_window.total_input_tokens, Tokens::from(45_321));
		assert_eq!(
			data.context_window.context_window_size,
			Tokens::from(200_000)
		);
		assert_eq!(
			data.context_window.used_percentage,
			Percentage::from(45_321.0 / 200_000.0 * 100.0)
		);

		// No window size means no percentage can be claimed.
		let bare = input.tasks[1].to_input();
		assert_eq!(bare.context_window.used_percentage, Percentage::from(0.0));
		assert_eq!(bare.model.display_name, "");
		let budget = input.tasks[2].to_input();
		assert_eq!(budget.effort.level, "12.0k");
	}

	#[test]
	fn short_model_name_finds_the_family() {
		assert_eq!(short_model_name("claude-opus-5"), "Opus");
		assert_eq!(short_model_name("claude-sonnet-5"), "Sonnet");
		assert_eq!(short_model_name("claude-haiku-4-5-20251001"), "Haiku");
		assert_eq!(short_model_name("claude-fable-5-1"), "Fable");
		assert_eq!(short_model_name("gpt-x"), "gpt-x");
		assert_eq!(short_model_name(""), "");
	}

	#[test]
	fn rows_render_one_entry_per_task_in_order() {
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		let rows = render_rows(
			&input,
			&default_subagent_segments(),
			"\u{2022}",
			false,
			false,
		);
		let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
		assert_eq!(ids, ["task-1", "task-2", "task-3"]);
		let first = strip_ansi(&rows[0].content);
		assert!(first.contains("security-reviewer"), "{first}");
		assert!(first.contains("running"), "{first}");
		assert!(first.contains("Opus"), "{first}");
	}

	#[test]
	fn rows_skip_tasks_that_render_nothing() {
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		// task-3 has no model, so a model-only layout leaves Claude Code's default row in place.
		let segments = [SegmentConfig::Simple(SegmentType::Model)];
		let rows = render_rows(&input, &segments, "\u{2022}", false, false);
		let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
		assert_eq!(ids, ["task-1"]);
	}

	#[test]
	fn grid_rows_line_up_dividers_across_tasks() {
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		let rows = render_rows(
			&input,
			&default_subagent_segments(),
			"\u{2022}",
			false,
			true,
		);
		let plain: Vec<String> = rows.iter().map(|r| strip_ansi(&r.content)).collect();
		assert_eq!(plain.len(), 3, "{plain:?}");
		let first_divider = |s: &str| s.find('\u{2022}');
		assert_eq!(
			first_divider(&plain[0]),
			first_divider(&plain[1]),
			"{plain:?}"
		);
		let last_divider = |s: &str| s.rfind('\u{2022}');
		assert_eq!(
			last_divider(&plain[0]),
			last_divider(&plain[1]),
			"{plain:?}"
		);
		assert!(
			plain[0].starts_with("\u{2699} security-reviewer running "),
			"{}",
			plain[0]
		);
		assert!(
			plain[1].starts_with("\u{2699} Explore           completed "),
			"{}",
			plain[1]
		);
		// task-3 has neither model nor description: the row ends at its last cell, without padding
		// or a dangling divider.
		assert_eq!(plain[2], "\u{2699} worker            pending");
	}

	#[test]
	fn grid_off_keeps_the_plain_rows() {
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		let rows = render_rows(
			&input,
			&default_subagent_segments(),
			"\u{2022}",
			false,
			false,
		);
		assert_eq!(strip_ansi(&rows[2].content), "\u{2699} worker pending");
	}

	#[test]
	fn grid_rows_skip_tasks_that_render_nothing() {
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		let segments = [SegmentConfig::Simple(SegmentType::Model)];
		let rows = render_rows(&input, &segments, "\u{2022}", false, true);
		let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
		assert_eq!(ids, ["task-1"]);
	}

	#[test]
	fn rows_are_clipped_to_the_panel_width() {
		use crate::text::visible_width;
		let narrow = FIXTURE.replace(r#""columns": 100"#, r#""columns": 30"#);
		let input = SubagentInput::from_reader(narrow.as_bytes()).unwrap();
		assert_eq!(input.columns, Some(30));
		for grid in [true, false] {
			let rows = render_rows(
				&input,
				&default_subagent_segments(),
				"\u{2022}",
				false,
				grid,
			);
			for row in &rows {
				assert!(
					visible_width(&row.content) <= 30,
					"{}",
					strip_ansi(&row.content)
				);
			}
			assert!(strip_ansi(&rows[0].content).ends_with('\u{2026}'));
			// Six columns cannot keep their widths in 30 cells, so the grid gives the padding up too.
			assert_eq!(
				strip_ansi(&rows[2].content).trim_end(),
				"\u{2699} worker pending"
			);
		}
	}

	#[test]
	fn the_default_layout_ends_with_the_live_activity() {
		let layout = default_subagent_segments();
		let tail: Vec<&SegmentType> = layout
			.iter()
			.rev()
			.take(2)
			.map(SegmentConfig::segment_type)
			.collect();
		assert_eq!(tail, [&SegmentType::TaskLabel, &SegmentType::Divider]);
		let input = SubagentInput::from_reader(FIXTURE.as_bytes()).unwrap();
		let rows = render_rows(&input, &layout, "\u{2022}", false, true);
		assert!(strip_ansi(&rows[0].content).ends_with("\u{2022} Reviewing auth middleware"));
	}

	#[test]
	fn a_long_description_does_not_push_the_activity_off_the_row() {
		let long = "summarise every README section in one sentence each and keep going until every heading is covered";
		let json = format!(
			r#"{{"columns": 80, "tasks": [
				{{"id": "a", "status": "running", "description": "list segment functions", "label": "Reading files", "model": "claude-sonnet-5", "tokenCount": 82787}},
				{{"id": "b", "status": "running", "description": "{long}", "label": "Writing summary.md", "model": "claude-fable-5-1", "tokenCount": 45746}}
			]}}"#
		);
		let input = SubagentInput::from_reader(json.as_bytes()).unwrap();
		let rows = render_rows(
			&input,
			&default_subagent_segments(),
			"\u{2022}",
			false,
			true,
		);
		let plain: Vec<String> = rows.iter().map(|r| strip_ansi(&r.content)).collect();
		assert!(plain[0].ends_with("\u{2022} Reading files"), "{}", plain[0]);
		assert!(
			plain[1].ends_with("\u{2022} Writing summary.md"),
			"{}",
			plain[1]
		);
		assert!(plain[1].contains('\u{2026}'), "{}", plain[1]);
		for row in &plain {
			assert!(row.chars().count() <= 80, "{row}");
		}
	}

	#[test]
	fn rows_serialize_as_one_json_object_per_line() {
		let row = Row {
			id: "task-1".to_owned(),
			content: "\u{1b}[1mname\u{1b}[0m".to_owned(),
		};
		assert_eq!(
			serde_json::to_string(&row).unwrap(),
			r#"{"id":"task-1","content":"\u001b[1mname\u001b[0m"}"#
		);
	}
}
