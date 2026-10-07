//! `statusline install --plugin` and `--native`. Plugin state goes through `claude plugin`, and only `statusLine` and
//! `subagentStatusLine` are edited directly.

use crate::format::Percentage;
use crate::install::{Ask, ok, prompt_yes_no, save_edits, skip, wants_subagent, warn};
use crate::settings::Settings;
use crate::util::home_dir;
use eyre::{Result, WrapErr, bail, eyre};
use owo_colors::OwoColorize;
use serde::Deserialize;
use serde_json::Value;
use statusline_core::claude_settings::{
	Edit, Entry, Outcome, Removal, ensure_entries, is_ours, read_settings, remove_entry,
	restore_entry, write_settings,
};
use std::ffi::OsStr;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PLUGIN_ID: &str = "statusline@ryanclark";
const MARKETPLACE: &str = "ryanclark";
const MARKETPLACE_SOURCE: &str = "ryanclark/statusline";
/// A marketplace added with older paths no longer checks out the plugin, so it is re-added.
const SPARSE_PATHS: [&str; 2] = [".claude-plugin", "crates/statusline/plugin"];
/// The first release that loads plugin hooks modules. An older CLI installs the plugin but never runs it.
const MIN_CLAUDE: semver::Version = semver::Version::new(2, 1, 287);

pub(crate) struct Context {
	pub claude: PathBuf,
	pub home: PathBuf,
	pub interactive: bool,
	pub dry_run: bool,
}

impl Context {
	fn claude_settings(&self) -> PathBuf {
		self.home.join(".claude").join("settings.json")
	}

	fn data_dir(&self) -> PathBuf {
		self.home.join(".statusline")
	}

	fn native_file(&self) -> PathBuf {
		self.data_dir().join("native-statusline.json")
	}

	fn claude(&self) -> Claude<'_> {
		Claude {
			program: &self.claude,
			cwd: &self.home,
			dry_run: self.dry_run,
		}
	}
}

pub(crate) struct PluginOptions {
	pub keep_native: bool,
	pub subagent: bool,
	pub five_hour_reset_threshold: Option<Percentage>,
	pub seven_day_reset_threshold: Option<Percentage>,
}

pub(crate) enum Mode {
	Plugin(PluginOptions),
	Native { remove_marketplace: bool },
}

pub(crate) fn run(mode: Mode, claude: PathBuf, dry_run: bool) -> Result<()> {
	let ctx = Context {
		claude,
		home: home_dir()?,
		interactive: std::io::stdin().is_terminal(),
		dry_run,
	};
	let mut out = std::io::stdout();
	let mut ask = prompt_yes_no;
	match mode {
		Mode::Plugin(opts) => {
			let exe = std::env::current_exe()?;
			let binary = locate_binary(std::env::var_os("PATH").as_deref(), &exe);
			install_plugin(&ctx, &binary, &opts, &mut out, &mut ask)
		}
		Mode::Native { remove_marketplace } => {
			install_native(&ctx, remove_marketplace, &mut out, &mut ask)
		}
	}
}

fn install_plugin(
	ctx: &Context,
	binary: &Path,
	opts: &PluginOptions,
	out: &mut dyn Write,
	ask: Ask<'_>,
) -> Result<()> {
	let claude = ctx.claude();
	let version = claude.version()?;
	if version < MIN_CLAUDE {
		bail!(
			"Claude Code {MIN_CLAUDE} or newer is required for the plugin and {} is {version}. Run `claude update`, or \
			 keep the native line with `statusline install`",
			ctx.claude.display()
		);
	}
	ok(out, format_args!("Claude Code {version}"))?;

	ensure_settings(ctx, opts, out)?;
	ensure_marketplace(ctx, &claude, out)?;
	let enabled = ensure_plugin(ctx, &claude, binary, out)?;

	// Read after the `claude` calls above, which write this file. `commit` replays the edits onto a fresh read.
	let path = ctx.claude_settings();
	let original = read_settings(&path)?;
	let mut settings = original.clone();
	let mut actions: Vec<Action> =
		plan_native_line(ctx, opts.keep_native, enabled, &mut settings, out, ask)?
			.into_iter()
			.collect();

	if settings.get(Entry::Subagent.key()).is_some() {
		ok(out, "Kept subagent status line")?;
	} else if wants_subagent(&settings, opts.subagent, ctx.interactive, ask) {
		ensure_entries(&mut settings, &[Entry::Subagent], &mut |_| false)?;
		actions.push(Action::new(
			Entry::Subagent,
			"Configured the subagent status line",
			"configure the subagent status line",
		));
	}

	commit(ctx, &path, &original, &settings, &actions, out)?;

	if ctx.dry_run {
		writeln!(out, "{}", "Dry run: nothing was changed".dimmed())?;
	} else {
		writeln!(
			out,
			"{} Restart open Claude Code sessions to load it. To go back: {}",
			"Plugin installed.".green().bold(),
			"statusline install --native".green()
		)?;
	}

	Ok(())
}

fn ensure_settings(ctx: &Context, opts: &PluginOptions, out: &mut dyn Write) -> Result<()> {
	let path = ctx.data_dir().join("settings.json");
	let existed = path.exists();
	if ctx.dry_run {
		return if existed {
			skip(out, format_args!("Kept {}", path.display()))
		} else {
			would(
				out,
				format_args!("save default settings to {}", path.display()),
			)
		};
	}
	Settings::ensure_at(
		&path,
		opts.five_hour_reset_threshold,
		opts.seven_day_reset_threshold,
	)?;
	if existed {
		skip(out, format_args!("Kept {}", path.display()))
	} else {
		ok(out, "Saved default settings")
	}
}

fn ensure_marketplace(ctx: &Context, claude: &Claude<'_>, out: &mut dyn Write) -> Result<()> {
	let old_paths =
		if has_marketplace(&claude.list(&["plugin", "marketplace", "list", "--json"])?) {
			let Some(old_paths) = read_settings(&ctx.claude_settings())
				.ok()
				.and_then(|s| stale_sparse_paths(&s))
			else {
				return skip(out, format_args!("Marketplace {MARKETPLACE} already added"));
			};
			// A declared marketplace cannot be re-added with other paths. Removing it keeps the plugin and its options.
			let args = [
				"plugin",
				"marketplace",
				"remove",
				MARKETPLACE,
				"--scope",
				"user",
				"--json",
			];
			claude.mutate(out, &args, None)?;
			Some(old_paths)
		} else {
			None
		};
	let ran = match claude.mutate(out, &marketplace_add_args(SPARSE_PATHS), None) {
		Ok(ran) => ran,
		Err(err) => {
			let Some(old_paths) = old_paths else {
				return Err(err);
			};
			// The add clones from GitHub, so it can fail after the remove. Restoring the old entry keeps the
			// installed plugin backed by a marketplace until a rerun moves it.
			let _ = claude.mutate(
				out,
				&marketplace_add_args(old_paths.iter().map(String::as_str)),
				None,
			);
			return Err(err.wrap_err(format!(
				"re-adding marketplace {MARKETPLACE} with the moved plugin path failed. Run `statusline install \
				 --plugin` again to retry"
			)));
		}
	};
	if ran == Ran::Planned {
		return Ok(());
	}
	if old_paths.is_some() {
		ok(
			out,
			format_args!("Re-added marketplace {MARKETPLACE} with the moved plugin path"),
		)
	} else {
		ok(out, format_args!("Added marketplace {MARKETPLACE}"))
	}
}

