use crate::lineedit::LineEdit;
use statusline_core::format::NAMED_COLORS;
use std::sync::LazyLock;

pub static NAMED: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
	std::iter::once("default")
		.chain(NAMED_COLORS.iter().map(|(name, _)| *name))
		.collect()
});

#[derive(Debug, Clone)]
pub enum ColorPick {
	Named(usize),
	Hex(LineEdit),
}

impl ColorPick {
	#[must_use]
	pub fn from_opt(s: Option<&str>) -> Self {
		match s {
			None => Self::Named(0),
			Some(value) => match NAMED.iter().position(|n| *n == value) {
				Some(idx) => Self::Named(idx),
				None => Self::Hex(LineEdit::with(value)),
			},
		}
	}

	pub fn insert(&mut self, c: char) {
		match self {
			Self::Hex(le) => le.insert(c),
			Self::Named(_) if c == '#' || c.is_ascii_hexdigit() => {
				let mut le = LineEdit::with("");
				le.insert(c);

				*self = Self::Hex(le);
			}
			Self::Named(_) => {}
		}
	}

	pub fn cycle(&mut self, delta: i32) {
		if let Self::Named(idx) = self {
			let len = NAMED.len();
			let magnitude = usize::try_from(delta.unsigned_abs()).unwrap_or(0) % len;
			let forward = if delta >= 0 {
				magnitude
			} else {
				len - magnitude
			};

			*idx = (*idx + forward) % len;
		}
	}

	#[must_use]
	pub fn to_opt(&self) -> Option<String> {
		match self {
			Self::Named(0) => None,
			Self::Named(idx) => Some(NAMED[*idx].to_owned()),
			Self::Hex(le) => {
				let v = le.value();

				if v.is_empty() {
					None
				} else {
					Some(v.to_owned())
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use statusline_core::format::parse_color;

	#[test]
	fn round_trip_none() {
		let cp = ColorPick::from_opt(None);
		assert!(matches!(cp, ColorPick::Named(0)));
		assert_eq!(cp.to_opt(), None);
	}

	#[test]
	fn round_trip_default_string() {
		let cp = ColorPick::from_opt(Some("default"));
		assert!(matches!(cp, ColorPick::Named(0)));
		assert_eq!(cp.to_opt(), None);
	}

	#[test]
	fn round_trip_named() {
		let cp = ColorPick::from_opt(Some("cyan"));
		assert!(matches!(cp, ColorPick::Named(_)));
		assert_eq!(cp.to_opt(), Some("cyan".to_owned()));
	}

	#[test]
	fn round_trip_hex() {
		let cp = ColorPick::from_opt(Some("#ff8800"));
		assert!(matches!(cp, ColorPick::Hex(_)));
		assert_eq!(cp.to_opt(), Some("#ff8800".to_owned()));
	}

	#[test]
	fn empty_hex_resolves_to_none() {
		let cp = ColorPick::Hex(LineEdit::with(""));
		assert_eq!(cp.to_opt(), None);
	}

	#[test]
	fn typing_switches_named_to_hex() {
		let mut cp = ColorPick::from_opt(None);
		cp.insert('#');
		cp.insert('f');
		assert!(matches!(cp, ColorPick::Hex(_)));
		assert_eq!(cp.to_opt(), Some("#f".to_owned()));

		let mut cp = ColorPick::from_opt(Some("cyan"));
		cp.insert('a');
		assert_eq!(cp.to_opt(), Some("a".to_owned()));

		let mut cp = ColorPick::from_opt(Some("cyan"));
		cp.insert('z');
		assert!(matches!(cp, ColorPick::Named(_)));
		assert_eq!(cp.to_opt(), Some("cyan".to_owned()));
	}

	#[test]
	fn cycle_wraps_forward_and_backward() {
		let mut cp = ColorPick::Named(NAMED.len() - 1);
		cp.cycle(1);
		assert!(matches!(cp, ColorPick::Named(0)));
		cp.cycle(-1);
		assert!(matches!(cp, ColorPick::Named(idx) if idx == NAMED.len() - 1));
	}

	#[test]
	fn cycle_is_noop_on_hex() {
		let mut cp = ColorPick::Hex(LineEdit::with("#abc"));
		cp.cycle(1);
		assert_eq!(cp.to_opt(), Some("#abc".to_owned()));
	}

	#[test]
	fn chosen_named_color_parses() {
		for name in &NAMED[1..] {
			assert!(
				parse_color(name).is_some(),
				"named color {name} did not parse"
			);
		}
		let cp = ColorPick::from_opt(Some("cyan"));
		let chosen = cp.to_opt().unwrap();
		assert!(parse_color(&chosen).is_some());
	}
}
