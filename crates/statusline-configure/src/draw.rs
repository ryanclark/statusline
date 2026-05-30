use crate::ConfigureError;
use crate::model::{self, EditorModel, Effect, Focus};
use crate::theme;
use crate::view::{self, RenderRow, RowKind};
use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, read};
use crossterm::style::{
	Attribute, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size};
use crossterm::{execute, queue};
use statusline_core::sample::SampleData;
use statusline_core::settings::Settings;
use std::io::Write;

struct TermGuard;

impl TermGuard {
	fn enter() -> Result<Self, ConfigureError> {
		enable_raw_mode()?;
		let guard = TermGuard;

		execute!(std::io::stdout(), Hide)?;

		Ok(guard)
	}
}

impl Drop for TermGuard {
	fn drop(&mut self) {
		let _ = execute!(std::io::stdout(), Show);
		let _ = disable_raw_mode();
	}
}

fn require_tty() -> Result<(), ConfigureError> {
	use std::io::IsTerminal as _;

	if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
		Ok(())
	} else {
		Err(ConfigureError::NotATty)
	}
}

fn colors_enabled() -> bool {
	use std::io::IsTerminal as _;

	std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

fn map_key(ev: KeyEvent, focus: Focus, editing: bool) -> Option<model::Key> {
	use model::Key;

	let shift = ev.modifiers.contains(KeyModifiers::SHIFT);
	let alt = ev.modifiers.contains(KeyModifiers::ALT);
	let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);

	if ctrl && matches!(ev.code, KeyCode::Char('c' | 'C')) {
		return Some(Key::Quit);
	}

	let letters_are_commands = focus == Focus::List || (focus == Focus::Options && !editing);

	Some(match ev.code {
		KeyCode::Up if shift || alt => Key::MoveUp,
		KeyCode::Down if shift || alt => Key::MoveDown,
		KeyCode::Up => Key::Up,
		KeyCode::Down => Key::Down,
		KeyCode::Char('[') if letters_are_commands => Key::MoveUp,
		KeyCode::Char(']') if letters_are_commands => Key::MoveDown,
		KeyCode::Left => Key::Left,
		KeyCode::Right => Key::Right,
		KeyCode::Char(' ') if letters_are_commands => Key::Toggle,
		KeyCode::Enter => Key::Enter,
		KeyCode::Esc => Key::Back,
		KeyCode::Backspace => Key::Backspace,
		KeyCode::Char('a') if letters_are_commands => Key::Add,
		KeyCode::Char('r') if letters_are_commands => Key::Replace,
		KeyCode::Char('d') if letters_are_commands => Key::AddDivider,
		KeyCode::Char('x') if letters_are_commands => Key::Remove,
		KeyCode::Char('g') if letters_are_commands => Key::Global,
		KeyCode::Char('s') if letters_are_commands => Key::Save,
		KeyCode::Char('q') if letters_are_commands => Key::Quit,
		KeyCode::Char(c) => Key::Char(c),
		_ => return None,
	})
}

#[must_use]
fn display_width(s: &str) -> usize {
	use unicode_width::UnicodeWidthStr as _;

	s.width()
}

#[must_use]
fn truncate_display(s: &str, max: usize) -> std::borrow::Cow<'_, str> {
	use unicode_width::UnicodeWidthChar as _;

	if display_width(s) <= max {
		return std::borrow::Cow::Borrowed(s);
	}

	if max == 0 {
		return std::borrow::Cow::Borrowed("");
	}

	let budget = max - 1;
	let mut out = String::new();
	let mut w = 0usize;

	for c in s.chars() {
		let cw = c.width().unwrap_or(0);
		if w + cw > budget {
			break;
		}
		out.push(c);
		w += cw;
	}

	out.push('\u{2026}');

	std::borrow::Cow::Owned(out)
}

#[must_use]
fn strip_ansi(s: &str) -> String {
	String::from_utf8_lossy(&strip_ansi_escapes::strip(s.as_bytes())).into_owned()
}

