use crate::model::{EditorModel, Focus};
use crate::options::{OptionKind, applicable_fields};
use crate::picker;
use statusline_core::catalog::{OptionSet, meta};
use statusline_core::sample::SampleData;
use statusline_core::segment::{DirtyConfig, SegmentConfig, SegmentLine, SegmentType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
	Preview,
	PreviewLabel,
	Border,
	Blank,
	Normal,
	Cursor,
	Decoration,
	Help,
}

#[derive(Debug, Clone)]
pub struct RenderRow {
	pub text: String,
	pub kind: RowKind,
	pub example: Option<String>,
}

impl RenderRow {
	fn new(text: impl Into<String>, kind: RowKind) -> Self {
		Self {
			text: text.into(),
			kind,
			example: None,
		}
	}

	fn blank() -> Self {
		Self::new(String::new(), RowKind::Blank)
	}
}

const CHROME_ROWS: usize = 8;

#[must_use]
pub fn preview_line(model: &EditorModel, sample: &SampleData) -> String {
	let segs: Vec<SegmentConfig> = model
		.rows
		.iter()
		.filter(|r| r.enabled)
		.map(|r| r.config.clone())
		.collect();
	let divider = model
		.divider
		.as_deref()
		.unwrap_or(statusline_core::constants::DIVIDER);

	let ctx = sample.render_context_with(divider, model.nerd_font, model.five, model.seven);
	let line = SegmentLine {
		segments: &segs,
		ctx,
	};

	format!("{line}")
}

#[must_use]
pub fn preview_highlighted(model: &EditorModel, sample: &SampleData) -> String {
	if !matches!(model.focus, Focus::List | Focus::Options) {
		return preview_line(model, sample);
	}

	let divider = model
		.divider
		.as_deref()
		.unwrap_or(statusline_core::constants::DIVIDER);
	let ctx = sample.render_context_with(divider, model.nerd_font, model.five, model.seven);

	let segs: Vec<SegmentConfig> = model
		.rows
		.iter()
		.map(|r| {
			let mut config = r.config.clone();
			config.options_mut().enabled = r.enabled;
			config
		})
		.collect();
	let line = SegmentLine {
		segments: &segs,
		ctx,
	};

	let brand = crate::theme::sgr_fg(crate::theme::BRAND_CT);
	let mut out = String::new();

	for (i, (idx, output, _)) in line.parts_with_indices().iter().enumerate() {
		if i > 0 {
			out.push(' ');
		}
		if *idx == model.cursor {
			out.push_str(&brand);
			out.push_str("[\u{1b}[0m");
			out.push_str(output);
			out.push_str(&brand);
			out.push_str("]\u{1b}[0m");
		} else {
			out.push_str(output);
		}
	}

	out
}

#[must_use]
pub fn page_size(term_rows: usize) -> usize {
	term_rows.saturating_sub(CHROME_ROWS).max(4)
}

#[must_use]
pub fn window(len: usize, cursor: usize, page: usize) -> usize {
	if len <= page {
		return 0;
	}

	let half = page / 2;

	cursor.saturating_sub(half).min(len - page)
}

fn has_options(ty: &SegmentType) -> bool {
	meta(ty).options != OptionSet::default()
}

const ID_COL: usize = 22;

fn pad_to(s: &str, width: usize) -> String {
	use unicode_width::UnicodeWidthStr as _;

	let n = s.width();

	if n >= width {
		s.to_owned()
	} else {
		format!("{s}{}", " ".repeat(width - n))
	}
}

fn segment_example(model: &EditorModel, sample: &SampleData, config: &SegmentConfig) -> String {
	let divider = model
		.divider
		.as_deref()
		.unwrap_or(statusline_core::constants::DIVIDER);
	let ctx = sample.render_context_with(divider, model.nerd_font, model.five, model.seven);

	let mut config = config.clone();

	config.options_mut().enabled = true;

	let segs = [config];

	let line = SegmentLine {
		segments: &segs,
		ctx,
	};
	format!("{line}")
}

