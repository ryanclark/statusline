use crate::format::Cents;
use owo_colors::OwoColorize;
use owo_colors::XtermColors;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
	#[error("not logged in to claude.ai")]
	NotLoggedIn,
	#[error("{0}")]
	Other(String),
}

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
	pub extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize)]
pub struct ExtraUsage {
	#[serde(default)]
	pub monthly_limit: Option<Cents>,
	#[serde(default)]
	pub used_credits: Option<Cents>,
}

impl ExtraUsage {
	#[must_use]
	pub fn format(&self, colored: bool) -> Option<String> {
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
pub struct PrepaidCredits {
	pub amount: Cents,
}

impl PrepaidCredits {
	#[must_use]
	pub fn balance(&self) -> Cents {
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
