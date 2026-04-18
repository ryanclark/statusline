use owo_colors::OwoColorize;

use crate::accounts::{find_for_identity, live_identity, load};
use crate::format::parse_color;

use super::{RenderContext, SegmentConfig, apply_style};

fn capitalize_first(s: &str) -> String {
	let mut chars = s.chars();
	match chars.next() {
		Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
		None => String::new(),
	}
}

pub(super) fn account(segment: &SegmentConfig, _ctx: &RenderContext<'_>) -> Option<String> {
	let (email, org_uuid) = live_identity()?;
	let file = load()?;
	let entry = find_for_identity(&file, &email, &org_uuid)?;

	let display = if segment.capitalize() {
		capitalize_first(&entry.nickname)
	} else {
		entry.nickname.clone()
	};

	let text = if segment.colors() {
		match entry.color.as_deref().and_then(parse_color) {
			Some(c) => format!("{}", display.color(c)),
			None => display.clone(),
		}
	} else {
		display.clone()
	};

	Some(apply_style(&text, segment.style()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn capitalize_first_basic() {
		assert_eq!(capitalize_first("work"), "Work");
		assert_eq!(capitalize_first("PERSONAL"), "PERSONAL");
		assert_eq!(capitalize_first("two-words"), "Two-words");
		assert_eq!(capitalize_first(""), "");
	}
}