struct Body {
	text: String,
	kind: RowKind,
	example_of: Option<SegmentConfig>,
}

impl Body {
	fn row(text: impl Into<String>, is_cursor: bool) -> Self {
		Self {
			text: text.into(),
			kind: if is_cursor {
				RowKind::Cursor
			} else {
				RowKind::Normal
			},
			example_of: None,
		}
	}

	fn row_with_example(text: impl Into<String>, is_cursor: bool, config: SegmentConfig) -> Self {
		Self {
			example_of: Some(config),
			..Self::row(text, is_cursor)
		}
	}

	fn decoration(text: impl Into<String>) -> Self {
		Self {
			text: text.into(),
			kind: RowKind::Decoration,
			example_of: None,
		}
	}

	fn cursor_sub(text: impl Into<String>) -> Self {
		Self {
			text: text.into(),
			kind: RowKind::Cursor,
			example_of: None,
		}
	}
}

#[must_use]
pub fn block(model: &EditorModel, sample: &SampleData, term_rows: usize) -> Vec<RenderRow> {
	let mut out = Vec::new();

	let body = body_rows(model);
	let mut page = page_size(term_rows).min(body.len());
	let footer = 6;
	let markers = if body.len() > page { 2 } else { 0 };

	if page + markers + footer > term_rows {
		page = term_rows.saturating_sub(markers + footer).max(1);
	}

	let cursor_idx = body
		.iter()
		.rposition(|b| b.kind == RowKind::Cursor)
		.unwrap_or(0);
	let start = window(body.len(), cursor_idx, page);
	let end = (start + page).min(body.len());

	if start > 0 {
		out.push(RenderRow::new(
			format!("  ↑ {start} more"),
			RowKind::Decoration,
		));
	}

	for b in &body[start..end] {
		let example = b
			.example_of
			.as_ref()
			.map(|config| segment_example(model, sample, config));

		out.push(RenderRow {
			text: b.text.clone(),
			kind: b.kind,
			example,
		});
	}

	let below = body.len() - end;
	if below > 0 {
		out.push(RenderRow::new(
			format!("  ↓ {below} more"),
			RowKind::Decoration,
		));
	}

	out.push(RenderRow::blank());
	out.push(RenderRow::new(help_line(model.focus), RowKind::Help));
	out.push(RenderRow::blank());
	out.push(RenderRow::new("Preview", RowKind::PreviewLabel));
	out.push(RenderRow::new(String::new(), RowKind::Border));
	out.push(RenderRow::new(
		preview_highlighted(model, sample),
		RowKind::Preview,
	));
	out.truncate(term_rows.max(1));

	out
}

fn body_rows(model: &EditorModel) -> Vec<Body> {
	match model.focus {
		Focus::Picker => picker_body(model),
		Focus::Global => global_body(model),
		Focus::List | Focus::Options => list_body(model),
	}
}

fn list_body(model: &EditorModel) -> Vec<Body> {
	let n = model.rows.len();
	let mut out = Vec::with_capacity(n + 1);

	for (i, row) in model.rows.iter().enumerate() {
		let on_segment = i == model.cursor;
		let expanded = on_segment && model.focus == Focus::Options;
		let pointer = if on_segment { '\u{276f}' } else { ' ' };
		let checkbox = if row.enabled { '\u{25c9}' } else { '\u{25cb}' };
		let ty = row.config.segment_type();
		let caret = if has_options(ty) {
			if expanded { " \u{25be}" } else { " \u{25b8}" }
		} else {
			""
		};
		let hidden = if row.enabled { "" } else { " (hidden)" };
		let id = segment_id(ty);
		let meta = format!("{id}{hidden}{caret}");

		if *ty == SegmentType::Divider {
			out.push(Body::row(
				format!("{pointer} {checkbox} {meta}"),
				on_segment,
			));
		} else {
			let left = format!("{pointer} {checkbox} {}  ", pad_to(&meta, ID_COL));

			out.push(Body::row_with_example(left, on_segment, row.config.clone()));
		}

		if expanded {
			append_options(&mut out, model, &row.config);
		}
	}

	let on_add = model.cursor == n;
	let pointer = if on_add { '\u{276f}' } else { ' ' };
	out.push(Body::row(
		format!("{pointer} + add segment\u{2026}"),
		on_add,
	));

	out
}

