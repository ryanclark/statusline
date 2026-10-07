mod accounts;
mod browser;
mod install;
mod plugin_install;
mod profiles;
mod session;
mod subagent;
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
use std::time::Duration;

/// The longest a heartbeat may silence the native line, since a plugin that stops without a final heartbeat leaves
/// the session with no line until it expires.
const MAX_HEARTBEAT: Duration = Duration::from_secs(30);

#[derive(Parser)]
#[command(version)]
struct Cli {
	#[command(subcommand)]
	command: Option<Commands>,

	#[arg(short)]
	five_hour_reset_threshold: Option<Percentage>,

	#[arg(short)]
	seven_day_reset_threshold: Option<Percentage>,

	/// Output format. `spans` prints JSON rows of styled text for the Claude Code plugin.
	#[arg(long, value_enum, default_value_t = OutputFormat::Ansi)]
	format: OutputFormat,

	/// With `--format spans`, how long the native status line stays silent for this session. Capped at 30 seconds.
	#[arg(long = "heartbeat-ms", value_name = "MS", default_value = "10000", value_parser = parse_heartbeat)]
	heartbeat: Duration,
}

fn parse_heartbeat(ms: &str) -> Result<Duration, std::num::ParseIntError> {
	Ok(Duration::from_millis(ms.parse()?).min(MAX_HEARTBEAT))
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
	Ansi,
	Spans,
}

