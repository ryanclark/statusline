use crate::constants::GRAY;
use owo_colors::OwoColorize;

use super::{Icon, RenderContext, SegmentConfig, apply_style, format_icon};

const CREDITS_ICON: Icon = Icon {
	unicode: "\u{25C9}",
	nerd: "\u{f15a}",
};

pub(super) fn credits(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let resp = ctx.credits?.ok()?;

	let icon_str = format_icon(segment, CREDITS_ICON, GRAY, ctx.nerd_font);
	let balance = resp.balance();
	let text = if segment.colors() {
		format!("{icon_str}{}", format_args!("{balance}").bold())
	} else {
		format!("{icon_str}{balance}")
	};
	Some(apply_style(&text, segment.style()))
}
