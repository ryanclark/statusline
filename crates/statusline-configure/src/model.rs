use crate::colorpick::ColorPick;
use crate::lineedit::LineEdit;
use crate::options::{OptionKind, applicable_fields, next_style};
use crate::picker::{self, PickerState};
use statusline_core::catalog::meta;
use statusline_core::format::Percentage;
use statusline_core::segment::{DirtyConfig, SegmentConfig, SegmentType};
use statusline_core::settings::Settings;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Row {
	pub config: SegmentConfig,
	pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
	List,
	Options,
	Picker,
	Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
	Up,
	Down,
	MoveUp,
	MoveDown,
	Toggle,
	EnterOptions,
	Back,
	Add,
	AddDivider,
	Replace,
	Remove,
	Global,
	Save,
	Quit,
	Char(char),
	Backspace,
	Enter,
	Left,
	Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
	Redraw,
	Save,
	Quit,
	ConfirmQuitUnsaved,
	OpenPicker,
	None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
	Up,
	Down,
}

#[derive(Debug, Default, Clone)]
pub struct OptionsState {
	pub field: usize,
	pub editing_label: Option<LineEdit>,
	pub editing_color: Option<ColorPick>,
	pub remembered_dirty: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct GlobalState {
	pub field: usize,
	pub divider: LineEdit,
	pub five: LineEdit,
	pub seven: LineEdit,
	pub nerd_font: bool,
}

const GLOBAL_FIELDS: usize = 4;

pub struct EditorModel {
	pub rows: Vec<Row>,
	pub cursor: usize,
	pub focus: Focus,
	pub dirty: bool,
	pub options: OptionsState,
	pub picker: PickerState,
	pub global: GlobalState,
	pub divider: Option<String>,
	pub nerd_font: bool,
	pub five: Percentage,
	pub seven: Percentage,
}

impl EditorModel {
	#[must_use]
	pub fn from_settings(s: &Settings) -> Self {
		let segs = s
			.segments
			.clone()
			.unwrap_or_else(statusline_core::segment::default_segments);
		let rows = segs
			.into_iter()
			.map(|config| Row {
				enabled: config.enabled(),
				config,
			})
			.collect();

		Self {
			rows,
			cursor: 0,
			focus: Focus::List,
			dirty: false,
			options: OptionsState::default(),
			picker: PickerState::default(),
			global: GlobalState::default(),
			divider: s.divider.clone(),
			nerd_font: s.nerd_font,
			five: s.five_hour_reset_threshold,
			seven: s.seven_day_reset_threshold,
		}
	}

	#[must_use]
	pub fn to_settings(&self, base: &Settings) -> Settings {
		let segments: Vec<SegmentConfig> = self
			.rows
			.iter()
			.map(|r| {
				let mut config = r.config.clone();
				config.options_mut().enabled = r.enabled;
				config.normalize();
				config
			})
			.collect();

		let segments = if base.segments.is_none()
			&& segments == statusline_core::segment::default_segments()
		{
			None
		} else {
			Some(segments)
		};

		Settings {
			segments,
			divider: self.divider.clone(),
			nerd_font: self.nerd_font,
			five_hour_reset_threshold: self.five,
			seven_day_reset_threshold: self.seven,
			..base.clone()
		}
	}

	pub fn apply(&mut self, key: Key) -> Effect {
		match self.focus {
			Focus::List => self.apply_list(key),
			Focus::Options => self.apply_options(key),
			Focus::Picker => self.apply_picker(key),
			Focus::Global => self.apply_global(key),
		}
	}

	fn apply_list(&mut self, key: Key) -> Effect {
		let n = self.rows.len();
		match key {
			Key::Up => {
				if self.cursor > 0 {
					self.cursor -= 1;
				}

				Effect::Redraw
			}
			Key::Down => {
				if self.cursor < n {
					self.cursor += 1;
				}

				Effect::Redraw
			}
			Key::MoveUp => self.move_row(Dir::Up),
			Key::MoveDown => self.move_row(Dir::Down),
			Key::Toggle => {
				if self.cursor < n {
					self.rows[self.cursor].enabled ^= true;
					self.dirty = true;
				}

				Effect::Redraw
			}
			Key::Remove => {
				if self.cursor < n {
					self.rows.remove(self.cursor);
					self.dirty = true;

					if self.cursor > self.rows.len() {
						self.cursor = self.rows.len();
					}
				}

				Effect::Redraw
			}
			Key::Add => {
				self.enter_picker();

				Effect::OpenPicker
			}
			Key::Replace => {
				if self.cursor < n {
					self.enter_replace();

					Effect::OpenPicker
				} else {
					Effect::None
				}
			}
			Key::AddDivider => {
				let at = self.cursor.min(self.rows.len());
				self.rows.insert(
					at,
					Row {
						config: SegmentConfig::Simple(SegmentType::Divider),
						enabled: true,
					},
				);
				self.dirty = true;

				Effect::Redraw
			}
			Key::Enter if self.cursor == n => {
				self.enter_picker();

				Effect::OpenPicker
			}
			Key::EnterOptions | Key::Enter | Key::Right => {
				if self.cursor < n {
					self.enter_options();

					Effect::Redraw
				} else {
					Effect::None
				}
			}
			Key::Global => {
				self.enter_global();

				Effect::Redraw
			}
			Key::Save => Effect::Save,
			Key::Quit => self.quit_effect(),
			_ => Effect::None,
		}
	}

