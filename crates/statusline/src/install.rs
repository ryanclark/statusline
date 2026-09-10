use crate::format::Percentage;
use crate::settings::Settings;
use crate::util::home_dir;
use eyre::Result;
use owo_colors::OwoColorize;
use statusline_core::claude_settings::{
	Entry, Outcome, ensure_entries, read_settings, write_settings,
};
use std::io::IsTerminal;

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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn subagent_offer_is_silent_without_a_terminal() {
		assert!(!offer_subagent(false, || panic!("must not prompt")));
		assert!(offer_subagent(true, || true));
		assert!(!offer_subagent(true, || false));
	}
}
