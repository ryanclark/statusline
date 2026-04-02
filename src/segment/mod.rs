mod context;
mod cost;
mod env;
mod git;
mod rate_limit;
mod render;

use crate::format::{Percentage, parse_color};
use crate::input::InputData;
use crate::usage::UsageResponse;
use owo_colors::{DynColors, OwoColorize};
use serde::{Deserialize, Serialize};
use std::fmt;

pub(crate) use git::{GitCache, load_git_cache};
pub(crate) use render::render_segment;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SegmentType {
	ContextPercentage,
	TotalInputTokens,
	InputTokens,
	OutputTokens,
	FiveHour,
	SevenDay,
	ExtraUsage,
	Divider,
	Cwd,
	ProjectDir,
	Model,
	ModelId,
	Version,
	Cost,
	GitBranch,
	SessionId,
	VimMode,
	AgentName,
	Worktree,
	LinesAdded,
	LinesRemoved,
	Duration,
	ApiDuration,
	CacheReadTokens,
	CacheHitRatio,
	ContextRemaining,
	ContextWindowSize,
	Exceeds200k,
	GitAheadBehind,
	GitStash,
	TokensPerSecond,
	CostRate,
}

fn default_true() -> bool {
	true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SegmentOptions {
	#[serde(rename = "type")]
	pub(crate) segment_type: SegmentType,
	#[serde(default = "default_true")]
	pub(crate) colors: bool,
	#[serde(default = "default_true")]
	pub(crate) icon: bool,
	#[serde(default)]
	pub(crate) icon_color: Option<String>,
	#[serde(default)]
	pub(crate) label: Option<String>,
	#[serde(default)]
	pub(crate) style: Option<String>,
	#[serde(default)]
	pub(crate) dirty: DirtyConfig,
	#[serde(default)]
	pub(crate) dirty_color: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum DirtyConfig {
	#[default]
	Off,
	On,
	Custom(String),
}

impl<'de> Deserialize<'de> for DirtyConfig {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let value = serde_json::Value::deserialize(deserializer)?;

		match value {
			serde_json::Value::Bool(true) => Ok(Self::On),
			serde_json::Value::Bool(false) => Ok(Self::Off),
			serde_json::Value::String(s) => Ok(Self::Custom(s)),
			_ => Err(serde::de::Error::custom(
				"expected bool or string for dirty",
			)),
		}
	}
}

impl Serialize for DirtyConfig {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			Self::Off => serializer.serialize_bool(false),
			Self::On => serializer.serialize_bool(true),
			Self::Custom(s) => serializer.serialize_str(s),
		}
	}
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum SegmentConfig {
	Simple(SegmentType),
	Advanced(SegmentOptions),
}

impl SegmentConfig {
	pub(crate) fn segment_type(&self) -> &SegmentType {
		match self {
			Self::Simple(t) => t,
			Self::Advanced(opts) => &opts.segment_type,
		}
	}

	pub(crate) fn colors(&self) -> bool {
		match self {
			Self::Simple(_) => true,
			Self::Advanced(opts) => opts.colors,
		}
	}

	pub(crate) fn icon(&self) -> bool {
		match self {
			Self::Simple(_) => true,
			Self::Advanced(opts) => opts.icon,
		}
	}

	fn icon_color(&self) -> Option<DynColors> {
		match self {
			Self::Simple(_) => None,
			Self::Advanced(opts) => opts.icon_color.as_deref().and_then(parse_color),
		}
	}

	fn label(&self) -> Option<&str> {
		match self {
			Self::Simple(_) => None,
			Self::Advanced(opts) => opts.label.as_deref(),
		}
	}

	fn style(&self) -> Option<&str> {
		match self {
			Self::Simple(_) => None,
			Self::Advanced(opts) => opts.style.as_deref(),
		}
	}

	fn dirty_indicator(&self) -> Option<&str> {
		match self {
			Self::Simple(_) => None,
			Self::Advanced(opts) => match &opts.dirty {
				DirtyConfig::Off => None,
				DirtyConfig::On => Some("\u{2731}"),
				DirtyConfig::Custom(s) => Some(s),
			},
		}
	}

	fn dirty_color(&self) -> Option<DynColors> {
		match self {
			Self::Simple(_) => None,
			Self::Advanced(opts) => opts.dirty_color.as_deref().and_then(parse_color),
		}
	}

	pub(crate) fn is_extra_usage(&self) -> bool {
		*self.segment_type() == SegmentType::ExtraUsage
	}
}

pub(crate) fn default_segments() -> Vec<SegmentConfig> {
	vec![
		SegmentConfig::Simple(SegmentType::ContextPercentage),
		SegmentConfig::Simple(SegmentType::TotalInputTokens),
		SegmentConfig::Simple(SegmentType::OutputTokens),
		SegmentConfig::Simple(SegmentType::Divider),
		SegmentConfig::Simple(SegmentType::FiveHour),
		SegmentConfig::Simple(SegmentType::SevenDay),
		SegmentConfig::Simple(SegmentType::Divider),
		SegmentConfig::Simple(SegmentType::ExtraUsage),
	]
}

