use crossterm::style::Color;

pub(crate) const TEXT_CT: Color = Color::Rgb {
	r: 179,
	g: 179,
	b: 179,
};

pub(crate) const DIM_CT: Color = Color::Rgb {
	r: 106,
	g: 114,
	b: 134,
};

pub(crate) const SEL_BG_CT: Color = Color::Rgb {
	r: 39,
	g: 35,
	b: 57,
};

pub(crate) const YELLOW_CT: Color = Color::Rgb {
	r: 230,
	g: 192,
	b: 123,
};

pub(crate) const SEL_NAME_CT: Color = Color::Rgb {
	r: 255,
	g: 255,
	b: 255,
};

pub(crate) const BRAND_CT: Color = Color::Rgb {
	r: 182,
	g: 156,
	b: 246,
};

pub(crate) fn sgr_fg(color: Color) -> String {
	match color {
		Color::Rgb { r, g, b } => format!("\u{1b}[38;2;{r};{g};{b}m"),
		_ => String::new(),
	}
}

#[allow(dead_code)]
mod reserved {
	use super::Color;

	pub(crate) const GREEN_CT: Color = Color::Rgb {
		r: 95,
		g: 211,
		b: 138,
	};

	pub(crate) const CYAN_CT: Color = Color::Rgb {
		r: 95,
		g: 203,
		b: 216,
	};

	pub(crate) const TREEIND_CT: Color = Color::Rgb {
		r: 77,
		g: 84,
		b: 102,
	};

	pub(crate) const HDRSUB_CT: Color = Color::Rgb {
		r: 86,
		g: 93,
		b: 110,
	};

	pub(crate) const RED_CT: Color = Color::Rgb {
		r: 233,
		g: 138,
		b: 150,
	};
}
