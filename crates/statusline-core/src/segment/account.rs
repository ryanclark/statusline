use owo_colors::OwoColorize;

use crate::format::parse_color;

use super::{RenderContext, SegmentConfig, apply_style};

fn capitalize_first(s: &str) -> String {
	let mut chars = s.chars();
	match chars.next() {
		Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
		None => String::new(),
	}
}

pub(super) fn account(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let acct = ctx.account.as_ref()?;
	if acct.nickname.is_empty() {
		return None;
	}

	let display = if segment.capitalize() {
		capitalize_first(&acct.nickname)
	} else {
		acct.nickname.clone()
	};

	let text = if segment.colors() {
		match acct.color.as_deref().and_then(parse_color) {
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
	use crate::input::InputData;
	use crate::segment::{AccountDisplay, RenderContext, SegmentConfig, SegmentType};

	fn strip_ansi(s: &str) -> String {
		String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
	}

	#[test]
	fn capitalize_first_basic() {
		assert_eq!(capitalize_first("work"), "Work");
		assert_eq!(capitalize_first("PERSONAL"), "PERSONAL");
		assert_eq!(capitalize_first("two-words"), "Two-words");
		assert_eq!(capitalize_first(""), "");
	}

	#[test]
	fn render_account_capitalized() {
		let input = InputData::default();
		let ctx = RenderContext {
			input: &input,
			usage: None,
			credits: None,
			git: None,
			five_threshold: 70.0.into(),
			seven_threshold: 100.0.into(),
			divider: "·",
			nerd_font: false,
			account: Some(AccountDisplay {
				nickname: "work".into(),
				color: Some("cyan".into()),
			}),
		};
		let seg = SegmentConfig::Simple(SegmentType::Account);
		let output = strip_ansi(&account(&seg, &ctx).unwrap());
		assert_eq!(output, "Work");
	}
}
