//! Column alignment for rows that share one segment list, such as the agent panel.

use std::borrow::Cow;

use super::PartKind;
use crate::text::{truncate_visible, visible_width};

/// Narrower than this a cell is an ellipsis and a few letters, so shrinking stops here and the
/// row-level cut takes over.
const MIN_COLUMN: usize = 8;

/// Lays rows out as a grid: every text segment is a column as wide as its widest cell, and a
/// divider follows the padded cell before it, so dividers line up down the panel. Rows are the
/// `parts_with_indices` output of lines rendered from the same segment list. A row stops at its last
/// non-empty cell, and a row with no text at all comes back empty. Line breaks are ignored, since a
/// panel row is a single line. With `max_width`, the widest columns give up width until every row
/// fits, and cells wider than their column are cut with an ellipsis.
#[must_use]
pub fn align_rows(
	rows: &[Vec<(usize, String, PartKind)>],
	max_width: Option<usize>,
) -> Vec<String> {
	// Columns come from every row together: a segment that renders nothing for one task still
	// holds its place there, since the other rows would otherwise shift under it.
	let mut columns: Vec<usize> = rows
		.iter()
		.flatten()
		.filter(|(_, _, kind)| *kind == PartKind::Text)
		.map(|(idx, _, _)| *idx)
		.collect();
	columns.sort_unstable();
	columns.dedup();

	let mut widths = vec![0; columns.len()];
	let mut divider_after: Vec<Option<&str>> = vec![None; columns.len()];
	for (idx, output, kind) in rows.iter().flatten() {
		match kind {
			PartKind::Text => {
				if let Ok(col) = columns.binary_search(idx) {
					widths[col] = widths[col].max(visible_width(output));
				}
			}
			// A divider belongs to the nearest text column before it in the segment list.
			PartKind::Divider => {
				let col = columns.partition_point(|c| c < idx);
				if let Some(slot) = col.checked_sub(1).and_then(|c| divider_after.get_mut(c)) {
					slot.get_or_insert(output);
				}
			}
			PartKind::Newline => {}
		}
	}

	if let Some(max_width) = max_width {
		fit_columns(&mut widths, &divider_after, max_width);
	}

	rows.iter()
		.map(|row| {
			let mut cells: Vec<Option<&str>> = vec![None; columns.len()];
			for (idx, output, kind) in row {
				if *kind == PartKind::Text
					&& let Ok(col) = columns.binary_search(idx)
				{
					cells[col] = Some(output);
				}
			}
			let Some(last) = cells.iter().rposition(Option::is_some) else {
				return String::new();
			};

			let mut line = String::new();
			for (col, cell) in cells.iter().enumerate().take(last + 1) {
				if col > 0 {
					line.push(' ');
				}
				let cell = cell.unwrap_or_default();
				let shown = visible_width(cell);
				let cell = if shown > widths[col] {
					let mut cut = cell.to_owned();
					truncate_visible(&mut cut, widths[col]);
					Cow::Owned(cut)
				} else {
					Cow::Borrowed(cell)
				};
				line.push_str(&cell);
				if col < last {
					line.extend(std::iter::repeat_n(' ', widths[col].saturating_sub(shown)));
					if let Some(divider) = divider_after[col] {
						line.push(' ');
						line.push_str(divider);
					}
				}
			}

			line
		})
		.collect()
}

