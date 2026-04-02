use crate::format::{Percentage, Tokens};
use serde::Deserialize;

#[derive(Default, Debug, Deserialize)]
pub(crate) struct ContextWindow {
	pub(crate) used_percentage: Percentage,
	#[serde(default)]
	pub(crate) remaining_percentage: Percentage,
	#[serde(default)]
	pub(crate) total_input_tokens: Tokens,
	pub(crate) total_output_tokens: Tokens,
	#[serde(default)]
	pub(crate) context_window_size: Tokens,
	#[serde(default)]
	pub(crate) current_usage: CurrentUsage,
}

#[cfg(test)]
impl ContextWindow {
	pub(crate) fn from_reader(
		reader: impl std::io::Read,
	) -> Result<ContextWindowWrapper, serde_json::Error> {
		serde_json::from_reader(reader)
	}
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub(crate) struct CurrentUsage {
	#[serde(default)]
	pub(crate) input_tokens: Tokens,
	#[serde(default)]
	pub(crate) cache_creation_input_tokens: Tokens,
	#[serde(default)]
	pub(crate) cache_read_input_tokens: Tokens,
}

#[cfg(test)]
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ContextWindowWrapper {
	#[serde(default)]
	pub(crate) context_window: ContextWindow,
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
		let cw = new(
			r#"{
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
		}"#,
		);
		assert_eq!(cw.total_input_tokens, 600.into());
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
