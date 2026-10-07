//! Rendered statusline text as styled spans, for hosts that draw their own text and refuse escape sequences.
//!
//! Segments keep rendering ANSI and this module parses it back, so spans match what the terminal path prints.

use crate::text::{AnsiToken, AnsiTokens};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Span {
	pub text: String,
	#[serde(flatten)]
	pub style: Style,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub fg: Option<Color>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bg: Option<Color>,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub bold: bool,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub dim: bool,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub italic: bool,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub underline: bool,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub strikethrough: bool,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub inverse: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub href: Option<String>,
}

/// Written as an Ink colour name or `#rrggbb`, the two forms Ink's `color` prop takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
	/// One of the 16 terminal colours, so it follows the user's palette instead of a fixed RGB value.
	Named(&'static str),
	Rgb(u8, u8, u8),
}

impl Color {
	fn ansi(n: u16) -> Self {
		Self::Named(ANSI_NAMES[usize::from(n)])
	}
}

impl std::fmt::Display for Color {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Named(name) => f.write_str(name),
			Self::Rgb(r, g, b) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
		}
	}
}

impl std::str::FromStr for Color {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if let Some(name) = ANSI_NAMES.iter().find(|name| **name == s) {
			return Ok(Self::Named(name));
		}
		let channel = |i: usize| {
			s.get(i..i + 2)
				.and_then(|hex| u8::from_str_radix(hex, 16).ok())
		};
		match (
			s.len(),
			s.strip_prefix('#'),
			channel(1),
			channel(3),
			channel(5),
		) {
			(7, Some(_), Some(r), Some(g), Some(b)) => Ok(Self::Rgb(r, g, b)),
			_ => Err(format!("unknown colour {s:?}")),
		}
	}
}

impl Serialize for Color {
	fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
		s.collect_str(self)
	}
}

impl<'de> Deserialize<'de> for Color {
	fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
		String::deserialize(d)?
			.parse()
			.map_err(serde::de::Error::custom)
	}
}

/// Splits `s` into rows on `\n` and each row into runs of identically styled text. Unknown escape sequences and
/// other control characters are dropped, since a host that refuses them would otherwise reject the whole row.
#[must_use]
pub fn ansi_to_spans(s: &str) -> Vec<Vec<Span>> {
	let mut rows = Vec::new();
	let mut row = Vec::new();
	let mut style = Style::default();
	let mut text = String::new();

	for token in AnsiTokens::new(s) {
		match token {
			AnsiToken::Text('\n') => {
				flush(&mut row, &mut text, &style);
				rows.push(std::mem::take(&mut row));
			}
			AnsiToken::Text('\t') => text.push(' '),
			AnsiToken::Text(c) if c.is_control() => {}
			AnsiToken::Text(c) => text.push(c),
			AnsiToken::Csi {
				params,
				final_byte: Some('m'),
			} => {
				flush(&mut row, &mut text, &style);
				apply_sgr(&mut style, params);
			}
			// OSC 8 is `8;params;target`, and an empty target closes the link.
			AnsiToken::Osc(body) => {
				if let Some(rest) = body.strip_prefix("8;") {
					flush(&mut row, &mut text, &style);
					let target = rest.split_once(';').map_or("", |(_, t)| t);
					style.href = (!target.is_empty()).then(|| target.to_owned());
				}
			}
			AnsiToken::Csi { .. } | AnsiToken::Escape => {}
		}
	}
	flush(&mut row, &mut text, &style);
	rows.push(row);

	while rows.len() > 1 && rows.last().is_some_and(Vec::is_empty) {
		rows.pop();
	}
	rows
}

fn flush(row: &mut Vec<Span>, text: &mut String, style: &Style) {
	if text.is_empty() {
		return;
	}
	match row.last_mut() {
		Some(last) if last.style == *style => {
			last.text.push_str(text);
			text.clear();
		}
		_ => row.push(Span {
			text: std::mem::take(text),
			style: style.clone(),
		}),
	}
}

fn apply_sgr(style: &mut Style, params: &str) {
	// An empty parameter means 0, while one that does not parse, such as the colon sub-parameter form `38:2:r:g:b`, is
	// skipped so it cannot read as a reset.
	let mut codes = params.split(';').filter_map(|p| {
		if p.is_empty() {
			Some(0)
		} else {
			p.parse::<u16>().ok()
		}
	});
	while let Some(code) = codes.next() {
		match code {
			0 => {
				*style = Style {
					href: style.href.take(),
					..Style::default()
				}
			}
			1 => style.bold = true,
			2 => style.dim = true,
			3 => style.italic = true,
			4 => style.underline = true,
			7 => style.inverse = true,
			9 => style.strikethrough = true,
			22 => {
				style.bold = false;
				style.dim = false;
			}
			23 => style.italic = false,
			24 => style.underline = false,
			27 => style.inverse = false,
			29 => style.strikethrough = false,
			30..=37 => style.fg = Some(Color::ansi(code - 30)),
			38 => style.fg = extended_color(&mut codes),
			39 => style.fg = None,
			40..=47 => style.bg = Some(Color::ansi(code - 40)),
			48 => style.bg = extended_color(&mut codes),
			49 => style.bg = None,
			90..=97 => style.fg = Some(Color::ansi(code - 90 + 8)),
			100..=107 => style.bg = Some(Color::ansi(code - 100 + 8)),
			_ => {}
		}
	}
}