pub(crate) struct RenderContext<'a> {
	pub(crate) input: &'a InputData,
	pub(crate) usage: Option<&'a UsageResponse>,
	pub(crate) git: Option<&'a GitCache>,
	pub(crate) five_threshold: Percentage,
	pub(crate) seven_threshold: Percentage,
	pub(crate) divider: &'a str,
	pub(crate) nerd_font: bool,
}

fn apply_style(text: &str, style: Option<&str>) -> String {
	match style {
		Some("bold") => format!("{}", text.bold()),
		Some("dim") => format!("{}", text.dimmed()),
		Some("italic") => format!("{}", text.italic()),
		Some("underline") => format!("{}", text.underline()),
		_ => text.to_owned(),
	}
}

#[derive(Clone, Copy)]
struct Icon {
	unicode: &'static str,
	nerd: &'static str,
}

fn format_icon(
	segment: &SegmentConfig,
	icon: Icon,
	default_color: DynColors,
	nerd_font: bool,
) -> String {
	if !segment.icon() {
		return String::new();
	}

	let default_icon = if nerd_font { icon.nerd } else { icon.unicode };
	let icon_text = segment.label().unwrap_or(default_icon);
	let color = segment.icon_color().unwrap_or(default_color);

	if segment.colors() {
		format!("{} ", icon_text.color(color))
	} else {
		format!("{icon_text} ")
	}
}

pub(crate) struct SegmentLine<'a> {
	pub(crate) segments: &'a [SegmentConfig],
	pub(crate) ctx: RenderContext<'a>,
}

impl fmt::Display for SegmentLine<'_> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let mut parts: Vec<(String, bool)> = Vec::new();

		for segment in self.segments {
			let is_divider = *segment.segment_type() == SegmentType::Divider;
			if let Some(output) = render_segment(segment, &self.ctx) {
				if is_divider && (parts.is_empty() || parts.last().is_some_and(|(_, d)| *d)) {
					continue;
				}
				parts.push((output, is_divider));
			}
		}

		while parts.last().is_some_and(|(_, d)| *d) {
			parts.pop();
		}

		for (i, (output, _)) in parts.iter().enumerate() {
			if i > 0 {
				write!(f, " ")?;
			}
			write!(f, "{output}")?;
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn deserialize_simple_segment() {
		let seg: SegmentConfig = serde_json::from_str(r#""context_percentage""#).unwrap();
		assert_eq!(*seg.segment_type(), SegmentType::ContextPercentage);
		assert!(seg.colors());
		assert!(seg.icon());
	}

	#[test]
	fn deserialize_advanced_segment() {
		let seg: SegmentConfig = serde_json::from_str(
			r#"{"type": "input_tokens", "colors": false, "icon": false, "label": "in:"}"#,
		)
		.unwrap();
		assert_eq!(*seg.segment_type(), SegmentType::InputTokens);
		assert!(!seg.colors());
		assert!(!seg.icon());
		assert_eq!(seg.label(), Some("in:"));
	}

	#[test]
	fn deserialize_mixed_list() {
		let list: Vec<SegmentConfig> = serde_json::from_str(
			r#"["context_percentage", {"type": "input_tokens", "colors": false}, "divider"]"#,
		)
		.unwrap();
		assert_eq!(list.len(), 3);
		assert_eq!(*list[0].segment_type(), SegmentType::ContextPercentage);
		assert_eq!(*list[1].segment_type(), SegmentType::InputTokens);
		assert!(!list[1].colors());
		assert_eq!(*list[2].segment_type(), SegmentType::Divider);
	}

	#[test]
	fn deserialize_unknown_segment_fails() {
		let err = serde_json::from_str::<SegmentConfig>(r#""not_a_segment""#);
		assert!(err.is_err());
	}

	#[test]
	fn deserialize_advanced_defaults() {
		let seg: SegmentConfig = serde_json::from_str(r#"{"type": "extra_usage"}"#).unwrap();
		assert!(seg.colors());
		assert!(seg.icon());
		assert!(seg.icon_color().is_none());
		assert!(seg.label().is_none());
		assert!(seg.style().is_none());
	}

	#[test]
	fn serialize_simple_roundtrip() {
		let seg = SegmentConfig::Simple(SegmentType::Model);
		let json = serde_json::to_string(&seg).unwrap();
		assert_eq!(json, r#""model""#);
	}

	#[test]
	fn default_segments_match_current_layout() {
		let segs = default_segments();
		let types: Vec<&SegmentType> = segs.iter().map(|s| s.segment_type()).collect();
		assert_eq!(
			types,
			vec![
				&SegmentType::ContextPercentage,
				&SegmentType::TotalInputTokens,
				&SegmentType::OutputTokens,
				&SegmentType::Divider,
				&SegmentType::FiveHour,
				&SegmentType::SevenDay,
				&SegmentType::Divider,
				&SegmentType::ExtraUsage,
			]
		);
	}
}