	fn quit_effect(&self) -> Effect {
		if self.dirty {
			Effect::ConfirmQuitUnsaved
		} else {
			Effect::Quit
		}
	}

	fn move_row(&mut self, dir: Dir) -> Effect {
		let n = self.rows.len();

		if self.cursor >= n {
			return Effect::None;
		}

		let target = match dir {
			Dir::Up => match self.cursor.checked_sub(1) {
				Some(t) => t,
				None => return Effect::None,
			},
			Dir::Down => {
				let t = self.cursor + 1;
				if t >= n {
					return Effect::None;
				}

				t
			}
		};

		self.rows.swap(self.cursor, target);
		self.cursor = target;
		self.dirty = true;

		Effect::Redraw
	}

	fn enter_options(&mut self) {
		self.options = OptionsState::default();
		self.focus = Focus::Options;
	}

	fn current_fields(&self) -> Vec<OptionKind> {
		match self.rows.get(self.cursor) {
			Some(row) => applicable_fields(meta(row.config.segment_type()).options),
			None => Vec::new(),
		}
	}

	fn apply_options(&mut self, key: Key) -> Effect {
		if self.options.editing_label.is_some() {
			return self.apply_options_label_edit(key);
		}

		if self.options.editing_color.is_some() {
			return self.apply_options_color_edit(key);
		}

		let fields = self.current_fields();
		match key {
			Key::Up => {
				if self.options.field > 0 {
					self.options.field -= 1;
				}

				Effect::Redraw
			}
			Key::Down => {
				if self.options.field + 1 < fields.len() {
					self.options.field += 1;
				}

				Effect::Redraw
			}
			Key::Toggle | Key::Right => {
				if let Some(&kind) = fields.get(self.options.field) {
					self.adjust_option(kind);
				}

				Effect::Redraw
			}
			Key::Enter => {
				if let Some(&kind) = fields.get(self.options.field) {
					self.start_edit(kind);
				}

				Effect::Redraw
			}
			Key::Left | Key::Back => {
				if let Some(row) = self.rows.get_mut(self.cursor) {
					row.config.normalize();
				}

				self.focus = Focus::List;

				Effect::Redraw
			}
			Key::Save
			| Key::Quit
			| Key::Global
			| Key::Add
			| Key::Replace
			| Key::AddDivider
			| Key::Remove => {
				if let Some(row) = self.rows.get_mut(self.cursor) {
					row.config.normalize();
				}

				self.focus = Focus::List;

				self.apply_list(key)
			}
			_ => Effect::None,
		}
	}

	fn adjust_option(&mut self, kind: OptionKind) {
		let Some(row) = self.rows.get_mut(self.cursor) else {
			return;
		};
		let opts = row.config.options_mut();

		match kind {
			OptionKind::Colors => opts.colors ^= true,
			OptionKind::Icon => opts.icon ^= true,
			OptionKind::Capitalize => {
				let current = opts.capitalize.unwrap_or(true);

				opts.capitalize = Some(!current);
			}
			OptionKind::Style => opts.style = next_style(opts.style.as_deref()),
			OptionKind::Dirty => {
				opts.dirty = match std::mem::take(&mut opts.dirty) {
					DirtyConfig::Off => DirtyConfig::On,
					DirtyConfig::On => match self.options.remembered_dirty.clone() {
						Some(custom) => DirtyConfig::Custom(custom),
						None => DirtyConfig::Off,
					},
					DirtyConfig::Custom(s) => {
						self.options.remembered_dirty = Some(s);

						DirtyConfig::Off
					}
				};
			}
			OptionKind::IconColor => {
				let mut pick = ColorPick::from_opt(opts.icon_color.as_deref());
				pick.cycle(1);

				opts.icon_color = pick.to_opt();
			}
			OptionKind::DirtyColor => {
				let mut pick = ColorPick::from_opt(opts.dirty_color.as_deref());
				pick.cycle(1);

				opts.dirty_color = pick.to_opt();
			}
			OptionKind::Label => return,
		}
		self.dirty = true;
	}

	fn start_edit(&mut self, kind: OptionKind) {
		let Some(row) = self.rows.get_mut(self.cursor) else {
			return;
		};

		match kind {
			OptionKind::Label => {
				let current = row.config.options_mut().label.clone().unwrap_or_default();

				self.options.editing_label = Some(LineEdit::with(&current));
			}
			OptionKind::IconColor => {
				let current = row.config.options_mut().icon_color.clone();

				self.options.editing_color = Some(ColorPick::from_opt(current.as_deref()));
			}
			OptionKind::DirtyColor => {
				let current = row.config.options_mut().dirty_color.clone();

				self.options.editing_color = Some(ColorPick::from_opt(current.as_deref()));
			}
			_ => {}
		}
	}

