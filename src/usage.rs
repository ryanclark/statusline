use owo_colors::OwoColorize;
use crate::format::{Cents, ColoredPercentage, Percentage};
use eyre::{Context, Result};
use owo_colors::XtermColors;
use serde::Deserialize;
use std::fmt;
use std::fmt::Formatter;
use crate::chrome::load_session_key;
use crate::constants::{DIVIDER, FIVE_HOUR_ICON, SEVEN_DAY_ICON};
use std::time::Duration;

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn fetch_usage(org_id: &str) -> Result<UsageResponse> {
	let session_key = load_session_key()?;

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

	let body = response.into_body().read_to_string().context("reading response body")?;

	serde_json::from_str(&body).context("parsing usage response JSON")
}

pub(crate) struct UsageDisplay<'a> {
	response: &'a UsageResponse,
	five_threshold: Percentage,
	seven_threshold: Percentage,
}

impl fmt::Display for UsageDisplay<'_> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		if let Some(ref five_hour) = self.response.five_hour {
			fmt_usage_period(f, FIVE_HOUR_ICON, five_hour, self.five_threshold)?;
		}

		if let Some(ref seven_day) = self.response.seven_day {
			fmt_usage_period(f, SEVEN_DAY_ICON, seven_day, self.seven_threshold)?;
		}

		if let Some(ref extra) = self.response.extra_usage {
			write!(f, "{} ", DIVIDER.color(XtermColors::ScorpionGray))?;
			write!(f, "{extra}")?;
		}

		Ok(())
	}
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageResponse {
	five_hour: Option<UsagePeriod>,
	seven_day: Option<UsagePeriod>,
	extra_usage: Option<ExtraUsage>,
}

impl UsageResponse {
	pub(crate) fn display(&self, five: Percentage, seven: Percentage) -> UsageDisplay<'_> {
		UsageDisplay { response: self, five_threshold: five, seven_threshold: seven }
	}
}

#[derive(Debug, Deserialize)]
struct UsagePeriod {
	utilization: Percentage,
	resets_at: ResetTime,
}

fn fmt_usage_period(f: &mut fmt::Formatter<'_>, icon: &str, period: &UsagePeriod, threshold: Percentage) -> fmt::Result {
	write!(f, "{} {} ", icon.color(XtermColors::LightGray), ColoredPercentage(period.utilization))?;

	if period.utilization > threshold {
		write!(f, "{} ", period.resets_at)?;
	}

	Ok(())
}

#[derive(Debug, Deserialize)]
struct ResetTime(chrono::DateTime<chrono::FixedOffset>);

impl ResetTime {
	fn duration_until(&self, now: chrono::DateTime<impl chrono::TimeZone>) -> Option<String> {
		let total_secs = self.0.signed_duration_since(now).num_seconds();

		if total_secs <= 0 {
			return None;
		}

		let days = total_secs / 86400;
		let hours = (total_secs % 86400) / 3600;
		let mins = (total_secs % 3600) / 60;
		let secs = total_secs % 60;

		Some(if days > 0 {
			format!("{days}d{hours}h")
		} else if hours > 0 {
			format!("{hours}h{mins}m")
		} else if mins > 0 {
			format!("{mins}m")
		} else {
			format!("{secs}s")
		})
	}
}

impl fmt::Display for ResetTime {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		if let Some(time_str) = self.duration_until(chrono::Utc::now()) {
			write!(f, "{}", time_str.color(XtermColors::LightGray))?;
		}

		Ok(())
	}
}

#[derive(Debug, Deserialize)]
struct ExtraUsage {
	monthly_limit: Cents,
	used_credits: Cents,
}

impl fmt::Display for ExtraUsage {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		let percentage = self.used_credits.as_percentage_of(self.monthly_limit);

