use crate::format::{Cents, Percentage};
use chrono::{DateTime, Utc};
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
	#[serde(default)]
	pub limits: Vec<Limit>,
}

impl UsageResponse {
	#[must_use]
	pub fn model_limit(&self, display_name: &str) -> Option<&Limit> {
		self.limits.iter().find(|limit| {
			limit
				.scope
				.as_ref()
				.and_then(|scope| scope.model.as_ref())
				.is_some_and(|model| model.display_name.eq_ignore_ascii_case(display_name))
		})
	}

	#[must_use]
	pub fn fable(&self) -> Option<&Limit> {
		self.model_limit("Fable")
	}
}

#[derive(Debug, Deserialize)]
pub struct Limit {
	#[serde(default)]
	pub kind: String,
	#[serde(default)]
	pub percent: Percentage,
	#[serde(default)]
	pub resets_at: Option<String>,
	#[serde(default)]
	pub scope: Option<LimitScope>,
}

impl Limit {
	#[must_use]
	pub fn countdown(&self, now: DateTime<Utc>) -> Option<String> {
		let reset_time = DateTime::parse_from_rfc3339(self.resets_at.as_deref()?).ok()?;
		let total_secs = reset_time.signed_duration_since(now).num_seconds();
		if total_secs <= 0 {
			return None;
		}

		#[allow(clippy::cast_sign_loss)]
		Some(crate::format::format_duration_secs(total_secs as u64))
	}

	#[must_use]
	pub fn model_name(&self) -> Option<&str> {
		self.scope
			.as_ref()?
			.model
			.as_ref()
			.map(|m| m.display_name.as_str())
			.filter(|name| !name.is_empty())
	}

	#[must_use]
	pub fn label(&self) -> String {
		if let Some(model) = self.model_name() {
			return format!("{model} (weekly)");
		}
		match self.kind.as_str() {
			"session" => "5-hour session".to_owned(),
			"weekly_all" => "7-day (all)".to_owned(),
			"" => "limit".to_owned(),
			other => other.replace('_', " "),
		}
	}
}

#[derive(Debug, Deserialize)]
pub struct LimitScope {
	#[serde(default)]
	pub model: Option<ScopeModel>,
}

#[derive(Debug, Deserialize)]
pub struct ScopeModel {
	#[serde(default)]
	pub display_name: String,
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
	fn fable_limit_found_by_scope_and_percent_parses_from_integer() {
		let json = r#"{
			"limits": [
				{"kind": "session", "percent": 12, "resets_at": "2026-07-16T17:30:00+00:00", "scope": null},
				{"kind": "weekly_all", "percent": 40, "resets_at": "2026-07-19T23:00:00+00:00", "scope": null},
				{"kind": "weekly_scoped", "percent": 37, "resets_at": null,
				 "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}}
			]
		}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		let fable = resp.fable().expect("Fable-scoped limit should be found");
		assert_eq!(fable.percent, Percentage::from(37.0));
		assert!(fable.resets_at.is_none());
	}

	#[test]
	fn model_limit_matches_case_insensitively() {
		let json = r#"{"limits": [{"percent": 5, "scope": {"model": {"display_name": "fABLE"}}}]}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		assert!(resp.model_limit("Fable").is_some());
		assert!(resp.model_limit("Opus").is_none());
	}

	#[test]
	fn fable_absent_when_no_model_scoped_limit() {
		let json = r#"{"limits": [{"kind": "session", "percent": 1, "scope": null}]}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		assert!(resp.fable().is_none());
	}

	#[test]
	fn limit_countdown_parses_fractional_rfc3339() {
		let now = DateTime::parse_from_rfc3339("2026-07-16T17:30:00+00:00")
			.unwrap()
			.with_timezone(&Utc);
		let limit = Limit {
			kind: "weekly_scoped".to_owned(),
			percent: Percentage::from(37.0),
			resets_at: Some("2026-07-16T19:30:00.365824+00:00".to_owned()),
			scope: None,
		};
		assert_eq!(limit.countdown(now), Some("2h0m".to_owned()));
	}

	#[test]
	fn limit_labels_cover_session_weekly_and_scoped() {
		let json = r#"{
			"limits": [
				{"kind": "session", "percent": 1},
				{"kind": "weekly_all", "percent": 2},
				{"kind": "weekly_scoped", "percent": 3, "scope": {"model": {"display_name": "Fable"}}}
			]
		}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		let labels: Vec<String> = resp.limits.iter().map(Limit::label).collect();
		assert_eq!(labels, ["5-hour session", "7-day (all)", "Fable (weekly)"]);
		assert_eq!(resp.limits[2].model_name(), Some("Fable"));
		assert!(resp.limits[0].model_name().is_none());
	}

	#[test]
	fn limit_countdown_none_for_null_or_past() {
		let now = DateTime::parse_from_rfc3339("2026-07-16T17:30:00+00:00")
			.unwrap()
			.with_timezone(&Utc);
		let null_reset = Limit {
			kind: "weekly_scoped".to_owned(),
			percent: Percentage::from(0.0),
			resets_at: None,
			scope: None,
		};
		assert_eq!(null_reset.countdown(now), None);
		let past = Limit {
			kind: "session".to_owned(),
			percent: Percentage::from(0.0),
			resets_at: Some("2026-07-16T17:00:00+00:00".to_owned()),
			scope: None,
		};
		assert_eq!(past.countdown(now), None);
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
