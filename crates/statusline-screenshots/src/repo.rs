use std::fs;
use std::path::Path;
use std::process::Command;

use eyre::{Result, WrapErr, bail};

use crate::scenario::GitSpec;

/// Builds a repo at `path` in the state the scenario's git segments describe, with a bare upstream beside it.
pub fn make(path: &Path, spec: &GitSpec) -> Result<()> {
	fs::create_dir_all(path).wrap_err_with(|| format!("creating {}", path.display()))?;
	let readme = path.join("README.md");
	git(path, &["init", "-q", "-b", &spec.branch])?;
	git(path, &["config", "user.email", "demo@example.com"])?;
	git(path, &["config", "user.name", "demo"])?;
	fs::write(&readme, "demo\n")?;
	git(path, &["add", "."])?;
	git(path, &["commit", "-q", "-m", "initial"])?;

	let parent = path.parent().unwrap_or(path);
	let upstream = parent.join("upstream.git");
	git(
		parent,
		&[
			"clone",
			"-q",
			"--bare",
			&path.to_string_lossy(),
			&upstream.to_string_lossy(),
		],
	)?;
	git(
		path,
		&["remote", "add", "origin", &upstream.to_string_lossy()],
	)?;
	git(path, &["fetch", "-q", "origin"])?;
	git(
		path,
		&[
			"branch",
			"-q",
			&format!("--set-upstream-to=origin/{}", spec.branch),
		],
	)?;

	for i in 0..spec.ahead {
		fs::write(&readme, format!("demo {i}\n"))?;
		git(path, &["commit", "-q", "-am", &format!("change {i}")])?;
	}
	for _ in 0..spec.stash {
		fs::write(path.join("wip.txt"), "wip\n")?;
		git(path, &["add", "wip.txt"])?;
		git(path, &["stash", "-q"])?;
	}
	if spec.dirty {
		fs::write(&readme, "dirty\n")?;
	}
	Ok(())
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
	let output = Command::new("git")
		.arg("-C")
		.arg(dir)
		.args(args)
		.output()
		.wrap_err("running git")?;
	if !output.status.success() {
		bail!(
			"git {} failed: {}",
			args.join(" "),
			String::from_utf8_lossy(&output.stderr).trim()
		);
	}
	Ok(())
}
