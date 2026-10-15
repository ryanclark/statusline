//! Versioned, provider-independent account usage exchanged with external commands.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ExternalUsageError {
	#[error("invalid usage source configuration")]
	Configuration,
	#[error("invalid usage snapshot")]
	Schema,
	#[error("usage account key mismatch")]
	Identity,
	#[error("invalid usage timestamp")]
	Timestamp,
	#[error("usage response too large")]
	PayloadSize,
	#[error("usage command failed")]
	ProcessExit,
	#[error("usage command timed out")]
	Timeout,
	#[error("usage cache unavailable")]
	Cache,
}

/// Invalid explicit configuration survives settings loading so it cannot enable another source.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum UsageSourceConfig {
	Valid(UsageSource),
	Invalid(serde_json::Value),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UsageSource {
	pub command: Vec<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub key_env: Option<String>,
	#[serde(default = "refresh_interval", with = "duration")]
	pub refresh_interval: Duration,
	#[serde(default = "max_age", with = "duration")]
	pub max_age: Duration,
	#[serde(default = "timeout", with = "duration")]
	pub timeout: Duration,
}
fn refresh_interval() -> Duration {
	Duration::from_secs(60)
}
fn max_age() -> Duration {
	Duration::from_secs(300)
}
fn timeout() -> Duration {
	Duration::from_secs(2)
}

impl UsageSource {
	pub fn validate(&self) -> Result<(), ExternalUsageError> {
		if self.command.is_empty()
			|| self.command[0].is_empty()
			|| self.command.len() > 64
			|| self.command.iter().map(String::len).sum::<usize>() > 64 * 1024
			|| self.command.iter().any(|s| s.contains('\0'))
			|| self
				.key_env
				.as_ref()
				.is_some_and(|s| s.is_empty() || s.contains(['=', '\0']))
			|| self.refresh_interval.is_zero()
			|| self.max_age.is_zero()
			|| self.timeout.is_zero()
			|| self.timeout > Duration::from_secs(60)
		{
			return Err(ExternalUsageError::Configuration);
		}
		Ok(())
	}
}

