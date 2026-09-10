use crate::lineedit::LineEdit;
use crate::model::Mode;
use statusline_core::catalog::{SegmentMeta, catalog, for_subagent};

#[derive(Debug, Default)]
pub struct PickerState {
	pub query: LineEdit,
	pub selected: usize,
	pub replace_at: Option<usize>,
}

/// The subagent layout only offers segments that mean something per task.
#[must_use]
pub fn filtered_for(query: &str, mode: Mode) -> Vec<&'static SegmentMeta> {
	let q = query.to_ascii_lowercase();
	let source: Vec<&'static SegmentMeta> = match mode {
		Mode::StatusLine => catalog().iter().collect(),
		Mode::Subagent => for_subagent(),
	};

	source
		.into_iter()
		.filter(|m| {
			q.is_empty()
				|| m.id.to_ascii_lowercase().contains(&q)
				|| m.label.to_ascii_lowercase().contains(&q)
				|| m.description.to_ascii_lowercase().contains(&q)
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn empty_query_returns_all() {
		assert_eq!(filtered_for("", Mode::StatusLine).len(), catalog().len());
	}

	#[test]
	fn filter_matches_id_label_description() {
		assert!(
			filtered_for("git", Mode::StatusLine)
				.iter()
				.any(|m| m.id == "git_branch")
		);
		assert!(
			filtered_for("branch", Mode::StatusLine)
				.iter()
				.any(|m| m.id == "git_branch")
		);
		assert!(!filtered_for("zzzzz", Mode::StatusLine).iter().any(|_| true));
	}

	#[test]
	fn filter_is_case_insensitive() {
		assert!(
			filtered_for("GIT", Mode::StatusLine)
				.iter()
				.any(|m| m.id == "git_branch")
		);
		assert!(
			filtered_for("Branch", Mode::StatusLine)
				.iter()
				.any(|m| m.id == "git_branch")
		);
	}

	#[test]
	fn filter_matches_description_only() {
		let hits = filtered_for("separator", Mode::StatusLine);
		assert!(hits.iter().any(|m| m.id == "divider"));
	}
}
