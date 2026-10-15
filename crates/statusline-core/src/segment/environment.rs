use super::{RenderContext, SegmentConfig, SegmentType, apply_style};
use crate::{format::parse_color, text::plain};
use owo_colors::OwoColorize;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct EnvironmentValues(BTreeMap<String, String>);

impl EnvironmentValues {
	#[must_use]
	pub fn resolve(segments: &[SegmentConfig], lookup: impl Fn(&str) -> Option<String>) -> Self {
		let mut values = BTreeMap::new();
		for segment in segments
			.iter()
			.filter(|s| s.enabled() && *s.segment_type() == SegmentType::Env)
		{
			let SegmentConfig::Advanced(options) = segment else {
				continue;
			};
			for name in [&options.var, &options.color_env].into_iter().flatten() {
				values
					.entry(name.clone())
					.or_insert_with(|| lookup(name).unwrap_or_default());
			}
		}
		Self(values)
	}
}

pub(super) fn render(segment: &SegmentConfig, context: &RenderContext<'_>) -> Option<String> {
	value(segment, context.environment?)
}

fn value(segment: &SegmentConfig, values: &EnvironmentValues) -> Option<String> {
	let SegmentConfig::Advanced(options) = segment else {
		return None;
	};
	let raw = values.0.get(options.var.as_deref()?)?;
	let display: String = plain(raw)
		.chars()
		.filter(|c| !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}'))
		.collect();
	if display.is_empty() {
		return None;
	}
	let color = options
		.color_env
		.as_ref()
		.and_then(|name| values.0.get(name))
		.and_then(|s| parse_color(s));
	let text = match color.filter(|_| segment.colors()) {
		Some(color) => display.color(color).to_string(),
		None => display,
	};
	Some(apply_style(&text, segment.style()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::text::plain;
	use serde_json::json;
	use std::cell::RefCell;

	fn segment() -> SegmentConfig {
		serde_json::from_value(json!({"type":"env","var":"ACCOUNT","color_env":"ACCOUNT_COLOR"}))
			.unwrap()
	}

	#[test]
	fn environment_uses_only_requested_variables_and_preserves_unicode() {
		let segment = segment();
		let looked_up = RefCell::new(Vec::new());
		let values = EnvironmentValues::resolve(std::slice::from_ref(&segment), |name| {
			looked_up.borrow_mut().push(name.to_owned());
			match name {
				"ACCOUNT" => Some("Work ☕".into()),
				"ACCOUNT_COLOR" => Some("#64c8dc".into()),
				_ => panic!("unconfigured variable"),
			}
		});
		let rendered = value(&segment, &values).unwrap();
		assert_eq!(plain(&rendered), "Work ☕");
		assert!(rendered.contains("38;2;100;200;220"));
		assert_eq!(looked_up.borrow().len(), 2);
		assert_eq!(
			serde_json::to_value(segment).unwrap(),
			json!({"type":"env","var":"ACCOUNT","color_env":"ACCOUNT_COLOR"})
		);
	}

	#[test]
	fn environment_hides_missing_values_and_ignores_invalid_colours() {
		let segment = segment();
		let missing = EnvironmentValues::resolve(std::slice::from_ref(&segment), |_| None);
		assert!(value(&segment, &missing).is_none());
		let values = EnvironmentValues::resolve(std::slice::from_ref(&segment), |name| {
			Some(
				if name == "ACCOUNT" {
					"Work ☕"
				} else {
					"invalid colour"
				}
				.into(),
			)
		});
		assert_eq!(value(&segment, &values).unwrap(), "Work ☕");
	}

	#[test]
	fn environment_removes_terminal_controls_and_extra_rows() {
		let segment = segment();
		let values = EnvironmentValues::resolve(std::slice::from_ref(&segment), |name| {
			(name == "ACCOUNT").then(|| "\u{1b}[31mWork ☕\n\r\u{1b}]0;hidden\u{7}".into())
		});
		assert_eq!(value(&segment, &values).unwrap(), "Work ☕");
	}
}