fn marketplace_add_args<'a>(sparse: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
	let mut args = vec![
		"plugin",
		"marketplace",
		"add",
		MARKETPLACE_SOURCE,
		"--scope",
		"user",
		"--sparse",
	];
	args.extend(sparse);
	args.push("--json");
	args
}

fn ensure_plugin(
	ctx: &Context,
	claude: &Claude<'_>,
	binary: &Path,
	out: &mut dyn Write,
) -> Result<bool> {
	let binary = binary.to_string_lossy();
	let plugins = claude.list(&["plugin", "list", "--json"])?;
	let Some(plugin) = find_plugin(&plugins) else {
		let config = format!("binary={binary}");
		let args = [
			"plugin", "install", PLUGIN_ID, "-s", "user", "--config", &config, "--json",
		];
		if claude.mutate(out, &args, None)? == Ran::Done {
			ok(out, format_args!("Installed {PLUGIN_ID} (binary={binary})"))?;
		}
		// A fresh install is enabled, so only a plugin that was already installed can be disabled.
		return Ok(true);
	};

	let enabled = plugin.enabled != Some(false);
	if !enabled {
		warn(
			out,
			format_args!(
				"{PLUGIN_ID} is installed but disabled. Run `claude plugin enable {PLUGIN_ID}`"
			),
		)?;
	}
	let current = read_settings(&ctx.claude_settings())
		.ok()
		.and_then(|s| configured_binary(&s).map(str::to_owned));
	if current.as_deref() == Some(&*binary) {
		skip(
			out,
			format_args!("{PLUGIN_ID} already installed with binary={binary}"),
		)?;
		return Ok(enabled);
	}

	let args = ["plugin", "configure", PLUGIN_ID, "--values-stdin"];
	let values = serde_json::json!({ "binary": binary }).to_string();
	if claude.mutate(out, &args, Some(&values))? == Ran::Done {
		ok(
			out,
			format_args!("Configured {PLUGIN_ID} (binary={binary})"),
		)?;
	}

	Ok(enabled)
}

/// Saves the native statusLine and removes it unless it should stay, returning the removal to commit.
fn plan_native_line(
	ctx: &Context,
	keep_native: bool,
	plugin_enabled: bool,
	settings: &mut Value,
	out: &mut dyn Write,
	ask: Ask<'_>,
) -> Result<Option<Action>> {
	let Some(current) = settings.get(Entry::StatusLine.key()).cloned() else {
		skip(out, "No native statusLine to keep or remove")?;
		return Ok(None);
	};
	save_native(ctx, &current, out)?;

	if keep_native {
		if is_ours(Entry::StatusLine, &current) {
			ok(
				out,
				"Kept native statusLine as the fallback. It stays silent while the plugin draws",
			)?;
		} else {
			warn(
				out,
				format_args!(
					"Kept your statusLine ({}). It draws alongside the plugin. Rerun without --keep-native to drop it",
					describe(&current)
				),
			)?;
		}
		return Ok(None);
	}
	if !plugin_enabled {
		// Nothing else draws while the plugin is disabled.
		warn(
			out,
			format_args!(
				"Kept the native statusLine ({}) because {PLUGIN_ID} is disabled. Enable it, then rerun statusline \
				 install --plugin",
				describe(&current)
			),
		)?;
		return Ok(None);
	}

	let removal = remove_entry(settings, Entry::StatusLine, &mut |_, value| {
		ctx.interactive
			&& ask(
				&format!(
					"Remove your own statusLine ({})? It is saved for `statusline install --native`",
					describe(value)
				),
				false,
			)
	})?;
	match removal {
		Removal::Removed => Ok(Some(Action::new(
			Entry::StatusLine,
			"Removed native statusLine",
			"remove the native statusLine",
		))),
		Removal::Declined => {
			skip(
				out,
				format_args!(
					"Kept your statusLine ({}), so it draws alongside the plugin",
					describe(&current)
				),
			)?;
			Ok(None)
		}
		Removal::Absent => Ok(None),
	}
}

fn install_native(
	ctx: &Context,
	remove_marketplace: bool,
	out: &mut dyn Write,
	ask: Ask<'_>,
) -> Result<()> {
	// Restore first, so a failed restore leaves the plugin drawing rather than no line at all.
	let path = ctx.claude_settings();
	let original = read_settings(&path)?;
	let mut settings = original.clone();
	let saved = read_native(ctx)?;
	let value = saved.clone().unwrap_or_else(|| Entry::StatusLine.desired());
	let restored = restore_entry(&mut settings, Entry::StatusLine, &value, &mut |_| {
		ctx.interactive
			&& ask(
				&format!(
					"statusLine is set to {}. Replace it with {}?",
					settings_value(&original),
					describe(&value)
				),
				false,
			)
	})?;
	let mut actions = Vec::new();
	match restored {
		Outcome::Written if saved.is_some() => {
			actions.push(Action::new(
				Entry::StatusLine,
				format!("Restored native statusLine ({})", describe(&value)),
				format!("restore native statusLine ({})", describe(&value)),
			));
		}
		Outcome::Written => actions.push(Action::new(
			Entry::StatusLine,
			"Configured the native statusLine",
			"configure the native statusLine",
		)),
		Outcome::Kept => skip(out, "Native statusLine already set")?,
		Outcome::Skipped => skip(out, "Left the native statusLine as it was")?,
	}
	commit(ctx, &path, &original, &settings, &actions, out)?;

	// Rollback must work when `claude` is missing or broken, so plugin failures are reported and the rest still runs.
	let claude = ctx.claude();
	let mut failed = None;
	if let Err(e) = uninstall_plugin(&claude, out) {
		warn(out, format_args!("Skipped the plugin uninstall: {e}"))?;
		failed = Some(e);
	}
	if remove_marketplace && let Err(e) = drop_marketplace(&claude, out) {
		warn(out, format_args!("Skipped the marketplace removal: {e}"))?;
		failed = failed.or(Some(e));
	}

	if ctx.dry_run {
		writeln!(out, "{}", "Dry run: nothing was changed".dimmed())?;
	} else if failed.is_none() {
		writeln!(
			out,
			"{} Restart open Claude Code sessions to apply it",
			"Native statusLine installed.".green().bold()
		)?;
	}

	failed.map_or(Ok(()), Err)
}

