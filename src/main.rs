mod browser;
mod constants;
mod context_window;
mod format;
mod input;
mod install;
mod segment;
mod settings;
mod usage;
mod util;

use crate::constants::{DIVIDER, RED};
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
		#[arg(short, long)]
		org_id: String,

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
			org_id,
			five_hour_reset_threshold,
			seven_day_reset_threshold,
		}) => {
			if let Err(e) = install(
				&org_id,
				five_hour_reset_threshold,
				seven_day_reset_threshold,
			) {
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
			let input = if std::io::IsTerminal::is_terminal(&stdin) {
				InputData::default()
			} else {
				InputData::from_reader(stdin.lock()).unwrap_or_else(|e| {
					eprintln!("{} {e}", "failed to parse input".red().bold());

					InputData::default()
				})
			};

			let segments = settings.segments.unwrap_or_else(default_segments);

			let needs_api = segments.iter().any(SegmentConfig::is_extra_usage);
			let usage_response = if needs_api {
				let browser = settings.browser.unwrap_or_else(|| {
					browser::Browser::detect_or_cached().unwrap_or(browser::Browser::Chrome)
				});
				match fetch_usage(&settings.org_id, browser) {
					Ok(resp) => Some(resp),
					Err(e) => {
						eprintln!("{}", format_args!("usage error: {e}").color(RED).dimmed());
						None
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
					usage: usage_response.as_ref(),
					git: git_cache.as_ref(),
					five_threshold: five,
					seven_threshold: seven,
					divider,
					nerd_font: settings.nerd_font,
				},
			};

			print!("{line}");
		}
	}
}
