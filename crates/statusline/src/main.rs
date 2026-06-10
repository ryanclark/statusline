mod accounts;
mod browser;
mod install;
mod profiles;
mod update;
mod usage;
mod usage_cache;

#[allow(unused_imports)]
pub(crate) use statusline_core::{
	constants, context_window, format, input, segment, settings, util,
};

use crate::constants::{DIVIDER, GRAY, GREEN, RED};
use crate::input::InputData;
use crate::install::install;
use crate::segment::{RenderContext, SegmentConfig, SegmentLine, default_segments, load_git_cache};
use crate::settings::Settings;
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
		#[arg(short, default_value_t = settings::DEFAULT_FIVE_HOUR_RESET.into(), value_name = "N")]
		five_hour_reset_threshold: Percentage,

		#[arg(short, default_value_t = settings::DEFAULT_SEVEN_DAY_RESET.into(), value_name = "N")]
		seven_day_reset_threshold: Percentage,
	},
	Profiles {
		#[arg(short, long)]
		browser: Option<browser::Browser>,
	},
	Configure,
	#[command(hide = true)]
	UsageRefresh {
		#[arg(long)]
		org: String,

		#[arg(long)]
		browser: browser::Browser,

		#[arg(long)]
		profile: Option<String>,
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
		Some(Commands::Profiles { browser }) => {
			let browser = browser
				.unwrap_or_else(|| browser::detect_or_cached().unwrap_or(browser::Browser::Chrome));
			match profiles::list(browser) {
				Ok(rows) => profiles::print(&rows),
				Err(e) => {
					eprintln!("{} {e:?}", "Listing profiles failed:".red().bold());
					std::process::exit(1);
				}
			}
		}
		Some(Commands::Configure) => {
			let path = match Settings::settings_path() {
				Ok(p) => p,
				Err(e) => {
					eprintln!("{} {e:?}", "error:".red().bold());
					std::process::exit(1);
				}
			};

			if let Some((email, org)) = accounts::live_identity()
				&& let Some(file) = accounts::load()
				&& let Some(account) = accounts::find_for_identity(&file, &email, &org)
				&& account.segments.is_some()
			{
				eprintln!(
					"{} account '{}' has its own `segments` in accounts.json; that override wins over settings.json when this account is active",
					"note:".yellow().bold(),
					account.nickname
				);
			}

			match statusline_configure::run(statusline_configure::Options {
				settings_path: path,
				sample: None,
			}) {
				Ok(statusline_configure::Outcome::Saved(p)) => {
					println!("{} {}", "saved".green().bold(), p.display());
				}
				Ok(statusline_configure::Outcome::Cancelled) => {}
				Err(e) => {
					eprintln!("{} {e}", "configure failed:".red().bold());
					std::process::exit(1);
				}
			}
		}
		Some(Commands::UsageRefresh {
			org,
			browser,
			profile,
		}) => usage_cache::run_refresh(&org, browser, profile.as_deref()),
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

			let needs_usage = segments.iter().any(SegmentConfig::is_extra_usage);
			let needs_credits = segments.iter().any(SegmentConfig::is_credits);

			let resolved = if needs_usage || needs_credits {
				match &identity {
					Some((_, org_uuid)) => {
						let browser = account
							.and_then(|a| a.browser)
							.or(settings.browser)
							.unwrap_or_else(|| {
								browser::detect_or_cached().unwrap_or(browser::Browser::Chrome)
							});
						let profile = account.and_then(|a| a.profile.as_deref());
						Some((org_uuid.as_str(), browser, profile))
					}
					None => None,
				}
			} else {
				None
			};

			let cached = usage_cache::read();
			let (mut usage_result, credits_result) =
				usage_cache::results(cached.as_ref(), needs_usage, needs_credits);

			match resolved {
				Some((org_uuid, browser, profile)) => {
					usage_cache::maybe_spawn_refresh(org_uuid, browser, profile);
				}
				None if needs_usage => {
					let err = usage::UsageError::Other("no active Claude account".to_owned());
					eprintln!("{}", format_args!("usage error: {err}").color(RED).dimmed());
					usage_result = Some(Err(err));
				}
				None => {}
			}

			let divider = settings.divider.as_deref().unwrap_or(DIVIDER);
			let git_cache = load_git_cache(&input.cwd);

			let account_display = account.map(|a| segment::AccountDisplay {
				nickname: a.nickname.clone(),
				color: a.color.clone(),
			});

			let line = SegmentLine {
				segments: &segments,
				ctx: RenderContext {
					input: &input,
					usage: usage_result.as_ref().map(|r| r.as_ref()),
					credits: credits_result.as_ref().map(|r| r.as_ref()),
					git: git_cache.as_ref(),
					five_threshold: five,
					seven_threshold: seven,
					divider,
					nerd_font: settings.nerd_font,
					account: account_display,
				},
			};

			if let Some(update) = update {
				let update_msg = format!(
					"{} {} {}",
					format_args!("v{} available", update.version).color(GREEN),
					divider.color(GRAY),
					"brew upgrade ryanclark/tap/statusline".dimmed()
				);
				let rendered = format!("{line}");
				if rendered.is_empty() {
					print!("{update_msg}");
				} else {
					print!("{rendered} {} {update_msg}", divider.color(GRAY));
				}
			} else {
				print!("{line}");
			}
		}
	}
}
