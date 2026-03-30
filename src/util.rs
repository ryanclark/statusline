use std::path::PathBuf;
use eyre::Result;

pub(crate) fn home_dir() -> Result<PathBuf> {
	dirs::home_dir().ok_or_else(|| eyre::eyre!("could not determine home directory"))
}

pub(crate) fn app_data_dir() -> Result<PathBuf> {
	Ok(home_dir()?.join(".statusline"))
}
