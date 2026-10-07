use std::path::Path;

use eyre::{Result, bail};
use serde_json::Value;
use statusline_core::spans::Span;

use crate::chrome::file_url;
use crate::render::{Frame, Row};
use crate::scenario::Scenario;

const PAGE: &str = include_str!("../templates/page.html");
const STYLE: &str = include_str!("../templates/style.css");

/// What a span with no colour of its own is drawn in, Claude Code's inactive text colour.
const INACTIVE: &str = "#999999";

/// The 16 terminal colour names spans carry, in Claude Code's dark theme.
const ANSI: [(&str, &str); 16] = [
	("black", "#3a3a3a"),
	("red", "#ff5f57"),
	("green", "#50c878"),
	("yellow", "#f0c850"),
	("blue", "#6aa8f0"),
	("magenta", "#b482dc"),
	("cyan", "#64c8dc"),
	("white", "#dcdcdc"),
	("gray", "#808080"),
	("redBright", "#ff7b72"),
	("greenBright", "#7ee787"),
	("yellowBright", "#f2cc60"),
	("blueBright", "#79c0ff"),
	("magentaBright", "#d2a8ff"),
	("cyanBright", "#a5d6ff"),
	("whiteBright", "#ffffff"),
];

/// The file URLs of the regular and bold faces of the font every page is drawn in.
pub struct Fonts {
	regular: String,
	bold: String,
}

impl Fonts {
	/// Finds the fonts in `dir`. Chrome would quietly fall back to another monospace font without them, which draws
	/// the wrong widths and no icons.
	pub fn find(dir: &Path) -> Result<Self> {
		let url = |name: &str| {
			let path = dir.join(name);
			if !path.is_file() {
				bail!(
					"{} is missing, install FiraCode Nerd Font Mono",
					path.display()
				);
			}
			Ok(file_url(&path))
		};
		Ok(Self {
			regular: url("FiraCodeNerdFontMono-Regular.ttf")?,
			bold: url("FiraCodeNerdFontMono-Bold.ttf")?,
		})
	}
}

