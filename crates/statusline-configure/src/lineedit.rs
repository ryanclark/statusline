#[derive(Debug, Default, Clone)]
pub struct LineEdit {
	pub buf: String,
	pub cursor: usize,
}

impl LineEdit {
	#[must_use]
	pub fn with(s: &str) -> Self {
		Self {
			buf: s.to_owned(),
			cursor: s.chars().count(),
		}
	}

	fn byte_offset(&self) -> usize {
		self.buf
			.char_indices()
			.nth(self.cursor)
			.map_or(self.buf.len(), |(i, _)| i)
	}

	pub fn insert(&mut self, c: char) {
		let at = self.byte_offset();
		self.buf.insert(at, c);
		self.cursor += 1;
	}

	pub fn backspace(&mut self) {
		if self.cursor == 0 {
			return;
		}

		let prev = self.cursor - 1;
		let start = self
			.buf
			.char_indices()
			.nth(prev)
			.map_or(self.buf.len(), |(i, _)| i);
		let end = self.byte_offset();

		self.buf.replace_range(start..end, "");

		self.cursor = prev;
	}

	pub fn left(&mut self) {
		if self.cursor > 0 {
			self.cursor -= 1;
		}
	}

	pub fn right(&mut self) {
		let len = self.buf.chars().count();

		if self.cursor < len {
			self.cursor += 1;
		}
	}

	#[must_use]
	pub fn value(&self) -> &str {
		&self.buf
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn with_places_cursor_at_end() {
		let le = LineEdit::with("hello");
		assert_eq!(le.value(), "hello");
		assert_eq!(le.cursor, 5);
	}

	#[test]
	fn insert_at_end() {
		let mut le = LineEdit::with("ab");
		le.insert('c');
		assert_eq!(le.value(), "abc");
		assert_eq!(le.cursor, 3);
	}

	#[test]
	fn insert_at_start() {
		let mut le = LineEdit::with("bc");
		le.cursor = 0;
		le.insert('a');
		assert_eq!(le.value(), "abc");
		assert_eq!(le.cursor, 1);
	}

	#[test]
	fn insert_in_middle() {
		let mut le = LineEdit::with("ac");
		le.cursor = 1;
		le.insert('b');
		assert_eq!(le.value(), "abc");
		assert_eq!(le.cursor, 2);
	}

	#[test]
	fn backspace_at_end() {
		let mut le = LineEdit::with("abc");
		le.backspace();
		assert_eq!(le.value(), "ab");
		assert_eq!(le.cursor, 2);
	}

	#[test]
	fn backspace_in_middle() {
		let mut le = LineEdit::with("abc");
		le.cursor = 2;
		le.backspace();
		assert_eq!(le.value(), "ac");
		assert_eq!(le.cursor, 1);
	}

	#[test]
	fn backspace_at_start_is_noop() {
		let mut le = LineEdit::with("abc");
		le.cursor = 0;
		le.backspace();
		assert_eq!(le.value(), "abc");
		assert_eq!(le.cursor, 0);
	}

	#[test]
	fn left_and_right_clamp_at_bounds() {
		let mut le = LineEdit::with("ab");
		le.right();
		assert_eq!(le.cursor, 2);
		le.right();
		assert_eq!(le.cursor, 2, "right clamps at end");
		le.left();
		le.left();
		assert_eq!(le.cursor, 0);
		le.left();
		assert_eq!(le.cursor, 0, "left clamps at start");
	}

	#[test]
	fn unicode_insert_is_char_aware() {
		let mut le = LineEdit::with("café");
		assert_eq!(le.cursor, 4);
		le.cursor = 3; // before 'é'
		le.insert('x');
		assert_eq!(le.value(), "cafxé");
		assert_eq!(le.cursor, 4);
		le.backspace();
		assert_eq!(le.value(), "café");
		assert_eq!(le.cursor, 3);
	}
}
