use std::path::Path;
use std::process::Command;

use eyre::{Result, WrapErr, bail};

/// Joins the PNGs matching the ffmpeg `pattern`, such as `hero-%02d.png`, into an APNG at `out` that loops forever
/// at a frame a second.
pub fn encode(pattern: &Path, out: &Path) -> Result<()> {
	let status = Command::new("ffmpeg")
		.args(["-loglevel", "error", "-y", "-framerate", "1", "-i"])
		.arg(pattern)
		.args(["-plays", "0", "-f", "apng"])
		.arg(out)
		.status()
		.wrap_err("running ffmpeg")?;
	if !status.success() {
		bail!("ffmpeg failed to encode {}", out.display());
	}
	Ok(())
}