/// The 16 terminal colors by their Ink names.
const ANSI_NAMES: [&str; 16] = [
	"black",
	"red",
	"green",
	"yellow",
	"blue",
	"magenta",
	"cyan",
	"white",
	"gray",
	"redBright",
	"greenBright",
	"yellowBright",
	"blueBright",
	"magentaBright",
	"cyanBright",
	"whiteBright",
];

/// Reads the tail of a 38 or 48 code, `5;n` for the 256-color table or `2;r;g;b` for truecolor.
fn extended_color(codes: &mut impl Iterator<Item = u16>) -> Option<Color> {
	match codes.next()? {
		5 => {
			let n = u8::try_from(codes.next()?).ok()?;
			Some(match n {
				0..=15 => Color::ansi(n.into()),
				16..=231 => {
					let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
					let n = n - 16;
					Color::Rgb(level(n / 36), level(n / 6 % 6), level(n % 6))
				}
				232..=255 => {
					let v = 8 + (n - 232) * 10;
					Color::Rgb(v, v, v)
				}
			})
		}
		2 => {
			// All three are consumed before validating, so a bad one cannot leave the others to be read as SGR codes.
			let (r, g, b) = (codes.next()?, codes.next()?, codes.next()?);
			Some(Color::Rgb(
				u8::try_from(r).ok()?,
				u8::try_from(g).ok()?,
				u8::try_from(b).ok()?,
			))
		}
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::constants::{GRAY, GREEN};
	use owo_colors::OwoColorize;

	fn span(text: &str, style: Style) -> Span {
		Span {
			text: text.to_owned(),
			style,
		}
	}

	fn fg(color: &str) -> Style {
		Style {
			fg: Some(color.parse().unwrap()),
			..Style::default()
		}
	}

	#[test]
	fn plain_text_is_one_unstyled_span() {
		assert_eq!(
			ansi_to_spans("warm"),
			vec![vec![span("warm", Style::default())]]
		);
	}

	#[test]
	fn owo_truecolor_and_dim_output_reads_back() {
		let rendered = format!(
			"{} {} {}",
			"warm".color(GREEN),
			"\u{2022}".color(GRAY),
			"59m".dimmed()
		);
		assert_eq!(
			ansi_to_spans(&rendered),
			vec![vec![
				span("warm", fg("#50c878")),
				span(" ", Style::default()),
				span("\u{2022}", fg("#585858")),
				span(" ", Style::default()),
				span(
					"59m",
					Style {
						dim: true,
						..Style::default()
					}
				),
			]]
		);
	}

	#[test]
	fn nested_bold_inside_color_keeps_the_color() {
		let rendered = format!("{}", format!("{}x", "a".bold()).color(GREEN));
		let rows = ansi_to_spans(&rendered);
		assert_eq!(rows[0][0].text, "a");
		assert!(rows[0][0].style.bold);
		assert_eq!(rows[0][0].style.fg, Some(Color::Rgb(0x50, 0xc8, 0x78)));
		// owo's bold closes with a full reset, which also drops the outer color, as a terminal would draw it.
		assert_eq!(rows[0][1], span("x", Style::default()));
	}

	#[test]
	fn osc8_links_become_href_and_close_on_an_empty_target() {
		let rendered = "\u{1b}]8;;https://github.com/o/r/pull/1\u{07}#1\u{1b}]8;;\u{07} tail";
		assert_eq!(
			ansi_to_spans(rendered),
			vec![vec![
				span(
					"#1",
					Style {
						href: Some("https://github.com/o/r/pull/1".to_owned()),
						..Style::default()
					}
				),
				span(" tail", Style::default()),
			]]
		);
	}

	#[test]
	fn st_terminated_osc_is_read_like_bel() {
		let rows = ansi_to_spans("\u{1b}]8;;https://x.y\u{1b}\\a\u{1b}]8;;\u{1b}\\b");
		assert_eq!(rows[0][0].style.href.as_deref(), Some("https://x.y"));
		assert_eq!(rows[0][1], span("b", Style::default()));
	}

	#[test]
	fn a_reset_inside_a_link_keeps_the_link() {
		let rows =
			ansi_to_spans("\u{1b}]8;;https://x.y\u{07}\u{1b}[31ma\u{1b}[0mb\u{1b}]8;;\u{07}");
		assert_eq!(rows[0][1].text, "b");
		assert_eq!(rows[0][1].style.href.as_deref(), Some("https://x.y"));
		assert_eq!(rows[0][1].style.fg, None);
	}

	#[test]
	fn newlines_split_rows_and_trailing_empty_rows_are_dropped() {
		let rows = ansi_to_spans("\u{1b}[31ma\nb\u{1b}[0m\n");
		assert_eq!(
			rows,
			vec![vec![span("a", fg("red"))], vec![span("b", fg("red"))]]
		);
	}

	#[test]
	fn named_bright_and_256_colors_map_to_names_or_hex() {
		let rows = ansi_to_spans(
			"\u{1b}[91ma\u{1b}[38;5;2mb\u{1b}[38;5;196mc\u{1b}[38;5;244md\u{1b}[39me",
		);
		assert_eq!(
			rows[0],
			vec![
				span("a", fg("redBright")),
				span("b", fg("green")),
				span("c", fg("#ff0000")),
				span("d", fg("#808080")),
				span("e", Style::default()),
			]
		);
	}

	#[test]
	fn background_and_attribute_toggles() {
		let rows = ansi_to_spans("\u{1b}[48;2;1;2;3;3;4ma\u{1b}[23;24;49mb");
		assert_eq!(
			rows[0][0].style,
			Style {
				bg: Some(Color::Rgb(1, 2, 3)),
				italic: true,
				underline: true,
				..Style::default()
			}
		);
		assert_eq!(rows[0][1], span("b", Style::default()));
	}

	#[test]
	fn non_sgr_csi_and_stray_controls_are_dropped() {
		assert_eq!(
			ansi_to_spans("a\u{1b}[2Kb\rc\u{7}d\te"),
			vec![vec![span("abcd e", Style::default())]]
		);
	}

	#[test]
	fn a_style_change_with_no_text_between_merges_runs() {
		assert_eq!(
			ansi_to_spans("\u{1b}[31ma\u{1b}[0m\u{1b}[31mb"),
			vec![vec![span("ab", fg("red"))]]
		);
	}

	#[test]
	fn serializes_only_set_fields() {
		let json = serde_json::to_string(&ansi_to_spans("\u{1b}[1;38;2;80;200;120mok")).unwrap();
		assert_eq!(json, r##"[[{"text":"ok","fg":"#50c878","bold":true}]]"##);
	}

	#[test]
	fn deserializes_what_it_serializes() {
		let rows = vec![vec![
			span(
				"#1",
				Style {
					fg: Some(Color::Named("green")),
					bg: Some(Color::Rgb(1, 2, 3)),
					bold: true,
					dim: true,
					italic: true,
					underline: true,
					strikethrough: true,
					inverse: true,
					href: Some("https://x.y".to_owned()),
				},
			),
			span(" tail", Style::default()),
		]];
		let json = serde_json::to_string(&rows).unwrap();
		assert_eq!(serde_json::from_str::<Vec<Vec<Span>>>(&json).unwrap(), rows);
		assert_eq!(
			serde_json::from_str::<Span>(r#"{"text":"x"}"#).unwrap(),
			span("x", Style::default())
		);
		assert!(serde_json::from_str::<Span>(r#"{"text":"x","fg":"teal"}"#).is_err());
		assert!(serde_json::from_str::<Span>(r##"{"text":"x","fg":"#12345g"}"##).is_err());
	}

	#[test]
	fn colon_subparameters_are_skipped_without_resetting() {
		let rows = ansi_to_spans("\u{1b}[1m\u{1b}[38:2:1:2:3mX");
		assert_eq!(
			rows[0],
			vec![span(
				"X",
				Style {
					bold: true,
					..Style::default()
				}
			)]
		);
	}

	#[test]
	fn an_out_of_range_truecolor_component_still_consumes_its_triple() {
		assert_eq!(
			ansi_to_spans("\u{1b}[38;2;300;1;4mX"),
			vec![vec![span("X", Style::default())]]
		);
	}

	#[test]
	fn other_escapes_are_dropped_whole() {
		assert_eq!(
			ansi_to_spans("\u{1b}(BX\u{1b}7Y\u{1b}#8Z"),
			vec![vec![span("XYZ", Style::default())]]
		);
	}

	#[test]
	fn eight_bit_csi_and_osc_are_read_like_their_escape_forms() {
		let rows = ansi_to_spans("\u{9b}31mX\u{9d}8;;https://x.y\u{07}Y");
		assert_eq!(rows[0][0], span("X", fg("red")));
		assert_eq!(rows[0][1].style.href.as_deref(), Some("https://x.y"));
	}

	#[test]
	fn empty_input_is_one_empty_row() {
		assert_eq!(ansi_to_spans(""), vec![Vec::<Span>::new()]);
	}
}