fn uninstall_plugin(claude: &Claude<'_>, out: &mut dyn Write) -> Result<()> {
	if find_plugin(&claude.list(&["plugin", "list", "--json"])?).is_none() {
		return skip(out, format_args!("{PLUGIN_ID} is not installed"));
	}
	let args = ["plugin", "uninstall", PLUGIN_ID, "-s", "user", "--json"];
	match claude.mutate_unless(out, &args, "not_installed")? {
		Ran::Done => ok(out, format_args!("Uninstalled {PLUGIN_ID}")),
		Ran::Already => skip(out, format_args!("{PLUGIN_ID} is not installed")),
		Ran::Planned => Ok(()),
	}
}

fn drop_marketplace(claude: &Claude<'_>, out: &mut dyn Write) -> Result<()> {
	if !has_marketplace(&claude.list(&["plugin", "marketplace", "list", "--json"])?) {
		return skip(out, format_args!("Marketplace {MARKETPLACE} is not added"));
	}
	let args = [
		"plugin",
		"marketplace",
		"remove",
		MARKETPLACE,
		"--scope",
		"user",
		"--json",
	];
	match claude.mutate_unless(out, &args, "not_configured")? {
		Ran::Done => ok(out, format_args!("Removed marketplace {MARKETPLACE}")),
		Ran::Already => skip(out, format_args!("Marketplace {MARKETPLACE} is not added")),
		Ran::Planned => Ok(()),
	}
}

/// Backs up and writes Claude Code's settings only when something changed. An edit whose entry changed underneath is
/// skipped.
fn commit(
	ctx: &Context,
	path: &Path,
	original: &Value,
	settings: &Value,
	actions: &[Action],
	out: &mut dyn Write,
) -> Result<()> {
	let edits = Edit::between(original, settings, &[Entry::StatusLine, Entry::Subagent]);
	if edits.is_empty() {
		return Ok(());
	}
	if ctx.dry_run {
		for action in actions {
			would(out, &action.planned)?;
		}
		return would(out, format_args!("back up and update {}", path.display()));
	}

	let stale = save_edits(path, &edits, || back_up(ctx, path, out))?;
	for entry in &stale {
		warn(
			out,
			format_args!(
				"{} changed while waiting, so it was left as it now is",
				entry.key()
			),
		)?;
	}
	if stale.len() == edits.len() {
		return Ok(());
	}
	for action in actions.iter().filter(|a| !stale.contains(&a.entry)) {
		ok(out, &action.done)?;
	}

	Ok(())
}

fn back_up(ctx: &Context, path: &Path, out: &mut dyn Write) -> Result<()> {
	if !path.exists() {
		return Ok(());
	}
	let backups = ctx.data_dir().join("backups");
	std::fs::create_dir_all(&backups)?;
	let millis = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map_or(0, |d| d.as_millis());
	let backup = backups.join(format!("claude-settings.{millis}.json"));
	std::fs::copy(path, &backup).wrap_err_with(|| format!("backing up {}", path.display()))?;
	ok(
		out,
		format_args!("Backed up {} to {}", path.display(), backup.display()),
	)
}

/// Saves the user's line, ours or not, for `--native` to restore. A saved line of their own is never replaced by ours,
/// since a plain `statusline install` between two plugin installs would otherwise lose it.
fn save_native(ctx: &Context, current: &Value, out: &mut dyn Write) -> Result<()> {
	if let Some(saved) = read_native(ctx)?
		&& (saved == *current
			|| (is_ours(Entry::StatusLine, current) && !is_ours(Entry::StatusLine, &saved)))
	{
		return Ok(());
	}
	let file = ctx.native_file();
	if ctx.dry_run {
		return would(
			out,
			format_args!("save the native statusLine to {}", file.display()),
		);
	}
	write_settings(&file, current).wrap_err_with(|| format!("saving {}", file.display()))?;
	ok(
		out,
		format_args!("Saved the native statusLine to {}", file.display()),
	)
}

fn read_native(ctx: &Context) -> Result<Option<Value>> {
	let file = ctx.native_file();
	match std::fs::read_to_string(&file) {
		Ok(data) => Ok(Some(
			serde_json::from_str(&data).wrap_err_with(|| format!("reading {}", file.display()))?,
		)),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(e.into()),
	}
}

/// Prefers a PATH entry that resolves to this executable, so a Homebrew install stores /opt/homebrew/bin/statusline,
/// which survives `brew upgrade`.
fn locate_binary(path_var: Option<&OsStr>, exe: &Path) -> PathBuf {
	let Ok(target) = exe.canonicalize() else {
		return exe.to_path_buf();
	};
	path_var
		.into_iter()
		.flat_map(std::env::split_paths)
		.filter(|dir| dir.is_absolute())
		.map(|dir| dir.join("statusline"))
		.find(|candidate| candidate.canonicalize().is_ok_and(|c| c == target))
		.unwrap_or_else(|| exe.to_path_buf())
}

fn has_marketplace(list: &[Marketplace]) -> bool {
	list.iter().any(|m| m.name == MARKETPLACE)
}

/// Returns the recorded sparse paths when they miss one we need. A full checkout, with none, always has the plugin.
fn stale_sparse_paths(settings: &Value) -> Option<Vec<String>> {
	let paths: Vec<String> = settings
		.pointer(&format!(
			"/extraKnownMarketplaces/{MARKETPLACE}/source/sparsePaths"
		))?
		.as_array()?
		.iter()
		.filter_map(|v| v.as_str().map(str::to_owned))
		.collect();
	SPARSE_PATHS
		.iter()
		.any(|p| !paths.iter().any(|v| v == p))
		.then_some(paths)
}

/// Only the user-scope install is ours, since that is the scope `--plugin` installs into and `--native` removes from.
fn find_plugin(list: &[InstalledPlugin]) -> Option<&InstalledPlugin> {
	list.iter()
		.find(|p| p.id == PLUGIN_ID && p.scope.as_deref().is_none_or(|s| s == "user"))
}

fn configured_binary(settings: &Value) -> Option<&str> {
	settings
		.get("pluginConfigs")?
		.get(PLUGIN_ID)?
		.get("options")?
		.get("binary")?
		.as_str()
}

fn parse_version(stdout: &str) -> Option<semver::Version> {
	semver::Version::parse(stdout.split_whitespace().next()?).ok()
}

/// The `--json` result of a mutating command, printed as the last line of stdout.
fn result_line(stdout: &str) -> Option<CliResult> {
	let line = stdout.lines().rev().find(|l| !l.trim().is_empty())?;
	serde_json::from_str(line).ok()
}

fn describe(value: &Value) -> String {
	value
		.get("command")
		.and_then(Value::as_str)
		.map_or_else(|| value.to_string(), str::to_owned)
}

fn settings_value(settings: &Value) -> String {
	settings
		.get(Entry::StatusLine.key())
		.map_or_else(|| "nothing".to_owned(), describe)
}

