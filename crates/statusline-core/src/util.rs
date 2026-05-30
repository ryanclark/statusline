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
