//! Renders the README screenshots from the scenarios in `screenshots/scenarios`.
//!
//! Each scenario runs the real binary with `--format spans` against its own HOME and git repo, and the spans are
//! drawn into a mock of Claude Code's prompt area, which headless Chrome saves as a PNG.
//!
//! ```text
//! cargo run -p statusline-screenshots --release              # every scenario
//! cargo run -p statusline-screenshots --release -- hero cache   # just these
//! ```

mod apng;
mod chrome;
mod page;
mod render;
mod repo;
mod scenario;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eyre::{Result, WrapErr, bail};
use statusline_core::util::home_dir;

use crate::scenario::{Clock, Scenario};

fn main() -> Result<()> {
	let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()?;
	// A relative CARGO_TARGET_DIR is relative to where cargo runs, which is the repo here. Passing it back explicitly
	// keeps the binary where this looks for it even if a cargo config names another target dir.
	let target = repo.join(env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| "target".into()));
	let status = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
		.args([
			"build",
			"-q",
			"--release",
			"-p",
			"statusline",
			"--target-dir",
		])
		.arg(&target)
		.current_dir(&repo)
		.status()
		.wrap_err("running cargo")?;
	if !status.success() {
		bail!("building statusline failed");
	}
	let binary = target.join("release/statusline");
	let out_dir = repo.join("screenshots");
	let fonts = page::Fonts::find(&home_dir()?.join("Library/Fonts"))?;

	let mut scenarios: Vec<PathBuf> = fs::read_dir(out_dir.join("scenarios"))?
		.map(|entry| entry.map(|e| e.path()))
		.collect::<Result<_, _>>()?;
	scenarios.retain(|path| path.extension().is_some_and(|ext| ext == "json"));
	scenarios.sort();

	let wanted: Vec<String> = env::args().skip(1).collect();
	if let Some(unknown) = wanted
		.iter()
		.find(|name| !scenarios.iter().any(|path| stem(path) == name.as_str()))
	{
		bail!("no scenario named {unknown:?}");
	}

	for path in &scenarios {
		let name = stem(path);
		if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
			continue;
		}
		let scenario = Scenario::load(path)?;
		let work = WorkDir::new()?;
		let mut pages = Vec::new();
		if let Some(spec) = &scenario.configure {
			let rows = render::configure(&scenario, spec, &work.0)
				.wrap_err_with(|| format!("rendering {name}"))?;
			pages.push(page::build_configure(&rows, &fonts)?);
		} else {
			for i in 0..scenario.frames {
				// Each frame's clock runs a second further, so countdowns fall and elapsed timers climb.
				let now = clock(next_second()? - Duration::from_secs(i.into()))?;
				let home = work.0.join(format!("home-{i}"));
				let frame = render::frame(&binary, &scenario, &home, now)
					.wrap_err_with(|| format!("rendering {name}"))?;
				pages.push(page::build(&scenario, &frame, &fonts)?);
			}
		}
		let out = out_dir.join(format!("{name}.png"));
		screenshot(name, &pages, &out, &work.0)?;
		println!("{}", out.display());
	}
	Ok(())
}

/// One page is a PNG. Several, a second apart, become a looping APNG so countdowns visibly tick.
fn screenshot(name: &str, pages: &[String], out: &Path, work: &Path) -> Result<()> {
	let mut paths = Vec::new();
	for (i, page) in pages.iter().enumerate() {
		let path = work.join(format!("{name}-{i:02}.html"));
		fs::write(&path, page)?;
		paths.push(path);
	}
	let mut size = (0, 0);
	for path in &paths {
		let (width, height) = chrome::measure(path, work)?;
		size = (size.0.max(width), size.1.max(height));
	}

	// Capture treats the file appearing as Chrome being done, so a stale one must not be there already.
	if out.exists() {
		fs::remove_file(out)?;
	}
	if let [path] = paths.as_slice() {
		return chrome::capture(path, out, size, work);
	}
	for path in &paths {
		chrome::capture(path, &path.with_extension("png"), size, work)?;
	}
	apng::encode(&work.join(format!("{name}-%02d.png")), out)
}

/// Waits for the next whole second. Scenario times are whole seconds and the binary reads the clock with its fraction,
/// so starting every frame at the same fraction keeps a countdown from repeating one second and skipping the next.
fn next_second() -> Result<SystemTime> {
	let now = SystemTime::now();
	let fraction = now.duration_since(UNIX_EPOCH)?.subsec_nanos();
	let wait = Duration::from_secs(1) - Duration::from_nanos(fraction.into());
	std::thread::sleep(wait);
	Ok(now + wait)
}

fn clock(at: SystemTime) -> Result<Clock> {
	let since = at.duration_since(UNIX_EPOCH)?;
	Ok(Clock {
		secs: i64::try_from(since.as_secs())?,
		millis: i64::try_from(since.as_millis())?,
	})
}

fn stem(path: &Path) -> &str {
	path.file_stem()
		.and_then(|s| s.to_str())
		.unwrap_or_default()
}

/// A scratch directory under the system temp dir, removed when dropped.
struct WorkDir(PathBuf);

impl WorkDir {
	fn new() -> Result<Self> {
		let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
		let path =
			env::temp_dir().join(format!("statusline-screenshots-{}-{nanos}", process::id()));
		fs::create_dir_all(&path).wrap_err_with(|| format!("creating {}", path.display()))?;
		Ok(Self(path))
	}
}

impl Drop for WorkDir {
	fn drop(&mut self) {
		// Best effort, since a leftover scratch directory in the temp dir costs nothing.
		let _ = fs::remove_dir_all(&self.0);
	}
}
