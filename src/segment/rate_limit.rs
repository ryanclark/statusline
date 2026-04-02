use crate::constants::{FIVE_HOUR_ICON, GRAY, SEVEN_DAY_ICON};
use crate::format::{ColoredPercentage, Percentage};
use crate::input::RateLimitPeriod;
use chrono::Utc;
use owo_colors::OwoColorize;

use super::{apply_style, format_icon, Icon, RenderContext, SegmentConfig};

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
		if let Some(countdown) = period.countdown(Utc::now()) {
			format!(" {}", countdown.dimmed())
		} else {
			String::new()
		}
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
		Icon { unicode: FIVE_HOUR_ICON, nerd: "\u{f017}" },
		ctx.five_threshold,
		ctx.nerd_font,
	)
}

pub(super) fn seven_day(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	format_rate_limit(
		segment,
		ctx.input.rate_limits.seven_day.as_ref(),
		Icon { unicode: SEVEN_DAY_ICON, nerd: "\u{f073}" },
		ctx.seven_threshold,
		ctx.nerd_font,
	)
}

pub(super) fn extra_usage(segment: &SegmentConfig, ctx: &RenderContext<'_>) -> Option<String> {
	let extra = ctx.usage.and_then(|u| u.extra_usage.as_ref())?;
	let text = if segment.colors() {
		extra.to_string()
	} else {
		format!("{}/{}", extra.used_credits, extra.monthly_limit)
	};
	
	Some(apply_style(&text, segment.style()))
}
