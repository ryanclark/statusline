mod accounts;
mod browser;
mod constants;
mod context_window;
mod format;
mod input;
mod install;
mod segment;
mod settings;
mod update;
mod usage;
mod util;

use crate::constants::{DIVIDER, GRAY, GREEN, RED};
use crate::input::InputData;
use crate::install::install;
use crate::segment::{RenderContext, SegmentConfig, SegmentLine, default_segments, load_git_cache};
use crate::settings::Settings;
use crate::usage::fetch_usage;
use clap::{Parser, Subcommand};
use format::Percentage;
use owo_colors::OwoColorize;

#[derive(Parser)]
struct Cli {
	#[command(subcommand)]
	command: Option<Commands>,

	#[arg(short)]
	five_hour_reset_threshold: Option<Percentage>,

	#[arg(short)]
	seven_day_reset_threshold: Option<Percentage>,
}

#[derive(Subcommand)]
enum Commands {
	Install {
		#[arg(short, default_value = "70", value_name = "N")]
		five_hour_reset_threshold: Percentage,

		#[arg(short, default_value = "100", value_name = "N")]
		seven_day_reset_threshold: Percentage,
	},
}

fn main() {
	let cli = Cli::parse();

	match cli.command {
		Some(Commands::Install {
			five_hour_reset_threshold,
			seven_day_reset_threshold,
		}) => {
			if let Err(e) = install(five_hour_reset_threshold, seven_day_reset_threshold) {
				eprintln!("{} {e:?}", "Installation failed:".red().bold());
			}
		}
		None => {
			let settings = match Settings::load() {
				Ok(s) => s,
				Err(e) => {
					eprintln!(
						"{} {e}. Run {} to set up.",
						"! error:".red().bold(),
						"statusline install".green()
					);

					return;
				}
			};

			let five = cli
				.five_hour_reset_threshold
				.unwrap_or(settings.five_hour_reset_threshold);
			let seven = cli
				.seven_day_reset_threshold
				.unwrap_or(settings.seven_day_reset_threshold);

			let stdin = std::io::stdin();
			let is_tty = std::io::IsTerminal::is_terminal(&stdin);
			let input = if is_tty {
				InputData::default()
			} else {
				InputData::from_reader(stdin.lock()).unwrap_or_else(|e| {
					eprintln!("{} {e}", "failed to parse input".red().bold());
					InputData::default()
				})
			};
			let is_fresh = input.context_window.used_percentage == 0.0.into();
			let update = if is_fresh && !settings.skip_update_check {
				update::check()
			} else {
				None
			};

			let accounts_file = accounts::load();
			let identity = accounts::live_identity();
			let account = match (&identity, &accounts_file) {
				(Some((email, org)), Some(file)) => accounts::find_for_identity(file, email, org),
				_ => None,
			};

			let segments = account
				.and_then(|a| a.segments.clone())
				.or(settings.segments)
				.unwrap_or_else(default_segments);

			let needs_api = segments.iter().any(SegmentConfig::is_extra_usage);
			let usage_result = if needs_api {
				match &identity {
					Some((_, org_uuid)) => {
						let browser = account
							.and_then(|a| a.browser)
							.or(settings.browser)
							.unwrap_or_else(|| {
								browser::Browser::detect_or_cached()
									.unwrap_or(browser::Browser::Chrome)
							});
						let profile = account.and_then(|a| a.profile.as_deref());
						let result = fetch_usage(org_uuid, browser, profile);
						if let Err(e) = &result {
							eprintln!("{}", format_args!("usage error: {e}").color(RED).dimmed());
						}
						Some(result)
					}
					None => {
						let err = usage::UsageError::Other("no active Claude account".to_owned());
						eprintln!("{}", format_args!("usage error: {err}").color(RED).dimmed());
						Some(Err(err))
					}
				}
			} else {
				None
			};

			let divider = settings.divider.as_deref().unwrap_or(DIVIDER);
			let git_cache = load_git_cache(&input.cwd);

			let line = SegmentLine {
				segments: &segments,
				ctx: RenderContext {
					input: &input,
					usage: usage_result.as_ref().map(|r| r.as_ref()),
					git: git_cache.as_ref(),
					five_threshold: five,
					seven_threshold: seven,
					divider,
					nerd_font: settings.nerd_font,
				},
			};

			if let Some(update) = update {
				let update_msg = format!(
					"{} {} {}",
					format_args!("v{} available", update.version).color(GREEN),
					divider.color(GRAY),
					"brew upgrade statusline".dimmed()
				);
				let rendered = format!("{line}");
				if rendered.is_empty() {
					print!("{update_msg}");
				} else {
					print!(
						"{rendered} {} {update_msg}",
						divider.color(GRAY)
					);
				}
			} else {
				print!("{line}");
			}
		}
	}
}
