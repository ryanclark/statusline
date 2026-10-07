use statusline_core::catalog::OptionSet;
use statusline_core::segment::Within;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
	Colors,
	Icon,
	IconColor,
	Label,
	Style,
	Dirty,
	DirtyColor,
	WarmColor,
	ColdColor,
	Capitalize,
	ShowCountdown,
	ShowTime,
	TimeFormat,
	Within,
	Details,
}

#[must_use]
pub fn applicable_fields(set: OptionSet) -> Vec<OptionKind> {
	let mut fields = Vec::new();

	if set.colors {
		fields.push(OptionKind::Colors);
	}
	if set.icon {
		fields.push(OptionKind::Icon);
		fields.push(OptionKind::IconColor);
	}
	if set.label {
		fields.push(OptionKind::Label);
	}
	if set.style {
		fields.push(OptionKind::Style);
	}
	if set.cache_state {
		fields.push(OptionKind::WarmColor);
		fields.push(OptionKind::ColdColor);
	}
	if set.countdown {
		fields.push(OptionKind::ShowCountdown);
		fields.push(OptionKind::ShowTime);
		fields.push(OptionKind::TimeFormat);
	}
	if set.dirty {
		fields.push(OptionKind::Dirty);
		fields.push(OptionKind::DirtyColor);
	}
	if set.capitalize {
		fields.push(OptionKind::Capitalize);
	}
	if set.within {
		fields.push(OptionKind::Within);
	}
	if set.details {
		fields.push(OptionKind::Details);
	}

	fields
}

/// The windows `within` steps through. A hand-written window steps to the next larger preset.
const WITHIN_PRESETS: &[Within] = &[
	Within::Window(Duration::from_secs(5 * 60)),
	Within::Window(Duration::from_secs(15 * 60)),
	Within::Window(Within::DEFAULT_WINDOW),
	Within::Window(Duration::from_secs(60 * 60)),
	Within::Window(Duration::from_secs(2 * 60 * 60)),
	Within::Session,
];

#[must_use]
pub fn next_within(current: Option<&Within>) -> Within {
	let next = match current.map_or(Some(Within::DEFAULT_WINDOW), Within::duration) {
		None => WITHIN_PRESETS.first(),
		Some(window) => WITHIN_PRESETS
			.iter()
			.find(|w| w.duration().is_none_or(|preset| preset > window)),
	};

	next.cloned().unwrap_or(Within::Session)
}

#[must_use]
pub fn next_style(current: Option<&str>) -> Option<String> {
	use statusline_core::segment::STYLES;

	let next = match current {
		None => STYLES.first(),
		Some(cur) => STYLES
			.iter()
			.position(|s| *s == cur)
			.and_then(|i| STYLES.get(i + 1)),
	};

	next.map(|s| (*s).to_owned())
}

#[cfg(test)]
mod tests {
	use super::*;
	use statusline_core::catalog::meta;
	use statusline_core::segment::SegmentType;

	#[test]
	fn git_branch_fields_include_dirty_pair() {
		let set = meta(&SegmentType::GitBranch).options;
		let fields = applicable_fields(set);
		assert_eq!(
			fields,
			vec![
				OptionKind::Colors,
				OptionKind::Style,
				OptionKind::Dirty,
				OptionKind::DirtyColor,
			]
		);
	}

	#[test]
	fn cache_warm_fields_include_state_colors() {
		let set = meta(&SegmentType::CacheWarm).options;
		assert_eq!(
			applicable_fields(set),
			vec![
				OptionKind::Colors,
				OptionKind::Icon,
				OptionKind::IconColor,
				OptionKind::Label,
				OptionKind::Style,
				OptionKind::WarmColor,
				OptionKind::ColdColor,
				OptionKind::ShowCountdown,
				OptionKind::ShowTime,
				OptionKind::TimeFormat,
			]
		);
	}

	#[test]
	fn rate_limit_fields_include_the_countdown_options() {
		let set = meta(&SegmentType::FiveHour).options;
		assert_eq!(
			applicable_fields(set),
			vec![
				OptionKind::Colors,
				OptionKind::Icon,
				OptionKind::IconColor,
				OptionKind::Label,
				OptionKind::Style,
				OptionKind::ShowCountdown,
				OptionKind::ShowTime,
				OptionKind::TimeFormat,
			]
		);
	}

	#[test]
	fn icon_flag_expands_to_icon_and_icon_color() {
		let set = meta(&SegmentType::TotalInputTokens).options;
		let fields = applicable_fields(set);
		assert_eq!(
			fields,
			vec![
				OptionKind::Colors,
				OptionKind::Icon,
				OptionKind::IconColor,
				OptionKind::Label,
				OptionKind::Style,
			]
		);
	}

	#[test]
	fn divider_exposes_colors_only() {
		let set = meta(&SegmentType::Divider).options;
		assert_eq!(applicable_fields(set), vec![OptionKind::Colors]);
	}

	#[test]
	fn account_has_capitalize() {
		let set = meta(&SegmentType::Account).options;
		let fields = applicable_fields(set);
		assert_eq!(
			fields,
			vec![
				OptionKind::Colors,
				OptionKind::Style,
				OptionKind::Capitalize
			]
		);
	}

	#[test]
	fn cache_miss_fields_end_with_within_and_details() {
		let fields = applicable_fields(meta(&SegmentType::CacheMisses).options);
		assert_eq!(
			fields,
			vec![OptionKind::Colors, OptionKind::Style, OptionKind::Within]
		);
		let fields = applicable_fields(meta(&SegmentType::CacheLastMiss).options);
		assert_eq!(
			fields,
			vec![
				OptionKind::Colors,
				OptionKind::Style,
				OptionKind::Within,
				OptionKind::Details,
			]
		);
	}

	#[test]
	fn within_cycles_through_the_presets() {
		let mut seen = Vec::new();
		let mut current: Option<Within> = None;
		for _ in 0..WITHIN_PRESETS.len() {
			let next = next_within(current.as_ref());
			seen.push(next.to_string());
			current = Some(next);
		}
		assert_eq!(seen, vec!["1h", "2h", "session", "5m", "15m", "30m"]);
		let mins = |m: u64| Within::Window(Duration::from_secs(m * 60));
		assert_eq!(next_within(Some(&mins(45))), mins(60));
		assert_eq!(next_within(Some(&mins(24 * 60))), Within::Session);
		assert_eq!(next_within(Some(&Within::parse("soon"))), mins(60));
	}

	#[test]
	fn next_style_cycles() {
		assert_eq!(next_style(None).as_deref(), Some("bold"));
		assert_eq!(next_style(Some("bold")).as_deref(), Some("dim"));
		assert_eq!(next_style(Some("dim")).as_deref(), Some("italic"));
		assert_eq!(next_style(Some("italic")).as_deref(), Some("underline"));
		assert_eq!(next_style(Some("underline")), None);
	}
}
