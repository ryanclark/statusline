import type { SessionUsage } from 'claude-code'

import type { UsageInput } from './usage'
import { obj } from './util'

type Period = { used_percentage: number; resets_at: number }

// Match the binary's stale-usage threshold and tolerance for clock skew between chats.
const STALE_MS = 5 * 60_000
const AHEAD_MS = 60_000
// Headers contain whole seconds; the account endpoint includes subsecond precision.
const RESET_SLOP_SECONDS = 1

function period(percent: unknown, reset: unknown, now: number): Period | null {
  // ECMAScript guarantees milliseconds, while the endpoint returns six fractional digits.
  const at = typeof reset === 'string' ? Date.parse(reset.replace(/(\.\d{3})\d+(?=Z|[+-]\d{2}:\d{2}$)/, '$1')) : NaN
  if (typeof percent !== 'number' || !Number.isFinite(percent) || percent < 0 || !Number.isFinite(at) || at <= now) {
    return null
  }
  return { used_percentage: percent, resets_at: Math.floor(at / 1000) }
}

// $.session.usage() reads the current process's last observations, so polling it does not refresh idle chats.
// Prefer a recent account snapshot, with the process's still-active windows as the immediate fallback.
export function rateLimits(session: SessionUsage['rateLimits'], shared: UsageInput | null, now: number): Record<string, Period> {
  const result: Record<string, Period> = {}
  for (const limit of session) {
    const value = period(limit.percentUsed, limit.resetsAt, now)
    if (value) {
      result[limit.kind] = value
    }
  }
  const at = shared?.fetched_at_ms
  if (at === null || at === undefined || !Number.isSafeInteger(at) || now - at > STALE_MS || at - now > AHEAD_MS) {
    return result
  }
  for (const kind of ['five_hour', 'seven_day']) {
    const raw = shared?.body?.[kind]
    // A null/missing window cannot suppress a valid fallback: it may have opened after that snapshot.
    const window = obj(raw)
    const value = period(window?.utilization, window?.resets_at, now)
    if (value && (!result[kind] || value.resets_at + RESET_SLOP_SECONDS >= result[kind].resets_at)) {
      result[kind] = value
    }
  }
  return result
}
