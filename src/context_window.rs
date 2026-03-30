use crate::constants::{CYAN, DIVIDER, DOWN_ARROW, PURPLE, UP_ARROW};
use crate::format::{ColoredPercentage, Percentage, Tokens};
use owo_colors::{OwoColorize, XtermColors};
use serde::Deserialize;
use std::fmt;

#[derive(Default, Debug, Deserialize)]
pub(crate) struct ContextWindow {
	used_percentage: Percentage,
	total_output_tokens: Tokens,
	current_usage: CurrentUsage,
}

impl ContextWindow {
	pub(crate) fn from_reader(reader: impl std::io::Read) -> Result<Self, serde_json::Error> {
		let data: InputData = serde_json::from_reader(reader)?;

		Ok(data.context_window.unwrap_or_default())
	}

	pub(crate) fn total_input_tokens(&self) -> Tokens {
		self.current_usage.input_tokens
			+ self.current_usage.cache_creation_input_tokens
			+ self.current_usage.cache_read_input_tokens
	}
}

impl fmt::Display for ContextWindow {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", ColoredPercentage(self.used_percentage))?;

		write!(f, " ")?;

		write!(f, "{} ", UP_ARROW.color(CYAN))?;
		write!(f, "{} ", self.total_input_tokens())?;

		write!(f, "{} ", DOWN_ARROW.color(PURPLE))?;
		write!(f, "{} ", self.total_output_tokens)?;

		write!(f, "{}", DIVIDER.color(XtermColors::ScorpionGray))?;

		Ok(())
	}
}

#[derive(Debug, Default, Deserialize)]
struct CurrentUsage {
	input_tokens: Tokens,
	cache_creation_input_tokens: Tokens,
	cache_read_input_tokens: Tokens,
}

#[derive(Debug, Default, Deserialize)]
struct InputData {
	context_window: Option<ContextWindow>,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn new(json: &str) -> ContextWindow {
		ContextWindow::from_reader(json.as_bytes()).unwrap()
	}

	#[test]
	fn total_input_tokens_sums_all_fields() {
		let cw = new(
			r#"{
			"context_window": {
				"used_percentage": 42.0,
				"total_output_tokens": 0,
				"current_usage": {
					"input_tokens": 100,
					"cache_creation_input_tokens": 200,
					"cache_read_input_tokens": 300
				}
			}
		}"#,
		);
		assert_eq!(cw.total_input_tokens(), 600.into());
	}

	#[test]
	fn total_input_tokens_handles_zero_fields() {
		let cw = new(
			r#"{
			"context_window": {
				"used_percentage": 0.0,
				"total_output_tokens": 0,
				"current_usage": {
					"input_tokens": 50,
					"cache_creation_input_tokens": 0,
					"cache_read_input_tokens": 0
				}
			}
		}"#,
		);
		assert_eq!(cw.total_input_tokens(), 50.into());
	}

	#[test]
	fn total_input_tokens_all_zero() {
		let cw = new(
			r#"{
			"context_window": {
				"used_percentage": 0.0,
				"total_output_tokens": 0,
				"current_usage": {
					"input_tokens": 0,
					"cache_creation_input_tokens": 0,
					"cache_read_input_tokens": 0
				}
			}
		}"#,
		);
		assert_eq!(cw.total_input_tokens(), 0.into());
	}

	#[test]
	fn default_context_window_is_zeroed() {
		let cw = ContextWindow::default();
		assert_eq!(cw.total_input_tokens(), 0.into());
	}

	fn render(json: &str) -> String {
		let cw = ContextWindow::from_reader(json.as_bytes()).unwrap();
		let bytes = strip_ansi_escapes::strip(format!("{cw}"));
		String::from_utf8(bytes).unwrap()
	}

	#[test]
	fn display_zero_usage() {
		let output = render(
			r#"{"context_window": {"used_percentage": 0, "total_output_tokens": 0, "current_usage": {"input_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#,
		);
		assert_eq!(output, "0% \u{2191} 0 \u{2193} 0 \u{2022}");
	}

	#[test]
	fn display_token_magnitudes() {
		let output = render(
			r#"{"context_window": {"used_percentage": 50, "total_output_tokens": 3000000, "current_usage": {"input_tokens": 1500, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#,
		);
		assert_eq!(output, "50% \u{2191} 1.5k \u{2193} 3.0M \u{2022}");
	}

	#[test]
	fn display_sums_all_input_token_types() {
		let output = render(
			r#"{"context_window": {"used_percentage": 25, "total_output_tokens": 500, "current_usage": {"input_tokens": 100, "cache_creation_input_tokens": 200, "cache_read_input_tokens": 700}}}"#,
		);
		assert_eq!(output, "25% \u{2191} 1.0k \u{2193} 500 \u{2022}");
	}

	#[test]
	fn display_missing_context_window_defaults_to_zero() {
		let output = render(r#"{}"#);
		assert_eq!(output, "0% \u{2191} 0 \u{2193} 0 \u{2022}");
	}

	#[test]
	fn display_large_percentage() {
		let output = render(
			r#"{"context_window": {"used_percentage": 99.5, "total_output_tokens": 10000000, "current_usage": {"input_tokens": 5000000, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}"#,
		);
		assert_eq!(output, "99.5% \u{2191} 5.0M \u{2193} 10.0M \u{2022}");
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
