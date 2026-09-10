use serde::{Deserialize, Deserializer};
use std::io;
use std::path::PathBuf;

pub fn home_dir() -> io::Result<PathBuf> {
	dirs::home_dir().ok_or_else(|| {
		io::Error::new(
			io::ErrorKind::NotFound,
			"could not determine home directory",
		)
	})
}

pub fn app_data_dir() -> io::Result<PathBuf> {
	Ok(home_dir()?.join(".statusline"))
}

/// Reads an explicit JSON `null` as the field's default. `#[serde(default)]` only covers a missing key,
/// and both Claude Code (`current_usage` before the first response) and the claude.ai usage API write
/// `null` for values they have not computed, which would otherwise fail the whole parse.
pub fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
	D: Deserializer<'de>,
	T: Default + Deserialize<'de>,
{
	Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}
