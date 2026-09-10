use crate::format::{Percentage, Tokens};
use crate::util::null_as_default;
use serde::Deserialize;

#[derive(Default, Debug, Deserialize)]
pub struct ContextWindow {
	#[serde(default, deserialize_with = "null_as_default")]
	pub used_percentage: Percentage,
	// Nothing used means everything is still available, so an unreported value reads as full rather
	// than as an alarming 0% remaining.
	#[serde(default = "full", deserialize_with = "null_as_full")]
	pub remaining_percentage: Percentage,
	#[serde(default, deserialize_with = "null_as_default")]
	pub total_input_tokens: Tokens,
	#[serde(default, deserialize_with = "null_as_default")]
	pub total_output_tokens: Tokens,
	#[serde(default, deserialize_with = "null_as_default")]
	pub context_window_size: Tokens,
	#[serde(default, deserialize_with = "null_as_default")]
	pub current_usage: CurrentUsage,
}

fn full() -> Percentage {
	Percentage::from(100.0)
}

fn null_as_full<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Percentage, D::Error> {
	Option::<Percentage>::deserialize(deserializer).map(|p| p.unwrap_or_else(full))
}

impl ContextWindow {
	pub fn from_reader(
		reader: impl std::io::Read,
	) -> Result<ContextWindowWrapper, serde_json::Error> {
		serde_json::from_reader(reader)
	}
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct CurrentUsage {
	#[serde(default)]
	pub input_tokens: Tokens,
	#[serde(default)]
	pub cache_creation_input_tokens: Tokens,
	#[serde(default)]
	pub cache_read_input_tokens: Tokens,
}

#[derive(Debug, Default, Deserialize)]
pub struct ContextWindowWrapper {
	#[serde(default)]
	pub context_window: ContextWindow,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn new(json: &str) -> ContextWindow {
		ContextWindow::from_reader(json.as_bytes())
			.unwrap()
			.context_window
	}

	#[test]
	fn total_input_tokens_from_json() {
		let cw = new(r#"{
			"context_window": {
				"used_percentage": 42.0,
				"total_input_tokens": 600,
				"total_output_tokens": 0,
				"current_usage": {
					"input_tokens": 100,
					"cache_creation_input_tokens": 200,
					"cache_read_input_tokens": 300
				}
			}
		}"#);
		assert_eq!(cw.total_input_tokens, 600.into());
	}

	#[test]
	fn null_current_usage_parses_as_empty() {
		// Claude Code sends null before the first API call and again right after /compact.
		let cw = new(
			r#"{"context_window": {"used_percentage": 3, "total_output_tokens": 0, "current_usage": null}}"#,
		);
		assert_eq!(cw.current_usage.input_tokens, 0.into());
		assert_eq!(cw.used_percentage, 3.0.into());
	}

	#[test]
	fn null_percentages_read_as_nothing_used() {
		let cw = new(
			r#"{"context_window": {"used_percentage": null, "remaining_percentage": null, "total_output_tokens": 0, "current_usage": null}}"#,
		);
		assert_eq!(cw.used_percentage, 0.0.into());
		assert_eq!(cw.remaining_percentage, 100.0.into());
	}

	#[test]
	fn default_context_window_is_zeroed() {
		let cw = ContextWindow::default();
		assert_eq!(cw.total_input_tokens, 0.into());
	}

	#[test]
	fn from_reader_rejects_malformed_json() {
		let err = ContextWindow::from_reader(b"not json" as &[u8]).unwrap_err();
		let msg = err.to_string();
		assert!(
			msg.contains("expected"),
			"error should describe what was expected, got: {msg}"
		);
	}
}
