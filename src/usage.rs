use crate::browser::Browser;
use crate::format::Cents;
use eyre::{Context, Result};
use owo_colors::OwoColorize;
use owo_colors::XtermColors;
use serde::Deserialize;
use std::fmt;
use std::fmt::Formatter;
use std::time::Duration;

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn fetch_usage(org_id: &str, browser: Browser) -> Result<UsageResponse> {
	let session_key = browser.load_session_key()?;

	let url = format!("https://claude.ai/api/organizations/{org_id}/usage");

	let response = ureq::get(&url)
		.header("Cookie", &format!("sessionKey={session_key}"))
		.header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36")
		.header("Accept", "application/json")
		.config()
		.timeout_global(Some(FETCH_TIMEOUT))
		.build()
		.call()
		.context("fetching usage data")?;

	let body = response
		.into_body()
		.read_to_string()
		.context("reading response body")?;

	serde_json::from_str(&body).context("parsing usage response JSON")
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageResponse {
	pub(crate) extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExtraUsage {
	pub(crate) monthly_limit: Cents,
	pub(crate) used_credits: Cents,
}

impl fmt::Display for ExtraUsage {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		let percentage = self.used_credits.as_percentage_of(self.monthly_limit);

		write!(
			f,
			"{}",
			format_args!("{}", self.used_credits)
				.color(percentage.color())
				.bold()
		)?;
		write!(
			f,
			"{}",
			format_args!("/{}", self.monthly_limit).color(XtermColors::LightGray)
		)?;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn strip_ansi(s: String) -> String {
		String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
	}

	#[test]
	fn usage_response_deserializes_with_extra() {
		let json = r#"{
			"extra_usage": {
				"monthly_limit": 10000.0,
				"used_credits": 2500.0
			}
		}"#;

		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		assert!(resp.extra_usage.is_some());
	}

	#[test]
	fn usage_response_deserializes_minimal() {
		let json = r#"{}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		assert!(resp.extra_usage.is_none());
	}

	#[test]
	fn usage_response_ignores_unknown_fields() {
		let json = r#"{
			"five_hour": {"utilization": 42.5, "resets_at": "2026-03-30T12:00:00+00:00"},
			"seven_day": {"utilization": 75.0, "resets_at": "2026-04-05T12:00:00+00:00"},
			"extra_usage": {"monthly_limit": 10000.0, "used_credits": 2500.0}
		}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		assert!(resp.extra_usage.is_some());
	}

	#[test]
	fn extra_usage_zero_limit_shows_dollar_amounts() {
		let extra = ExtraUsage {
			monthly_limit: 0.0.into(),
			used_credits: 500.0.into(),
		};
		let output = strip_ansi(format!("{extra}"));
		assert!(
			output.contains("$5"),
			"should show used credits, got: {output}"
		);
		assert!(
			output.contains("$0"),
			"should show zero limit, got: {output}"
		);
	}

	#[test]
	fn extra_usage_normal_display() {
		let extra = ExtraUsage {
			monthly_limit: 10000.0.into(),
			used_credits: 2500.0.into(),
		};
		let output = strip_ansi(format!("{extra}"));
		assert_eq!(output, "$25/$100");
	}
}