	fn apply_options_label_edit(&mut self, key: Key) -> Effect {
		let Some(editor) = self.options.editing_label.as_mut() else {
			return Effect::None;
		};

		match key {
			Key::Char(c) => editor.insert(c),
			Key::Backspace => editor.backspace(),
			Key::Left => editor.left(),
			Key::Right => editor.right(),
			Key::Enter | Key::Back => return self.commit_label_edit(),
			Key::Quit => return self.quit_effect(),
			_ => {}
		}
		Effect::Redraw
	}

	fn commit_label_edit(&mut self) -> Effect {
		let Some(editor) = self.options.editing_label.take() else {
			return Effect::None;
		};

		if let Some(row) = self.rows.get_mut(self.cursor) {
			let value = editor.value().to_owned();
			let new = (!value.is_empty()).then_some(value);
			let opts = row.config.options_mut();

			if opts.label != new {
				opts.label = new;
				self.dirty = true;
			}
		}

		Effect::Redraw
	}

	fn apply_options_color_edit(&mut self, key: Key) -> Effect {
		let Some(pick) = self.options.editing_color.as_mut() else {
			return Effect::None;
		};

		match key {
			Key::Up | Key::Right => pick.cycle(1),
			Key::Down | Key::Left => pick.cycle(-1),
			Key::Char(c) => pick.insert(c),
			Key::Backspace => {
				if let ColorPick::Hex(le) = pick {
					le.backspace();
				}
			}
			Key::Enter | Key::Back => return self.commit_color_edit(),
			Key::Quit => return self.quit_effect(),
			_ => {}
		}
		Effect::Redraw
	}

	fn commit_color_edit(&mut self) -> Effect {
		let Some(pick) = self.options.editing_color.take() else {
			return Effect::None;
		};
		let value = pick.to_opt();
		let fields = self.current_fields();
		let kind = fields.get(self.options.field).copied();

		if let Some(row) = self.rows.get_mut(self.cursor) {
			let opts = row.config.options_mut();
			let slot = match kind {
				Some(OptionKind::IconColor) => Some(&mut opts.icon_color),
				Some(OptionKind::DirtyColor) => Some(&mut opts.dirty_color),
				_ => None,
			};

			if let Some(slot) = slot
				&& *slot != value
			{
				*slot = value;
				self.dirty = true;
			}
		}

		Effect::Redraw
	}

	fn enter_picker(&mut self) {
		self.picker = PickerState::default();
		self.focus = Focus::Picker;
	}

	fn enter_replace(&mut self) {
		if self.cursor >= self.rows.len() {
			return;
		}

		self.picker = PickerState {
			replace_at: Some(self.cursor),
			..PickerState::default()
		};
		self.focus = Focus::Picker;
	}

	fn apply_picker(&mut self, key: Key) -> Effect {
		match key {
			Key::Char(c) => {
				self.picker.query.insert(c);
				self.picker.selected = 0;

				Effect::Redraw
			}
			Key::Backspace => {
				self.picker.query.backspace();
				self.picker.selected = 0;

				Effect::Redraw
			}
			Key::Up => {
				if self.picker.selected > 0 {
					self.picker.selected -= 1;
				}

				Effect::Redraw
			}
			Key::Down => {
				let len = picker::filtered(self.picker.query.value()).len();

				if self.picker.selected + 1 < len {
					self.picker.selected += 1;
				}

				Effect::Redraw
			}
			Key::Enter => {
				let results = picker::filtered(self.picker.query.value());

				if self.picker.selected < results.len() {
					let ty = results[self.picker.selected].ty.clone();

					match self.picker.replace_at {
						Some(i) if i < self.rows.len() => {
							self.rows[i].config = SegmentConfig::Simple(ty);
							self.cursor = i;
						}
						_ => {
							let at = self.cursor.min(self.rows.len());
							self.rows.insert(
								at,
								Row {
									config: SegmentConfig::Simple(ty),
									enabled: true,
								},
							);
						}
					}

					self.dirty = true;
				}

				self.focus = Focus::List;

				Effect::Redraw
			}
			Key::Back => {
				self.focus = Focus::List;

				Effect::Redraw
			}
			Key::Quit => self.quit_effect(),
			_ => Effect::None,
		}
	}

	fn enter_global(&mut self) {
		self.global = GlobalState {
			field: 0,
			divider: LineEdit::with(self.divider.as_deref().unwrap_or_default()),
			five: LineEdit::with(&format_threshold(self.five)),
			seven: LineEdit::with(&format_threshold(self.seven)),
			nerd_font: self.nerd_font,
		};
		self.focus = Focus::Global;
	}

	fn apply_global(&mut self, key: Key) -> Effect {
		match key {
			Key::Up => {
				if self.global.field > 0 {
					self.global.field -= 1;
				}

				Effect::Redraw
			}
			Key::Down => {
				if self.global.field + 1 < GLOBAL_FIELDS {
					self.global.field += 1;
				}

				Effect::Redraw
			}
			Key::Toggle | Key::Char(' ') | Key::Left | Key::Right if self.global.field == 1 => {
				self.global.nerd_font ^= true;

				Effect::Redraw
			}
			Key::Char(c) => {
				if let Some(editor) = self.global_editor_mut() {
					editor.insert(c);
				}

				Effect::Redraw
			}
			Key::Backspace => {
				if let Some(editor) = self.global_editor_mut() {
					editor.backspace();
				}

				Effect::Redraw
			}
			Key::Left => {
				if let Some(editor) = self.global_editor_mut() {
					editor.left();
				}

				Effect::Redraw
			}
			Key::Right => {
				if let Some(editor) = self.global_editor_mut() {
					editor.right();
				}

				Effect::Redraw
			}
			Key::Back | Key::Enter => {
				self.commit_global();
				self.focus = Focus::List;

				Effect::Redraw
			}
			Key::Quit => self.quit_effect(),
			_ => Effect::None,
		}
	}