/// An entry of `claude plugin marketplace list --json`.
#[derive(Debug, Deserialize)]
struct Marketplace {
	#[serde(default)]
	name: String,
}

/// An entry of `claude plugin list --json`.
#[derive(Debug, Deserialize)]
struct InstalledPlugin {
	#[serde(default)]
	id: String,
	scope: Option<String>,
	enabled: Option<bool>,
}

/// The `--json` result line of a mutating `claude plugin` command.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliResult {
	message: Option<String>,
	failure_code: Option<String>,
}

/// A settings edit, worded for both the applied message and the dry run.
struct Action {
	entry: Entry,
	done: String,
	planned: String,
}

impl Action {
	fn new(entry: Entry, done: impl Into<String>, planned: impl Into<String>) -> Self {
		Self {
			entry,
			done: done.into(),
			planned: planned.into(),
		}
	}
}

/// What a mutating `claude` call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ran {
	Done,
	/// A dry run printed the command instead.
	Planned,
	/// It failed because the thing was already gone.
	Already,
}

struct Claude<'a> {
	program: &'a Path,
	/// The home dir, so project settings from the current directory do not leak into the lists.
	cwd: &'a Path,
	dry_run: bool,
}

impl Claude<'_> {
	fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<Output> {
		let mut child = Command::new(self.program)
			.args(args)
			.current_dir(self.cwd)
			// A child waiting on a prompt nobody can see would hang the install.
			.stdin(if stdin.is_some() {
				Stdio::piped()
			} else {
				Stdio::null()
			})
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.wrap_err_with(|| {
				format!(
					"running {} (pass --claude <path> if it is not on PATH)",
					self.program.display()
				)
			})?;
		if let Some(input) = stdin
			&& let Some(mut pipe) = child.stdin.take()
		{
			pipe.write_all(input.as_bytes())?;
		}

		Ok(child.wait_with_output()?)
	}

	fn ok_stdout(&self, args: &[&str], stdin: Option<&str>) -> Result<String> {
		let output = self.run(args, stdin)?;
		if !output.status.success() {
			return Err(failure(args, &output));
		}

		Ok(String::from_utf8_lossy(&output.stdout).into_owned())
	}

	fn version(&self) -> Result<semver::Version> {
		let stdout = self.ok_stdout(&["--version"], None)?;
		parse_version(&stdout).ok_or_else(|| {
			eyre!(
				"could not read a version from `{} --version`: {}",
				self.program.display(),
				stdout.trim()
			)
		})
	}

	fn list<T: serde::de::DeserializeOwned>(&self, args: &[&str]) -> Result<Vec<T>> {
		serde_json::from_str(&self.ok_stdout(args, None)?)
			.wrap_err_with(|| format!("parsing `claude {}`", args.join(" ")))
	}

	fn mutate(&self, out: &mut dyn Write, args: &[&str], stdin: Option<&str>) -> Result<Ran> {
		if self.dry_run {
			return self.plan(out, args, stdin);
		}
		self.ok_stdout(args, stdin)?;
		Ok(Ran::Done)
	}

	/// Returns [`Ran::Already`] when the command fails with the `failureCode` `already`.
	fn mutate_unless(&self, out: &mut dyn Write, args: &[&str], already: &str) -> Result<Ran> {
		if self.dry_run {
			return self.plan(out, args, None);
		}
		let output = self.run(args, None)?;
		if output.status.success() {
			return Ok(Ran::Done);
		}
		let code =
			result_line(&String::from_utf8_lossy(&output.stdout)).and_then(|r| r.failure_code);
		if code.as_deref() == Some(already) {
			return Ok(Ran::Already);
		}
		Err(failure(args, &output))
	}

	fn plan(&self, out: &mut dyn Write, args: &[&str], stdin: Option<&str>) -> Result<Ran> {
		would(
			out,
			format_args!("run {} {}", self.program.display(), args.join(" ")),
		)?;
		if let Some(input) = stdin {
			writeln!(out, "    with stdin {input}")?;
		}
		Ok(Ran::Planned)
	}
}

/// Prefers the `--json` result's message, since stderr also carries warnings about unrelated settings.
fn failure(args: &[&str], output: &Output) -> eyre::Report {
	let message = result_line(&String::from_utf8_lossy(&output.stdout))
		.and_then(|r| r.message)
		.unwrap_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_owned());
	eyre!("`claude {}` failed: {message}", args.join(" "))
}

