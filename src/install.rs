use crate::settings::Settings;
use crate::util::home_dir;
use eyre::{Context, Result};
use owo_colors::OwoColorize;
use std::fs;
use crate::format::Percentage;

pub(crate) fn install(
	org_id: &str,
	five_hour_reset_threshold: Percentage,
	seven_day_reset_threshold: Percentage,
) -> Result<()> {
	Settings::ensure(
		org_id,
		five_hour_reset_threshold,
		seven_day_reset_threshold,
	)?;

	println!("{} Saved settings", "✓".green());

	ensure_claude_code_settings()?;

	println!("{}", "Installation complete".green().bold());

	Ok(())
}

fn ensure_claude_code_settings() -> Result<()> {
	let home = home_dir()?;
	let settings_path = home.join(".claude").join("settings.json");

	let mut settings: serde_json::Value = match fs::read_to_string(&settings_path) {
		Ok(data) => serde_json::from_str(&data).context("failed to parse settings.json")?,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			serde_json::json!({})
		}
		Err(e) => return Err(e).context("reading Claude Code settings.json"),
	};

	if settings.get("statusLine").is_some() {
		eprint!(
			"{} statusline already configured. Overwrite? [y/N] ",
			"?".yellow().bold()
		);

		let mut answer = String::new();
		std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut answer)?;

		if !answer.trim().eq_ignore_ascii_case("y") {
			println!("{} Skipped Claude Code settings", "–".dimmed());

			return Ok(());
		}
	}

	settings["statusLine"] = serde_json::json!({
		"type": "command",
		"command": "statusline",
	});

	if let Some(parent) = settings_path.parent() {
		fs::create_dir_all(parent)?;
	}

	let data = serde_json::to_string_pretty(&settings)?;
	fs::write(&settings_path, data)?;

	println!("{} Updated {}", "✓".green(), settings_path.display());

	Ok(())
}
