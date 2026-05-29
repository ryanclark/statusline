use crate::browser::Browser;
use crate::format::Cents;
use owo_colors::OwoColorize;
use owo_colors::XtermColors;
use serde::Deserialize;
use std::time::Duration;

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36";

#[derive(Debug, thiserror::Error)]
pub(crate) enum UsageError {
	#[error("not logged in to claude.ai")]
	NotLoggedIn,
	#[error("{0}")]
	Other(String),
}

pub(crate) fn fetch_usage(
	org_id: &str,
	browser: Browser,
	profile: Option<&str>,
) -> Result<UsageResponse, UsageError> {
	let url = format!("https://claude.ai/api/organizations/{org_id}/usage");
	let body = fetch_authenticated(&url, browser, profile)?;
	serde_json::from_str(&body).map_err(|e| UsageError::Other(format!("parsing usage response: {e}")))
}

pub(crate) fn fetch_credits(
	org_id: &str,
	browser: Browser,
	profile: Option<&str>,
) -> Result<PrepaidCredits, UsageError> {
	let url = format!("https://claude.ai/api/organizations/{org_id}/prepaid/credits");
	let body = fetch_authenticated(&url, browser, profile)?;
	serde_json::from_str(&body)
		.map_err(|e| UsageError::Other(format!("parsing credits response: {e}")))
}

fn fetch_authenticated(
	url: &str,
	browser: Browser,
	profile: Option<&str>,
) -> Result<String, UsageError> {
	let session_key = browser.load_session_key(profile).map_err(|e| {
		if e.to_string().contains("sessionKey cookie not found") {
			UsageError::NotLoggedIn
		} else {
			UsageError::Other(e.to_string())
		}
	})?;

	let response = ureq::get(url)
		.header("Cookie", &format!("sessionKey={session_key}"))
		.header("User-Agent", USER_AGENT)
		.header("Accept", "application/json")
		.config()
		.timeout_global(Some(FETCH_TIMEOUT))
		.build()
		.call()
		.map_err(|e| UsageError::Other(format!("fetching {url}: {e}")))?;

	response
		.into_body()
		.read_to_string()
		.map_err(|e| UsageError::Other(format!("reading response body: {e}")))
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageResponse {
	pub(crate) extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExtraUsage {
	#[serde(default)]
	pub(crate) monthly_limit: Option<Cents>,
	#[serde(default)]
	pub(crate) used_credits: Option<Cents>,
}

impl ExtraUsage {
	pub(crate) fn format(&self, colored: bool) -> Option<String> {
		let monthly_limit = self.monthly_limit?;
		let used_credits = self.used_credits?;

		if colored {
			let percentage = used_credits.as_percentage_of(monthly_limit);
			Some(format!(
				"{}{}",
				format_args!("{used_credits}")
					.color(percentage.color())
					.bold(),
				format_args!("/{monthly_limit}").color(XtermColors::LightGray),
			))
		} else {
			Some(format!("{used_credits}/{monthly_limit}"))
		}
	}
}

#[derive(Debug, Deserialize)]
pub(crate) struct PrepaidCredits {
	pub(crate) amount: Cents,
}

impl PrepaidCredits {
	pub(crate) fn balance(&self) -> Cents {
		self.amount
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
			monthly_limit: Some(0.0.into()),
			used_credits: Some(500.0.into()),
		};
		let output = strip_ansi(extra.format(true).unwrap());
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
			monthly_limit: Some(10000.0.into()),
			used_credits: Some(2500.0.into()),
		};
		let output = strip_ansi(extra.format(true).unwrap());
		assert_eq!(output, "$25/$100");
	}

	#[test]
	fn extra_usage_null_fields_format_returns_none() {
		let extra = ExtraUsage {
			monthly_limit: None,
			used_credits: None,
		};
		assert!(extra.format(true).is_none());
		assert!(extra.format(false).is_none());
	}

	#[test]
	fn usage_response_deserializes_null_extra_fields() {
		let json = r#"{"extra_usage": {"monthly_limit": null, "used_credits": null}}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		let extra = resp.extra_usage.unwrap();
		assert!(extra.monthly_limit.is_none());
		assert!(extra.used_credits.is_none());
	}

	#[test]
	fn prepaid_credits_deserializes() {
		// The real API includes extra fields (currency, auto_reload_settings, …)
		// that we deliberately ignore — balance always renders in dollars.
		let json = r#"{
			"amount": 3304,
			"currency": "EUR",
			"auto_reload_settings": null,
			"pending_invoice_amount_cents": null,
			"last_paid_purchase_cents": null
		}"#;
		let credits: PrepaidCredits = serde_json::from_str(json).unwrap();
		assert_eq!(credits.balance().to_string(), "$33");
	}

	#[test]
	fn prepaid_credits_minimal() {
		let json = r#"{"amount": 1500}"#;
		let credits: PrepaidCredits = serde_json::from_str(json).unwrap();
		assert_eq!(credits.balance().to_string(), "$15");
	}
}