	fn global_editor_mut(&mut self) -> Option<&mut LineEdit> {
		match self.global.field {
			0 => Some(&mut self.global.divider),
			2 => Some(&mut self.global.five),
			3 => Some(&mut self.global.seven),
			_ => None,
		}
	}

	fn commit_global(&mut self) {
		let divider = self.global.divider.value();
		let new_divider = (!divider.is_empty()).then(|| divider.to_owned());

		if new_divider != self.divider {
			self.divider = new_divider;
			self.dirty = true;
		}

		if self.global.nerd_font != self.nerd_font {
			self.nerd_font = self.global.nerd_font;
			self.dirty = true;
		}

		if let Ok(five) = Percentage::from_str(self.global.five.value())
			&& five != self.five
		{
			self.five = five;
			self.dirty = true;
		}

		if let Ok(seven) = Percentage::from_str(self.global.seven.value())
			&& seven != self.seven
		{
			self.seven = seven;
			self.dirty = true;
		}
	}
}

fn format_threshold(p: Percentage) -> String {
	p.value().to_string()
}

#[cfg(test)]
mod tests {
	use super::*;
	use statusline_core::segment::{SegmentConfig, SegmentType};

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

	#[test]
	fn from_settings_uses_default_segments_when_none() {
		let mut s = crate::default_settings();
		s.segments = None;
		let m = EditorModel::from_settings(&s);
		let types: Vec<&SegmentType> = m.rows.iter().map(|r| r.config.segment_type()).collect();
		assert_eq!(
			types,
			vec![
				&SegmentType::ContextPercentage,
				&SegmentType::TotalInputTokens,
				&SegmentType::OutputTokens,
				&SegmentType::Divider,
				&SegmentType::FiveHour,
				&SegmentType::SevenDay,
				&SegmentType::Divider,
				&SegmentType::ExtraUsage,
			]
		);
		assert!(m.rows.iter().all(|r| r.enabled));
		assert_eq!(m.cursor, 0);
		assert_eq!(m.focus, Focus::List);
		assert!(!m.dirty);
	}

	#[test]
	fn down_reaches_add_row_then_stops() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		let n = m.rows.len();
		assert_eq!(m.apply(Key::Down), Effect::Redraw);
		assert_eq!(m.cursor, 1);
		assert_eq!(m.apply(Key::Down), Effect::Redraw);
		assert_eq!(m.cursor, n);
		assert_eq!(m.apply(Key::Down), Effect::Redraw);
		assert_eq!(m.cursor, n);
		assert!(!m.dirty);
	}

