# Per-Account Browser Profile & Segment Overrides

**Date:** 2026-04-18
**Status:** Design approved, ready for plan

## Motivation

Users who switch between multiple Claude Code accounts (work / personal / etc.) keep each account's `claude.ai` session in a separate browser profile. Today `statusline`'s `extra_usage` segment hardcodes the `Default` Chromium profile and fetches usage for a single `settings.org_id`, so only one account's data is reachable.

Separately: users want to disable the update-check banner.

## Goals

1. `extra_usage` uses the browser profile that matches the currently-active Claude Code account.
2. The org being queried matches the live Claude Code account — no manual `settings.org_id` kept in sync.
3. Per-account layout overrides so each account can have its own `segments` list.
4. Optional `skip_update_check` setting.
5. A helper subcommand to list available browser profiles.

## Non-goals

- Changing how cookie decryption works.
- Multi-profile fan-out (fetching both accounts at once). One active account at a time.
- Generalized per-account overrides for every setting — only `segments`, `browser`, and `profile` are per-account.
- Migrating existing `settings.json` files. Legacy `org_id` is silently ignored; no rewrite.

## Config Surface

### `~/.statusline/settings.json`

- **Remove** `org_id` from the `Settings` struct. Files still containing `org_id` continue to load (no `deny_unknown_fields`); the value is discarded.
- **Add** `skip_update_check: bool`, default `false`.

Everything else unchanged: `five_hour_reset_threshold`, `seven_day_reset_threshold`, `segments`, `divider`, `nerd_font`, `browser`.

### `~/.statusline/accounts.json`

Each `AccountEntry` gains three optional fields:

```json
{
  "accounts": [
    {
      "nickname": "work",
      "email": "ryan@work.com",
      "organization_uuid": "work-org-uuid",
      "color": "cyan",
      "browser": "chrome",
      "profile": "Profile 2",
      "segments": ["context_percentage", "divider", "extra_usage"]
    },
    {
      "nickname": "personal",
      "email": "ryan@personal.com",
      "organization_uuid": "personal-org-uuid"
    }
  ]
}
```

- `browser` — `"chrome" | "brave" | "firefox"`. Optional.
- `profile` — Browser profile directory name. For Chromium: `Default`, `Profile 1`, etc. For Firefox: a path under `Library/Application Support/Firefox` (e.g. `Profiles/abc.default-release`). Optional.
- `segments` — Same shape as `settings.segments`. When present, fully replaces the global list for this account.

## Resolution Rules

At each render:

1. **Live identity**: read `.claude.json` via existing `live_identity()`, yielding `(email, org_uuid)`. If absent, treat as "no active account."
2. **Account entry**: look up the `AccountEntry` where `email` and `organization_uuid` match the live identity. May be `None`.
3. **Segment list**: `account.segments` → `settings.segments` → `default_segments()`.
4. **`needs_api`**: computed against the resolved segment list.
5. **Browser**: `account.browser` → `settings.browser` → `Browser::detect_or_cached()`.
6. **Profile**: `account.profile` → `None` (defaults: `Default` for Chromium, auto-detected for Firefox).
7. **Org for fetch**: live `org_uuid`. If `extra_usage` is in the resolved list and live identity is missing, surface `UsageError::Other("no active Claude account")` through the existing error render path.

## Components

### `src/accounts.rs` (new module)

Moves the accounts-file types and loader out of `src/segment/account.rs` so `main.rs` can use them too. Exposes:

- `pub(crate) struct AccountEntry { nickname, email, organization_uuid, color, browser: Option<Browser>, profile: Option<String>, segments: Option<Vec<SegmentConfig>> }`
- `pub(crate) fn load() -> Option<AccountsFile>` — reads the sidecar, returning `None` on any error (current behavior preserved).
- `pub(crate) fn find_for_identity(file: &AccountsFile, email: &str, org_uuid: &str) -> Option<&AccountEntry>`
- `pub(crate) fn live_identity() -> Option<(String, String)>` — moved from `segment/account.rs`.

`src/segment/account.rs` becomes a thin caller that reuses these helpers for the rendered nickname.

