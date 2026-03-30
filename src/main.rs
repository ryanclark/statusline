mod chrome;
mod constants;
mod context_window;
mod format;
mod install;
mod settings;
mod usage;
mod util;

use crate::context_window::ContextWindow;
use crate::install::install;
use crate::settings::Settings;
use crate::usage::fetch_usage;
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use crate::constants::RED;
use crate::format::Percentage;

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
					eprintln!("{} {e}. Run {} to set up.", "! error:".red().bold(), "statusline install".green());

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
			let cw = if std::io::IsTerminal::is_terminal(&stdin) {
				ContextWindow::default()
			} else {
				ContextWindow::from_reader(stdin.lock()).unwrap_or_else(|e| {
					eprintln!("{} {e}", "failed to parse input".red().bold());

					ContextWindow::default()
				})
			};

			match fetch_usage(&settings.org_id) {
				Ok(response) => print!("{cw} {}", response.display(five, seven)),
				Err(e) => print!("{cw} {}", format_args!("error: {e}").color(RED).dimmed()),
			}
		}
	}
}
