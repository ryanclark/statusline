use crate::format::Percentage;
use crate::settings::Settings;
use crate::util::home_dir;
use eyre::Result;
use owo_colors::OwoColorize;
use statusline_core::claude_settings::{
	Edit, Entry, Outcome, ensure_entries, read_settings, replay, write_settings,
};
use std::io::IsTerminal;
use std::path::Path;

pub(crate) fn install(
	five_hour_reset_threshold: Option<Percentage>,
	seven_day_reset_threshold: Option<Percentage>,
	subagent: bool,
) -> Result<()> {
	let settings_path = Settings::settings_path()?;
	let existed = settings_path.exists();
	Settings::ensure_at(
		&settings_path,
		five_hour_reset_threshold,
		seven_day_reset_threshold,
	)?;

	if existed {
		println!("{} Kept {}", "✓".green(), settings_path.display());
	} else {
		println!("{} Saved default settings", "✓".green());
	}

	let path = home_dir()?.join(".claude").join("settings.json");
	let results = wire_claude_settings(
		&path,
		subagent,
		std::io::stdin().is_terminal(),
		&mut prompt_yes_no,
	)?;
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

/// Writes the answers onto a fresh read taken after the last prompt, so edits Claude Code saved meanwhile are kept.
pub(crate) fn wire_claude_settings(
	path: &Path,
	subagent: bool,
	interactive: bool,
	ask: Ask<'_>,
) -> Result<Vec<(Entry, Outcome)>> {
	let original = read_settings(path)?;
	let mut settings = original.clone();

	let mut entries = vec![Entry::StatusLine];
	let subagent_configured = settings.get(Entry::Subagent.key()).is_some();
	if subagent
		|| (!subagent_configured
			&& offer_subagent(interactive, || {
				ask("Also install the subagent status line?", true)
			})) {
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
	if edits.is_empty() {
		return Ok(results);
	}
	let current = read_settings(path)?;
	let mut next = current.clone();
	for stale in replay(&mut next, &edits)? {
		if let Some((_, outcome)) = results.iter_mut().find(|(entry, _)| *entry == stale) {
			*outcome = Outcome::Skipped;
		}
	}
	if next != current {
		write_settings(path, &next)?;
		println!("{} Updated {}", "✓".green(), path.display());
	}

	Ok(results)
}

/// A yes/no question and whether an empty answer means yes.
pub(crate) type Ask<'a> = &'a mut dyn FnMut(&str, bool) -> bool;

/// A piped or scripted install must never wait on a question nobody will answer.
pub(crate) fn offer_subagent(interactive: bool, ask: impl FnOnce() -> bool) -> bool {
	interactive && ask()
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

#[cfg(test)]
mod tests {
	use super::*;

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

		let results = wire_claude_settings(&path, false, true, &mut |_, _| {
			// An open session saving a /model change while the question waits.
			std::fs::write(
				&path,
				r#"{"model": "sonnet", "statusLine": {"type": "command", "command": "other"}}"#,
			)
			.unwrap();
			true
		})
		.unwrap();
		assert_eq!(results[0], (Entry::StatusLine, Outcome::Written));
		let settings = read_settings(&path).unwrap();
		assert_eq!(settings["model"], "sonnet");
		assert_eq!(settings["statusLine"]["command"], "statusline");
		std::fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn subagent_offer_is_silent_without_a_terminal() {
		assert!(!offer_subagent(false, || panic!("must not prompt")));
		assert!(offer_subagent(true, || true));
		assert!(!offer_subagent(true, || false));
	}
}
