//! Width of terminal text once colour and hyperlink escape sequences are taken out.

/// Characters the terminal will actually place, ignoring SGR colour runs and OSC sequences such as
/// OSC 8 hyperlinks. Each remaining character counts as one cell, so double-width glyphs are
/// under-counted by one.
#[must_use]
pub fn visible_width(s: &str) -> usize {
	text_chars(s).filter(|&c| takes_a_cell(c)).count()
}

/// `s` with its SGR colour runs and OSC sequences taken out.
#[must_use]
pub fn plain(s: &str) -> String {
	text_chars(s).collect()
}

fn text_chars(s: &str) -> impl Iterator<Item = char> + '_ {
	AnsiTokens::new(s).filter_map(|token| match token {
		AnsiToken::Text(c) => Some(c),
		_ => None,
	})
}

/// The span parser draws a tab as one space and drops every other control character, so width counts the same.
fn takes_a_cell(c: char) -> bool {
	c == '\t' || !c.is_control()
}

/// One piece of terminal text: a character to draw or a whole escape sequence.
#[derive(Debug)]
pub(crate) enum AnsiToken<'a> {
	Text(char),
	/// The final byte is `None` when the string ends before the sequence does.
	Csi {
		params: &'a str,
		final_byte: Option<char>,
	},
	Osc(&'a str),
	/// Any other escape, such as a charset designation, which carries nothing worth keeping.
	Escape,
}

/// Splits terminal text into [`AnsiToken`]s, so the width and span paths agree on what an escape is.
pub(crate) struct AnsiTokens<'a> {
	s: &'a str,
	at: usize,
}

impl<'a> AnsiTokens<'a> {
	pub(crate) fn new(s: &'a str) -> Self {
		Self { s, at: 0 }
	}

	/// Byte offset of the next token.
	pub(crate) fn offset(&self) -> usize {
		self.at
	}
}

impl<'a> Iterator for AnsiTokens<'a> {
	type Item = AnsiToken<'a>;

	fn next(&mut self) -> Option<AnsiToken<'a>> {
		let rest = &self.s[self.at..];
		let mut chars = rest.char_indices().peekable();
		let (_, c) = chars.next()?;
		// 0x9B and 0x9D are the 8-bit forms of `ESC [` and `ESC ]`.
		let intro = match c {
			'\u{1b}' => chars.next().map(|(_, c)| c),
			'\u{9b}' => Some('['),
			'\u{9d}' => Some(']'),
			c => {
				self.at += c.len_utf8();
				return Some(AnsiToken::Text(c));
			}
		};
		let start = chars.peek().map_or(rest.len(), |&(i, _)| i);
		let token = match intro {
			// CSI: parameter and intermediate bytes, closed by the first byte in 0x40..=0x7E.
			Some('[') => {
				let end = chars.find(|&(_, c)| ('\u{40}'..='\u{7e}').contains(&c));
				AnsiToken::Csi {
					params: &rest[start..end.map_or(rest.len(), |(i, _)| i)],
					final_byte: end.map(|(_, c)| c),
				}
			}
			// OSC: closed by BEL, by ST (ESC \) or by its 8-bit form 0x9C.
			Some(']') => {
				let mut end = rest.len();
				while let Some((i, c)) = chars.next() {
					let st = c == '\u{1b}' && chars.next_if(|&(_, c)| c == '\\').is_some();
					if st || c == '\u{07}' || c == '\u{9c}' {
						end = i;
						break;
					}
				}
				AnsiToken::Osc(&rest[start..end])
			}
			// Intermediate bytes (0x20..=0x2F) run until a final byte, as in charset designations like `ESC ( B`.
			Some(c) if is_intermediate(c) => {
				while chars.next_if(|&(_, c)| is_intermediate(c)).is_some() {}
				chars.next();
				AnsiToken::Escape
			}
			Some(_) | None => AnsiToken::Escape,
		};
		self.at += chars.peek().map_or(rest.len(), |&(i, _)| i);
		Some(token)
	}
}

fn is_intermediate(c: char) -> bool {
	('\u{20}'..='\u{2f}').contains(&c)
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
	let mut tokens = AnsiTokens::new(s);
	while shown < keep {
		match tokens.next() {
			Some(AnsiToken::Text(c)) if takes_a_cell(c) => shown += 1,
			Some(AnsiToken::Text(_)) => {}
			Some(AnsiToken::Csi { .. }) => styled = true,
			// OSC 8 is `8;params;target`, and an empty target closes the link.
			Some(AnsiToken::Osc(body)) => {
				if let Some(rest) = body.strip_prefix("8;") {
					link_open = rest
						.split_once(';')
						.is_some_and(|(_, target)| !target.is_empty());
				}
			}
			Some(AnsiToken::Escape) => {}
			None => break,
		}
	}
	let cut = tokens.offset();

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

	#[test]
	fn width_agrees_with_the_span_parser() {
		for s in [
			"\u{1b}[1;38;2;80;200;120mok\u{1b}[0m tail",
			"\u{9b}31mX\u{9b}0m",
			"\u{1b}]8;;https://x.y\u{1b}\\link\u{1b}]8;;\u{1b}\\",
			"\u{9d}8;;https://x.y\u{9c}link\u{9d}8;;\u{9c}",
			"\u{1b}(BX\u{1b}7Y\u{1b}#8Z",
			"a\u{7}b\u{9c}c\td\re",
		] {
			let spans: usize = crate::spans::ansi_to_spans(s)
				.iter()
				.flatten()
				.map(|span| span.text.chars().count())
				.sum();
			assert_eq!(visible_width(s), spans, "{s:?}");
		}
	}
}
