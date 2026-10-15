# External usage sources

Statusline can read account limits from any program that implements this JSON protocol. The program runs in the background; rendering reads a private cache. This works with the native status line and the plugin. No service-specific integration is required.

Add `usage_source` to `~/.statusline/settings.json`, keeping your existing settings:

```json
{
  "usage_source": {
    "command": ["python3", "/absolute/path/usage-file.py", "/absolute/path/accounts.json"],
    "key_env": "MY_ACCOUNT_KEY",
    "refresh_interval": "60s",
    "max_age": "5m",
    "timeout": "2s"
  }
}
```

`command` is an argv array, executed directly. Shell expansion, pipes, `~` expansion and command substitution are not performed. Use absolute paths when the status-line environment has a different `PATH`. The command inherits the process environment. Treat a configured command as trusted executable code.

Alternatively, set `STATUSLINE_USAGE_SOURCE` to the JSON source object (without the outer `usage_source` property). This overrides the settings file for that process. An invalid explicit configuration remains an error; it never selects a different account or provider. Unrelated context, session, Git and activity segments continue to render.

The three durations default to `60s`, `5m` and `2s`. They must be positive; subprocess timeouts are limited to 60 seconds. `key_env` names the variable containing an opaque account key. A configured but missing or empty key hides external usage. Without `key_env`, statusline uses the input conversation ID. The source configuration and resolved key jointly identify the cache and refresh lock.

## Command contract: version 1

The command reads one JSON object from stdin and writes one JSON object to stdout, then exits successfully. Diagnostics belong on stderr. Statusline reports concise error categories rather than displaying arbitrary command output. A nonzero exit, malformed response, timeout or response larger than 1 MiB leaves the previous good snapshot intact. Descendants are stopped with the timed-out command. Failed attempts use the same refresh interval as successful attempts.

Request:

```json
{"version":1,"key":"work","session_id":"conversation-id","model_id":"model-id"}
```

`model_id` is optional. The key is opaque and must be returned unchanged. `session_id` identifies the conversation; it does not imply account ownership.

Response:

```json
{
  "version": 1,
  "key": "work",
  "observed_at": "2026-10-15T12:00:00Z",
  "account": {"id": "provider:organization:user", "label": "Work", "color": "#64c8dc"},
  "limits": [
    {"id": "five_hour", "label": "Session", "used_percent": 12.5, "window_seconds": 18000, "resets_at": "2026-10-15T14:30:00Z"},
    {"id": "burst", "label": "Burst", "used_percent": 42, "window_seconds": 3600, "resets_at": "2026-10-15T13:00:00Z"}
  ],
  "spend": {"currency": "EUR", "exponent": 2, "used_minor": 1234, "limit_minor": 5000, "balance_minor": 8000}
}
```

`version`, `key`, `observed_at`, `account.id` and `account.label` are required. Account IDs must be nonempty. IDs and keys need not be equal. Unknown fields are tolerated. Optional measurements can be omitted or null; absent/null `limits` means no limits. Each limit has a unique, nonempty `id`; its other fields are optional. Percentages must be finite and nonnegative. Values above 100 are retained. Timestamps use RFC 3339. `observed_at` is when the provider measured the data, not when this command reads it.

Spending uses signed 64-bit integer minor units, a three-letter uppercase currency code and an exponent from 0 through 18. For example, EUR 1234 with exponent 2 is EUR 12.34; JPY 1234 with exponent 0 is JPY 1234. Statusline preserves integer precision and prints the currency code. Missing money stays absent; it is never guessed from tokens or credits with unspecified units.

A future observation is unusable. A snapshot older than `max_age` displays with `(stale)` and is dimmed when colours are enabled. Reading the cache does not extend freshness. Windows whose reset time has passed are hidden until a new observation arrives. Changing the account key immediately stops using the previous key's cache, including results from a still-running refresh.

## Displaying external values

Existing `five_hour` and `seven_day` segments use only the limit IDs with those exact names. A different duration is never presented as a five-hour limit. `spend_limit` and `fable_usage` likewise use their named IDs. The `account` segment uses the external account label and colour. `extra_usage` shows used/limit money, and `credits` shows the supplied balance.

Use `usage_limit` for any other quota:

```json
{"type":"usage_limit","limit_id":"burst","label":"Burst","show_countdown":true}
```

The provider's label is used unless the segment supplies one. `icon: false` hides this label. Standard colour, style and countdown options apply. The editor exposes the limit ID.

While an external source is configured, the native binary does not read browser cookies or the global Claude login for account usage. The plugin receives `needs.usage: false` before its first account poll. Its session/context/activity collection continues. Older plugins may still perform their own poll, but its results never replace the configured external source; update the plugin and binary together to suppress that redundant work.

## Arbitrary environment text

This feature is independent of usage sources and works in main and subagent layouts:

```json
{"type":"env","var":"MY_ACCOUNT_NAME","color_env":"MY_ACCOUNT_COLOR"}
```

Set `MY_ACCOUNT_NAME='Work ☕'` and `MY_ACCOUNT_COLOR='#64c8dc'` in the parent Claude process. Any Unicode text is supported. Missing/empty values hide the segment; absent or invalid colours leave it uncoloured. Terminal control sequences and line breaks are removed. `colors: false` produces plain output. These fields are also editable with `statusline configure`.

## A standalone provider

[usage-file.py](examples/usage-file.py) reads a JSON file containing an object keyed by account key. Each value has the response fields above. A separate collector updates that file, retaining each observation's original timestamp. Configure the example command, export `MY_ACCOUNT_KEY=work`, and include `account` and your desired limit segments. It uses only Python's standard library and makes no network or credential requests.

The [version-1 fixture](../crates/statusline-core/tests/fixtures/external-usage-v1.json) is checked by the Rust protocol tests. The example also runs through native and plugin rendering in integration tests. Services such as Beast can implement the same command contract without adding dependencies to statusline.
