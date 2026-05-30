use statusline_core::sample::SampleData;
use statusline_core::settings::{Settings, SettingsError};
use std::path::{Path, PathBuf};

mod colorpick;
mod draw;
mod lineedit;
pub mod model;
mod options;
mod picker;
mod theme;
mod view;

pub use model::{EditorModel, Effect, Focus, Key, Row};

#[derive(Debug, thiserror::Error)]
pub enum ConfigureError {
	#[error(transparent)]
	Settings(#[from] SettingsError),
	#[error("terminal error: {0}")]
	Terminal(#[from] std::io::Error),
	#[error("not a terminal — run `statusline configure` in an interactive terminal")]
	NotATty,
}

pub struct Options {
	pub settings_path: PathBuf,
	pub sample: Option<SampleData>,
}

pub enum Outcome {
	Saved(PathBuf),
	Cancelled,
}

pub fn run(opts: Options) -> Result<Outcome, ConfigureError> {
	let settings = load_or_default(&opts.settings_path)?;
	let sample = opts.sample.unwrap_or_else(SampleData::representative);
	match edit_with_sample(&settings, &sample)? {
		Some(updated) => {
			updated.save(&opts.settings_path)?;
			Ok(Outcome::Saved(opts.settings_path))
		}
		None => Ok(Outcome::Cancelled),
	}
}

pub fn edit(settings: &Settings) -> Result<Option<Settings>, ConfigureError> {
	let sample = SampleData::representative();
	edit_with_sample(settings, &sample)
}

fn edit_with_sample(
	settings: &Settings,
	sample: &SampleData,
) -> Result<Option<Settings>, ConfigureError> {
	draw::run_editor(settings, sample)
}

fn load_or_default(path: &Path) -> Result<Settings, ConfigureError> {
	match Settings::load_from(path) {
		Ok(s) => Ok(s),
		Err(SettingsError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
			Ok(default_settings())
		}
		Err(e) => Err(e.into()),
	}
}

pub(crate) fn default_settings() -> Settings {
	use statusline_core::settings::{DEFAULT_FIVE_HOUR_RESET, DEFAULT_SEVEN_DAY_RESET};
	Settings {
		five_hour_reset_threshold: DEFAULT_FIVE_HOUR_RESET.into(),
		seven_day_reset_threshold: DEFAULT_SEVEN_DAY_RESET.into(),
		segments: None,
		divider: None,
		nerd_font: false,
		browser: None,
		skip_update_check: false,
		extra: serde_json::Map::default(),
	}
}