	#[test]
	fn move_down_reorders_and_follows_cursor() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		assert_eq!(m.apply(Key::MoveDown), Effect::Redraw);
		assert_eq!(*m.rows[0].config.segment_type(), SegmentType::Cwd);
		assert_eq!(*m.rows[1].config.segment_type(), SegmentType::Model);
		assert_eq!(m.cursor, 1);
		assert!(m.dirty);
	}

	#[test]
	fn move_up_at_top_is_noop() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		assert_eq!(m.apply(Key::MoveUp), Effect::None);
		assert_eq!(*m.rows[0].config.segment_type(), SegmentType::Model);
		assert!(!m.dirty);
	}

	#[test]
	fn move_down_at_bottom_is_noop() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.cursor = 1;
		assert_eq!(m.apply(Key::MoveDown), Effect::None);
		assert!(!m.dirty);
	}

	#[test]
	fn move_on_add_row_is_noop() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		assert_eq!(m.apply(Key::MoveUp), Effect::None);
		assert_eq!(m.apply(Key::MoveDown), Effect::None);
		assert!(!m.dirty);
	}

	#[test]
	fn move_up_reorders_and_follows_cursor() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cost, SegmentType::Cwd]);
		m.cursor = 2;
		assert_eq!(m.apply(Key::MoveUp), Effect::Redraw);
		assert_eq!(*m.rows[1].config.segment_type(), SegmentType::Cwd);
		assert_eq!(*m.rows[2].config.segment_type(), SegmentType::Cost);
		assert_eq!(m.cursor, 1);
		assert!(m.dirty);
	}

	#[test]
	fn toggle_keeps_row_and_saves_it_disabled() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.apply(Key::Toggle);
		assert!(!m.rows[0].enabled);
		assert!(m.dirty);
		assert_eq!(m.rows.len(), 2, "toggled-off row is kept in the list");
		let base = crate::default_settings();
		let s = m.to_settings(&base);
		let segs = s.segments.unwrap();
		assert_eq!(segs.len(), 2, "hidden rows persist (as enabled:false)");
		assert_eq!(*segs[0].segment_type(), SegmentType::Model);
		assert!(!segs[0].enabled());
		assert_eq!(*segs[1].segment_type(), SegmentType::Cwd);
		assert!(segs[1].enabled());
	}

	#[test]
	fn toggle_twice_re_enables() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Toggle);
		m.apply(Key::Toggle);
		assert!(m.rows[0].enabled);
	}

	#[test]
	fn remove_drops_row_and_clamps_cursor() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.cursor = 1;
		m.apply(Key::Remove);
		assert_eq!(m.rows.len(), 1);
		assert!(m.cursor <= m.rows.len());
		assert!(m.dirty);
	}

	#[test]
	fn remove_last_remaining_clamps_to_zero() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = 0;
		assert_eq!(m.apply(Key::Remove), Effect::Redraw);
		assert!(m.rows.is_empty());
		assert_eq!(m.cursor, 0);
	}

	#[test]
	fn remove_on_add_row_is_noop() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		assert_eq!(m.apply(Key::Remove), Effect::Redraw);
		assert_eq!(m.rows.len(), 1);
		assert!(!m.dirty);
	}

	#[test]
	fn enter_on_real_row_opens_options() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = 0;
		assert_eq!(m.apply(Key::Enter), Effect::Redraw);
		assert_eq!(m.focus, Focus::Options);
	}

	#[test]
	fn enter_options_key_opens_options() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = 0;
		assert_eq!(m.apply(Key::EnterOptions), Effect::Redraw);
		assert_eq!(m.focus, Focus::Options);
	}

	#[test]
	fn right_on_real_row_opens_options() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = 0;
		assert_eq!(m.apply(Key::Right), Effect::Redraw);
		assert_eq!(m.focus, Focus::Options);
	}

	#[test]
	fn global_key_focuses_global() {
		let mut m = model(&[SegmentType::Model]);
		assert_eq!(m.apply(Key::Global), Effect::Redraw);
		assert_eq!(m.focus, Focus::Global);
	}

	#[test]
	fn save_returns_save_effect() {
		let mut m = model(&[SegmentType::Model]);
		assert_eq!(m.apply(Key::Save), Effect::Save);
	}

	#[test]
	fn quit_clean_quits_dirty_confirms() {
		let mut m = model(&[SegmentType::Model]);
		assert_eq!(m.apply(Key::Quit), Effect::Quit);
		m.apply(Key::Toggle);
		assert_eq!(m.apply(Key::Quit), Effect::ConfirmQuitUnsaved);
	}

	#[test]
	fn save_keeps_hidden_rows_as_disabled() {
		let mut m = model(&[SegmentType::GitBranch, SegmentType::Cwd]);
		m.rows[0].config.options_mut().dirty = DirtyConfig::On;
		m.cursor = 0;
		m.apply(Key::Toggle);
		let s = m.to_settings(&crate::default_settings());
		let segs = s.segments.expect("explicit list saved");
		assert_eq!(segs.len(), 2, "hidden row was deleted on save");
		assert!(!segs[0].enabled(), "hidden row not persisted as disabled");
		let SegmentConfig::Advanced(opts) = &segs[0] else {
			panic!("disabled row must stay Advanced");
		};
		assert_eq!(opts.dirty, DirtyConfig::On, "options lost on hide+save");
		assert!(segs[1].enabled());
	}

	#[test]
	fn from_settings_restores_disabled_rows_as_hidden() {
		let mut s = crate::default_settings();
		s.segments = Some(vec![
			serde_json::from_str(r#"{"type":"model","enabled":false}"#).unwrap(),
			serde_json::from_str(r#""cwd""#).unwrap(),
		]);
		let m = EditorModel::from_settings(&s);
		assert!(!m.rows[0].enabled, "enabled:false must hide the row");
		assert!(m.rows[1].enabled);
	}

	#[test]
	fn untouched_default_list_stays_unpinned_on_save() {
		let mut s = crate::default_settings();
		s.segments = None;
		let mut m = EditorModel::from_settings(&s);
		m.apply(Key::Global);
		m.apply(Key::Down);
		m.apply(Key::Toggle);
		m.apply(Key::Back);
		let out = m.to_settings(&s);
		assert!(out.nerd_font);
		assert!(
			out.segments.is_none(),
			"default segment list was pinned into settings.json"
		);
	}

	#[test]
	fn edited_list_is_pinned_on_save() {
		let mut s = crate::default_settings();
		s.segments = None;
		let mut m = EditorModel::from_settings(&s);
		m.cursor = 0;
		m.apply(Key::Remove);
		let out = m.to_settings(&s);
		assert!(out.segments.is_some(), "an edited list must be saved");
	}

	#[test]
	fn hex_color_is_typeable_from_default() {
		let mut m = model(&[SegmentType::FiveHour]);
		m.apply(Key::Enter);
		m.apply(Key::Down);
		m.apply(Key::Down);
		m.apply(Key::Enter);
		for c in "#ff8800".chars() {
			m.apply(Key::Char(c));
		}
		m.apply(Key::Enter);
		let SegmentConfig::Advanced(opts) = &m.rows[0].config else {
			panic!("editing upgrades to Advanced");
		};
		assert_eq!(opts.icon_color.as_deref(), Some("#ff8800"));
	}

	#[test]
	fn opening_and_closing_global_panel_preserves_fractional_threshold() {
		let mut m = model(&[SegmentType::Model]);
		m.five = 72.5.into();
		m.apply(Key::Global);
		m.apply(Key::Back);
		assert_eq!(m.five, 72.5.into(), "threshold rounded by open/close");
		assert!(!m.dirty, "untouched panel must not dirty the model");
	}

	#[test]
	fn non_finite_threshold_input_keeps_previous_value() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		m.apply(Key::Down);
		m.apply(Key::Down);
		m.apply(Key::Backspace);
		m.apply(Key::Backspace);
		for c in "nan".chars() {
			m.apply(Key::Char(c));
		}
		m.apply(Key::Back);
		assert_eq!(m.five, 70.0.into());
		assert!(!m.dirty);
	}

	#[test]
	fn noop_label_and_color_commits_do_not_dirty() {
		let mut m = model(&[SegmentType::FiveHour]);
		m.apply(Key::Enter);
		for _ in 0..3 {
			m.apply(Key::Down);
		}
		m.apply(Key::Enter);
		m.apply(Key::Enter);
		assert!(!m.dirty, "unchanged label commit must not dirty");

		let mut m = model(&[SegmentType::FiveHour]);
		m.apply(Key::Enter);
		m.apply(Key::Down);
		m.apply(Key::Down);
		m.apply(Key::Enter);
		m.apply(Key::Enter);
		assert!(!m.dirty, "unchanged color commit must not dirty");
	}

	#[test]
	fn dirty_custom_indicator_survives_cycling() {
		let mut m = model(&[SegmentType::GitBranch]);
		m.rows[0].config.options_mut().dirty = DirtyConfig::Custom("✗".to_owned());
		m.apply(Key::Enter);
		m.apply(Key::Down);
		m.apply(Key::Down);
		m.apply(Key::Toggle);
		m.apply(Key::Toggle);
		m.apply(Key::Toggle);
		let SegmentConfig::Advanced(opts) = &m.rows[0].config else {
			panic!("advanced");
		};
		assert!(
			matches!(&opts.dirty, DirtyConfig::Custom(s) if s == "✗"),
			"custom dirty indicator destroyed: {:?}",
			opts.dirty
		);
	}

	#[test]
	fn quit_works_in_every_pane() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Add);
		assert_eq!(m.focus, Focus::Picker);
		assert_eq!(m.apply(Key::Quit), Effect::Quit);

		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		assert_eq!(m.apply(Key::Quit), Effect::Quit);

		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::Options);
		assert_eq!(m.apply(Key::Quit), Effect::Quit);

		let mut m = model(&[SegmentType::FiveHour]);
		m.apply(Key::Enter);
		m.options.editing_label = Some(LineEdit::with("partial"));
		assert_eq!(m.apply(Key::Quit), Effect::Quit);
	}

	#[test]
	fn quit_in_pane_confirms_when_dirty() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Toggle);
		m.apply(Key::Add);
		assert_eq!(m.apply(Key::Quit), Effect::ConfirmQuitUnsaved);
	}

	#[test]
	fn save_and_global_work_from_options_pane() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Enter);
		assert_eq!(m.apply(Key::Save), Effect::Save);

		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Enter);
		assert_eq!(m.apply(Key::Global), Effect::Redraw);
		assert_eq!(m.focus, Focus::Global);
	}

	#[test]
	fn list_commands_collapse_options_and_apply() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.cursor = 0;
		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::Options);
		assert_eq!(m.apply(Key::Remove), Effect::Redraw);
		assert_eq!(m.focus, Focus::List);
		assert_eq!(m.rows.len(), 1);

		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Enter);
		assert_eq!(m.apply(Key::Add), Effect::OpenPicker);
		assert_eq!(m.focus, Focus::Picker);
	}

	#[test]
	fn global_nerd_font_toggles_with_space_char() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		m.apply(Key::Down);
		assert!(!m.global.nerd_font);
		m.apply(Key::Char(' '));
		assert!(m.global.nerd_font, "space should flip the nerd_font toggle");
	}

	#[test]
	fn add_row_opens_picker() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		assert_eq!(m.apply(Key::Enter), Effect::OpenPicker);
		assert_eq!(m.focus, Focus::Picker);
	}

	#[test]
	fn add_key_on_add_row_opens_picker() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		assert_eq!(m.apply(Key::Add), Effect::OpenPicker);
		assert_eq!(m.focus, Focus::Picker);
	}

	#[test]
	fn add_key_on_normal_row_opens_picker() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.cursor = 0;
		assert_eq!(m.apply(Key::Add), Effect::OpenPicker);
		assert_eq!(m.focus, Focus::Picker);
	}

	#[test]
	fn to_settings_preserves_base_extra_keys() {
		let mut base = crate::default_settings();
		base.extra.insert(
			"future_key".to_owned(),
			serde_json::Value::String("keep me".to_owned()),
		);
		let m = model(&[SegmentType::Model]);
		let out = m.to_settings(&base);
		assert_eq!(
			out.extra.get("future_key"),
			Some(&serde_json::Value::String("keep me".to_owned()))
		);
		assert_eq!(out.segments.unwrap().len(), 1);
	}

	#[test]
	fn to_settings_normalizes_all_default_advanced_to_simple() {
		let mut m = model(&[SegmentType::Model]);
		{
			let opts = m.rows[0].config.options_mut();
			opts.label = Some("temp".to_owned());
			opts.label = None;
		}
		assert!(
			matches!(m.rows[0].config, SegmentConfig::Advanced(_)),
			"row is Advanced before saving"
		);

		let base = crate::default_settings();
		let out = m.to_settings(&base);
		let segs = out.segments.unwrap();
		assert_eq!(segs.len(), 1);
		assert!(
			matches!(segs[0], SegmentConfig::Simple(SegmentType::Model)),
			"to_settings collapses an all-default Advanced row to Simple: {:?}",
			segs[0]
		);
	}

	#[test]
	fn first_option_edit_upgrades_to_advanced() {
		let mut m = model(&[SegmentType::GitBranch]);
		m.cursor = 0;
		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::Options);
		m.apply(Key::Toggle);
		assert!(matches!(m.rows[0].config, SegmentConfig::Advanced(_)));
		assert!(!m.rows[0].config.colors());
		assert!(m.dirty);
	}

	#[test]
	fn label_edit_sets_label_and_normalizes_away_when_cleared() {
		let mut m = model(&[SegmentType::TotalInputTokens]);
		m.cursor = 0;
		m.apply(Key::Enter);
		m.options.field = 3;
		m.apply(Key::Enter);
		assert!(m.options.editing_label.is_some());
		for c in "in:".chars() {
			m.apply(Key::Char(c));
		}
		m.apply(Key::Enter);
		assert_eq!(
			m.rows[0].config.clone().options_mut().label.as_deref(),
			Some("in:")
		);
		assert!(matches!(m.rows[0].config, SegmentConfig::Advanced(_)));

		m.apply(Key::Enter);
		m.apply(Key::Backspace);
		m.apply(Key::Backspace);
		m.apply(Key::Backspace);
		m.apply(Key::Enter);
		m.apply(Key::Back);
		assert_eq!(m.focus, Focus::List);
		assert!(matches!(
			m.rows[0].config,
			SegmentConfig::Simple(SegmentType::TotalInputTokens)
		));
	}

	#[test]
	fn color_cycle_writes_icon_color() {
		let mut m = model(&[SegmentType::TotalInputTokens]);
		m.cursor = 0;
		m.apply(Key::Enter);
		m.options.field = 2;
		m.apply(Key::Toggle);
		assert!(matches!(m.rows[0].config, SegmentConfig::Advanced(_)));
		assert_eq!(
			m.rows[0].config.clone().options_mut().icon_color.as_deref(),
			Some("red")
		);
		assert!(m.dirty);
	}

	#[test]
	fn left_collapses_options_accordion() {
		let mut m = model(&[SegmentType::TotalInputTokens]);
		m.cursor = 0;
		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::Options);
		m.options.field = 2;

		m.apply(Key::Left);
		assert_eq!(m.focus, Focus::List, "Left collapses back to the list");
		assert!(
			matches!(m.rows[0].config, SegmentConfig::Simple(_)),
			"untouched cyclic field leaves the row Simple after normalize"
		);
	}

	#[test]
	fn right_cycles_color_forward_with_wrap() {
		use crate::colorpick::NAMED;

		let mut m = model(&[SegmentType::TotalInputTokens]);
		m.cursor = 0;
		m.apply(Key::Enter);
		m.options.field = 2;

		m.apply(Key::Right);
		assert_eq!(
			m.rows[0].config.clone().options_mut().icon_color.as_deref(),
			Some("red"),
			"Right from default steps to the first named color"
		);

		for _ in 1..NAMED.len() {
			m.apply(Key::Right);
		}
		assert_eq!(
			m.rows[0].config.clone().options_mut().icon_color,
			None,
			"a full forward cycle wraps back to default"
		);
	}

	#[test]
	fn dirty_toggle_cycles_off_on() {
		let mut m = model(&[SegmentType::GitBranch]);
		m.cursor = 0;
		m.apply(Key::Enter);
		m.options.field = 2;
		m.apply(Key::Toggle);
		assert!(matches!(
			m.rows[0].config.clone().options_mut().dirty,
			DirtyConfig::On
		));
		m.apply(Key::Toggle);
		assert!(matches!(
			m.rows[0].config.clone().options_mut().dirty,
			DirtyConfig::Off
		));
	}

	#[test]
	fn style_cycles_through_values() {
		let mut m = model(&[SegmentType::Cwd]);
		m.cursor = 0;
		m.apply(Key::Enter);
		m.options.field = 0;
		m.apply(Key::Toggle);
		assert_eq!(
			m.rows[0].config.clone().options_mut().style.as_deref(),
			Some("bold")
		);
	}

	#[test]
	fn options_back_returns_to_list() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = 0;
		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::Options);
		m.apply(Key::Back);
		assert_eq!(m.focus, Focus::List);
	}

	#[test]
	fn global_threshold_parses_and_applies() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		m.global.field = 2;
		m.global.five = LineEdit::with("");
		m.apply(Key::Char('5'));
		m.apply(Key::Char('5'));
		m.apply(Key::Back);
		assert_eq!(m.focus, Focus::List);
		let base = crate::default_settings();
		let out = m.to_settings(&base);
		assert_eq!(out.five_hour_reset_threshold, 55.0.into());
		assert!(m.dirty);
	}

	#[test]
	fn global_bad_threshold_keeps_old_value() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		m.global.field = 2;
		m.global.five = LineEdit::with("not-a-number");
		m.apply(Key::Back);
		let base = crate::default_settings();
		let out = m.to_settings(&base);
		assert_eq!(out.five_hour_reset_threshold, 70.0.into());
	}

	#[test]
	fn global_nerd_font_toggle_and_divider() {
		let mut m = model(&[SegmentType::Model]);
		m.apply(Key::Global);
		m.global.divider = LineEdit::with("");
		m.apply(Key::Char('|'));
		m.global.field = 1;
		m.apply(Key::Toggle);
		m.apply(Key::Back);
		let base = crate::default_settings();
		let out = m.to_settings(&base);
		assert_eq!(out.divider.as_deref(), Some("|"));
		assert!(out.nerd_font);
		assert!(m.dirty);
	}

	#[test]
	fn enter_inserts_at_cursor() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.cursor = 1;
		m.focus = Focus::Picker;
		assert_eq!(m.focus, Focus::Picker);
		for c in "divider".chars() {
			m.apply(Key::Char(c));
		}
		assert_eq!(m.picker.selected, 0);
		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::List);
		let types: Vec<&SegmentType> = m.rows.iter().map(|r| r.config.segment_type()).collect();
		assert_eq!(
			types,
			vec![
				&SegmentType::Model,
				&SegmentType::Divider,
				&SegmentType::Cwd
			]
		);
		assert!(m.dirty);
	}

	#[test]
	fn picker_esc_cancels_without_insert() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		m.apply(Key::Add);
		m.apply(Key::Back);
		assert_eq!(m.focus, Focus::List);
		assert_eq!(m.rows.len(), 1);
		assert!(!m.dirty);
	}

	#[test]
	fn picker_allows_inserting_divider_multiple_times() {
		let mut m = model(&[SegmentType::Divider]);
		m.cursor = 1;
		m.apply(Key::Add);
		for c in "divider".chars() {
			m.apply(Key::Char(c));
		}
		m.apply(Key::Enter);
		let dividers = m
			.rows
			.iter()
			.filter(|r| *r.config.segment_type() == SegmentType::Divider)
			.count();
		assert_eq!(dividers, 2);
	}

	#[test]
	fn picker_down_clamps_within_results() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		m.apply(Key::Add);
		for c in "git".chars() {
			m.apply(Key::Char(c));
		}
		let len = picker::filtered("git").len();
		for _ in 0..(len + 5) {
			m.apply(Key::Down);
		}
		assert!(m.picker.selected < len);
	}

	#[test]
	fn d_inserts_divider_at_cursor() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.cursor = 1;
		assert_eq!(m.apply(Key::AddDivider), Effect::Redraw);
		let types: Vec<&SegmentType> = m.rows.iter().map(|r| r.config.segment_type()).collect();
		assert_eq!(
			types,
			vec![
				&SegmentType::Model,
				&SegmentType::Divider,
				&SegmentType::Cwd
			]
		);
		assert_eq!(m.focus, Focus::List);
		assert_eq!(m.cursor, 1);
		assert!(m.dirty);
	}

	#[test]
	fn replace_swaps_segment_in_place() {
		let mut m = model(&[SegmentType::Model, SegmentType::Cwd]);
		m.cursor = 0;
		assert_eq!(m.apply(Key::Replace), Effect::OpenPicker);
		assert_eq!(m.focus, Focus::Picker);
		assert_eq!(m.picker.replace_at, Some(0));

		for c in "version".chars() {
			m.apply(Key::Char(c));
		}
		m.apply(Key::Enter);
		assert_eq!(m.focus, Focus::List);
		let types: Vec<&SegmentType> = m.rows.iter().map(|r| r.config.segment_type()).collect();
		assert_eq!(types, vec![&SegmentType::Version, &SegmentType::Cwd]);
		assert_eq!(m.rows.len(), 2, "replace does not change the row count");
		assert_eq!(m.cursor, 0);
		assert!(m.dirty);
	}

	#[test]
	fn replace_keeps_enabled_state() {
		let mut m = model(&[SegmentType::Model]);
		m.rows[0].enabled = false;
		m.cursor = 0;
		m.apply(Key::Replace);
		for c in "version".chars() {
			m.apply(Key::Char(c));
		}
		m.apply(Key::Enter);
		assert_eq!(*m.rows[0].config.segment_type(), SegmentType::Version);
		assert!(
			!m.rows[0].enabled,
			"replace preserves the row's enabled flag"
		);
	}

	#[test]
	fn replace_on_add_row_is_noop() {
		let mut m = model(&[SegmentType::Model]);
		m.cursor = m.rows.len();
		assert_eq!(m.apply(Key::Replace), Effect::None);
		assert_eq!(m.focus, Focus::List);
		assert!(!m.dirty);
	}
}