mod duration {
	use super::{Deserialize, Duration};
	pub fn serialize<S: serde::Serializer>(
		value: &Duration,
		serializer: S,
	) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&crate::format::format_window(*value))
	}
	pub fn deserialize<'de, D: serde::Deserializer<'de>>(
		deserializer: D,
	) -> Result<Duration, D::Error> {
		let value = String::deserialize(deserializer)?;
		crate::format::parse_duration(&value)
			.ok_or_else(|| serde::de::Error::custom("expected a duration such as 60s"))
	}
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UsageRequest {
	pub version: u32,
	pub key: String,
	pub session_id: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub model_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UsageSnapshot {
	pub version: u32,
	pub key: String,
	pub observed_at: DateTime<Utc>,
	pub account: ExternalAccount,
	#[serde(default, deserialize_with = "limits")]
	pub limits: Vec<ExternalLimit>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub spend: Option<ExternalSpend>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExternalAccount {
	pub id: String,
	pub label: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExternalLimit {
	pub id: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub label: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub used_percent: Option<f64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub window_seconds: Option<u64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExternalSpend {
	pub currency: String,
	pub exponent: u32,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub used_minor: Option<i64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub limit_minor: Option<i64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub balance_minor: Option<i64>,
}

/// `Some` in a render context owns account usage even while its first result is pending.
#[derive(Clone, Copy)]
pub struct ExternalUsage<'a> {
	pub snapshot: Option<&'a UsageSnapshot>,
	pub state: SnapshotState,
	pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotState {
	Fresh,
	Stale,
	Unusable,
}

fn limits<'de, D: serde::Deserializer<'de>>(
	deserializer: D,
) -> Result<Vec<ExternalLimit>, D::Error> {
	Ok(Option::<Vec<ExternalLimit>>::deserialize(deserializer)?.unwrap_or_default())
}

impl UsageSnapshot {
	pub fn parse(
		bytes: &[u8],
		expected_key: &str,
		_now: DateTime<Utc>,
	) -> Result<Self, ExternalUsageError> {
		if bytes.len() > MAX_RESPONSE_BYTES {
			return Err(ExternalUsageError::PayloadSize);
		}
		let value: serde_json::Value =
			serde_json::from_slice(bytes).map_err(|_| ExternalUsageError::Schema)?;
		if value
			.get("observed_at")
			.and_then(serde_json::Value::as_str)
			.and_then(|at| DateTime::parse_from_rfc3339(at).ok())
			.is_none()
		{
			return Err(ExternalUsageError::Timestamp);
		}
		let snapshot: Self =
			serde_json::from_value(value).map_err(|_| ExternalUsageError::Schema)?;
		if snapshot.key.is_empty() || snapshot.key != expected_key {
			return Err(ExternalUsageError::Identity);
		}
		let mut ids = std::collections::BTreeSet::new();
		if snapshot.version != 1
			|| snapshot.account.id.trim().is_empty()
			|| snapshot.limits.iter().any(|limit| {
				limit.id.is_empty()
					|| !ids.insert(&limit.id)
					|| limit
						.used_percent
						.is_some_and(|value| !value.is_finite() || value < 0.0)
			}) || snapshot.spend.as_ref().is_some_and(|spend| {
			spend.exponent > 18
				|| spend.currency.len() != 3
				|| !spend.currency.bytes().all(|c| c.is_ascii_uppercase())
		}) {
			return Err(ExternalUsageError::Schema);
		}
		Ok(snapshot)
	}
	#[must_use]
	pub fn state(&self, now: DateTime<Utc>, max_age: Duration) -> SnapshotState {
		match (now - self.observed_at).to_std() {
			Err(_) => SnapshotState::Unusable,
			Ok(age) if age > max_age => SnapshotState::Stale,
			Ok(_) => SnapshotState::Fresh,
		}
	}
	#[must_use]
	pub fn limit(&self, id: &str) -> Option<&ExternalLimit> {
		self.limits.iter().find(|limit| limit.id == id)
	}
}
impl ExternalSpend {
	#[must_use]
	pub fn format(&self, minor: i64) -> String {
		let sign = if minor < 0 { "-" } else { "" };
		let absolute = minor.unsigned_abs();
		let Some(scale) = 10_u64.checked_pow(self.exponent) else {
			return String::new();
		};
		let amount = if self.exponent == 0 {
			format!("{sign}{absolute}")
		} else {
			format!(
				"{sign}{}.{:0width$}",
				absolute / scale,
				absolute % scale,
				width = usize::try_from(self.exponent).unwrap_or(0)
			)
		};
		format!("{} {amount}", self.currency)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::TimeDelta;
	use serde_json::json;

	fn fixture() -> serde_json::Value {
		json!({"version":1,"key":"account-key","observed_at":"2026-10-15T12:00:00Z",
			"account":{"id":"provider:organization:user","label":"work","color":"#64c8dc"},
			"limits":[{"id":"five_hour","label":"5h","used_percent":12.5,"window_seconds":18000,"resets_at":"2026-10-15T15:00:00Z"},
				{"id":"custom","label":"Burst","used_percent":125.5}],
			"spend":{"currency":"USD","exponent":2,"used_minor":1234,"limit_minor":5000,"balance_minor":8000},"future_field":true})
	}
	fn now() -> DateTime<Utc> {
		"2026-10-15T12:00:00Z".parse().unwrap()
	}
	fn parse(value: &serde_json::Value) -> Result<UsageSnapshot, ExternalUsageError> {
		UsageSnapshot::parse(&serde_json::to_vec(value).unwrap(), "account-key", now())
	}
	#[test]
	fn external_usage_documented_fixture_conforms() {
		let bytes = include_bytes!("../tests/fixtures/external-usage-v1.json");
		let snapshot = UsageSnapshot::parse(bytes, "work", now()).unwrap();
		assert_eq!(snapshot.limit("burst").unwrap().used_percent, Some(42.0));
		let mut malformed: serde_json::Value = serde_json::from_slice(bytes).unwrap();
		malformed["observed_at"] = json!("invalid time");
		assert_eq!(
			UsageSnapshot::parse(&serde_json::to_vec(&malformed).unwrap(), "work", now())
				.unwrap_err(),
			ExternalUsageError::Timestamp
		);
	}

	#[test]
	fn external_usage_protocol_preserves_exact_units_and_arbitrary_limits() {
		let snapshot = parse(&fixture()).unwrap();
		assert_eq!(
			snapshot.limit("five_hour").unwrap().used_percent,
			Some(12.5)
		);
		assert_eq!(snapshot.limit("custom").unwrap().used_percent, Some(125.5));
		assert!(snapshot.limit("seven_day").is_none());
		assert_eq!(snapshot.spend.as_ref().unwrap().used_minor, Some(1234));
		let mut value = fixture();
		value["limits"] = json!([{"id":"custom","used_percent":null,"resets_at":null}]);
		value["spend"] = serde_json::Value::Null;
		assert!(
			parse(&value)
				.unwrap()
				.limit("custom")
				.unwrap()
				.used_percent
				.is_none()
		);
		value["limits"] = serde_json::Value::Null;
		assert!(parse(&value).unwrap().limits.is_empty());
	}
	#[test]
	fn external_usage_rejects_wrong_identity_version_and_oversized_payloads() {
		assert_eq!(
			UsageSnapshot::parse(
				&serde_json::to_vec(&fixture()).unwrap(),
				"another-account",
				now()
			)
			.unwrap_err(),
			ExternalUsageError::Identity
		);
		for (field, value) in [
			("version", json!(2)),
			("account", json!({"id":"","label":"work"})),
			("limits", json!([{"id":"x"},{"id":"x"}])),
		] {
			let mut bad = fixture();
			bad[field] = value;
			assert!(parse(&bad).is_err());
		}
		assert_eq!(
			UsageSnapshot::parse(&vec![b' '; MAX_RESPONSE_BYTES + 1], "a", now()).unwrap_err(),
			ExternalUsageError::PayloadSize
		);
	}
	#[test]
	fn external_usage_freshness_uses_original_observation_time() {
		let snapshot = parse(&fixture()).unwrap();
		assert_eq!(
			snapshot.state(now() + TimeDelta::seconds(300), max_age()),
			SnapshotState::Fresh
		);
		assert_eq!(
			snapshot.state(now() + TimeDelta::seconds(301), max_age()),
			SnapshotState::Stale
		);
		assert_eq!(
			snapshot.state(now() - TimeDelta::seconds(1), max_age()),
			SnapshotState::Unusable
		);
	}
	#[test]
	fn external_usage_spending_formats_currency_without_float_rounding() {
		let mut spend = parse(&fixture()).unwrap().spend.unwrap();
		spend.currency = "EUR".into();
		assert_eq!(spend.format(1234), "EUR 12.34");
		spend.currency = "JPY".into();
		spend.exponent = 0;
		assert_eq!(spend.format(1234), "JPY 1234");
		assert_eq!(spend.format(i64::MIN), "JPY -9223372036854775808");
	}
	#[test]
	fn external_usage_source_defaults_and_invalid_settings_survive_loading() {
		let source: UsageSource = serde_json::from_value(json!({"command":["provider"]})).unwrap();
		assert_eq!(
			(source.refresh_interval, source.max_age, source.timeout),
			(
				Duration::from_secs(60),
				Duration::from_secs(300),
				Duration::from_secs(2)
			)
		);
		assert!(source.validate().is_ok());
		let invalid: UsageSourceConfig =
			serde_json::from_value(json!({"command":"not argv"})).unwrap();
		assert!(matches!(invalid, UsageSourceConfig::Invalid(_)));
		let mut invalid = source;
		invalid.max_age = Duration::ZERO;
		assert!(invalid.validate().is_err());
	}
}
