//! Claude Code's subagent status line: the agent panel pipes every visible task as one JSON object
//! and takes back one `{"id", "content"}` line per row to override.

use crate::context_window::{ContextWindow, CurrentUsage};
use crate::format::{Percentage, Tokens};
use crate::input::{AgentInfo, EffortInfo, InputData, ModelInfo};
use crate::segment::{RenderContext, SegmentConfig, SegmentLine, SegmentType};
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
	]
	.into_iter()
	.map(SegmentConfig::Simple)
	.collect()
}

/// Renders every task; a task whose row comes out empty is left out so Claude Code keeps its
/// default rendering for it.
#[must_use]
pub fn render_rows(
	input: &SubagentInput,
	segments: &[SegmentConfig],
	divider: &str,
	nerd_font: bool,
) -> Vec<Row> {
	input
		.tasks
		.iter()
		.filter_map(|task| {
			let data = task.to_input();
			let line = SegmentLine {
				segments,
				ctx: RenderContext {
					input: &data,
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
			};
			let content = line.to_string();
			(!content.is_empty()).then(|| Row {
				id: task.id.clone(),
				content,
			})
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
		let rows = render_rows(&input, &default_subagent_segments(), "\u{2022}", false);
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
		let rows = render_rows(&input, &segments, "\u{2022}", false);
		let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
		assert_eq!(ids, ["task-1"]);
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