fn append_options(out: &mut Vec<Body>, model: &EditorModel, config: &SegmentConfig) {
	let fields = applicable_fields(meta(config.segment_type()).options);

	for (i, kind) in fields.iter().enumerate() {
		let is_cursor = i == model.options.field;
		let value = option_value(model, config, *kind);
		let text = format!("        \u{2514} {:<11} {value}", option_label(*kind));

		if is_cursor {
			out.push(Body::cursor_sub(text));
		} else {
			out.push(Body::decoration(text));
		}
	}
}

fn option_label(kind: OptionKind) -> &'static str {
	match kind {
		OptionKind::Colors => "colors",
		OptionKind::Icon => "icon",
		OptionKind::IconColor => "icon color",
		OptionKind::Label => "label",
		OptionKind::Style => "style",
		OptionKind::Dirty => "dirty mark",
		OptionKind::DirtyColor => "dirty color",
		OptionKind::Capitalize => "capitalize",
	}
}

fn option_value(model: &EditorModel, config: &SegmentConfig, kind: OptionKind) -> String {
	if let Some(le) = &model.options.editing_label
		&& kind == OptionKind::Label
	{
		return format!("\"{}\u{2502}\"", le.value());
	}

	if let Some(pick) = &model.options.editing_color
		&& matches!(kind, OptionKind::IconColor | OptionKind::DirtyColor)
	{
		return pick.to_opt().unwrap_or_else(|| "default".to_owned());
	}

	let opts = match config {
		SegmentConfig::Advanced(o) => Some(o),
		SegmentConfig::Simple(_) => None,
	};
	let on_off = |v: bool| if v { "on" } else { "off" };

	match kind {
		OptionKind::Colors => on_off(opts.is_none_or(|o| o.colors)).to_owned(),
		OptionKind::Icon => on_off(opts.is_none_or(|o| o.icon)).to_owned(),
		OptionKind::Capitalize => {
			on_off(opts.and_then(|o| o.capitalize).unwrap_or(true)).to_owned()
		}
		OptionKind::IconColor => {
			let value = opts
				.and_then(|o| o.icon_color.clone())
				.unwrap_or_else(|| "default".to_owned());

			needs_icon_hint(value, config)
		}
		OptionKind::DirtyColor => opts
			.and_then(|o| o.dirty_color.clone())
			.unwrap_or_else(|| "default".to_owned()),
		OptionKind::Label => {
			let value = opts
				.and_then(|o| o.label.as_deref())
				.map_or_else(|| "(none)".to_owned(), |l| format!("\"{l}\""));

			needs_icon_hint(value, config)
		}
		OptionKind::Style => opts
			.and_then(|o| o.style.as_deref())
			.unwrap_or("none")
			.to_owned(),
		OptionKind::Dirty => match opts.map(|o| &o.dirty) {
			Some(DirtyConfig::On) => "\u{2731} on".to_owned(),
			Some(DirtyConfig::Custom(s)) => format!("\u{2731} {s}"),
			Some(DirtyConfig::Off) | None => "off".to_owned(),
		},
	}
}

fn needs_icon_hint(value: String, config: &SegmentConfig) -> String {
	if config.icon() {
		value
	} else {
		format!("{value} (needs icon)")
	}
}