#[must_use]
fn visible_truncate(s: &str, max: usize) -> String {
	use unicode_width::UnicodeWidthChar as _;
	let mut out = String::new();
	let mut visible = 0usize;
	let mut truncated = false;
	let mut chars = s.chars().peekable();

	while let Some(c) = chars.next() {
		if c == '\u{1b}' {
			out.push(c);

			while let Some(&e) = chars.peek() {
				out.push(e);
				chars.next();
				if e == 'm' {
					break;
				}
			}

			continue;
		}

		let cw = c.width().unwrap_or(0);
		if visible + cw > max {
			truncated = true;

			break;
		}

		out.push(c);
		visible += cw;
	}

	if truncated {
		out.push_str("\u{1b}[0m");
	}
	out
}

fn paint_row(
	out: &mut impl Write,
	row: &RenderRow,
	width: usize,
	color: bool,
) -> Result<(), ConfigureError> {
	match row.kind {
		RowKind::Preview => {
			queue!(out, Print(visible_truncate(&row.text, width)))?;
		}
		RowKind::Blank => {}
		RowKind::PreviewLabel | RowKind::Help | RowKind::Decoration => {
			let text = truncate_display(&row.text, width);
			if color {
				queue!(
					out,
					SetForegroundColor(theme::DIM_CT),
					Print(text),
					ResetColor
				)?;
			} else {
				queue!(out, Print(text))?;
			}
		}
		RowKind::Border => {
			let rule = "\u{2500}".repeat(width); // ─
			if color {
				queue!(
					out,
					SetForegroundColor(theme::DIM_CT),
					Print(rule),
					ResetColor
				)?;
			} else {
				queue!(out, Print(rule))?;
			}
		}
		RowKind::Cursor => {
			let text = truncate_display(&row.text, width);
			let mut used = display_width(&text);
			let stripped = row.example.as_deref().map(strip_ansi).unwrap_or_default();
			let example = truncate_display(&stripped, width.saturating_sub(used));

			used += display_width(&example);

			let pad = " ".repeat(width.saturating_sub(used));

			if color {
				queue!(
					out,
					SetBackgroundColor(theme::SEL_BG_CT),
					SetForegroundColor(theme::SEL_NAME_CT),
					Print(text),
					Print(example),
					Print(pad),
					ResetColor,
				)?;
			} else {
				queue!(
					out,
					SetAttribute(Attribute::Reverse),
					Print(text),
					Print(example),
					Print(pad),
					SetAttribute(Attribute::Reset),
				)?;
			}
		}
		RowKind::Normal => {
			let text = truncate_display(&row.text, width);
			let used = display_width(&text);
			let example = row
				.example
				.as_deref()
				.map(|e| visible_truncate(e, width.saturating_sub(used)))
				.unwrap_or_default();

			if color {
				queue!(
					out,
					SetForegroundColor(theme::TEXT_CT),
					Print(text),
					ResetColor,
					Print(example),
				)?;
			} else {
				queue!(out, Print(text), Print(strip_ansi(&example)))?;
			}
		}
	}
	Ok(())
}

