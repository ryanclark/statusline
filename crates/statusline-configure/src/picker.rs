use crate::lineedit::LineEdit;
use statusline_core::catalog::{SegmentMeta, catalog};

#[derive(Debug, Default)]
pub struct PickerState {
	pub query: LineEdit,
	pub selected: usize,
	pub replace_at: Option<usize>,
}

#[must_use]
pub fn filtered(query: &str) -> Vec<&'static SegmentMeta> {
	let q = query.to_ascii_lowercase();

	catalog()
		.iter()
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
		assert_eq!(filtered("").len(), catalog().len());
	}

	#[test]
	fn filter_matches_id_label_description() {
		assert!(filtered("git").iter().any(|m| m.id == "git_branch"));
		assert!(filtered("branch").iter().any(|m| m.id == "git_branch"));
		assert!(!filtered("zzzzz").iter().any(|_| true));
	}

	#[test]
	fn filter_is_case_insensitive() {
		assert!(filtered("GIT").iter().any(|m| m.id == "git_branch"));
		assert!(filtered("Branch").iter().any(|m| m.id == "git_branch"));
	}

	#[test]
	fn filter_matches_description_only() {
		let hits = filtered("separator");
		assert!(hits.iter().any(|m| m.id == "divider"));
	}
}
