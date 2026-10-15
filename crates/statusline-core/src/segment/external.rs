use super::{
	Icon, RenderContext, SegmentConfig, SegmentType, apply_style, format_icon, timing::reset_hint,
};
use crate::constants::{FIVE_HOUR_ICON, GRAY, SEVEN_DAY_ICON};
use crate::external_usage::{ExternalUsage, SnapshotState};
use crate::format::{ColoredPercentage, Percentage, parse_color};
use crate::text::plain;
use owo_colors::OwoColorize;

pub(super) fn owns(ty: &SegmentType) -> bool {
	matches!(
		ty,
		SegmentType::FiveHour
			| SegmentType::SevenDay
			| SegmentType::UsageLimit
			| SegmentType::SpendLimit
			| SegmentType::FableUsage
			| SegmentType::ExtraUsage
			| SegmentType::Credits
			| SegmentType::Account
	)
}

fn clean(text: &str) -> String {
	plain(text)
		.chars()
		.filter(|c| !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}'))
		.collect()
}

pub(super) fn render(
	segment: &SegmentConfig,
	ctx: &RenderContext<'_>,
	external: ExternalUsage<'_>,
) -> Option<String> {
	if external.state == SnapshotState::Unusable {
		return None;
	}
	let snapshot = external.snapshot?;
	let text = match segment.segment_type() {
		SegmentType::Account => {
			let label = clean(&snapshot.account.label);
			let mut chars = label.chars();
			let label = if segment.capitalize() {
				chars
					.next()
					.map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
					.unwrap_or_default()
			} else {
				label
			};
			match snapshot
				.account
				.color
				.as_deref()
				.and_then(parse_color)
				.filter(|_| segment.colors())
			{
				Some(color) => label.color(color).to_string(),
				None => label,
			}
		}
		SegmentType::ExtraUsage => {
			let spend = snapshot.spend.as_ref()?;
			let used = spend.format(spend.used_minor?);
			match spend.limit_minor {
				Some(limit) => format!("{used}/{}", spend.format(limit)),
				None => used,
			}
		}
		SegmentType::Credits => {
			let spend = snapshot.spend.as_ref()?;
			let icon = format_icon(
				segment,
				Icon {
					unicode: "◉",
					nerd: "\u{f15a}",
				},
				GRAY,
				ctx.nerd_font,
			);
			format!("{icon}{}", spend.format(spend.balance_minor?))
		}
		ty => {
			let (id, icon, threshold) = match ty {
				SegmentType::FiveHour => (
					"five_hour",
					Icon {
						unicode: FIVE_HOUR_ICON,
						nerd: "\u{f017}",
					},
					ctx.five_threshold,
				),
				SegmentType::SevenDay => (
					"seven_day",
					Icon {
						unicode: SEVEN_DAY_ICON,
						nerd: "\u{f073}",
					},
					ctx.seven_threshold,
				),
				SegmentType::SpendLimit => (
					"spend_limit",
					Icon {
						unicode: "◉",
						nerd: "\u{f0d6}",
					},
					Percentage::default(),
				),
				SegmentType::FableUsage => (
					"fable_usage",
					Icon {
						unicode: "▣",
						nerd: "\u{f02d}",
					},
					Percentage::default(),
				),
				SegmentType::UsageLimit => {
					let SegmentConfig::Advanced(options) = segment else {
						return None;
					};
					(
						options.limit_id.as_deref()?,
						Icon {
							unicode: "",
							nerd: "",
						},
						Percentage::default(),
					)
				}
				_ => return None,
			};
			let limit = snapshot.limit(id)?;
			if limit.resets_at.is_some_and(|at| at <= external.now) {
				return None;
			}
			let percent = Percentage::from(limit.used_percent?);
			let prefix = if *ty == SegmentType::UsageLimit {
				if segment.icon() {
					let label = clean(segment.label().or(limit.label.as_deref()).unwrap_or(id));
					if segment.colors() {
						format!("{} ", label.color(segment.icon_color().unwrap_or(GRAY)))
					} else {
						format!("{label} ")
					}
				} else {
					String::new()
				}
			} else {
				format_icon(segment, icon, GRAY, ctx.nerd_font)
			};
			let value = if segment.colors() {
				ColoredPercentage(percent).to_string()
			} else {
				percent.to_string()
			};
			let reset = if percent > threshold {
				limit
					.resets_at
					.and_then(|at| reset_hint(segment, at.timestamp(), external.now))
					.map(|s| format!(" {s}"))
					.unwrap_or_default()
			} else {
				String::new()
			};
			format!("{prefix}{value}{reset}")
		}
	};
	if text.is_empty() {
		return None;
	}
	let text = if external.state == SnapshotState::Stale {
		let stale = format!("{} (stale)", plain(&text));
		if segment.colors() {
			stale.dimmed().to_string()
		} else {
			stale
		}
	} else {
		text
	};
	let styled = apply_style(&text, segment.style());
	Some(if segment.colors() {
		styled
	} else {
		plain(&styled)
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::external_usage::UsageSnapshot;
	use crate::sample::SampleData;
	use crate::segment::render_segment;
	use chrono::{TimeDelta, Utc};
	use serde_json::json;

	#[test]
	fn external_usage_overrides_only_account_data_and_omits_expired_windows() {
		let now = Utc::now();
		let value = json!({"version":1,"key":"work","observed_at":now,"account":{"id":"work","label":"Work","color":"#64c8dc"},
			"limits":[{"id":"custom","label":"Burst","used_percent":125.5,"resets_at":now+TimeDelta::hours(2)},
				{"id":"five_hour","used_percent":10,"resets_at":now}],
			"spend":{"currency":"EUR","exponent":2,"used_minor":1234,"limit_minor":5000,"balance_minor":8000}});
		let snapshot =
			UsageSnapshot::parse(&serde_json::to_vec(&value).unwrap(), "work", now).unwrap();
		let sample = SampleData::representative();
		let mut ctx = sample.render_context();
		ctx.external_usage = Some(ExternalUsage {
			snapshot: Some(&snapshot),
			state: SnapshotState::Fresh,
			now,
		});
		for ty in [
			SegmentType::FiveHour,
			SegmentType::SevenDay,
			SegmentType::FableUsage,
		] {
			assert!(render_segment(&SegmentConfig::Simple(ty), &ctx).is_none());
		}
		let limit: SegmentConfig = serde_json::from_value(
			json!({"type":"usage_limit","limit_id":"custom","colors":false,"show_countdown":false}),
		)
		.unwrap();
		assert_eq!(render_segment(&limit, &ctx).unwrap(), "Burst 126%");
		let extra = SegmentConfig::Simple(SegmentType::ExtraUsage);
		assert_eq!(
			plain(&render_segment(&extra, &ctx).unwrap()),
			"EUR 12.34/EUR 50.00"
		);
		let account = render_segment(&SegmentConfig::Simple(SegmentType::Account), &ctx).unwrap();
		assert_eq!(plain(&account), "Work");
		assert!(account.contains("38;2;100;200;220"));
		ctx.external_usage.as_mut().unwrap().state = SnapshotState::Stale;
		assert!(render_segment(&limit, &ctx).unwrap().ends_with(" (stale)"));
		ctx.external_usage.as_mut().unwrap().snapshot = None;
		assert!(render_segment(&extra, &ctx).is_none());
		assert!(
			render_segment(&SegmentConfig::Simple(SegmentType::ContextPercentage), &ctx).is_some()
		);
	}
}
