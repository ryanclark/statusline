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

pub use model::{EditorModel, Effect, Focus, Key, Mode, Row};

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
	/// Claude Code's settings.json, so the subagent tab can tell whether it is wired up and install it.
	pub claude_settings_path: Option<PathBuf>,
}

pub enum Outcome {
	Saved(PathBuf),
	Cancelled,
}

pub fn run(opts: Options) -> Result<Outcome, ConfigureError> {
	let settings = load_or_default(&opts.settings_path)?;
	let sample = opts.sample.unwrap_or_else(SampleData::representative);
	match edit_with_sample(&settings, &sample, opts.claude_settings_path.as_deref())? {
		Some(updated) => {
			updated.save(&opts.settings_path)?;
			Ok(Outcome::Saved(opts.settings_path))
		}
		None => Ok(Outcome::Cancelled),
	}
}

pub fn edit(settings: &Settings) -> Result<Option<Settings>, ConfigureError> {
	let sample = SampleData::representative();
	edit_with_sample(settings, &sample, None)
}

fn edit_with_sample(
	settings: &Settings,
	sample: &SampleData,
	claude_settings_path: Option<&Path>,
) -> Result<Option<Settings>, ConfigureError> {
	draw::run_editor(settings, sample, claude_settings_path)
}

fn load_or_default(path: &Path) -> Result<Settings, ConfigureError> {
	match Settings::load_from(path) {
		Ok(s) => Ok(s),
		Err(SettingsError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
			Ok(Settings::default())
		}
		Err(e) => Err(e.into()),
	}
}
