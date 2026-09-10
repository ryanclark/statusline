use crate::format::Percentage;
use crate::settings::Settings;
use crate::util::home_dir;
use eyre::{Context, Result, bail};
use owo_colors::OwoColorize;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;

/// The Claude Code settings.json entries this tool can own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
	StatusLine,
	Subagent,
}

impl Entry {
	fn key(self) -> &'static str {
		match self {
			Self::StatusLine => "statusLine",
			Self::Subagent => "subagentStatusLine",
		}
	}

	fn label(self) -> &'static str {
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
pub(crate) enum Outcome {
	Written,
	Kept,
	Skipped,
}

pub(crate) fn install(
	five_hour_reset_threshold: Percentage,
	seven_day_reset_threshold: Percentage,
	subagent: bool,
) -> Result<()> {
	Settings::ensure(five_hour_reset_threshold, seven_day_reset_threshold)?;

	println!("{} Saved settings", "✓".green());

	let path = home_dir()?.join(".claude").join("settings.json");
	let mut settings = read_settings(&path)?;
	let interactive = std::io::stdin().is_terminal();

	let mut entries = vec![Entry::StatusLine];
	let subagent_configured = settings.get(Entry::Subagent.key()).is_some();
	if subagent
		|| (!subagent_configured
			&& offer_subagent(interactive, || {
				prompt_yes_no("Also install the subagent status line?", true)
			})) {
		entries.push(Entry::Subagent);
	}

	let results = ensure_entries(&mut settings, &entries, &mut |entry| {
		interactive
			&& prompt_yes_no(
				&format!("{} already configured. Overwrite?", entry.label()),
				false,
			)
	})?;

	if results
		.iter()
		.any(|(_, outcome)| *outcome == Outcome::Written)
	{
		write_settings(&path, &settings)?;
		println!("{} Updated {}", "✓".green(), path.display());
	}
	for (entry, outcome) in results {
		match outcome {
			Outcome::Written => println!("{} Configured the {}", "✓".green(), entry.label()),
			Outcome::Kept => println!(
				"{} The {} was already configured",
				"✓".green(),
				entry.label()
			),
			Outcome::Skipped => println!("{} Left the {} as it was", "–".dimmed(), entry.label()),
		}
	}

	println!("{}", "Installation complete".green().bold());

	Ok(())
}

/// Applies each entry in turn. A value that already matches is kept without asking, and a different
/// one is only replaced when `confirm_overwrite` agrees, so declining one entry never blocks the next.
pub(crate) fn ensure_entries(
	settings: &mut serde_json::Value,
	entries: &[Entry],
	confirm_overwrite: &mut dyn FnMut(Entry) -> bool,
) -> Result<Vec<(Entry, Outcome)>> {
	let Some(object) = settings.as_object_mut() else {
		bail!("Claude Code settings.json is not a JSON object");
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

/// A piped or scripted install must never wait on a question nobody will answer.
pub(crate) fn offer_subagent(interactive: bool, ask: impl FnOnce() -> bool) -> bool {
	interactive && ask()
}

fn prompt_yes_no(question: &str, default_yes: bool) -> bool {
	let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
	eprint!("{} {question} {hint} ", "?".yellow().bold());

	let mut answer = String::new();
	if std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut answer).is_err() {
		return false;
	}
	match answer.trim() {
		"" => default_yes,
		a => a.eq_ignore_ascii_case("y") || a.eq_ignore_ascii_case("yes"),
	}
}

fn read_settings(path: &Path) -> Result<serde_json::Value> {
	match fs::read_to_string(path) {
		Ok(data) => serde_json::from_str(&data).context("failed to parse settings.json"),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
		Err(e) => Err(e).context("reading Claude Code settings.json"),
	}
}

/// Claude Code writes the same file, so the replacement lands in one rename rather than a partial
/// write it could read half-way through.
pub(crate) fn write_settings(path: &Path, settings: &serde_json::Value) -> Result<()> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let data = serde_json::to_string_pretty(settings)?;
	let tmp = path.with_extension("json.tmp");
	fs::write(&tmp, data)?;
	fs::rename(&tmp, path)?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

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
	fn subagent_offer_is_silent_without_a_terminal() {
		assert!(!offer_subagent(false, || panic!("must not prompt")));
		assert!(offer_subagent(true, || true));
		assert!(!offer_subagent(true, || false));
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
}
