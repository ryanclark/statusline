use crate::constants::{
	FABLE_USAGE_ICON, FIVE_HOUR_ICON, GRAY, RED, SEVEN_DAY_ICON, SPEND_LIMIT_ICON, YELLOW,
};
use crate::format::{ColoredPercentage, Percentage};
use crate::input::RateLimitPeriod;
use crate::usage::UsageError;
use chrono::Utc;
use owo_colors::OwoColorize;

use super::timing::reset_hint;
use super::{Icon, RenderContext, SegmentConfig, apply_style, format_icon};

fn format_rate_limit(
	segment: &SegmentConfig,
	period: Option<&RateLimitPeriod>,
	icon: Icon,
	threshold: Percentage,
	nerd_font: bool,
) -> Option<String> {
	let period = period?;

	let icon_str = format_icon(segment, icon, GRAY, nerd_font);

	let pct = if segment.colors() {
		format!("{}", ColoredPercentage(period.used_percentage))
	} else {
		format!("{}", period.used_percentage)
	};

	let reset = if period.used_percentage > threshold {
		reset_hint(segment, period.resets_at, Utc::now())
			.map(|hint| format!(" {}", hint.dimmed()))
			.unwrap_or_default()
	} else {
		String::new()
	};

	let text = format!("{icon_str}{pct}{reset}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn five_hour(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	format_rate_limit(
		segment,
		ctx.input.rate_limits.five_hour.as_ref(),
		Icon {
			unicode: FIVE_HOUR_ICON,
			nerd: "\u{f017}",
		},
		ctx.five_threshold,
		ctx.nerd_font,
	)
}

pub(super) fn seven_day(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	format_rate_limit(
		segment,
		ctx.input.rate_limits.seven_day.as_ref(),
		Icon {
			unicode: SEVEN_DAY_ICON,
			nerd: "\u{f073}",
		},
		ctx.seven_threshold,
		ctx.nerd_font,
	)
}

pub(super) fn spend_limit(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	// A gateway spend limit has no reset-threshold setting, so the countdown shows whenever there is
	// any usage at all.
	format_rate_limit(
		segment,
		ctx.input.rate_limits.spend_limit.as_ref(),
		Icon {
			unicode: SPEND_LIMIT_ICON,
			nerd: "\u{f0d6}",
		},
		Percentage::default(),
		ctx.nerd_font,
	)
}

pub(super) fn fable_usage(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let limit = ctx.usage?.ok()?.fable()?;

	let icon_str = format_icon(
		segment,
		Icon {
			unicode: FABLE_USAGE_ICON,
			nerd: "\u{f02d}",
		},
		GRAY,
		ctx.nerd_font,
	);

	let pct = if segment.colors() {
		format!("{}", ColoredPercentage(limit.percent))
	} else {
		format!("{}", limit.percent)
	};

	let reset = limit
		.resets_at_epoch()
		.and_then(|at| reset_hint(segment, at, Utc::now()))
		.map(|hint| format!(" {}", hint.dimmed()))
		.unwrap_or_default();

	let text = format!("{icon_str}{pct}{reset}");

	Some(apply_style(&text, segment.style()))
}

pub(super) fn extra_usage(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let result = ctx.usage?;

	match result {
		Ok(resp) => {
			let extra = resp.extra_usage.as_ref()?;
			let text = extra.format(segment.colors())?;
			Some(apply_style(&text, segment.style()))
		}
		Err(e) => {
			let (icon, color, msg) = match e {
				UsageError::NotLoggedIn => {
					let icon = if ctx.nerd_font {
						"\u{f023}"
					} else {
						"\u{2205}"
					};
					(icon, YELLOW, "log in to claude.ai")
				}
				UsageError::Other(msg) => {
					let icon = if ctx.nerd_font {
						"\u{f071}"
					} else {
						"\u{2a2f}"
					};
					(icon, RED, msg.as_str())
				}
			};
			let text = if segment.colors() {
				format!("{} {}", icon.color(color), msg.color(color))
			} else {
				format!("{icon} {msg}")
			};
			Some(apply_style(&text, segment.style()))
		}
	}
}