fn picker_body(model: &EditorModel) -> Vec<Body> {
	let mut out = Vec::new();
	let prompt = if model.picker.replace_at.is_some() {
		"replace with"
	} else {
		"search"
	};

	out.push(Body::decoration(format!(
		"  {prompt}: {}",
		model.picker.query.value()
	)));

	let results = picker::filtered(model.picker.query.value());
	let mut last_category = None;

	for (i, m) in results.iter().enumerate() {
		if last_category != Some(m.category) {
			out.push(Body::decoration(format!("  {}", m.category.label())));
			last_category = Some(m.category);
		}

		let on = i == model.picker.selected;
		let pointer = if on { '\u{276f}' } else { ' ' };
		let id = m.id;

		if m.ty == SegmentType::Divider {
			out.push(Body::row(format!("  {pointer} {id}"), on));
		} else {
			let left = format!("  {pointer} {} ", pad_to(id, ID_COL));

			out.push(Body::row_with_example(
				left,
				on,
				SegmentConfig::Simple(m.ty.clone()),
			));
		}
	}

	out
}

fn global_body(model: &EditorModel) -> Vec<Body> {
	let g = &model.global;
	let nerd = if g.nerd_font { "on" } else { "off" };
	let fields = [
		("divider".to_owned(), format!("\"{}\"", g.divider.value())),
		("nerd_font".to_owned(), nerd.to_owned()),
		("5h reset at".to_owned(), format!("{}%", g.five.value())),
		("7d reset at".to_owned(), format!("{}%", g.seven.value())),
	];
	let mut out = Vec::with_capacity(fields.len() + 1);

	out.push(Body::decoration("  global options"));

	for (i, (label, value)) in fields.iter().enumerate() {
		let on = i == g.field;
		let pointer = if on { '\u{276f}' } else { ' ' };
		let text = format!("  {pointer} {label:<13} {value}");

		if on {
			out.push(Body::cursor_sub(text));
		} else {
			out.push(Body::decoration(text));
		}
	}

	out
}

fn segment_id(ty: &SegmentType) -> &'static str {
	meta(ty).id
}

