mod account;
mod context;
mod cost;
mod credits;
mod env;
mod git;
mod rate_limit;
mod render;

use crate::format::{Percentage, parse_color};
use crate::input::InputData;
use crate::usage::{PrepaidCredits, UsageError, UsageResponse};
use owo_colors::{DynColors, OwoColorize};
use serde::{Deserialize, Serialize};
use std::fmt;

pub use git::{GitCache, load_git_cache};
pub use render::render_segment;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentType {
	ContextPercentage,
	TotalInputTokens,
	InputTokens,
	OutputTokens,
	FiveHour,
	SevenDay,
	ExtraUsage,
	Credits,
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
	Account,
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

impl SegmentType {
	#[must_use]
	pub fn all() -> Vec<SegmentType> {
		use SegmentType::*;

		fn succ(t: &SegmentType) -> Option<SegmentType> {
			use SegmentType::*;
			Some(match t {
				ContextPercentage => TotalInputTokens,
				TotalInputTokens => InputTokens,
				InputTokens => OutputTokens,
				OutputTokens => FiveHour,
				FiveHour => SevenDay,
				SevenDay => ExtraUsage,
				ExtraUsage => Credits,
				Credits => Divider,
				Divider => Cwd,
				Cwd => ProjectDir,
				ProjectDir => Model,
				Model => ModelId,
				ModelId => Version,
				Version => Cost,
				Cost => GitBranch,
				GitBranch => SessionId,
				SessionId => VimMode,
				VimMode => AgentName,
				AgentName => Worktree,
				Worktree => Account,
				Account => LinesAdded,
				LinesAdded => LinesRemoved,
				LinesRemoved => Duration,
				Duration => ApiDuration,
				ApiDuration => CacheReadTokens,
				CacheReadTokens => CacheHitRatio,
				CacheHitRatio => ContextRemaining,
				ContextRemaining => ContextWindowSize,
				ContextWindowSize => Exceeds200k,
				Exceeds200k => GitAheadBehind,
				GitAheadBehind => GitStash,
				GitStash => TokensPerSecond,
				TokensPerSecond => CostRate,
				CostRate => return None,
			})
		}

		let mut all = vec![ContextPercentage];
		while let Some(next) = succ(all.last().expect("starts non-empty")) {
			all.push(next);
		}
		all
	}
}

fn default_true() -> bool {
	true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(b: &bool) -> bool {
	*b
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct SegmentOptions {
	#[serde(rename = "type")]
	pub segment_type: SegmentType,
	#[serde(default = "default_true")]
	pub colors: bool,
	#[serde(default = "default_true")]
	pub icon: bool,
	#[serde(default)]
	pub icon_color: Option<String>,
	#[serde(default)]
	pub label: Option<String>,
	#[serde(default)]
	pub style: Option<String>,
	#[serde(default)]
	pub dirty: DirtyConfig,
	#[serde(default)]
	pub dirty_color: Option<String>,
	#[serde(default)]
	pub capitalize: Option<bool>,
	#[serde(default = "default_true", skip_serializing_if = "is_true")]
	pub enabled: bool,
	#[serde(flatten)]
	pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DirtyConfig {
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

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SegmentConfig {
	Simple(SegmentType),
	Advanced(SegmentOptions),
}

impl SegmentConfig {
	#[must_use]
	pub fn segment_type(&self) -> &SegmentType {
		match self {
			Self::Simple(t) => t,
			Self::Advanced(opts) => &opts.segment_type,
		}
	}

	#[must_use]
	pub fn colors(&self) -> bool {
		match self {
			Self::Simple(_) => true,
			Self::Advanced(opts) => opts.colors,
		}
	}

	#[must_use]
	pub fn icon(&self) -> bool {
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
				DirtyConfig::On => Some(DIRTY_INDICATOR),
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

	#[must_use]
	pub fn capitalize(&self) -> bool {
		match self {
			Self::Simple(_) => true,
			Self::Advanced(opts) => opts.capitalize.unwrap_or(true),
		}
	}

	#[must_use]
	pub fn enabled(&self) -> bool {
		match self {
			Self::Simple(_) => true,
			Self::Advanced(opts) => opts.enabled,
		}
	}

	#[must_use]
	pub fn is_extra_usage(&self) -> bool {
		*self.segment_type() == SegmentType::ExtraUsage
	}

	#[must_use]
	pub fn is_credits(&self) -> bool {
		*self.segment_type() == SegmentType::Credits
	}

	pub fn options_mut(&mut self) -> &mut SegmentOptions {
		if let Self::Simple(t) = self {
			*self = Self::Advanced(SegmentOptions {
				segment_type: t.clone(),
				colors: true,
				icon: true,
				icon_color: None,
				label: None,
				style: None,
				dirty: DirtyConfig::Off,
				dirty_color: None,
				capitalize: None,
				enabled: true,
				extra: serde_json::Map::new(),
			});
		}
		match self {
			Self::Advanced(opts) => opts,
			Self::Simple(_) => unreachable!("just upgraded to Advanced"),
		}
	}

	pub fn normalize(&mut self) {
		if let Self::Advanced(opts) = self {
			let is_default = opts.colors
				&& opts.icon && opts.icon_color.is_none()
				&& opts.label.is_none()
				&& opts.style.is_none()
				&& matches!(opts.dirty, DirtyConfig::Off)
				&& opts.dirty_color.is_none()
				&& opts.capitalize.unwrap_or(true)
				&& opts.enabled
				&& opts.extra.is_empty();
			if is_default {
				*self = Self::Simple(opts.segment_type.clone());
			}
		}
	}
}

#[must_use]
pub fn default_segments() -> Vec<SegmentConfig> {
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

#[derive(Debug, Clone, Default)]
pub struct AccountDisplay {
	pub nickname: String,
	pub color: Option<String>,
}

pub struct RenderContext<'a> {
	pub input: &'a InputData,
	pub usage: Option<Result<&'a UsageResponse, &'a UsageError>>,
	pub credits: Option<Result<&'a PrepaidCredits, &'a UsageError>>,
	pub git: Option<&'a GitCache>,
	pub five_threshold: Percentage,
	pub seven_threshold: Percentage,
	pub divider: &'a str,
	pub nerd_font: bool,
	pub account: Option<AccountDisplay>,
}

pub const STYLES: &[&str] = &["bold", "dim", "italic", "underline"];

pub const DIRTY_INDICATOR: &str = "\u{2731}";

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

pub struct SegmentLine<'a> {
	pub segments: &'a [SegmentConfig],
	pub ctx: RenderContext<'a>,
}

impl SegmentLine<'_> {
	#[must_use]
	pub fn parts(&self) -> Vec<(String, bool)> {
		self.parts_with_indices()
			.into_iter()
			.map(|(_, output, is_divider)| (output, is_divider))
			.collect()
	}

	#[must_use]
	pub fn parts_with_indices(&self) -> Vec<(usize, String, bool)> {
		let mut parts: Vec<(usize, String, bool)> = Vec::new();

		for (idx, segment) in self.segments.iter().enumerate() {
			if !segment.enabled() {
				continue;
			}
			let is_divider = *segment.segment_type() == SegmentType::Divider;
			if let Some(output) = render_segment(segment, &self.ctx) {
				if is_divider && (parts.is_empty() || parts.last().is_some_and(|(_, _, d)| *d)) {
					continue;
				}
				parts.push((idx, output, is_divider));
			}
		}

		while parts.last().is_some_and(|(_, _, d)| *d) {
			parts.pop();
		}

		parts
	}
}

impl fmt::Display for SegmentLine<'_> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		for (i, (output, _)) in self.parts().iter().enumerate() {
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
		let types: Vec<&SegmentType> = segs.iter().map(SegmentConfig::segment_type).collect();
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

	#[test]
	fn options_mut_upgrades_simple_to_advanced() {
		let mut seg = SegmentConfig::Simple(SegmentType::GitBranch);
		let opts = seg.options_mut();
		assert_eq!(opts.segment_type, SegmentType::GitBranch);
		assert!(opts.colors);
		assert!(opts.icon);
		assert!(opts.icon_color.is_none());
		assert!(opts.label.is_none());
		assert!(opts.style.is_none());
		assert!(matches!(opts.dirty, DirtyConfig::Off));
		assert!(opts.dirty_color.is_none());
		assert!(opts.capitalize.is_none());
		assert!(matches!(seg, SegmentConfig::Advanced(_)));
	}

	#[test]
	fn options_mut_is_idempotent_on_advanced() {
		let mut seg = SegmentConfig::Simple(SegmentType::Model);
		seg.options_mut().label = Some("custom".to_owned());
		let opts = seg.options_mut();
		assert_eq!(opts.label.as_deref(), Some("custom"));
		assert_eq!(opts.segment_type, SegmentType::Model);
	}

	#[test]
	fn normalize_collapses_all_default_advanced_to_simple() {
		let mut seg = SegmentConfig::Simple(SegmentType::Cwd);
		let _ = seg.options_mut();
		assert!(matches!(seg, SegmentConfig::Advanced(_)));
		seg.normalize();
		assert!(matches!(seg, SegmentConfig::Simple(SegmentType::Cwd)));
	}

	#[test]
	fn normalize_leaves_non_default_advanced_unchanged() {
		let mut seg = SegmentConfig::Simple(SegmentType::GitBranch);
		seg.options_mut().colors = false;
		seg.normalize();
		assert!(matches!(seg, SegmentConfig::Advanced(_)));
		assert!(!seg.colors());
	}

	#[test]
	fn styles_const_drives_apply_style() {
		for style in STYLES {
			assert_ne!(apply_style("x", Some(style)), "x", "{style} not honored");
		}
	}

	#[test]
	fn dirty_on_uses_the_shared_indicator() {
		let mut seg = SegmentConfig::Simple(SegmentType::GitBranch);
		seg.options_mut().dirty = DirtyConfig::On;
		assert_eq!(seg.dirty_indicator(), Some(DIRTY_INDICATOR));
	}

	#[test]
	fn parts_compose_exactly_like_display() {
		let sample = crate::sample::SampleData::representative();
		let segments = vec![
			SegmentConfig::Simple(SegmentType::Divider),
			SegmentConfig::Simple(SegmentType::Cwd),
			SegmentConfig::Simple(SegmentType::Divider),
			SegmentConfig::Simple(SegmentType::Divider),
			SegmentConfig::Simple(SegmentType::Model),
			SegmentConfig::Simple(SegmentType::Divider),
		];
		let line = SegmentLine {
			segments: &segments,
			ctx: sample.render_context(),
		};
		let parts = line.parts();
		assert!(parts.first().is_some_and(|(_, d)| !*d), "leading divider");
		assert!(parts.last().is_some_and(|(_, d)| !*d), "trailing divider");
		let joined = parts
			.iter()
			.map(|(s, _)| s.as_str())
			.collect::<Vec<_>>()
			.join(" ");
		assert_eq!(joined, line.to_string());
	}

	#[test]
	fn enabled_false_round_trips_and_defaults_true() {
		let seg: SegmentConfig =
			serde_json::from_str(r#"{"type":"model","enabled":false}"#).unwrap();
		assert!(!seg.enabled());
		let json = serde_json::to_string(&seg).unwrap();
		assert!(json.contains(r#""enabled":false"#), "{json}");

		let seg: SegmentConfig = serde_json::from_str(r#"{"type":"model"}"#).unwrap();
		assert!(seg.enabled());
		assert!(SegmentConfig::Simple(SegmentType::Model).enabled());
		let json = serde_json::to_string(&seg).unwrap();
		assert!(!json.contains("enabled"), "{json}");
	}

	#[test]
	fn disabled_segment_is_skipped_by_segment_line() {
		let sample = crate::sample::SampleData::representative();
		let disabled: SegmentConfig =
			serde_json::from_str(r#"{"type":"model","enabled":false}"#).unwrap();
		let segments = vec![disabled, SegmentConfig::Simple(SegmentType::Cwd)];
		let line = SegmentLine {
			segments: &segments,
			ctx: sample.render_context(),
		};
		let out = line.to_string();
		assert!(!out.contains("Opus"), "disabled segment rendered: {out}");
		assert!(out.contains("project"), "enabled segment missing: {out}");
	}

	#[test]
	fn advanced_segment_preserves_unknown_keys() {
		let seg: SegmentConfig =
			serde_json::from_str(r#"{"type":"git_branch","dirty":true,"max_len":20}"#).unwrap();
		let json = serde_json::to_string(&seg).unwrap();
		assert!(json.contains(r#""max_len":20"#), "lost unknown key: {json}");
	}

	#[test]
	fn normalize_collapses_capitalize_some_true() {
		let mut seg = SegmentConfig::Simple(SegmentType::Model);
		seg.options_mut().capitalize = Some(true);
		seg.normalize();
		assert!(matches!(seg, SegmentConfig::Simple(SegmentType::Model)));
	}

	#[test]
	fn normalize_round_trips_back_to_simple_after_clearing() {
		let mut seg = SegmentConfig::Simple(SegmentType::Model);
		seg.options_mut().label = Some("in:".to_owned());
		seg.normalize();
		assert!(matches!(seg, SegmentConfig::Advanced(_)));
		seg.options_mut().label = None;
		seg.normalize();
		assert!(matches!(seg, SegmentConfig::Simple(SegmentType::Model)));
	}
}