/// Takes width away from the widest column, repeatedly, until a full row fits in `max_width`.
/// Free text such as a task description is what usually overflows, and it is also what reads
/// best cut short, so the widest column is the right one to squeeze.
fn fit_columns(widths: &mut [usize], divider_after: &[Option<&str>], max_width: usize) {
	let separators = widths.len().saturating_sub(1)
		+ divider_after
			.iter()
			.flatten()
			.map(|divider| visible_width(divider) + 1)
			.sum::<usize>();
	let mut total = widths.iter().sum::<usize>() + separators;

	while total > max_width {
		let Some((col, &width)) = widths
			.iter()
			.enumerate()
			.filter(|(_, width)| **width > MIN_COLUMN)
			.max_by_key(|(_, width)| **width)
		else {
			return;
		};
		let shrink = (total - max_width).min(width - MIN_COLUMN);
		widths[col] -= shrink;
		total -= shrink;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn text(idx: usize, s: &str) -> (usize, String, PartKind) {
		(idx, s.to_owned(), PartKind::Text)
	}

	fn divider(idx: usize) -> (usize, String, PartKind) {
		(idx, "\u{2022}".to_owned(), PartKind::Divider)
	}

	#[test]
	fn cells_pad_to_the_widest_row_and_dividers_line_up() {
		let rows = vec![
			vec![text(0, "aa"), text(1, "b"), divider(2), text(3, "c")],
			vec![text(0, "a"), divider(2), text(3, "cc")],
			vec![text(0, "aaa")],
		];
		assert_eq!(
			align_rows(&rows, None),
			vec!["aa  b \u{2022} c", "a     \u{2022} cc", "aaa"]
		);
	}

	#[test]
	fn a_missing_middle_cell_keeps_its_column_and_divider() {
		let rows = vec![
			vec![
				text(0, "name"),
				text(1, "running"),
				divider(2),
				text(3, "desc"),
			],
			vec![text(0, "n"), divider(2), text(3, "d")],
		];
		let out = align_rows(&rows, None);
		assert_eq!(out[0], "name running \u{2022} desc");
		assert_eq!(out[1], "n            \u{2022} d");
		assert_eq!(out[0].find('\u{2022}'), out[1].find('\u{2022}'));
	}

	#[test]
	fn the_last_cell_is_not_padded_and_trailing_dividers_are_dropped() {
		let rows = vec![
			vec![text(0, "short"), divider(1), text(2, "x")],
			vec![text(0, "much longer name")],
		];
		let out = align_rows(&rows, None);
		assert_eq!(out[0], "short            \u{2022} x");
		assert_eq!(out[1], "much longer name");
	}

	#[test]
	fn widths_ignore_colour_escapes() {
		let rows = vec![
			vec![text(0, "\u{1b}[31mab\u{1b}[0m"), divider(1), text(2, "x")],
			vec![text(0, "abcd"), divider(1), text(2, "y")],
		];
		let out = align_rows(&rows, None);
		assert_eq!(out[0], "\u{1b}[31mab\u{1b}[0m   \u{2022} x");
		assert_eq!(out[1], "abcd \u{2022} y");
	}

	#[test]
	fn rows_without_text_come_back_empty() {
		let rows = vec![vec![text(0, "a")], vec![], vec![divider(1)]];
		assert_eq!(align_rows(&rows, None), vec!["a", "", ""]);
	}

	#[test]
	fn repeated_dividers_after_one_column_collapse_and_leading_ones_are_dropped() {
		let rows = vec![
			vec![
				divider(0),
				text(1, "a"),
				divider(2),
				divider(3),
				text(4, "b"),
			],
			vec![text(1, "aa"), divider(3), text(4, "b")],
		];
		assert_eq!(
			align_rows(&rows, None),
			vec!["a  \u{2022} b", "aa \u{2022} b"]
		);
	}

	#[test]
	fn the_widest_column_shrinks_until_the_rows_fit_the_width() {
		let long = "x".repeat(30);
		let rows = vec![
			vec![text(0, "a"), divider(1), text(2, &long), text(3, "tail")],
			vec![text(0, "bb"), divider(1), text(2, "yyyyy"), text(3, "t")],
		];
		let out = align_rows(&rows, Some(24));
		for row in &out {
			assert!(visible_width(row) <= 24, "{row:?}");
		}
		// The long cell is cut, the short one padded to the same shrunk width, and the tail column
		// survives on every row.
		assert!(out[0].contains('\u{2026}'), "{}", out[0]);
		assert!(out[0].ends_with(" tail"), "{}", out[0]);
		assert!(out[1].ends_with(" t"), "{}", out[1]);
		// Byte offsets differ because of the multi-byte ellipsis, so compare cell positions.
		let column_of = |s: &str, needle: &str| s[..s.find(needle).unwrap()].chars().count();
		assert_eq!(
			column_of(&out[0], " tail"),
			column_of(&out[1], " t"),
			"{out:?}"
		);
	}

	#[test]
	fn a_generous_width_changes_nothing() {
		let rows = vec![
			vec![text(0, "aa"), divider(1), text(2, "c")],
			vec![text(0, "a"), divider(1), text(2, "cc")],
		];
		assert_eq!(align_rows(&rows, Some(80)), align_rows(&rows, None));
	}

	#[test]
	fn line_breaks_are_ignored() {
		let rows = vec![vec![
			text(0, "a"),
			(1, "\n".to_owned(), PartKind::Newline),
			text(2, "b"),
		]];
		assert_eq!(align_rows(&rows, None), vec!["a b"]);
	}
}
