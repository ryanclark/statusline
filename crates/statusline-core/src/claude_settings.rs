//! The entries statusline owns in Claude Code's `~/.claude/settings.json`, shared by the installer
//! and the configure editor so both merge the same way.

use std::fs;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ClaudeSettingsError {
	#[error("reading Claude Code settings.json: {0}")]
	Io(#[from] std::io::Error),
	#[error("Claude Code settings.json is not valid JSON: {0}")]
	Json(#[from] serde_json::Error),
	#[error("Claude Code settings.json is not a JSON object")]
	NotAnObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
	StatusLine,
	Subagent,
}

impl Entry {
	#[must_use]
	pub fn key(self) -> &'static str {
		match self {
			Self::StatusLine => "statusLine",
			Self::Subagent => "subagentStatusLine",
		}
	}

	#[must_use]
	pub fn label(self) -> &'static str {
		match self {
			Self::StatusLine => "status line",
			Self::Subagent => "subagent status line",
		}
	}

	#[must_use]
	pub fn desired(self) -> serde_json::Value {
		let command = match self {
			Self::StatusLine => "statusline",
			Self::Subagent => "statusline subagent",
		};
		serde_json::json!({"type": "command", "command": command})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
	Written,
	Kept,
	Skipped,
}

/// Applies each entry in turn. A value that already matches is kept without asking, and a different
/// one is only replaced when `confirm_overwrite` agrees, so declining one entry never blocks the next.
pub fn ensure_entries(
	settings: &mut serde_json::Value,
	entries: &[Entry],
	confirm_overwrite: &mut dyn FnMut(Entry) -> bool,
) -> Result<Vec<(Entry, Outcome)>, ClaudeSettingsError> {
	let Some(object) = settings.as_object_mut() else {
		return Err(ClaudeSettingsError::NotAnObject);
	};

	Ok(entries
		.iter()
		.map(|&entry| {
			let desired = entry.desired();
			let outcome = match object.get(entry.key()) {
				Some(existing) if *existing == desired => Outcome::Kept,
				Some(_) if !confirm_overwrite(entry) => Outcome::Skipped,
				_ => {
					object.insert(entry.key().to_owned(), desired);
					Outcome::Written
				}
			};
			(entry, outcome)
		})
		.collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
	Removed,
	Absent,
	Declined,
}

/// Removes `entry` when it runs statusline (see [`is_ours`]), and any other value only when `confirm_foreign` agrees.
pub fn remove_entry(
	settings: &mut serde_json::Value,
	entry: Entry,
	confirm_foreign: &mut dyn FnMut(Entry, &serde_json::Value) -> bool,
) -> Result<Removal, ClaudeSettingsError> {
	let Some(object) = settings.as_object_mut() else {
		return Err(ClaudeSettingsError::NotAnObject);
	};

	Ok(match object.get(entry.key()) {
		None => Removal::Absent,
		Some(existing) if !is_ours(entry, existing) && !confirm_foreign(entry, existing) => {
			Removal::Declined
		}
		// `remove` is a swap remove under preserve_order and would move the user's last key into this slot.
		Some(_) => {
			object.shift_remove(entry.key());
			Removal::Removed
		}
	})
}

/// Puts back a value [`remove_entry`] took out. A different existing value is replaced only when `confirm_overwrite`
/// agrees.
pub fn restore_entry(
	settings: &mut serde_json::Value,
	entry: Entry,
	value: &serde_json::Value,
	confirm_overwrite: &mut dyn FnMut(Entry) -> bool,
) -> Result<Outcome, ClaudeSettingsError> {
	let Some(object) = settings.as_object_mut() else {
		return Err(ClaudeSettingsError::NotAnObject);
	};

	Ok(match object.get(entry.key()) {
		Some(existing) if existing == value => Outcome::Kept,
		Some(_) if !confirm_overwrite(entry) => Outcome::Skipped,
		_ => {
			object.insert(entry.key().to_owned(), value.clone());
			Outcome::Written
		}
	})
}

/// Whether `value` runs the statusline binary, at any path or with any flags. A wrapper script is not ours, since it
/// may draw anything.
#[must_use]
pub fn is_ours(entry: Entry, value: &serde_json::Value) -> bool {
	if value.get("type").and_then(serde_json::Value::as_str) != Some("command") {
		return false;
	}
	let Some(command) = value.get("command").and_then(serde_json::Value::as_str) else {
		return false;
	};
	let (program, rest) = first_word(command.trim_start());
	let ours = Path::new(program)
		.file_name()
		.is_some_and(|name| name == "statusline");
	match entry {
		Entry::StatusLine => ours,
		Entry::Subagent => ours && first_word(rest.trim_start()).0 == "subagent",
	}
}

/// Splits off one shell word, allowing a quoted path with spaces. Anything fancier is never ours anyway.
fn first_word(command: &str) -> (&str, &str) {
	if let Some(quote) = command.chars().next().filter(|c| *c == '\'' || *c == '"')
		&& let Some(end) = command[1..].find(quote)
	{
		return (&command[1..=end], &command[end + 2..]);
	}
	command
		.split_once(char::is_whitespace)
		.unwrap_or((command, ""))
}

/// A change to one entry, decided against one read and replayed onto a fresh one, since a prompt can wait for minutes
/// while Claude Code rewrites the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
	pub entry: Entry,
	/// What the entry held when the change was decided.
	pub seen: Option<serde_json::Value>,
	/// The entry's new value, or `None` to remove it.
	pub value: Option<serde_json::Value>,
}

impl Edit {
	#[must_use]
	pub fn between(
		before: &serde_json::Value,
		after: &serde_json::Value,
		entries: &[Entry],
	) -> Vec<Self> {
		entries
			.iter()
			.filter(|entry| before.get(entry.key()) != after.get(entry.key()))
			.map(|&entry| Self {
				entry,
				seen: before.get(entry.key()).cloned(),
				value: after.get(entry.key()).cloned(),
			})
			.collect()
	}

	/// Applies the edit only if the entry still holds `seen`, and returns whether it did.
	pub fn apply(&self, settings: &mut serde_json::Value) -> Result<bool, ClaudeSettingsError> {
		let Some(object) = settings.as_object_mut() else {
			return Err(ClaudeSettingsError::NotAnObject);
		};
		if object.get(self.entry.key()) != self.seen.as_ref() {
			return Ok(false);
		}
		match &self.value {
			Some(value) => {
				object.insert(self.entry.key().to_owned(), value.clone());
			}
			None => {
				object.shift_remove(self.entry.key());
			}
		}
		Ok(true)
	}
}

/// Applies `edits` to a fresh read of the settings and returns the entries whose edit was skipped.
pub fn replay(
	settings: &mut serde_json::Value,
	edits: &[Edit],
) -> Result<Vec<Entry>, ClaudeSettingsError> {
	let mut stale = Vec::new();
	for edit in edits {
		if !edit.apply(settings)? {
			stale.push(edit.entry);
		}
	}

	Ok(stale)
}

pub fn read_settings(path: &Path) -> Result<serde_json::Value, ClaudeSettingsError> {
	match fs::read_to_string(path) {
		Ok(data) => Ok(serde_json::from_str(&data)?),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
		Err(e) => Err(e.into()),
	}
}

/// Claude Code writes the same file, so the replacement lands in one rename rather than a partial
/// write it could read half-way through. A settings.json linked in from a dotfiles repo is written at the link's
/// target, since renaming over the link would turn it into a plain file the repo no longer sees.
pub fn write_settings(
	path: &Path,
	settings: &serde_json::Value,
) -> Result<(), ClaudeSettingsError> {
	let target = resolve_links(path);
	if let Some(parent) = target.parent() {
		fs::create_dir_all(parent)?;
	}
	let data = serde_json::to_string_pretty(settings)?;
	let mut tmp_name = target.file_name().unwrap_or_default().to_owned();
	tmp_name.push(".tmp");
	// Next to the target rather than the link, so the rename never crosses filesystems.
	let tmp = target.with_file_name(tmp_name);
	fs::write(&tmp, data)?;
	if let Err(err) = replace(&tmp, &target) {
		let _ = fs::remove_file(&tmp);
		return Err(err.into());
	}

	Ok(())
}

fn replace(tmp: &Path, target: &Path) -> std::io::Result<()> {
	// The file can hold tokens in env, so a mode the user tightened must not fall back to the umask default.
	if let Ok(meta) = fs::metadata(target) {
		fs::set_permissions(tmp, meta.permissions())?;
	}
	fs::rename(tmp, target)
}

/// Bounded so a link loop ends instead of spinning.
const MAX_LINK_HOPS: usize = 40;

/// Follows the link chain by hand instead of canonicalizing, so a link whose target does not exist yet still
/// resolves to the file it should create.
fn resolve_links(path: &Path) -> std::path::PathBuf {
	let mut current = path.to_path_buf();
	for _ in 0..MAX_LINK_HOPS {
		let Ok(next) = fs::read_link(&current) else {
			break;
		};
		current = match current.parent() {
			Some(parent) if next.is_relative() => parent.join(next),
			_ => next,
		};
	}
	current
}

/// Whether settings.json carries any value for `entry`, ours or not.
#[must_use]
pub fn is_configured(path: &Path, entry: Entry) -> bool {
	read_settings(path).is_ok_and(|s| s.get(entry.key()).is_some())
}

/// Installs `entry` without prompting: an existing different value is left alone and reported as
/// skipped, since only an interactive install may replace it.
pub fn install_entry(path: &Path, entry: Entry) -> Result<Outcome, ClaudeSettingsError> {
	let mut settings = read_settings(path)?;
	let results = ensure_entries(&mut settings, &[entry], &mut |_| false)?;
	let outcome = results.first().map_or(Outcome::Skipped, |(_, o)| *o);
	if outcome == Outcome::Written {
		write_settings(path, &settings)?;
	}

	Ok(outcome)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	use std::fs;

	fn never(_: Entry) -> bool {
		panic!("no overwrite prompt expected")
	}

	#[test]
	fn ensure_entries_adds_both_keys_and_keeps_everything_else() {
		let mut settings = json!({"$schema": "https://json.schemastore.org/claude-code-settings.json", "permissions": {"allow": ["Bash(ls:*)"]}});
		let results = ensure_entries(
			&mut settings,
			&[Entry::StatusLine, Entry::Subagent],
			&mut never,
		)
		.unwrap();
		assert_eq!(
			results,
			vec![
				(Entry::StatusLine, Outcome::Written),
				(Entry::Subagent, Outcome::Written)
			]
		);
		assert_eq!(
			settings["statusLine"],
			json!({"type": "command", "command": "statusline"})
		);
		assert_eq!(
			settings["subagentStatusLine"],
			json!({"type": "command", "command": "statusline subagent"})
		);
		assert_eq!(settings["permissions"]["allow"][0], "Bash(ls:*)");
		assert!(settings["$schema"].is_string());
	}

	#[test]
	fn ensure_entries_keeps_an_identical_entry_without_asking() {
		let mut settings = json!({"statusLine": {"type": "command", "command": "statusline"}});
		let results = ensure_entries(&mut settings, &[Entry::StatusLine], &mut never).unwrap();
		assert_eq!(results, vec![(Entry::StatusLine, Outcome::Kept)]);
	}

	#[test]
	fn a_declined_overwrite_skips_that_key_but_still_writes_the_next() {
		let mut settings = json!({"statusLine": {"type": "command", "command": "my-own-script"}});
		let mut asked = Vec::new();
		let results = ensure_entries(
			&mut settings,
			&[Entry::StatusLine, Entry::Subagent],
			&mut |entry| {
				asked.push(entry);
				false
			},
		)
		.unwrap();
		assert_eq!(asked, vec![Entry::StatusLine]);
		assert_eq!(
			results,
			vec![
				(Entry::StatusLine, Outcome::Skipped),
				(Entry::Subagent, Outcome::Written)
			]
		);
		assert_eq!(settings["statusLine"]["command"], "my-own-script");
		assert_eq!(
			settings["subagentStatusLine"]["command"],
			"statusline subagent"
		);
	}

	#[test]
	fn a_confirmed_overwrite_replaces_the_entry() {
		let mut settings = json!({"subagentStatusLine": {"type": "command", "command": "other"}});
		let results = ensure_entries(&mut settings, &[Entry::Subagent], &mut |_| true).unwrap();
		assert_eq!(results, vec![(Entry::Subagent, Outcome::Written)]);
		assert_eq!(
			settings["subagentStatusLine"]["command"],
			"statusline subagent"
		);
	}

	#[test]
	fn settings_that_are_not_an_object_are_refused() {
		let mut settings = json!(["not", "an", "object"]);
		assert!(ensure_entries(&mut settings, &[Entry::StatusLine], &mut never).is_err());
	}

	#[test]
	fn write_settings_creates_parents_and_leaves_no_temp_file() {
		let dir = std::env::temp_dir().join(format!("statusline-install-{}", std::process::id()));
		let path = dir.join(".claude").join("settings.json");
		write_settings(
			&path,
			&json!({"statusLine": {"type": "command", "command": "statusline"}}),
		)
		.unwrap();
		let written: serde_json::Value =
			serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
		assert_eq!(written["statusLine"]["command"], "statusline");
		assert_eq!(
			fs::read_dir(path.parent().unwrap()).unwrap().count(),
			1,
			"only settings.json"
		);
		fs::remove_dir_all(&dir).unwrap();
	}
	#[test]
	fn is_configured_and_install_entry_work_on_disk() {
		let dir = std::env::temp_dir().join(format!("statusline-claude-{}", std::process::id()));
		let path = dir.join("settings.json");
		assert!(!is_configured(&path, Entry::Subagent));
		assert_eq!(
			install_entry(&path, Entry::Subagent).unwrap(),
			Outcome::Written
		);
		assert!(is_configured(&path, Entry::Subagent));
		assert_eq!(
			install_entry(&path, Entry::Subagent).unwrap(),
			Outcome::Kept
		);

		// A different existing command is never replaced without a person confirming.
		fs::write(
			&path,
			r#"{"subagentStatusLine": {"type": "command", "command": "other"}}"#,
		)
		.unwrap();
		assert_eq!(
			install_entry(&path, Entry::Subagent).unwrap(),
			Outcome::Skipped
		);
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn remove_entry_takes_out_only_our_own_command_without_asking() {
		let mut settings = json!({
			"$schema": "https://json.schemastore.org/claude-code-settings.json",
			"statusLine": {"type": "command", "command": "statusline"},
			"subagentStatusLine": {"type": "command", "command": "statusline subagent"},
			"enabledPlugins": {"statusline@ryanclark": true}
		});
		let removed = remove_entry(&mut settings, Entry::StatusLine, &mut |_, _| {
			panic!("our own command needs no confirmation")
		})
		.unwrap();
		assert_eq!(removed, Removal::Removed);
		assert!(settings.get("statusLine").is_none());
		assert_eq!(
			settings["subagentStatusLine"]["command"],
			"statusline subagent"
		);
		assert_eq!(settings["enabledPlugins"]["statusline@ryanclark"], true);

		let keys: Vec<&str> = settings
			.as_object()
			.unwrap()
			.keys()
			.map(String::as_str)
			.collect();
		assert_eq!(keys, ["$schema", "subagentStatusLine", "enabledPlugins"]);

		let again = remove_entry(&mut settings, Entry::StatusLine, &mut |_, _| panic!()).unwrap();
		assert_eq!(again, Removal::Absent);
	}

	#[test]
	fn remove_entry_asks_before_taking_out_a_foreign_command() {
		let foreign = json!({"type": "command", "command": "~/bin/my-line.sh", "padding": 0});
		let mut settings = json!({"statusLine": foreign.clone()});

		let mut seen = Vec::new();
		let declined = remove_entry(&mut settings, Entry::StatusLine, &mut |entry, value| {
			seen.push((entry, value.clone()));
			false
		})
		.unwrap();
		assert_eq!(declined, Removal::Declined);
		assert_eq!(seen, vec![(Entry::StatusLine, foreign.clone())]);
		assert_eq!(settings["statusLine"], foreign);

		let removed = remove_entry(&mut settings, Entry::StatusLine, &mut |_, _| true).unwrap();
		assert_eq!(removed, Removal::Removed);
		assert_eq!(settings, json!({}));
	}

	#[test]
	fn remove_entry_takes_out_our_program_at_any_path_or_with_extra_fields() {
		for ours in [
			json!({"type": "command", "command": "/opt/homebrew/bin/statusline"}),
			json!({"type": "command", "command": "statusline", "padding": 0}),
			json!({"type": "command", "command": "~/.cargo/bin/statusline -f 80"}),
			json!({"type": "command", "command": "'/Users/me/My Tools/statusline' --x"}),
		] {
			let mut settings = json!({"statusLine": ours});
			let removed = remove_entry(&mut settings, Entry::StatusLine, &mut |_, value| {
				panic!("{value} is ours and needs no confirmation")
			})
			.unwrap();
			assert_eq!(removed, Removal::Removed);
			assert_eq!(settings, json!({}));
		}

		for theirs in [
			json!({"type": "command", "command": "statusline-wrapper"}),
			json!({"type": "command", "command": "sh -c statusline"}),
			json!({"type": "static", "command": "statusline"}),
		] {
			let mut settings = json!({"statusLine": theirs.clone()});
			let declined =
				remove_entry(&mut settings, Entry::StatusLine, &mut |_, _| false).unwrap();
			assert_eq!(declined, Removal::Declined, "{theirs}");
		}
	}

	#[test]
	fn edits_replay_onto_a_fresh_read_but_skip_an_entry_that_changed() {
		let before = json!({"statusLine": {"type": "command", "command": "mine"}});
		let after =
			json!({"subagentStatusLine": {"type": "command", "command": "statusline subagent"}});
		let edits = Edit::between(&before, &after, &[Entry::StatusLine, Entry::Subagent]);
		assert_eq!(edits.len(), 2);

		let mut fresh =
			json!({"model": "sonnet", "statusLine": {"type": "command", "command": "mine"}});
		assert!(replay(&mut fresh, &edits).unwrap().is_empty());
		assert_eq!(
			fresh,
			json!({"model": "sonnet", "subagentStatusLine": {"type": "command", "command": "statusline subagent"}})
		);

		let mut changed = json!({"statusLine": {"type": "command", "command": "newer"}});
		assert_eq!(replay(&mut changed, &edits).unwrap(), [Entry::StatusLine]);
		assert_eq!(changed["statusLine"]["command"], "newer");
	}

	#[cfg(unix)]
	#[test]
	fn write_settings_through_a_symlink_keeps_the_link_and_the_target_mode() {
		use std::os::unix::fs::PermissionsExt;

		let dir = std::env::temp_dir().join(format!("statusline-symlink-{}", std::process::id()));
		let _ = fs::remove_dir_all(&dir);
		let dotfiles = dir.join("dotfiles");
		let claude = dir.join(".claude");
		fs::create_dir_all(&dotfiles).unwrap();
		fs::create_dir_all(&claude).unwrap();
		let target = dotfiles.join("claude-settings.json");
		fs::write(&target, r#"{"model": "opus"}"#).unwrap();
		fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
		let link = claude.join("settings.json");
		std::os::unix::fs::symlink("../dotfiles/claude-settings.json", &link).unwrap();

		write_settings(&link, &json!({"model": "sonnet"})).unwrap();

		assert!(
			fs::symlink_metadata(&link)
				.unwrap()
				.file_type()
				.is_symlink(),
			"the link must survive the write"
		);
		let written: serde_json::Value =
			serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
		assert_eq!(written["model"], "sonnet");
		assert_eq!(
			fs::metadata(&target).unwrap().permissions().mode() & 0o777,
			0o600
		);
		assert_eq!(fs::read_dir(&claude).unwrap().count(), 1, "only the link");
		assert_eq!(
			fs::read_dir(&dotfiles).unwrap().count(),
			1,
			"no temp file left"
		);
		fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn restore_entry_puts_a_saved_value_back_and_guards_a_different_one() {
		let saved = json!({"type": "command", "command": "statusline", "padding": 1});
		let mut settings = json!({"model": "opus"});
		assert_eq!(
			restore_entry(&mut settings, Entry::StatusLine, &saved, &mut never).unwrap(),
			Outcome::Written
		);
		assert_eq!(settings["statusLine"], saved);
		assert_eq!(
			restore_entry(&mut settings, Entry::StatusLine, &saved, &mut never).unwrap(),
			Outcome::Kept
		);

		settings["statusLine"] = json!({"type": "command", "command": "other"});
		assert_eq!(
			restore_entry(&mut settings, Entry::StatusLine, &saved, &mut |_| false).unwrap(),
			Outcome::Skipped
		);
		assert_eq!(settings["statusLine"]["command"], "other");
	}

	#[test]
	fn install_entry_keeps_the_existing_key_order() {
		let dir = std::env::temp_dir().join(format!("statusline-order-{}", std::process::id()));
		let path = dir.join("settings.json");
		fs::create_dir_all(&dir).unwrap();
		fs::write(
			&path,
			r#"{"zeta": 1, "$schema": "https://json.schemastore.org/claude-code-settings.json", "alpha": {"y": 1, "b": 2}}"#,
		)
		.unwrap();

		assert_eq!(
			install_entry(&path, Entry::StatusLine).unwrap(),
			Outcome::Written
		);
		let written = fs::read_to_string(&path).unwrap();
		let position = |key: &str| {
			written
				.find(&format!("\"{key}\""))
				.unwrap_or_else(|| panic!("{key} missing from {written}"))
		};
		assert!(position("zeta") < position("$schema"), "{written}");
		assert!(position("$schema") < position("alpha"), "{written}");
		assert!(position("y") < position("b"), "{written}");
		assert!(position("alpha") < position("statusLine"), "{written}");
		fs::remove_dir_all(&dir).unwrap();
	}
}