### `src/browser.rs`

```rust
pub(crate) fn load_session_key(self, profile: Option<&str>) -> Result<String>
```

- `ChromiumConfig` splits current `db_rel_path` into a base directory (`Library/Application Support/Google/Chrome` / `.../BraveSoftware/Brave-Browser`) and we append `{profile.unwrap_or("Default")}/Cookies`.
- `firefox_session_key` takes `profile: Option<&str>`; when `Some`, uses it directly instead of calling `default_firefox_profile`.
- Existing error contexts unchanged.

### `src/usage.rs`

Signature becomes `fetch_usage(org_id: &str, browser: Browser, profile: Option<&str>) -> Result<UsageResponse, UsageError>`. Implementation forwards `profile` to `browser.load_session_key(profile)`.

### `src/main.rs`

Happy path:

```rust
let accounts_file = accounts::load();
let identity = accounts::live_identity();
let account = identity.as_ref().and_then(|(email, org)| {
    accounts_file.as_ref().and_then(|f| accounts::find_for_identity(f, email, org))
});

let segments = account
    .and_then(|a| a.segments.clone())
    .or(settings.segments)
    .unwrap_or_else(default_segments);

let needs_api = segments.iter().any(SegmentConfig::is_extra_usage);

let usage_result = if needs_api {
    match identity {
        Some((_, org_uuid)) => {
            let browser = account.and_then(|a| a.browser)
                .or(settings.browser)
                .unwrap_or_else(|| Browser::detect_or_cached().unwrap_or(Browser::Chrome));
            let profile = account.and_then(|a| a.profile.as_deref());
            Some(fetch_usage(&org_uuid, browser, profile))
        }
        None => Some(Err(UsageError::Other("no active Claude account".into()))),
    }
} else {
    None
};
```

Update check:

```rust
let update = if is_fresh && !settings.skip_update_check {
    update::check()
} else {
    None
};
```

### `statusline install`

- `Commands::Install` loses `org_id`.
- `install::install` signature: `(five_hour, seven_day) -> Result<()>`.
- `Settings::ensure` signature: `(five_hour, seven_day) -> Result<Settings>`.

### `statusline profiles` subcommand

```
statusline profiles [--browser chrome|brave|firefox]
```

- Defaults to `Browser::detect_or_cached()` when flag omitted.
- **Chromium**: read `Library/Application Support/<vendor>/Local State` as JSON, iterate `profile.info_cache`, print one row per entry:
  ```
  DIRECTORY         USER                 NAME
  Default           ryan@work.com        Work
  Profile 1         ryan@personal.com    Personal
  ```
- **Firefox**: reuse parsing from `default_firefox_profile`, but emit every `[Profile*]` section's `Path` and `Name`.

## Error Handling

- **No live identity + `extra_usage` configured**: single inline error `"no active Claude account"` — same visual treatment as today's fetch errors.
- **Profile directory missing on disk**: propagates from `rusqlite::Connection::open_with_flags` with the existing "opening X cookies database" context — inline error, no crash.
- **Accounts file missing or malformed**: treated as no account (current behavior); segments fall back to the global list.
- **Legacy `org_id` in settings.json**: ignored silently.

## Testing

- `accounts.rs`: parses entries with and without the new optional fields.
- `settings.rs`: accepts legacy `org_id` without error; `skip_update_check` round-trips; default is `false`.
- `browser.rs`: chromium path composition uses the provided profile; firefox path uses provided profile when `Some`, falls back to detection when `None`.
- `main.rs` / integration-level (if feasible with existing structure): segment resolution picks account's segments when identity matches, falls back to settings otherwise.
- `profiles` subcommand: Chromium `Local State` fixture parses into expected rows; Firefox fixture parses all profiles, not just the default.

## Breaking Changes

- `statusline install -o <org-id>` no longer accepts `-o`. Users who scripted this need to drop the flag. Documented in README.
- CLI-scripted users who read `settings.json` and expect `org_id` to be there will no longer find it after a fresh install.

Everything else is additive.

## Open Questions

None at time of writing.
