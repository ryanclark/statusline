use crate::format::Percentage;
use crate::settings::Settings;
use crate::util::home_dir;
use eyre::Result;
use owo_colors::OwoColorize;
use serde_json::Value;
use statusline_core::claude_settings::{
	Edit, Entry, Outcome, ensure_entries, read_settings, replay, write_settings,
};
use std::io::{IsTerminal, Write};
use std::path::Path;

pub(crate) fn install(
	five_hour_reset_threshold: Option<Percentage>,
	seven_day_reset_threshold: Option<Percentage>,
	subagent: bool,
) -> Result<()> {
	let mut out = std::io::stdout();
	let settings_path = Settings::settings_path()?;
	let existed = settings_path.exists();
	Settings::ensure_at(
		&settings_path,
		five_hour_reset_threshold,
		seven_day_reset_threshold,
	)?;

	if existed {
		ok(&mut out, format_args!("Kept {}", settings_path.display()))?;
	} else {
		ok(&mut out, "Saved default settings")?;
	}

	let path = home_dir()?.join(".claude").join("settings.json");
	let results = wire_claude_settings(
		&path,
		subagent,
		std::io::stdin().is_terminal(),
		&mut prompt_yes_no,
		&mut out,
	)?;
	for (entry, outcome) in results {
		match outcome {
			Outcome::Written => ok(&mut out, format_args!("Configured the {}", entry.label()))?,
			Outcome::Kept => ok(
				&mut out,
				format_args!("The {} was already configured", entry.label()),
			)?,
			Outcome::Skipped => skip(
				&mut out,
				format_args!("Left the {} as it was", entry.label()),
			)?,
		}
	}

	writeln!(out, "{}", "Installation complete".green().bold())?;

	Ok(())
}

pub(crate) fn wire_claude_settings(
	path: &Path,
	subagent: bool,
	interactive: bool,
	ask: Ask<'_>,
	out: &mut dyn Write,
) -> Result<Vec<(Entry, Outcome)>> {
	let original = read_settings(path)?;
	let mut settings = original.clone();

	let mut entries = vec![Entry::StatusLine];
	if wants_subagent(&settings, subagent, interactive, ask) {
		entries.push(Entry::Subagent);
	}

	let mut results = ensure_entries(&mut settings, &entries, &mut |entry| {
		interactive
			&& ask(
				&format!("{} already configured. Overwrite?", entry.label()),
				false,
			)
	})?;

	let edits = Edit::between(&original, &settings, &entries);
	let stale = save_edits(path, &edits, || Ok(()))?;
	for entry in &stale {
		if let Some((_, outcome)) = results.iter_mut().find(|(e, _)| e == entry) {
			*outcome = Outcome::Skipped;
		}
	}
	// Every edit that still applied changed the file.
	if stale.len() < edits.len() {
		ok(out, format_args!("Updated {}", path.display()))?;
	}

	Ok(results)
}

/// Writes `edits` onto a fresh read taken after the last prompt, so edits Claude Code saved meanwhile are kept, and
/// returns the entries left as they now are because they changed underneath. `before_write` runs only when the file
/// is about to change.
pub(crate) fn save_edits(
	path: &Path,
	edits: &[Edit],
	before_write: impl FnOnce() -> Result<()>,
) -> Result<Vec<Entry>> {
	if edits.is_empty() {
		return Ok(Vec::new());
	}
	let current = read_settings(path)?;
	let mut next = current.clone();
	let stale = replay(&mut next, edits)?;
	if next != current {
		before_write()?;
		write_settings(path, &next)?;
	}

	Ok(stale)
}

/// A yes/no question and whether an empty answer means yes.
pub(crate) type Ask<'a> = &'a mut dyn FnMut(&str, bool) -> bool;

/// Whether to add the subagent line. It is only offered while unset, and a piped or scripted install must never wait
/// on a question nobody will answer.
pub(crate) fn wants_subagent(
	settings: &Value,
	requested: bool,
	interactive: bool,
	ask: Ask<'_>,
) -> bool {
	requested
		|| (interactive
			&& settings.get(Entry::Subagent.key()).is_none()
			&& ask("Also install the subagent status line?", true))
}

pub(crate) fn prompt_yes_no(question: &str, default_yes: bool) -> bool {
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

pub(crate) fn ok(out: &mut dyn Write, msg: impl std::fmt::Display) -> Result<()> {
	writeln!(out, "{} {msg}", "✓".green())?;
	Ok(())
}

pub(crate) fn skip(out: &mut dyn Write, msg: impl std::fmt::Display) -> Result<()> {
	writeln!(out, "{} {msg}", "–".dimmed())?;
	Ok(())
}

pub(crate) fn warn(out: &mut dyn Write, msg: impl std::fmt::Display) -> Result<()> {
	writeln!(out, "{} {msg}", "!".yellow().bold())?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn wiring_writes_onto_what_claude_code_saved_during_a_prompt() {
		let dir = std::env::temp_dir().join(format!("statusline-wire-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("settings.json");
		std::fs::write(
			&path,
			r#"{"model": "opus", "statusLine": {"type": "command", "command": "other"}}"#,
		)
		.unwrap();

		let mut out = Vec::new();
		let results = wire_claude_settings(
			&path,
			false,
			true,
			&mut |_, _| {
				// An open session saving a /model change while the question waits.
				std::fs::write(
					&path,
					r#"{"model": "sonnet", "statusLine": {"type": "command", "command": "other"}}"#,
				)
				.unwrap();
				true
			},
			&mut out,
		)
		.unwrap();
		assert_eq!(results[0], (Entry::StatusLine, Outcome::Written));
		let settings = read_settings(&path).unwrap();
		assert_eq!(settings["model"], "sonnet");
		assert_eq!(settings["statusLine"]["command"], "statusline");
		std::fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn subagent_offer_is_silent_without_a_terminal_or_once_set() {
		let unset = json!({});
		let set = json!({"subagentStatusLine": {"type": "command", "command": "x"}});
		let never = &mut |q: &str, _| panic!("must not prompt: {q}");
		assert!(!wants_subagent(&unset, false, false, never));
		assert!(!wants_subagent(&set, false, true, never));
		assert!(wants_subagent(&set, true, false, never));
		assert!(wants_subagent(&unset, false, true, &mut |_, _| true));
		assert!(!wants_subagent(&unset, false, true, &mut |_, _| false));
	}
}
