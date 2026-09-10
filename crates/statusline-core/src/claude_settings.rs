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

	fn desired(self) -> serde_json::Value {
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

pub fn read_settings(path: &Path) -> Result<serde_json::Value, ClaudeSettingsError> {
	match fs::read_to_string(path) {
		Ok(data) => Ok(serde_json::from_str(&data)?),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
		Err(e) => Err(e.into()),
	}
}

/// Claude Code writes the same file, so the replacement lands in one rename rather than a partial
/// write it could read half-way through.
pub fn write_settings(
	path: &Path,
	settings: &serde_json::Value,
) -> Result<(), ClaudeSettingsError> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let data = serde_json::to_string_pretty(settings)?;
	let tmp = path.with_extension("json.tmp");
	fs::write(&tmp, data)?;
	fs::rename(&tmp, path)?;

	Ok(())
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
}
