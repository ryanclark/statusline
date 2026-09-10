//! The countdown and clock time shown next to a reset or expiry instant.

use std::fmt;

use chrono::{DateTime, Local, TimeZone, Utc};

use super::{SegmentConfig, TimeFormat};

/// What a segment shows for the future instant `at`: the countdown, the local clock time, both,
/// or nothing, as its options ask. `None` once the instant has passed.
pub(super) fn reset_hint(segment: &SegmentConfig, at: i64, now: DateTime<Utc>) -> Option<String> {
	reset_hint_in(segment, at, now, &Local)
}

pub(super) fn reset_hint_in<Tz: TimeZone>(
	segment: &SegmentConfig,
	at: i64,
	now: DateTime<Utc>,
	tz: &Tz,
) -> Option<String>
where
	Tz::Offset: fmt::Display,
{
	let countdown = crate::format::countdown_to(at, now)?;
	let clock = if segment.show_time() {
		clock_time(at, segment.time_format(), tz)
	} else {
		None
	};

	match (segment.show_countdown(), clock) {
		(true, Some(clock)) => Some(format!("{countdown} ({clock})")),
		(true, None) => Some(countdown),
		(false, Some(clock)) => Some(clock),
		(false, None) => None,
	}
}

fn clock_time<Tz: TimeZone>(at: i64, format: TimeFormat, tz: &Tz) -> Option<String>
where
	Tz::Offset: fmt::Display,
{
	let at = DateTime::from_timestamp(at, 0)?.with_timezone(tz);
	let text = match format {
		TimeFormat::H24 => at.format("%H:%M"),
		TimeFormat::H12 => at.format("%-I:%M%P"),
	};

	Some(text.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::format::countdown_to;
	use crate::segment::SegmentType;
	use chrono::FixedOffset;

	fn now() -> DateTime<Utc> {
		Utc.with_ymd_and_hms(2026, 3, 30, 12, 0, 0).unwrap()
	}

	fn tz() -> FixedOffset {
		FixedOffset::east_opt(3600).unwrap()
	}

	// 14:10 UTC, so 15:10 in the test zone.
	fn at() -> i64 {
		now().timestamp() + 2 * 3600 + 10 * 60
	}

	fn five_hour(json_options: &str) -> SegmentConfig {
		serde_json::from_str(&format!(r#"{{"type":"five_hour"{json_options}}}"#)).unwrap()
	}

	#[test]
	fn rate_limits_show_only_the_countdown_by_default() {
		let seg = SegmentConfig::Simple(SegmentType::FiveHour);
		assert_eq!(
			reset_hint_in(&seg, at(), now(), &tz()),
			countdown_to(at(), now())
		);
	}

	#[test]
	fn the_clock_follows_the_countdown_when_asked() {
		let seg = five_hour(r#","show_time":true"#);
		let countdown = countdown_to(at(), now()).unwrap();
		assert_eq!(
			reset_hint_in(&seg, at(), now(), &tz()).as_deref(),
			Some(format!("{countdown} (15:10)").as_str())
		);
	}

	#[test]
	fn twelve_hour_clocks_drop_the_leading_zero_and_add_the_period() {
		let seg = five_hour(r#","show_time":true,"time_format":"12h""#);
		let hint = reset_hint_in(&seg, at(), now(), &tz()).unwrap();
		assert!(hint.ends_with("(3:10pm)"), "{hint}");
		let morning = at() - 12 * 3600;
		let hint =
			reset_hint_in(&seg, morning, now() - chrono::Duration::hours(12), &tz()).unwrap();
		assert!(hint.ends_with("(3:10am)"), "{hint}");
	}

	#[test]
	fn the_countdown_can_be_hidden_leaving_the_clock() {
		let seg = five_hour(r#","show_time":true,"show_countdown":false"#);
		assert_eq!(
			reset_hint_in(&seg, at(), now(), &tz()).as_deref(),
			Some("15:10")
		);
	}

	#[test]
	fn nothing_shows_when_both_are_off() {
		let seg = five_hour(r#","show_countdown":false"#);
		assert_eq!(reset_hint_in(&seg, at(), now(), &tz()), None);
	}

	#[test]
	fn cache_warm_shows_the_clock_by_default() {
		let seg = SegmentConfig::Simple(SegmentType::CacheWarm);
		let countdown = countdown_to(at(), now()).unwrap();
		assert_eq!(
			reset_hint_in(&seg, at(), now(), &tz()).as_deref(),
			Some(format!("{countdown} (15:10)").as_str())
		);
		let off: SegmentConfig =
			serde_json::from_str(r#"{"type":"cache_warm","show_time":false}"#).unwrap();
		assert_eq!(
			reset_hint_in(&off, at(), now(), &tz()).as_deref(),
			Some(countdown.as_str())
		);
	}

	#[test]
	fn a_passed_instant_shows_nothing_even_with_the_clock_on() {
		let seg = five_hour(r#","show_time":true"#);
		assert_eq!(
			reset_hint_in(&seg, now().timestamp() - 1, now(), &tz()),
			None
		);
	}
}