#[must_use]
pub fn help_line(focus: Focus) -> &'static str {
	match focus {
		Focus::List => {
			"space on/off \u{b7} shift + \u{2191}\u{2193} reorder \u{b7} \u{2192} options \u{b7} a add \u{b7} r replace \u{b7} d divider \u{b7} x remove \u{b7} g global \u{b7} s save \u{b7} q quit"
		}
		Focus::Options => {
			"\u{2191}\u{2193} field \u{b7} space/\u{2192} change \u{b7} \u{21b5} edit \u{b7} \u{2190} back"
		}
		Focus::Picker => {
			"type to filter \u{b7} \u{2191}\u{2193} select \u{b7} \u{21b5} add \u{b7} esc cancel"
		}
		Focus::Global => "\u{2191}\u{2193} field \u{b7} \u{21b5} edit \u{b7} \u{2190} back",
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::model::{EditorModel, Key, Row};
	use statusline_core::segment::{SegmentConfig, SegmentType};

	fn strip_ansi(s: &str) -> String {
		String::from_utf8(strip_ansi_escapes::strip(s.as_bytes())).expect("valid utf8")
	}

	fn model(types: &[SegmentType]) -> EditorModel {
		let base = crate::default_settings();
		let mut m = EditorModel::from_settings(&base);
		m.rows = types
			.iter()
			.cloned()
			.map(|t| Row {
				config: SegmentConfig::Simple(t),
				enabled: true,
			})
			.collect();
		m
	}

	fn default_model() -> EditorModel {
		model(&[
			SegmentType::ContextPercentage,
			SegmentType::TotalInputTokens,
			SegmentType::OutputTokens,
			SegmentType::Divider,
			SegmentType::FiveHour,
			SegmentType::SevenDay,
			SegmentType::Divider,
			SegmentType::ExtraUsage,
		])
	}

	#[test]
	fn block_never_exceeds_terminal_height() {
		let sample = SampleData::representative();
		for term in [6usize, 8, 10, 12, 24] {
			let m = default_model();
			let rows = block(&m, &sample, term);
			assert!(
				rows.len() <= term,
				"list: term={term} but block={} rows",
				rows.len()
			);

			let mut m = default_model();
			m.apply(Key::Add);
			for _ in 0..20 {
				m.apply(Key::Down);
			}
			let rows = block(&m, &sample, term);
			assert!(
				rows.len() <= term,
				"picker: term={term} but block={} rows",
				rows.len()
			);
		}
	}

	#[test]
	fn options_window_follows_the_selected_field() {
		let sample = SampleData::representative();
		let mut m = default_model();
		m.cursor = 4;
		m.apply(Key::Enter);
		for _ in 0..4 {
			m.apply(Key::Down);
		}
		let rows = block(&m, &sample, 12);
		assert!(
			rows.iter()
				.any(|r| r.kind == RowKind::Cursor && r.text.contains('\u{2514}')),
			"selected option field is off-window: {:?}",
			rows.iter().map(|r| &r.text).collect::<Vec<_>>()
		);
	}

	#[test]
	fn pad_to_pads_by_display_width() {
		assert_eq!(pad_to("日本", 6), "日本  ");
		assert_eq!(pad_to("abc", 5), "abc  ");
		assert_eq!(pad_to("abcdef", 3), "abcdef");
	}

	#[test]
	fn label_and_icon_color_hint_when_icon_off() {
		let mut m = model(&[SegmentType::FiveHour]);
		m.rows[0].config.options_mut().icon = false;
		let config = &m.rows[0].config;
		assert!(
			option_value(&m, config, OptionKind::Label).contains("needs icon"),
			"label renders only with the icon; the pane must say so"
		);
		assert!(
			option_value(&m, config, OptionKind::IconColor).contains("needs icon"),
			"icon color renders only with the icon; the pane must say so"
		);
	}

	#[test]
	fn preview_reflects_enabled_and_order() {
		let sample = SampleData::representative();
		let mut m = model(&[
			SegmentType::ContextPercentage,
			SegmentType::Divider,
			SegmentType::Model,
		]);
		let line = strip_ansi(&preview_line(&m, &sample));
		assert!(
			line.contains('%'),
			"preview should show a percentage: {line}"
		);
		assert!(
			line.contains("Opus"),
			"preview should include the model name: {line}"
		);

		m.rows[2].enabled = false;
		let line2 = strip_ansi(&preview_line(&m, &sample));
		assert_ne!(line, line2);
		assert!(
			!line2.contains("Opus"),
			"hidden model should drop out: {line2}"
		);
	}

	#[test]
	fn list_rows_mark_disabled_and_checkbox() {
		let mut m = model(&[SegmentType::Model]);
		m.rows[0].enabled = false;
		let body = list_body(&m);
		assert!(
			body[0].text.contains('\u{25cb}'),
			"disabled → ◯: {}",
			body[0].text
		);
		assert!(
			body[0].text.contains("hidden"),
			"disabled → (hidden): {}",
			body[0].text
		);
		let last = body.last().unwrap();
		assert!(
			last.text.contains("add segment"),
			"trailing add row: {}",
			last.text
		);

		let m2 = model(&[SegmentType::Model]);
		let body2 = list_body(&m2);
		assert!(
			body2[0].text.contains('\u{25c9}'),
			"enabled → ◉: {}",
			body2[0].text
		);
	}

	#[test]
	fn accordion_shows_options_only_when_expanded() {
		let mut m = model(&[SegmentType::GitBranch]);
		m.cursor = 0;

		let collapsed = list_body(&m);
		assert!(
			!collapsed.iter().any(|b| b.text.contains('\u{2514}')),
			"no └ rows when collapsed"
		);

		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::Options);
		let expanded = list_body(&m);
		let subs: Vec<&str> = expanded
			.iter()
			.filter(|b| b.text.contains('\u{2514}'))
			.map(|b| b.text.as_str())
			.collect();
		assert!(!subs.is_empty(), "└ rows appear when expanded");
		assert!(
			subs.iter().any(|t| t.contains("dirty mark")),
			"git_branch accordion includes a dirty line: {subs:?}"
		);
		assert!(
			expanded[0].text.contains('\u{25be}'),
			"expanded caret ▾: {}",
			expanded[0].text
		);
	}

	#[test]
	fn window_keeps_cursor_visible() {
		let page = 8;
		assert_eq!(window(page, 0, page), 0);
		assert_eq!(window(3, 2, page), 0);

		let n = 12;
		assert_eq!(window(n, 0, page), 0);
		let start = window(n, 6, page);
		assert!(start <= 6 && 6 < start + page, "cursor visible mid-list");
		let start = window(n, n - 1, page);
		assert_eq!(start, n - page);
		assert!(n - 1 < start + page, "cursor visible at end");
	}

	#[test]
	fn page_size_floors() {
		assert_eq!(page_size(24), 16);
		assert_eq!(page_size(12), 4);
		assert_eq!(page_size(3), 4);
		assert_eq!(page_size(50), 42);
	}

	#[test]
	fn block_subtracts_chrome_once() {
		let term_rows = 24usize;
		let mut m = model(&[SegmentType::Model]);
		m.rows = (0..40)
			.map(|_| Row {
				config: SegmentConfig::Simple(SegmentType::Model),
				enabled: true,
			})
			.collect();
		m.cursor = 0;

		let block = block(&m, &SampleData::representative(), term_rows);
		let body_shown = block
			.iter()
			.filter(|r| matches!(r.kind, RowKind::Normal | RowKind::Cursor))
			.count();
		assert_eq!(
			body_shown,
			page_size(term_rows),
			"body window must equal page_size(term_rows), proving a single chrome subtraction"
		);
		assert_eq!(body_shown, 16);
	}

	#[test]
	fn picker_block_lists_filtered() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		m.apply(Key::Add);
		assert_eq!(m.focus, Focus::Picker);
		for c in "git".chars() {
			m.apply(Key::Char(c));
		}
		let block = block(&m, &SampleData::representative(), 24);
		let texts: Vec<&str> = block.iter().map(|r| r.text.as_str()).collect();
		assert!(
			texts.iter().any(|t| t.contains("git_branch")),
			"picker lists git_branch: {texts:?}"
		);
		assert!(
			block
				.iter()
				.any(|r| r.kind == RowKind::Decoration && r.text.contains("Git")),
			"picker shows a category header"
		);
	}

	#[test]
	fn block_pins_preview_at_bottom_under_label_and_border() {
		let m = model(&[SegmentType::Model]);
		let block = block(&m, &SampleData::representative(), 24);

		let n = block.len();
		assert_eq!(block[n - 1].kind, RowKind::Preview);
		assert_eq!(block[n - 2].kind, RowKind::Border);
		assert_eq!(block[n - 3].kind, RowKind::PreviewLabel);
		assert_eq!(block[n - 3].text, "Preview");

		assert!(
			block
				.iter()
				.any(|r| r.kind == RowKind::Help && r.text.contains("save")),
			"help line present in the footer"
		);
		assert!(
			matches!(block[0].kind, RowKind::Normal | RowKind::Cursor),
			"body leads the block: {:?}",
			block[0].kind
		);
	}

	#[test]
	fn long_list_windows_with_more_markers() {
		let mut m = model(&[SegmentType::Model]);
		m.rows = (0..30)
			.map(|_| Row {
				config: SegmentConfig::Simple(SegmentType::Model),
				enabled: true,
			})
			.collect();
		m.cursor = 20;
		let block = block(&m, &SampleData::representative(), 12);
		let texts: Vec<&str> = block.iter().map(|r| r.text.as_str()).collect();
		assert!(
			texts.iter().any(|t| t.contains("more")),
			"windowed block shows a more-marker: {texts:?}"
		);
	}

	#[test]
	fn global_block_shows_fields() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		assert_eq!(m.focus, Focus::Global);
		let block = block(&m, &SampleData::representative(), 24);
		let texts: Vec<&str> = block.iter().map(|r| r.text.as_str()).collect();
		assert!(texts.iter().any(|t| t.contains("nerd_font")));
		assert!(texts.iter().any(|t| t.contains("divider")));
		assert!(
			texts.iter().any(|t| t.contains("global options")),
			"global pane shows its title: {texts:?}"
		);
	}

	#[test]
	fn global_thresholds_show_percent_suffix() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		let body = global_body(&m);
		let reset_rows: Vec<&str> = body
			.iter()
			.filter(|b| b.text.contains("reset at"))
			.map(|b| b.text.as_str())
			.collect();
		assert_eq!(reset_rows.len(), 2, "two reset-at rows: {reset_rows:?}");
		assert!(
			reset_rows.iter().any(|t| t.contains("70%")),
			"5h reset at shows 70%: {reset_rows:?}"
		);
		assert!(
			reset_rows.iter().all(|t| t.contains('%')),
			"both reset-at rows carry a %: {reset_rows:?}"
		);
		assert_eq!(m.global.five.value(), "70");
		assert_eq!(m.global.seven.value(), "100");
	}

	#[test]
	fn list_rows_show_aligned_example() {
		let sample = SampleData::representative();
		let m = model(&[SegmentType::ContextPercentage, SegmentType::Model]);
		let rows = block(&m, &sample, 30);
		let example_for = |id: &str| {
			rows.iter()
				.find(|r| r.text.contains(id))
				.and_then(|r| r.example.as_deref())
				.map(strip_ansi)
		};
		assert!(
			example_for("context_percentage").is_some_and(|e| e.contains('%')),
			"context row carries a % example"
		);
		assert!(
			example_for("model").is_some_and(|e| e.contains("Opus")),
			"model row carries an Opus example"
		);
	}

	#[test]
	fn picker_rows_show_aligned_example() {
		let sample = SampleData::representative();
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		m.apply(Key::Add);
		assert_eq!(m.focus, Focus::Picker);
		for c in "model".chars() {
			m.apply(Key::Char(c));
		}
		let rows = block(&m, &sample, 30);
		assert!(
			rows.iter().any(|r| r
				.example
				.as_deref()
				.map(strip_ansi)
				.is_some_and(|e| e.contains("Opus"))),
			"a filtered picker row carries the model example"
		);
	}

	#[test]
	fn preview_highlight_marks_cursor_segment() {
		let sample = SampleData::representative();
		let mut m = model(&[
			SegmentType::ContextPercentage,
			SegmentType::Divider,
			SegmentType::Model,
		]);
		m.focus = Focus::List;
		m.cursor = 0;

		let marked = strip_ansi(&preview_highlighted(&m, &sample));
		assert!(
			marked.contains('['),
			"marker open bracket present: {marked}"
		);
		assert!(
			marked.contains(']'),
			"marker close bracket present: {marked}"
		);
		assert!(marked.contains('%'), "percentage still present: {marked}");
		let open = marked.find('[').unwrap();
		let close = marked.find(']').unwrap();
		assert!(open < close, "brackets are ordered: {marked}");
		assert!(
			marked[open..=close].contains('%'),
			"the percentage sits inside the brackets: {marked}"
		);
	}

	#[test]
	fn preview_highlight_no_marker_on_add_row() {
		let sample = SampleData::representative();
		let mut m = model(&[
			SegmentType::ContextPercentage,
			SegmentType::Divider,
			SegmentType::Model,
		]);
		m.focus = Focus::List;
		m.cursor = m.rows.len();

		let highlighted = strip_ansi(&preview_highlighted(&m, &sample));
		let plain = strip_ansi(&preview_line(&m, &sample));
		assert_eq!(
			highlighted, plain,
			"no marker on the add row: highlighted must equal preview_line"
		);
		assert!(
			!highlighted.contains('['),
			"no bracket marker on the add row: {highlighted}"
		);
	}

	#[test]
	fn preview_highlight_collapse_matches_preview_line() {
		let sample = SampleData::representative();
		let mut m = model(&[
			SegmentType::ContextPercentage,
			SegmentType::Divider,
			SegmentType::Model,
		]);
		m.focus = Focus::List;
		m.rows[0].enabled = false;
		m.cursor = 0;

		assert_eq!(
			strip_ansi(&preview_highlighted(&m, &sample)),
			strip_ansi(&preview_line(&m, &sample)),
			"no-marker path reproduces preview_line"
		);
	}
}