		write!(f, "{}", format_args!("{}", self.used_credits).color(percentage.color()).bold())?;
		write!(f, "{}", format_args!("/{}", self.monthly_limit).color(XtermColors::LightGray))?;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn usage_response_deserializes_full() {
		let json = r#"{
			"five_hour": {
				"utilization": 42.5,
				"resets_at": "2026-03-30T12:00:00+00:00"
			},
			"seven_day": {
				"utilization": 75.0,
				"resets_at": "2026-04-05T12:00:00+00:00"
			},
			"extra_usage": {
				"monthly_limit": 10000.0,
				"used_credits": 2500.0
			}
		}"#;

		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		let five_hour = resp.five_hour.unwrap();
		assert_eq!(five_hour.utilization, 42.5.into());
		let seven_day = resp.seven_day.unwrap();
		assert_eq!(seven_day.utilization, 75.0.into());
		assert!(resp.extra_usage.is_some());
	}

	#[test]
	fn usage_response_deserializes_minimal() {
		let json = r#"{}"#;
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		assert!(resp.five_hour.is_none());
		assert!(resp.seven_day.is_none());
		assert!(resp.extra_usage.is_none());
	}

	fn strip_ansi(s: String) -> String {
		String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
	}

	fn render_usage(json: &str, five: f64, seven: f64) -> String {
		let resp: UsageResponse = serde_json::from_str(json).unwrap();
		let display = resp.display(five.into(), seven.into());
		strip_ansi(format!("{display}"))
	}

	#[test]
	fn extra_usage_zero_limit_shows_dollar_amounts() {
		let extra = ExtraUsage {
			monthly_limit: 0.0.into(),
			used_credits: 500.0.into(),
		};
		let output = strip_ansi(format!("{extra}"));
		assert!(output.contains("$5"), "should show used credits, got: {output}");
		assert!(output.contains("$0"), "should show zero limit, got: {output}");
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

	#[test]
	fn usage_display_both_periods_below_threshold() {
		let output = render_usage(
			r#"{
				"five_hour": {"utilization": 42.5, "resets_at": "2099-03-30T12:00:00+00:00"},
				"seven_day": {"utilization": 75.0, "resets_at": "2099-04-05T12:00:00+00:00"}
			}"#,
			100.0,
			100.0,
		);
		assert!(output.contains("42.5%"), "should show five-hour utilization, got: {output}");
		assert!(output.contains("75%"), "should show seven-day utilization, got: {output}");
	}

	#[test]
	fn usage_display_above_threshold_shows_reset_time() {
		let output = render_usage(
			r#"{
				"five_hour": {"utilization": 80.0, "resets_at": "2099-03-30T12:00:00+00:00"}
			}"#,
			70.0,
			100.0,
		);
		assert!(output.contains("80%"), "got: {output}");
		assert!(output.contains("d"), "above threshold should show reset time, got: {output}");
	}

	#[test]
	fn usage_display_empty_response() {
		let output = render_usage(r#"{}"#, 70.0, 100.0);
		assert_eq!(output, "");
	}

	#[test]
	fn usage_display_with_extra_usage() {
		let output = render_usage(
			r#"{"extra_usage": {"monthly_limit": 10000.0, "used_credits": 2500.0}}"#,
			70.0,
			100.0,
		);
		assert!(output.contains("$25"), "should show used credits, got: {output}");
		assert!(output.contains("$100"), "should show monthly limit, got: {output}");
	}

	fn fixed_now() -> chrono::DateTime<chrono::Utc> {
		chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00")
			.unwrap()
			.to_utc()
	}

	fn reset_time(offset_secs: i64) -> ResetTime {
		ResetTime((fixed_now() + chrono::Duration::seconds(offset_secs)).fixed_offset())
	}

	#[test]
	fn duration_until_days_and_hours() {
		assert_eq!(reset_time(90061).duration_until(fixed_now()), Some("1d1h".to_owned()));
	}

	#[test]
	fn duration_until_hours_and_minutes() {
		assert_eq!(reset_time(7260).duration_until(fixed_now()), Some("2h1m".to_owned()));
	}

	#[test]
	fn duration_until_minutes_only() {
		assert_eq!(reset_time(300).duration_until(fixed_now()), Some("5m".to_owned()));
	}

	#[test]
	fn duration_until_past_is_none() {
		assert_eq!(reset_time(-3600).duration_until(fixed_now()), None);
	}

	#[test]
	fn duration_until_exactly_zero_is_none() {
		assert_eq!(reset_time(0).duration_until(fixed_now()), None);
	}

	#[test]
	fn duration_until_one_second() {
		assert_eq!(reset_time(1).duration_until(fixed_now()), Some("1s".to_owned()));
	}

	#[test]
	fn duration_until_59_seconds() {
		assert_eq!(reset_time(59).duration_until(fixed_now()), Some("59s".to_owned()));
	}

	#[test]
	fn duration_until_60_seconds_is_one_minute() {
		assert_eq!(reset_time(60).duration_until(fixed_now()), Some("1m".to_owned()));
	}
}