/// The page for one frame of `scenario`.
pub fn build(scenario: &Scenario, frame: &Frame, fonts: &Fonts) -> Result<String> {
	let style = fill(
		STYLE,
		&[("font", &fonts.regular), ("font_bold", &fonts.bold)],
	)?;

	let model = frame.input.get("model");
	let field = |key: &str| {
		model
			.and_then(|m| m.get(key))
			.and_then(Value::as_str)
			.unwrap_or("")
	};
	let name = match field("display_name") {
		"" => field("id"),
		name => name,
	};
	let version = frame
		.input
		.get("version")
		.and_then(Value::as_str)
		.unwrap_or("2.1.290");
	let transcript: String = scenario
		.transcript
		.iter()
		.map(|line| format!(r#"<div class="row t">{}</div>"#, escape(line)))
		.collect();

	fill(
		PAGE,
		&[
			("style", &style),
			("version", &escape(version)),
			("model", &escape(name)),
			("plan", &escape(&scenario.plan)),
			("cwd", &escape(&format!("~/{}", scenario.cwd))),
			("transcript", &transcript),
			("prompt", &escape(&scenario.prompt)),
			("status", &status_html(scenario, &frame.line)),
			("panel", &panel_html(&frame.panel)),
		],
	)
}

/// The plugin draws on Claude Code's mode row after its label. A native `statusLine` gets a row of its own with the
/// mode row under it.
fn status_html(scenario: &Scenario, rows: &[Row]) -> String {
	let html = rows_html(rows);
	let label = match scenario.mode.as_deref() {
		None | Some("") => None,
		Some(mode) => Some(format!(r#"<span class="mode">⏵⏵ {}</span>"#, escape(mode))),
	};
	match (label, scenario.plugin) {
		(None, _) => html,
		(Some(label), true) => {
			format!(
				r#"<div class="moderow">{label}<span class="sep"> · </span><div>{html}</div></div>"#
			)
		}
		(Some(label), false) => format!(r#"{html}<div class="row">{label}</div>"#),
	}
}

fn panel_html(panel: &[Row]) -> String {
	if panel.is_empty() {
		return String::new();
	}
	let mut html =
		r#"<div class="panel"><div class="row"><span class="lead">● main</span></div>"#.to_owned();
	for row in panel {
		html.push_str(r#"<div class="row"><span class="dim">○ </span>"#);
		row.iter().for_each(|span| html.push_str(&span_html(span)));
		html.push_str("</div>");
	}
	html.push_str("</div>");
	html
}

fn rows_html(rows: &[Row]) -> String {
	rows.iter()
		.map(|row| {
			let spans: String = row.iter().map(span_html).collect();
			format!(r#"<div class="row">{spans}</div>"#)
		})
		.collect()
}

fn span_html(span: &Span) -> String {
	let style = &span.style;
	let color = match style.fg.as_deref().filter(|fg| !fg.is_empty()) {
		Some(fg) => ANSI
			.iter()
			.find(|(name, _)| *name == fg)
			.map_or(fg, |(_, hex)| hex),
		None => INACTIVE,
	};
	let mut css = format!("color:{color}");
	for (on, rule) in [
		(style.bold, "font-weight:700"),
		(style.dim, "opacity:.55"),
		(style.italic, "font-style:italic"),
		(style.underline, "text-decoration:underline"),
	] {
		if on {
			css.push(';');
			css.push_str(rule);
		}
	}
	format!(
		r#"<span style="{}">{}</span>"#,
		escape(&css),
		escape(&span.text)
	)
}

/// Escapes text for element content and quoted attributes alike.
fn escape(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for c in text.chars() {
		match c {
			'&' => out.push_str("&amp;"),
			'<' => out.push_str("&lt;"),
			'>' => out.push_str("&gt;"),
			'"' => out.push_str("&quot;"),
			'\'' => out.push_str("&#x27;"),
			c => out.push(c),
		}
	}
	out
}

/// Replaces each `{{name}}` in `template` with its value in one pass, so a value is never itself read as a
/// placeholder. A placeholder without a value, or a value without a placeholder, is a bug in the template.
fn fill(template: &str, values: &[(&str, &str)]) -> Result<String> {
	let mut out = String::with_capacity(template.len());
	let mut used = vec![false; values.len()];
	let mut rest = template;
	while let Some(start) = rest.find("{{") {
		let Some(len) = rest[start + 2..].find("}}") else {
			bail!("unclosed placeholder in template");
		};
		let name = &rest[start + 2..start + 2 + len];
		let Some(i) = values.iter().position(|(key, _)| *key == name) else {
			bail!("template placeholder {{{{{name}}}}} has no value");
		};
		used[i] = true;
		out.push_str(&rest[..start]);
		out.push_str(values[i].1);
		rest = &rest[start + 2 + len + 2..];
	}
	out.push_str(rest);
	if let Some(i) = used.iter().position(|used| !used) {
		bail!("template has no {{{{{}}}}} placeholder", values[i].0);
	}
	Ok(out)
}

#[cfg(test)]
mod tests {
	use super::*;
	use statusline_core::spans::Style;

	#[test]
	fn fill_replaces_placeholders_once() {
		let filled = fill("<{{a}}|{{b}}|{{a}}>", &[("a", "{{b}}"), ("b", "&")]).unwrap();
		assert_eq!(filled, "<{{b}}|&|{{b}}>");
	}

	#[test]
	fn fill_rejects_missing_and_unused_values() {
		assert!(fill("{{a}}", &[]).is_err());
		assert!(fill("plain", &[("a", "x")]).is_err());
	}

	#[test]
	fn every_scenario_fills_the_templates() {
		let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../screenshots/scenarios");
		let row = || {
			vec![vec![Span {
				text: "<5m".to_owned(),
				style: Style {
					fg: Some("green".to_owned()),
					..Style::default()
				},
			}]]
		};
		let fonts = Fonts {
			regular: "file:///fonts/regular.ttf".to_owned(),
			bold: "file:///fonts/bold.ttf".to_owned(),
		};
		for entry in std::fs::read_dir(dir).unwrap() {
			let scenario = Scenario::load(&entry.unwrap().path()).unwrap();
			let frame = Frame {
				line: row(),
				panel: row(),
				input: scenario.input.clone(),
			};
			let html = build(&scenario, &frame, &fonts).unwrap();
			assert!(!html.contains("{{"));
			assert!(html.contains("&lt;5m"));
		}
	}

	#[test]
	fn spans_map_named_colors_and_escape_text() {
		let span = Span {
			text: "<a & 'b'>".to_owned(),
			style: Style {
				fg: Some("green".to_owned()),
				bold: true,
				dim: true,
				..Style::default()
			},
		};
		assert_eq!(
			span_html(&span),
			r#"<span style="color:#50c878;font-weight:700;opacity:.55">&lt;a &amp; &#x27;b&#x27;&gt;</span>"#
		);
		let plain = Span {
			text: "x".to_owned(),
			..Span::default()
		};
		assert_eq!(span_html(&plain), r#"<span style="color:#999999">x</span>"#);
		let empty = Span {
			text: "x".to_owned(),
			style: Style {
				fg: Some(String::new()),
				..Style::default()
			},
		};
		assert_eq!(span_html(&empty), r#"<span style="color:#999999">x</span>"#);
	}
}
