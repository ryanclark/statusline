//! Width of terminal text once colour and hyperlink escape sequences are taken out.

/// Characters the terminal will actually place, ignoring SGR colour runs and OSC sequences such as
/// OSC 8 hyperlinks. Each remaining character counts as one cell, so double-width glyphs are
/// under-counted by one.
#[must_use]
pub fn visible_width(s: &str) -> usize {
	let mut width = 0;
	let mut chars = s.chars();
	while let Some(c) = chars.next() {
		if c != '\u{1b}' {
			width += 1;
			continue;
		}
		match chars.next() {
			// CSI: parameter and intermediate bytes, closed by the first byte in 0x40..=0x7E.
			Some('[') => {
				for c in chars.by_ref() {
					if ('\u{40}'..='\u{7e}').contains(&c) {
						break;
					}
				}
			}
			// OSC: closed by BEL or by ST (ESC \).
			Some(']') => {
				let mut after_esc = false;
				for c in chars.by_ref() {
					if c == '\u{07}' || (after_esc && c == '\\') {
						break;
					}
					after_esc = c == '\u{1b}';
				}
			}
			Some(_) | None => {}
		}
	}

	width
}

/// Cuts `s` to at most `width` cells in place, ending it in an ellipsis when anything was
/// dropped. Escape sequences before the cut are kept, a hyperlink cut open is closed, and styles
/// are reset so the cut cannot leak colour into whatever the panel draws next.
pub fn truncate_visible(s: &mut String, width: usize) {
	if visible_width(s) <= width {
		return;
	}
	let Some(keep) = width.checked_sub(1) else {
		s.clear();
		return;
	};

	let mut shown = 0;
	let mut styled = false;
	let mut link_open = false;
	let mut cut = s.len();
	let mut chars = s.char_indices();
	while let Some((at, c)) = chars.next() {
		if shown == keep {
			cut = at;
			break;
		}
		if c != '\u{1b}' {
			shown += 1;
			continue;
		}
		match chars.next().map(|(_, c)| c) {
			Some('[') => {
				for (_, c) in chars.by_ref() {
					if ('\u{40}'..='\u{7e}').contains(&c) {
						break;
					}
				}
				styled = true;
			}
			Some(']') => {
				let mut after_esc = false;
				let mut end = s.len();
				for (i, c) in chars.by_ref() {
					if c == '\u{07}' || (after_esc && c == '\\') {
						end = i + c.len_utf8();
						break;
					}
					after_esc = c == '\u{1b}';
				}
				// OSC 8 with a target opens a link, and with none closes it.
				if let Some(body) = s[at..end].strip_prefix("\u{1b}]8;;") {
					link_open = !(body.starts_with('\u{07}') || body.starts_with("\u{1b}\\"));
				}
			}
			_ => {}
		}
	}

	s.truncate(cut);
	s.push('\u{2026}');
	if link_open {
		s.push_str("\u{1b}]8;;\u{07}");
	}
	if styled {
		s.push_str("\u{1b}[0m");
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn cut(s: &str, width: usize) -> String {
		let mut s = s.to_owned();
		truncate_visible(&mut s, width);
		s
	}

	#[test]
	fn short_text_is_returned_unchanged() {
		assert_eq!(cut("warm", 4), "warm");
		assert_eq!(cut("\u{1b}[2mwarm\u{1b}[0m", 10), "\u{1b}[2mwarm\u{1b}[0m");
	}

	#[test]
	fn long_text_ends_in_an_ellipsis_within_the_width() {
		assert_eq!(cut("abcdef", 4), "abc\u{2026}");
		assert_eq!(visible_width(&cut("abcdef", 4)), 4);
		assert_eq!(cut("abcdef", 1), "\u{2026}");
		assert_eq!(cut("abcdef", 0), "");
	}

	#[test]
	fn escapes_before_the_cut_are_kept_and_styles_are_reset_after_it() {
		assert_eq!(
			cut("\u{1b}[31mabcdef\u{1b}[0m", 4),
			"\u{1b}[31mabc\u{2026}\u{1b}[0m"
		);
	}

	#[test]
	fn a_cut_hyperlink_is_closed() {
		let link = "\u{1b}]8;;https://x.y/z\u{07}owner/name\u{1b}]8;;\u{07} tail";
		let out = cut(link, 6);
		assert_eq!(
			out,
			"\u{1b}]8;;https://x.y/z\u{07}owner\u{2026}\u{1b}]8;;\u{07}"
		);
	}

	#[test]
	fn plain_text_counts_characters_not_bytes() {
		assert_eq!(visible_width("warm"), 4);
		assert_eq!(visible_width("\u{2668} 4m"), 4);
		assert_eq!(visible_width(""), 0);
	}

	#[test]
	fn sgr_colour_runs_take_no_space() {
		assert_eq!(visible_width("\u{1b}[38;2;80;200;120mwarm\u{1b}[0m"), 4);
		assert_eq!(visible_width("\u{1b}[2m12.3k\u{1b}[22m tok"), 9);
	}

	#[test]
	fn osc_hyperlinks_take_no_space() {
		let bel = "\u{1b}]8;;https://github.com/a/b\u{07}a/b\u{1b}]8;;\u{07}";
		assert_eq!(visible_width(bel), 3);
		let st = "\u{1b}]8;;https://github.com/a/b\u{1b}\\a/b\u{1b}]8;;\u{1b}\\";
		assert_eq!(visible_width(st), 3);
	}

	#[test]
	fn an_unterminated_escape_hides_the_rest_of_the_string() {
		assert_eq!(visible_width("ab\u{1b}[38;2"), 2);
		assert_eq!(visible_width("ab\u{1b}]8;;http"), 2);
	}
}