fn paint_block(
	out: &mut impl Write,
	rows: &[RenderRow],
	width: usize,
	color: bool,
) -> Result<u16, ConfigureError> {
	queue!(out, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
	let mut printed: u16 = 0;

	for row in rows {
		if printed > 0 {
			queue!(out, Print("\r\n"))?;
		}

		paint_row(out, row, width, color)?;
		printed = printed.saturating_add(1);
	}

	Ok(printed)
}

fn draw(
	out: &mut impl Write,
	model: &EditorModel,
	sample: &SampleData,
	color: bool,
) -> Result<(), ConfigureError> {
	let printed = paint_frame(out, model, sample, color)?;

	if printed > 1 {
		queue!(out, MoveUp(printed - 1))?;
	}

	queue!(out, MoveToColumn(0))?;
	out.flush()?;

	Ok(())
}

fn paint_frame(
	out: &mut impl Write,
	model: &EditorModel,
	sample: &SampleData,
	color: bool,
) -> Result<u16, ConfigureError> {
	let (cols, rows) = size().unwrap_or((80, 24));
	let width = usize::from(cols);
	let block = view::block(model, sample, usize::from(rows));

	paint_block(out, &block, width, color)
}

fn finish(
	out: &mut impl Write,
	model: &EditorModel,
	sample: &SampleData,
	color: bool,
) -> Result<(), ConfigureError> {
	paint_frame(out, model, sample, color)?;

	queue!(out, Print("\r\n"))?;
	out.flush()?;

	Ok(())
}

fn confirm_discard(out: &mut impl Write, color: bool) -> Result<bool, ConfigureError> {
	let prompt = "discard unsaved changes? [y/N]";

	queue!(out, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;

	if color {
		queue!(
			out,
			SetForegroundColor(theme::YELLOW_CT),
			Print(prompt),
			ResetColor
		)?;
	} else {
		queue!(out, Print(prompt))?;
	}

	out.flush()?;

	loop {
		let Event::Key(k) = read()? else {
			continue;
		};

		if k.kind == KeyEventKind::Release {
			continue;
		}

		return Ok(matches!(k.code, KeyCode::Char('y' | 'Y')));
	}
}

pub(crate) fn run_editor(
	settings: &Settings,
	sample: &SampleData,
) -> Result<Option<Settings>, ConfigureError> {
	require_tty()?;
	let mut model = EditorModel::from_settings(settings);
	let color = colors_enabled();
	let _guard = TermGuard::enter()?;
	let mut out = std::io::stdout();

	draw(&mut out, &model, sample, color)?;

	loop {
		let Event::Key(k) = read()? else {
			continue;
		};

		if k.kind == KeyEventKind::Release {
			continue;
		}

		let editing =
			model.options.editing_label.is_some() || model.options.editing_color.is_some();
		let Some(key) = map_key(k, model.focus, editing) else {
			continue;
		};

		match model.apply(key) {
			Effect::Save => {
				let updated = model.to_settings(settings);
				finish(&mut out, &model, sample, color)?;

				return Ok(Some(updated));
			}
			Effect::Quit => {
				finish(&mut out, &model, sample, color)?;

				return Ok(None);
			}
			Effect::ConfirmQuitUnsaved => {
				if confirm_discard(&mut out, color)? {
					finish(&mut out, &model, sample, color)?;

					return Ok(None);
				}

				draw(&mut out, &model, sample, color)?;
			}
			Effect::Redraw | Effect::OpenPicker => {
				draw(&mut out, &model, sample, color)?;
			}
			Effect::None => {}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crossterm::event::{KeyEventKind, KeyEventState};

	fn ev(code: KeyCode) -> KeyEvent {
		KeyEvent {
			code,
			modifiers: KeyModifiers::NONE,
			kind: KeyEventKind::Press,
			state: KeyEventState::NONE,
		}
	}

	fn ev_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
		KeyEvent {
			code,
			modifiers: mods,
			kind: KeyEventKind::Press,
			state: KeyEventState::NONE,
		}
	}

	#[test]
	fn arrows_map_regardless_of_focus() {
		for focus in [Focus::List, Focus::Picker, Focus::Options, Focus::Global] {
			assert_eq!(map_key(ev(KeyCode::Up), focus, false), Some(model::Key::Up));
			assert_eq!(
				map_key(ev(KeyCode::Down), focus, false),
				Some(model::Key::Down)
			);
			assert_eq!(
				map_key(ev(KeyCode::Left), focus, false),
				Some(model::Key::Left)
			);
			assert_eq!(
				map_key(ev(KeyCode::Right), focus, false),
				Some(model::Key::Right)
			);
			assert_eq!(
				map_key(ev(KeyCode::Enter), focus, false),
				Some(model::Key::Enter)
			);
			assert_eq!(
				map_key(ev(KeyCode::Esc), focus, false),
				Some(model::Key::Back)
			);
			assert_eq!(
				map_key(ev(KeyCode::Backspace), focus, false),
				Some(model::Key::Backspace)
			);
		}
	}

	#[test]
	fn space_is_toggle_only_where_letters_are_commands() {
		assert_eq!(
			map_key(ev(KeyCode::Char(' ')), Focus::List, false),
			Some(model::Key::Toggle)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char(' ')), Focus::Options, false),
			Some(model::Key::Toggle)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char(' ')), Focus::Picker, false),
			Some(model::Key::Char(' '))
		);
		assert_eq!(
			map_key(ev(KeyCode::Char(' ')), Focus::Global, false),
			Some(model::Key::Char(' '))
		);
		assert_eq!(
			map_key(ev(KeyCode::Char(' ')), Focus::Options, true),
			Some(model::Key::Char(' '))
		);
	}

	#[test]
	fn shift_and_alt_arrows_move_rows() {
		assert_eq!(
			map_key(ev_mod(KeyCode::Up, KeyModifiers::SHIFT), Focus::List, false),
			Some(model::Key::MoveUp)
		);
		assert_eq!(
			map_key(
				ev_mod(KeyCode::Down, KeyModifiers::SHIFT),
				Focus::List,
				false
			),
			Some(model::Key::MoveDown)
		);
		assert_eq!(
			map_key(ev_mod(KeyCode::Up, KeyModifiers::ALT), Focus::List, false),
			Some(model::Key::MoveUp)
		);
		assert_eq!(
			map_key(ev_mod(KeyCode::Down, KeyModifiers::ALT), Focus::List, false),
			Some(model::Key::MoveDown)
		);
	}

	#[test]
	fn bracket_keys_move_rows_only_where_letters_are_commands() {
		assert_eq!(
			map_key(ev(KeyCode::Char('[')), Focus::List, false),
			Some(model::Key::MoveUp)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char(']')), Focus::List, false),
			Some(model::Key::MoveDown)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('[')), Focus::Picker, false),
			Some(model::Key::Char('['))
		);
		assert_eq!(
			map_key(ev(KeyCode::Char(']')), Focus::Global, false),
			Some(model::Key::Char(']'))
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('[')), Focus::Options, true),
			Some(model::Key::Char('['))
		);
	}

	#[test]
	fn d_adds_divider_in_list_but_is_text_in_picker() {
		assert_eq!(
			map_key(ev(KeyCode::Char('d')), Focus::List, false),
			Some(model::Key::AddDivider)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('d')), Focus::Picker, false),
			Some(model::Key::Char('d'))
		);
	}

	#[test]
	fn list_letters_are_commands() {
		assert_eq!(
			map_key(ev(KeyCode::Char('a')), Focus::List, false),
			Some(model::Key::Add)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('r')), Focus::List, false),
			Some(model::Key::Replace)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('d')), Focus::List, false),
			Some(model::Key::AddDivider)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('x')), Focus::List, false),
			Some(model::Key::Remove)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('g')), Focus::List, false),
			Some(model::Key::Global)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('s')), Focus::List, false),
			Some(model::Key::Save)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('q')), Focus::List, false),
			Some(model::Key::Quit)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('z')), Focus::List, false),
			Some(model::Key::Char('z'))
		);
	}

	#[test]
	fn picker_letters_are_text() {
		for c in ['a', 'x', 'g', 's', 'q', 'z'] {
			assert_eq!(
				map_key(ev(KeyCode::Char(c)), Focus::Picker, false),
				Some(model::Key::Char(c))
			);
		}
	}

	#[test]
	fn global_letters_are_text() {
		for c in ['a', 'q', 's', 'z'] {
			assert_eq!(
				map_key(ev(KeyCode::Char(c)), Focus::Global, false),
				Some(model::Key::Char(c))
			);
		}
	}

	#[test]
	fn options_letters_depend_on_editing() {
		assert_eq!(
			map_key(ev(KeyCode::Char('q')), Focus::Options, false),
			Some(model::Key::Quit)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('s')), Focus::Options, false),
			Some(model::Key::Save)
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('q')), Focus::Options, true),
			Some(model::Key::Char('q'))
		);
		assert_eq!(
			map_key(ev(KeyCode::Char('s')), Focus::Options, true),
			Some(model::Key::Char('s'))
		);
	}

	#[test]
	fn ctrl_c_quits_in_every_focus() {
		for focus in [Focus::List, Focus::Picker, Focus::Options, Focus::Global] {
			assert_eq!(
				map_key(
					ev_mod(KeyCode::Char('c'), KeyModifiers::CONTROL),
					focus,
					true
				),
				Some(model::Key::Quit)
			);
		}
	}

	#[test]
	fn unknown_keys_are_ignored() {
		assert_eq!(map_key(ev(KeyCode::Tab), Focus::List, false), None);
		assert_eq!(map_key(ev(KeyCode::F(1)), Focus::List, false), None);
	}

	#[test]
	fn paint_block_emits_no_trailing_newline() {
		let rows = vec![
			RenderRow {
				text: "a".to_owned(),
				kind: RowKind::Normal,
				example: None,
			},
			RenderRow {
				text: "b".to_owned(),
				kind: RowKind::Normal,
				example: None,
			},
		];
		let mut buf: Vec<u8> = Vec::new();
		let printed = paint_block(&mut buf, &rows, 80, false).unwrap();
		assert_eq!(printed, 2);
		let s = String::from_utf8(buf).unwrap();
		assert!(
			!s.ends_with("\r\n"),
			"trailing newline after the last row: {s:?}"
		);
	}

	#[test]
	fn truncate_display_is_width_aware() {
		assert_eq!(truncate_display("hello", 10), "hello");
		assert_eq!(truncate_display("hello", 5), "hello");
		assert_eq!(truncate_display("hello", 3), "he…");
		assert_eq!(truncate_display("hello", 0), "");
		assert_eq!(truncate_display("héllo", 3), "hé…");
		assert_eq!(truncate_display("日本語テスト", 3), "日…");
		assert_eq!(truncate_display("日本", 4), "日本");
		assert!(display_width(&truncate_display("日本語テスト", 5)) <= 5);
	}

	#[test]
	fn display_width_counts_wide_glyphs_as_two() {
		assert_eq!(display_width("abc"), 3);
		assert_eq!(display_width("日本"), 4);
		assert_eq!(display_width("+ add segment\u{2026}"), 14);
	}

	#[test]
	fn visible_truncate_cuts_plain_string_to_max_chars() {
		assert_eq!(visible_truncate("hello world", 5), "hello\u{1b}[0m");
	}

	#[test]
	fn visible_truncate_ignores_escape_width_and_resets_on_cut() {
		let s = "\u{1b}[31mhello\u{1b}[0m";
		let got = visible_truncate(s, 3);
		assert_eq!(got, "\u{1b}[31mhel\u{1b}[0m");
		let visible: String = {
			let mut out = String::new();
			let mut chars = got.chars().peekable();
			while let Some(c) = chars.next() {
				if c == '\u{1b}' {
					while let Some(&e) = chars.peek() {
						chars.next();
						if e == 'm' {
							break;
						}
					}
					continue;
				}
				out.push(c);
			}
			out
		};
		assert_eq!(visible, "hel");
	}

	#[test]
	fn visible_truncate_returns_short_string_unchanged() {
		assert_eq!(visible_truncate("hi", 5), "hi");
		assert_eq!(visible_truncate("hello", 5), "hello");
		let s = "\u{1b}[31mhi\u{1b}[0m";
		assert_eq!(visible_truncate(s, 5), s);
	}

	#[test]
	fn visible_truncate_max_zero_does_not_panic() {
		assert_eq!(visible_truncate("hello", 0), "\u{1b}[0m");
		assert_eq!(visible_truncate("", 0), "");
	}
}