fn would(out: &mut dyn Write, msg: impl std::fmt::Display) -> Result<()> {
	writeln!(out, "{} would {msg}", "→".cyan())?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	use std::fs;
	use std::os::unix::fs::PermissionsExt;

	// Recorded from Claude Code 2.1.290 against a directory marketplace in a scratch HOME.
	const MARKETPLACE_LIST: &str = r#"[
  {
    "name": "ryanclark",
    "source": "directory",
    "path": "/tmp/mkt",
    "installLocation": "/tmp/mkt"
  }
]
"#;
	const PLUGIN_LIST: &str = r#"[
  {
    "id": "statusline@ryanclark",
    "version": "0.1.0",
    "scope": "user",
    "enabled": true,
    "installPath": "/tmp/home/.claude/plugins/cache/ryanclark/statusline/0.1.0",
    "readFromFolder": "/tmp/mkt/plugin",
    "folderVersion": "0.1.0",
    "installedAt": "2026-10-06T13:25:21.661Z",
    "lastUpdated": "2026-10-06T13:25:21.661Z",
    "projectEnabled": false,
    "hasUserConfig": true
  }
]
"#;
	const MARKETPLACE_ADD: &str = r#"{"command":"marketplace-add","outcome":"ok","marketplace":"ryanclark","message":"Successfully added marketplace: ryanclark (declared in user settings)"}"#;
	const INSTALL: &str = r#"{"command":"install","outcome":"ok","plugin":"statusline@ryanclark","pluginId":"statusline@ryanclark","scope":"user","message":"Successfully installed plugin: statusline@ryanclark (scope: user)\n3 userConfig options not yet set — run /plugin configure statusline@ryanclark in Claude Code, or pass --config KEY=VALUE.","configApplied":true}"#;
	const UNINSTALL: &str = r#"{"command":"uninstall","outcome":"ok","plugin":"statusline@ryanclark","pluginId":"statusline@ryanclark","scope":"user","keptData":false,"message":"Successfully uninstalled plugin: statusline (scope: user)"}"#;
	const UNINSTALL_MISSING: &str = r#"{"command":"uninstall","outcome":"failed","plugin":"statusline@ryanclark","scope":"user","message":"Plugin \"statusline@ryanclark\" not found in installed plugins","failureCode":"not_installed"}"#;
	const MARKETPLACE_REMOVE: &str = r#"{"command":"marketplace-remove","outcome":"ok","marketplace":"ryanclark","message":"Successfully removed marketplace: ryanclark (from user settings)"}"#;
	const INSTALL_NOT_FOUND: &str = r#"{"command":"install","outcome":"failed","plugin":"statusline@ryanclark","scope":"user","message":"Plugin \"statusline\" not found in marketplace \"ryanclark\". Your local copy may be out of date — try `claude plugin marketplace update ryanclark`.","failureCode":"not_found"}"#;
	/// What `claude plugin install --config binary=…` leaves in ~/.claude/settings.json.
	const SETTINGS_AFTER_INSTALL: &str = r#"{
  "enabledPlugins": {
    "statusline@ryanclark": true
  },
  "extraKnownMarketplaces": {
    "ryanclark": {
      "source": {
        "source": "github",
        "repo": "ryanclark/statusline"
      }
    }
  },
  "pluginConfigs": {
    "statusline@ryanclark": {
      "options": {
        "binary": "/opt/homebrew/bin/statusline"
      }
    }
  }
}"#;

	/// What `claude plugin marketplace add … --sparse .claude-plugin plugin` wrote before the plugin moved.
	const SETTINGS_OLD_SPARSE: &str = r#"{
  "extraKnownMarketplaces": {
    "ryanclark": {
      "source": {
        "source": "github",
        "repo": "ryanclark/statusline",
        "sparsePaths": [
          ".claude-plugin",
          "plugin"
        ]
      }
    }
  }
}"#;

	#[test]
	fn recorded_lists_are_matched_by_name_id_and_scope() {
		let marketplaces: Vec<Marketplace> = serde_json::from_str(MARKETPLACE_LIST).unwrap();
		assert!(has_marketplace(&marketplaces));
		assert!(!has_marketplace(&[]));

		let plugins: Vec<InstalledPlugin> = serde_json::from_str(PLUGIN_LIST).unwrap();
		assert_eq!(find_plugin(&plugins).unwrap().enabled, Some(true));

		let parse = |v: Value| serde_json::from_value::<Vec<InstalledPlugin>>(v).unwrap();
		let project_only = parse(json!([{"id": "statusline@ryanclark", "scope": "project"}]));
		assert!(find_plugin(&project_only).is_none());
		let other = parse(json!([{"id": "statusline@someone-else", "scope": "user"}]));
		assert!(find_plugin(&other).is_none());
	}

	#[test]
	fn result_line_reads_the_last_json_line_and_its_failure_code() {
		let r = result_line(&format!("noise\n{UNINSTALL_MISSING}\n\n")).unwrap();
		assert_eq!(r.failure_code.as_deref(), Some("not_installed"));
		let r = result_line(INSTALL).unwrap();
		assert_eq!(r.failure_code, None);
		assert!(r.message.unwrap().starts_with("Successfully installed"));
		assert!(result_line("Configuration saved. Restart Claude Code to apply it.\n").is_none());
	}

	#[test]
	fn version_is_the_leading_semver() {
		assert_eq!(
			parse_version("2.1.290 (Claude Code)\n"),
			Some(semver::Version::new(2, 1, 290))
		);
		assert!(parse_version("2.1.286 (Claude Code)").unwrap() < MIN_CLAUDE);
		assert_eq!(parse_version("Claude Code"), None);
	}

	#[test]
	fn configured_binary_reads_the_plugin_options_the_cli_wrote() {
		let settings: Value = serde_json::from_str(SETTINGS_AFTER_INSTALL).unwrap();
		assert_eq!(
			configured_binary(&settings),
			Some("/opt/homebrew/bin/statusline")
		);
		assert_eq!(configured_binary(&json!({})), None);
	}

	#[test]
	fn sparse_paths_are_stale_only_when_the_plugin_path_is_missing() {
		let old: Value = serde_json::from_str(SETTINGS_OLD_SPARSE).unwrap();
		assert_eq!(
			stale_sparse_paths(&old),
			Some(vec![".claude-plugin".to_owned(), "plugin".to_owned()])
		);
		let current = SETTINGS_OLD_SPARSE.replace(r#""plugin""#, r#""crates/statusline/plugin""#);
		assert_eq!(
			stale_sparse_paths(&serde_json::from_str(&current).unwrap()),
			None
		);
		let full: Value = serde_json::from_str(SETTINGS_AFTER_INSTALL).unwrap();
		assert_eq!(stale_sparse_paths(&full), None);
		assert_eq!(stale_sparse_paths(&json!({})), None);
	}

	fn temp_dir(name: &str) -> PathBuf {
		let dir =
			std::env::temp_dir().join(format!("statusline-plugin-{name}-{}", std::process::id()));
		let _ = fs::remove_dir_all(&dir);
		fs::create_dir_all(&dir).unwrap();
		dir.canonicalize().unwrap()
	}

	#[test]
	fn locate_binary_keeps_the_path_symlink_that_is_this_executable() {
		let dir = temp_dir("locate");
		let cellar = dir.join("Cellar/statusline/1.1.0/bin");
		let bin = dir.join("bin");
		let other = dir.join("other");
		for d in [&cellar, &bin, &other] {
			fs::create_dir_all(d).unwrap();
		}
		let exe = cellar.join("statusline");
		fs::write(&exe, "").unwrap();
		std::os::unix::fs::symlink(&exe, bin.join("statusline")).unwrap();
		fs::write(other.join("statusline"), "older build").unwrap();

		let path = std::env::join_paths([&other, &bin]).unwrap();
		assert_eq!(locate_binary(Some(&path), &exe), bin.join("statusline"));

		// A dev build that is not on PATH is stored as itself rather than as the older build PATH would find.
		let only_other = std::env::join_paths([&other]).unwrap();
		assert_eq!(locate_binary(Some(&only_other), &exe), exe);
		assert_eq!(locate_binary(None, &exe), exe);
		fs::remove_dir_all(&dir).unwrap();
	}

	/// A stand-in for `claude` that answers with the recorded outputs, keeps installed state in marker files and
	/// logs each call, so the flow runs against a real child process.
	struct Fake {
		root: PathBuf,
		home: PathBuf,
	}

	impl Fake {
		fn new(name: &str) -> Self {
			let root = temp_dir(name);
			let home = root.join("home");
			fs::create_dir_all(home.join(".claude")).unwrap();
			for (file, data) in [
				("version", "2.1.290 (Claude Code)\n"),
				("marketplaces.json", MARKETPLACE_LIST),
				("plugins.json", PLUGIN_LIST),
				("add.json", MARKETPLACE_ADD),
				("install.json", INSTALL),
				("uninstall.json", UNINSTALL),
				("uninstall-missing.json", UNINSTALL_MISSING),
				("remove.json", MARKETPLACE_REMOVE),
				("settings-after-install.json", SETTINGS_AFTER_INSTALL),
			] {
				fs::write(root.join(file), data).unwrap();
			}
			let script = root.join("claude");
			fs::write(
				&script,
				r#"#!/bin/sh
d=$(dirname "$0")
printf '%s\n' "$*" >> "$d/calls.log"
case "$*" in
--version) cat "$d/version" ;;
"plugin marketplace list --json") if [ -f "$d/has-marketplace" ]; then cat "$d/marketplaces.json"; else echo '[]'; fi ;;
"plugin marketplace add "*" crates/statusline/plugin "*)
  if [ -f "$d/add-fails" ]; then cat "$d/add-fails"; exit 1; fi
  touch "$d/has-marketplace"; cat "$d/add.json" ;;
"plugin marketplace add "*) touch "$d/has-marketplace"; cat "$d/add.json" ;;
"plugin marketplace remove "*) rm "$d/has-marketplace"; cat "$d/remove.json" ;;
"plugin list --json") if [ -f "$d/installed" ]; then cat "$d/plugins.json"; else echo '[]'; fi ;;
"plugin install "*)
  if [ -f "$d/install-fails" ]; then cat "$d/install-fails"; exit 1; fi
  touch "$d/installed"; cp "$d/settings-after-install.json" .claude/settings.json; cat "$d/install.json" ;;
