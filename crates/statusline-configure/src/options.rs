use statusline_core::catalog::OptionSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
	Colors,
	Icon,
	IconColor,
	Label,
	Style,
	Dirty,
	DirtyColor,
	Capitalize,
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
	if set.dirty {
		fields.push(OptionKind::Dirty);
		fields.push(OptionKind::DirtyColor);
	}
	if set.capitalize {
		fields.push(OptionKind::Capitalize);
	}

	fields
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
	fn next_style_cycles() {
		assert_eq!(next_style(None).as_deref(), Some("bold"));
		assert_eq!(next_style(Some("bold")).as_deref(), Some("dim"));
		assert_eq!(next_style(Some("dim")).as_deref(), Some("italic"));
		assert_eq!(next_style(Some("italic")).as_deref(), Some("underline"));
		assert_eq!(next_style(Some("underline")), None);
	}
}