#[derive(Subcommand)]
enum Commands {
	#[command(group(clap::ArgGroup::new("mode").args(["plugin", "native"])))]
	Install {
		/// Show the 5-hour reset countdown above this percentage (default 70). An existing settings
		/// file keeps its value unless this is given.
		#[arg(short, value_name = "N")]
		five_hour_reset_threshold: Option<Percentage>,

		/// Show the 7-day reset countdown above this percentage (default 100). An existing settings
		/// file keeps its value unless this is given.
		#[arg(short, value_name = "N")]
		seven_day_reset_threshold: Option<Percentage>,

		/// Also wire Claude Code's subagentStatusLine to `statusline subagent`.
		#[arg(long)]
		subagent: bool,

		/// Install the Claude Code plugin pointed at this binary and remove the native statusLine.
		#[arg(long)]
		plugin: bool,

		/// Uninstall the plugin and put back the native statusLine it replaced.
		#[arg(long)]
		native: bool,

		/// With --plugin, keep the native statusLine as a fallback for sessions where the plugin does not load.
		#[arg(long, requires = "plugin", conflicts_with = "native")]
		keep_native: bool,

		/// With --native, also remove the ryanclark marketplace.
		#[arg(long, requires = "native", conflicts_with = "plugin")]
		remove_marketplace: bool,

		/// The Claude Code executable to run, when `claude` is not on PATH.
		#[arg(long, value_name = "PATH", default_value = "claude", requires = "mode")]
		claude: std::path::PathBuf,

		/// Print what --plugin or --native would change without changing it.
		#[arg(long, requires = "mode")]
		dry_run: bool,
	},
	Profiles {
		#[arg(short, long)]
		browser: Option<browser::Browser>,
	},
	Configure,
	/// Render the agent panel rows for Claude Code's subagentStatusLine (reads JSON on stdin).
	Subagent,
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
			subagent,
			plugin,
			native,
			keep_native,
			remove_marketplace,
			claude,
			dry_run,
		}) if plugin || native => {
			let mode = if plugin {
				plugin_install::Mode::Plugin(plugin_install::PluginOptions {
					keep_native,
					subagent,
					five_hour_reset_threshold,
					seven_day_reset_threshold,
				})
			} else {
				plugin_install::Mode::Native { remove_marketplace }
			};
			if let Err(e) = plugin_install::run(mode, claude, dry_run) {
				eprintln!("{} {e:?}", "Installation failed:".red().bold());
				std::process::exit(1);
			}
		}
		Some(Commands::Install {
			five_hour_reset_threshold,
			seven_day_reset_threshold,
			subagent,
			..
		}) => {
			if let Err(e) = install(
				five_hour_reset_threshold,
				seven_day_reset_threshold,
				subagent,
			) {
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
		Some(Commands::Subagent) => subagent::run(cli.format),
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
				claude_settings_path: util::home_dir()
					.ok()
					.map(|h| h.join(".claude").join("settings.json")),
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
			let stdin = std::io::stdin();
			let is_tty = std::io::IsTerminal::is_terminal(&stdin);
			let mut raw = Vec::new();
			if !is_tty && let Err(e) = std::io::Read::read_to_end(&mut stdin.lock(), &mut raw) {
				eprintln!("{} {e}", "failed to read input".red().bold());
			}

			if let Some(session_id) = session::requested(&raw) {
				session::print_report(&session_id);
				return;
			}

			let spans = matches!(cli.format, OutputFormat::Spans);

			if spans {
				session::heartbeat(&raw, cli.heartbeat);
			} else if !is_tty && session::drawn_by_plugin(&raw) {
				// The plugin is drawing, so print nothing but still capture, since this input is complete where the
				// plugin's is not.
				if Settings::load().is_ok_and(|s| s.capture_snapshots) {
					session::capture(&raw, session::Origin::Native);
				}
				return;
			}

			let settings = match Settings::load() {
				Ok(s) => s,
				// A plugin can be installed without `statusline install`, and the defaults are a usable line.
				Err(settings::SettingsError::Io(e))
					if spans && e.kind() == std::io::ErrorKind::NotFound =>
				{
					Settings::default()
				}
				Err(e) => {
					eprintln!(
						"{} {e}. Run {} to set up.",
						"! error:".red().bold(),
						"statusline install".green()
					);
					// The plugin surfaces an error only on a non-zero exit, while Claude Code's own line ignores the
					// status.
					if spans {
						std::process::exit(1);
					}

					return;
				}
			};

			let five = cli
				.five_hour_reset_threshold
				.unwrap_or(settings.five_hour_reset_threshold);
			let seven = cli
				.seven_day_reset_threshold
				.unwrap_or(settings.seven_day_reset_threshold);

			let input = if is_tty {
				InputData::default()
			} else {
				InputData::from_reader(raw.as_slice()).unwrap_or_else(|e| {
					eprintln!("{} {e}", "failed to parse input".red().bold());
					if spans {
						std::process::exit(1);
					}
					InputData::default()
				})
			};
			if settings.capture_snapshots {
				let origin = if spans {
					session::Origin::Plugin
				} else {
					session::Origin::Native
				};
				session::capture(&raw, origin);
			}
			let is_fresh = input.context_window.used_percentage == 0.0.into();
			// A spans host polls every second, and a fresh session would otherwise run the network check on each poll.
			let update = if is_fresh && !spans && !settings.skip_update_check {
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

			let needs_usage = segments
				.iter()
				.any(|s| s.is_extra_usage() || s.is_fable_usage());
			let needs_credits = segments.iter().any(SegmentConfig::is_credits);

			let plugin_usage = input.mod_info.as_ref().and_then(|m| m.usage.as_ref());
			let from_cookies = plugin_usage.is_none() && (needs_usage || needs_credits);

			let resolved = if from_cookies {
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

			let usage_cache::Resolved {
				usage: mut usage_result,
				credits: credits_result,
				stale: usage_stale,
			} = usage_cache::resolve(
				plugin_usage,
				usage_cache::read,
				needs_usage,
				needs_credits,
				chrono::Utc::now(),
			);

			match resolved {
				Some((org_uuid, browser, profile)) => {
					usage_cache::maybe_spawn_refresh(org_uuid, browser, profile);
				}
				None if needs_usage && plugin_usage.is_none() => {
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
					usage_stale,
					git: git_cache.as_ref(),
					five_threshold: five,
					seven_threshold: seven,
					divider,
					nerd_font: settings.nerd_font,
					account: account_display,
					task: None,
				},
			};

			let mut rendered = format!("{line}");
			if let Some(update) = update {
				let update_msg = format!(
					"{} {} {}",
					format_args!("v{} available", update.version).color(GREEN),
					divider.color(GRAY),
					"brew upgrade ryanclark/tap/statusline".dimmed()
				);
				if rendered.is_empty() {
					rendered = update_msg;
				} else {
					rendered = format!("{rendered} {} {update_msg}", divider.color(GRAY));
				}
			}

			match cli.format {
				OutputFormat::Ansi => print!("{rendered}"),
				OutputFormat::Spans => {
					let rows = statusline_core::spans::ansi_to_spans(&rendered);
					match serde_json::to_string(&rows) {
						Ok(json) => print!("{json}"),
						Err(e) => {
							eprintln!("{} {e}", "failed to encode spans".red().bold());
							std::process::exit(1);
						}
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cli_definition_is_consistent() {
		use clap::CommandFactory as _;
		Cli::command().debug_assert();
	}

	#[test]
	fn plugin_flags_leave_plain_install_alone() {
		let parses = |args: &[&str]| Cli::try_parse_from(args).is_ok();
		assert!(parses(&["statusline", "install"]));
		assert!(parses(&["statusline", "install", "--subagent"]));
		assert!(parses(&[
			"statusline",
			"install",
			"--plugin",
			"--keep-native",
			"--dry-run"
		]));
		assert!(parses(&[
			"statusline",
			"install",
			"--native",
			"--remove-marketplace",
			"--claude",
			"/x/claude"
		]));
		assert!(!parses(&["statusline", "install", "--plugin", "--native"]));
		assert!(!parses(&[
			"statusline",
			"install",
			"--remove-marketplace",
			"--plugin"
		]));
		assert!(!parses(&[
			"statusline",
			"install",
			"--keep-native",
			"--native"
		]));
		assert!(!parses(&["statusline", "install", "--keep-native"]));
		assert!(!parses(&["statusline", "install", "--remove-marketplace"]));
		assert!(!parses(&["statusline", "install", "--dry-run"]));
		let Some(Commands::Install { keep_native, .. }) =
			Cli::parse_from(["statusline", "install", "--plugin"]).command
		else {
			panic!("install should parse");
		};
		assert!(
			!keep_native,
			"--plugin removes the native statusLine unless told to keep it"
		);
		assert!(!parses(&["statusline", "install", "--claude", "/x/claude"]));
	}

	#[test]
	fn heartbeat_is_capped_at_thirty_seconds() {
		let heartbeat = |args: &[&str]| Cli::try_parse_from(args).map(|cli| cli.heartbeat);
		assert_eq!(heartbeat(&["statusline"]).unwrap(), Duration::from_secs(10));
		assert_eq!(
			heartbeat(&["statusline", "--heartbeat-ms", "1802000"]).unwrap(),
			MAX_HEARTBEAT
		);
		assert!(heartbeat(&["statusline", "--heartbeat-ms", "soon"]).is_err());
	}
}