"plugin configure "*) cat > "$d/configure.stdin"; echo 'Configuration saved. Restart Claude Code to apply it.' ;;
"plugin uninstall "*)
  if [ -f "$d/installed" ]; then rm "$d/installed"; cat "$d/uninstall.json"; else cat "$d/uninstall-missing.json"; exit 1; fi ;;
*) echo "unexpected: $*" >&2; exit 64 ;;
esac
"#,
			)
			.unwrap();
			fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
			Self { root, home }
		}

		fn ctx(&self, dry_run: bool) -> Context {
			Context {
				claude: self.root.join("claude"),
				home: self.home.clone(),
				interactive: false,
				dry_run,
			}
		}

		fn settings_path(&self) -> PathBuf {
			self.home.join(".claude/settings.json")
		}

		fn settings(&self) -> Value {
			serde_json::from_str(&fs::read_to_string(self.settings_path()).unwrap()).unwrap()
		}

		/// Takes the calls so far, so each step of a test sees only its own.
		fn calls(&self) -> Vec<String> {
			let log = self.root.join("calls.log");
			let calls = fs::read_to_string(&log).unwrap_or_default();
			let _ = fs::remove_file(&log);
			calls.lines().map(str::to_owned).collect()
		}

		fn backups(&self) -> usize {
			fs::read_dir(self.home.join(".statusline/backups")).map_or(0, Iterator::count)
		}

		fn plugin(&self, dry_run: bool, keep_native: bool) -> (Result<()>, String) {
			let mut out = Vec::new();
			let result = install_plugin(
				&self.ctx(dry_run),
				Path::new("/opt/homebrew/bin/statusline"),
				&PluginOptions {
					keep_native,
					subagent: false,
					five_hour_reset_threshold: None,
					seven_day_reset_threshold: None,
				},
				&mut out,
				&mut |q, _| panic!("a piped install must not ask: {q}"),
			);
			(result, String::from_utf8(out).unwrap())
		}

		fn native(&self, dry_run: bool, remove_marketplace: bool) -> (Result<()>, String) {
			let mut out = Vec::new();
			let result = install_native(
				&self.ctx(dry_run),
				remove_marketplace,
				&mut out,
				&mut |q, _| panic!("a piped install must not ask: {q}"),
			);
			(result, String::from_utf8(out).unwrap())
		}
	}

	impl Drop for Fake {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.root);
		}
	}

	const BINARY_CONFIG: &str = "binary=/opt/homebrew/bin/statusline";

	#[test]
	fn plugin_install_adds_installs_keeps_the_native_line_and_reruns_as_a_no_op() {
		let fake = Fake::new("fresh");
		fs::write(
			fake.settings_path(),
			r#"{"model": "opus", "statusLine": {"type": "command", "command": "statusline"}}"#,
		)
		.unwrap();
		// The fake install replaces the file the way the CLI rewrites it, so seed the copy it writes with the same
		// statusLine to check the flow reads the file after the children ran.
		let mut after: Value = serde_json::from_str(SETTINGS_AFTER_INSTALL).unwrap();
		after["statusLine"] = json!({"type": "command", "command": "statusline"});
		fs::write(
			fake.root.join("settings-after-install.json"),
			after.to_string(),
		)
		.unwrap();

		let (result, out) = fake.plugin(false, true);
		result.unwrap();
		assert_eq!(
			fake.calls(),
			vec![
				"--version".to_owned(),
				"plugin marketplace list --json".to_owned(),
				"plugin marketplace add ryanclark/statusline --scope user --sparse .claude-plugin crates/statusline/plugin --json"
					.to_owned(),
				"plugin list --json".to_owned(),
				format!("plugin install statusline@ryanclark -s user --config {BINARY_CONFIG} --json"),
			]
		);
		assert!(out.contains("Added marketplace ryanclark"), "{out}");
		assert!(
			out.contains("Kept native statusLine as the fallback"),
			"{out}"
		);
		assert_eq!(
			fake.settings(),
			after,
			"settings.json is the CLI's own write"
		);
		assert_eq!(fake.backups(), 0);
		assert!(fake.home.join(".statusline/settings.json").exists());
		assert_eq!(
			serde_json::from_str::<Value>(
				&fs::read_to_string(fake.home.join(".statusline/native-statusline.json")).unwrap()
			)
			.unwrap(),
			json!({"type": "command", "command": "statusline"})
		);

		let saved = fs::read_to_string(fake.settings_path()).unwrap();
		let (result, out) = fake.plugin(false, true);
		result.unwrap();
		assert_eq!(
			fake.calls(),
			[
				"--version",
				"plugin marketplace list --json",
				"plugin list --json"
			]
		);
		assert!(out.contains("already installed with binary="), "{out}");
		assert!(!out.contains("Saved the native"), "{out}");
		assert_eq!(fs::read_to_string(fake.settings_path()).unwrap(), saved);
		assert_eq!(fake.backups(), 0);
	}

	#[test]
	fn plugin_install_re_adds_a_marketplace_with_the_old_sparse_paths() {
		let fake = Fake::new("sparse");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		let mut settings: Value = serde_json::from_str(SETTINGS_AFTER_INSTALL).unwrap();
		settings["extraKnownMarketplaces"] = serde_json::from_str::<Value>(SETTINGS_OLD_SPARSE)
			.unwrap()["extraKnownMarketplaces"]
			.clone();
		fs::write(fake.settings_path(), settings.to_string()).unwrap();

		let (result, out) = fake.plugin(true, true);
		result.unwrap();
		let calls = fake.calls();
		assert_eq!(
			calls,
			[
				"--version",
				"plugin marketplace list --json",
				"plugin list --json"
			]
		);
		assert!(
			out.contains("would run")
				&& out.contains("plugin marketplace remove ryanclark --scope user --json"),
			"{out}"
		);
		assert!(
			out.contains("--sparse .claude-plugin crates/statusline/plugin --json"),
			"{out}"
		);

		let (result, out) = fake.plugin(false, true);
		result.unwrap();
		assert_eq!(
			fake.calls(),
			[
				"--version",
				"plugin marketplace list --json",
				"plugin marketplace remove ryanclark --scope user --json",
				"plugin marketplace add ryanclark/statusline --scope user --sparse .claude-plugin crates/statusline/plugin --json",
				"plugin list --json",
			]
		);
		assert!(out.contains("Re-added marketplace ryanclark"), "{out}");

		// The CLI rewrites the entry with the new paths, so a rerun leaves the marketplace alone.
		settings["extraKnownMarketplaces"]["ryanclark"]["source"]["sparsePaths"] =
			json!([".claude-plugin", "crates/statusline/plugin"]);
		fs::write(fake.settings_path(), settings.to_string()).unwrap();
		let (result, out) = fake.plugin(false, true);
		result.unwrap();
		assert_eq!(
			fake.calls(),
			[
				"--version",
				"plugin marketplace list --json",
				"plugin list --json"
			]
		);
		assert!(out.contains("Marketplace ryanclark already added"), "{out}");
	}

	#[test]
	fn plugin_install_restores_the_old_marketplace_when_the_re_add_fails() {
		let fake = Fake::new("sparse-fails");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		fs::write(
			fake.root.join("add-fails"),
			r#"{"command":"marketplace-add","outcome":"error","message":"Failed to clone ryanclark/statusline"}"#,
		)
		.unwrap();
		let mut settings: Value = serde_json::from_str(SETTINGS_AFTER_INSTALL).unwrap();
		settings["extraKnownMarketplaces"] = serde_json::from_str::<Value>(SETTINGS_OLD_SPARSE)
			.unwrap()["extraKnownMarketplaces"]
			.clone();
		fs::write(fake.settings_path(), settings.to_string()).unwrap();

		let (result, _) = fake.plugin(false, true);
		let err = format!("{:?}", result.unwrap_err());
		assert!(
			err.contains("Failed to clone ryanclark/statusline"),
			"{err}"
		);
		assert!(err.contains("statusline install --plugin"), "{err}");
		assert_eq!(
			fake.calls(),
			[
				"--version",
				"plugin marketplace list --json",
				"plugin marketplace remove ryanclark --scope user --json",
				"plugin marketplace add ryanclark/statusline --scope user --sparse .claude-plugin crates/statusline/plugin --json",
				"plugin marketplace add ryanclark/statusline --scope user --sparse .claude-plugin plugin --json",
			]
		);
		assert!(fake.root.join("has-marketplace").exists());
	}

	#[test]
	fn remove_native_backs_up_saves_the_line_and_native_puts_it_back() {
		let fake = Fake::new("remove");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		let custom = json!({"type": "command", "command": "statusline", "padding": 0});
		let original = json!({
			"$schema": "https://json.schemastore.org/claude-code-settings.json",
			"statusLine": {"type": "command", "command": "statusline"},
			"enabledPlugins": {"statusline@ryanclark": true},
			"pluginConfigs": {"statusline@ryanclark": {"options": {"binary": "/usr/local/bin/statusline"}}},
			"subagentStatusLine": {"type": "command", "command": "statusline subagent"}
		});
		fs::write(fake.settings_path(), original.to_string()).unwrap();

		let (result, out) = fake.plugin(false, false);
		result.unwrap();
		let calls = fake.calls();
		assert_eq!(
			calls.last().unwrap(),
			"plugin configure statusline@ryanclark --values-stdin"
		);
		assert_eq!(
			serde_json::from_str::<Value>(
				&fs::read_to_string(fake.root.join("configure.stdin")).unwrap()
			)
			.unwrap(),
			json!({"binary": "/opt/homebrew/bin/statusline"})
		);
		let settings = fake.settings();
		assert!(settings.get("statusLine").is_none(), "{out}");
		assert_eq!(
			settings["subagentStatusLine"]["command"],
			"statusline subagent"
		);
		assert!(out.contains("Removed native statusLine"), "{out}");
		assert!(out.contains("Kept subagent status line"), "{out}");
		assert_eq!(fake.backups(), 1);
		let backup = fs::read_dir(fake.home.join(".statusline/backups"))
			.unwrap()
			.next()
			.unwrap()
			.unwrap()
			.path();
		assert_eq!(
			serde_json::from_str::<Value>(&fs::read_to_string(backup).unwrap()).unwrap(),
			original
		);

		// A rerun finds no statusLine and must keep the saved one for --native.
		let (result, _) = fake.plugin(false, false);
		result.unwrap();
		fake.calls();
		assert_eq!(fake.backups(), 1);

		// --native restores the saved value, edited here so it differs from what was removed.
		fs::write(
			fake.home.join(".statusline/native-statusline.json"),
			custom.to_string(),
		)
		.unwrap();
		let (result, out) = fake.native(false, true);
		result.unwrap();
		assert_eq!(
			fake.calls(),
			[
				"plugin list --json",
				"plugin uninstall statusline@ryanclark -s user --json",
				"plugin marketplace list --json",
				"plugin marketplace remove ryanclark --scope user --json"
			]
		);
		assert_eq!(fake.settings()["statusLine"], custom, "{out}");
		assert!(out.contains("Restored native statusLine"), "{out}");
		assert_eq!(fake.backups(), 2);

		let (result, out) = fake.native(false, false);
		result.unwrap();
		assert_eq!(fake.calls(), ["plugin list --json"]);
		assert!(out.contains("Native statusLine already set"), "{out}");
		assert_eq!(fake.backups(), 2);
	}

	#[test]
	fn the_settings_write_keeps_what_the_install_child_wrote() {
		let fake = Fake::new("reread");
		fs::write(
			fake.settings_path(),
			r#"{"statusLine": {"type": "command", "command": "statusline"}}"#,
		)
		.unwrap();
		let mut after: Value = serde_json::from_str(SETTINGS_AFTER_INSTALL).unwrap();
		after["statusLine"] = json!({"type": "command", "command": "statusline"});
		fs::write(
			fake.root.join("settings-after-install.json"),
			after.to_string(),
		)
		.unwrap();

		let (result, out) = fake.plugin(false, false);
		result.unwrap();
		let mut expected: Value = serde_json::from_str(SETTINGS_AFTER_INSTALL).unwrap();
		expected.as_object_mut().unwrap().remove("statusLine");
		assert_eq!(fake.settings(), expected, "{out}");
	}

	#[test]
	fn a_saved_foreign_line_is_not_replaced_by_ours() {
		let fake = Fake::new("keepsaved");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		let theirs = json!({"type": "command", "command": "~/bin/my-line.sh"});
		fs::create_dir_all(fake.home.join(".statusline")).unwrap();
		let saved = fake.home.join(".statusline/native-statusline.json");
		fs::write(&saved, theirs.to_string()).unwrap();
		// A plain `statusline install` after `--plugin` wrote ours over the line that was saved.
		fs::write(
			fake.settings_path(),
			json!({"statusLine": {"type": "command", "command": "statusline"}, "subagentStatusLine": {"type": "command", "command": "x"}})
				.to_string(),
		)
		.unwrap();

		let (result, out) = fake.plugin(false, true);
		result.unwrap();
		assert_eq!(
			serde_json::from_str::<Value>(&fs::read_to_string(&saved).unwrap()).unwrap(),
			theirs,
			"{out}"
		);
	}

	#[test]
	fn a_foreign_status_line_survives_a_piped_remove_native() {
		let fake = Fake::new("foreign");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		let theirs = json!({"type": "command", "command": "~/bin/my-line.sh"});
		fs::write(
			fake.settings_path(),
			json!({"statusLine": theirs, "subagentStatusLine": {"type": "command", "command": "x"}}).to_string(),
		)
		.unwrap();

		let (result, out) = fake.plugin(false, false);
		result.unwrap();
		assert_eq!(fake.settings()["statusLine"], theirs);
		assert!(out.contains("draws alongside the plugin"), "{out}");
		assert_eq!(fake.backups(), 0);
	}

	#[test]
	fn remove_native_keeps_the_line_while_the_plugin_is_disabled() {
		let fake = Fake::new("disabled");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		fs::write(
			fake.root.join("plugins.json"),
			PLUGIN_LIST.replace(r#""enabled": true"#, r#""enabled": false"#),
		)
		.unwrap();
		let ours = json!({"type": "command", "command": "statusline"});
		fs::write(
			fake.settings_path(),
			json!({"statusLine": ours, "subagentStatusLine": {"type": "command", "command": "statusline subagent"}})
				.to_string(),
		)
		.unwrap();

		let (result, out) = fake.plugin(false, false);
		result.unwrap();
		assert_eq!(fake.settings()["statusLine"], ours, "{out}");
		assert!(out.contains("is installed but disabled"), "{out}");
		assert!(out.contains("Kept the native statusLine"), "{out}");
		assert_eq!(fake.backups(), 0);
	}

	#[test]
	fn our_binary_at_any_path_counts_as_the_silent_fallback() {
		let fake = Fake::new("ourpath");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		let ours =
			json!({"type": "command", "command": "/opt/homebrew/bin/statusline", "padding": 0});
		fs::write(
			fake.settings_path(),
			json!({"statusLine": ours, "subagentStatusLine": {"type": "command", "command": "x"}})
				.to_string(),
		)
		.unwrap();

		let (result, out) = fake.plugin(false, true);
		result.unwrap();
		assert!(
			out.contains("Kept native statusLine as the fallback"),
			"{out}"
		);
		assert!(!out.contains("draws alongside"), "{out}");

		let (result, out) = fake.plugin(false, false);
		result.unwrap();
		assert!(fake.settings().get("statusLine").is_none(), "{out}");
		assert!(out.contains("Removed native statusLine"), "{out}");
	}

	#[test]
	fn native_keeps_the_plugin_when_the_restore_fails() {
		let fake = Fake::new("restorefails");
		fs::write(fake.root.join("installed"), "").unwrap();
		fs::write(fake.root.join("has-marketplace"), "").unwrap();
		fs::write(fake.settings_path(), r#"{"model": "opus"}"#).unwrap();
		fs::create_dir_all(fake.home.join(".statusline")).unwrap();
		fs::write(
			fake.home.join(".statusline/native-statusline.json"),
			"{not json",
		)
		.unwrap();

		let (result, out) = fake.native(false, true);
		assert!(result.is_err(), "{out}");
		let calls = fake.calls();
		assert!(
			!calls
				.iter()
				.any(|c| c.starts_with("plugin uninstall")
					|| c.starts_with("plugin marketplace remove")),
			"{calls:?}"
		);
		assert!(fake.root.join("installed").exists());
		assert_eq!(fake.settings(), json!({"model": "opus"}));
	}

	#[test]
	fn a_failed_install_leaves_settings_untouched() {
		let fake = Fake::new("fails");
		fs::write(fake.root.join("install-fails"), INSTALL_NOT_FOUND).unwrap();
		let before = r#"{"statusLine": {"type": "command", "command": "statusline"}}"#;
		fs::write(fake.settings_path(), before).unwrap();

		let (result, _) = fake.plugin(false, false);
		let err = format!("{:#}", result.unwrap_err());
		assert!(err.contains("not found in marketplace"), "{err}");
		assert_eq!(fs::read_to_string(fake.settings_path()).unwrap(), before);
		assert!(
			!fake
				.home
				.join(".statusline/native-statusline.json")
				.exists()
		);
	}

	#[test]
	fn an_old_claude_is_refused_before_anything_runs() {
		let fake = Fake::new("old");
		fs::write(fake.root.join("version"), "2.1.286 (Claude Code)\n").unwrap();
		let (result, _) = fake.plugin(false, true);
		assert!(format!("{:#}", result.unwrap_err()).contains("2.1.287"));
		assert_eq!(fake.calls(), ["--version"]);
		assert!(!fake.home.join(".statusline").exists());
	}

	#[test]
	fn dry_run_only_queries_and_writes_nothing() {
		let fake = Fake::new("dry");
		fs::write(
			fake.settings_path(),
			r#"{"statusLine": {"type": "command", "command": "statusline"}}"#,
		)
		.unwrap();

		let (result, out) = fake.plugin(true, false);
		result.unwrap();
		assert_eq!(
			fake.calls(),
			[
				"--version",
				"plugin marketplace list --json",
				"plugin list --json"
			]
		);
		assert!(out.contains("would run"), "{out}");
		assert!(out.contains("would remove the native statusLine"), "{out}");
		assert!(!fake.home.join(".statusline").exists());

		let (result, out) = fake.native(true, false);
		result.unwrap();
		assert_eq!(fake.calls(), ["plugin list --json"]);
		assert!(out.contains("not installed"), "{out}");
		assert!(!fake.home.join(".statusline").exists());
	}

	#[test]
	fn native_restores_the_default_and_tolerates_a_missing_claude() {
		let fake = Fake::new("nativeonly");
		let mut out = Vec::new();
		let ctx = Context {
			claude: fake.root.join("no-such-claude"),
			..fake.ctx(false)
		};
		let result = install_native(&ctx, true, &mut out, &mut |_, _| panic!());
		let out = String::from_utf8(out).unwrap();
		assert!(
			result.is_err(),
			"a failed plugin step still fails the command"
		);
		assert!(out.contains("Skipped the plugin uninstall"), "{out}");
		assert!(out.contains("Skipped the marketplace removal"), "{out}");
		assert_eq!(
			fake.settings()["statusLine"],
			json!({"type": "command", "command": "statusline"})
		);
		assert_eq!(fake.backups(), 0, "there was no settings file to back up");
	}
}
