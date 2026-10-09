import type { HttpResponse } from 'claude-code'

import { dataPath, obj } from './util'
import type { Json } from './util'

export const USAGE_URL = 'https://api.anthropic.com/api/oauth/usage'
export const PROFILE_URL = 'https://api.anthropic.com/api/oauth/profile'
export const USAGE_HEADERS = { 'anthropic-beta': 'oauth-2025-04-20' }
// Claude Code keeps its own snapshot for a minute too. Polling every 30s drew 429s within minutes.
export const FETCH_EVERY_MS = 60_000
// Without a verified account, requests cannot be coordinated across chats. Keep the private fallback conservative.
export const PRIVATE_FETCH_EVERY_MS = 5 * 60_000
// Login handles freeze the credentials they were minted with; a still-valid old token does not necessarily get a 401.
export const REAUTHORIZE_EVERY_MS = 15 * 60_000
// After a 429 the endpoint kept answering 429 while it was polled, so the wait grows to half an hour.
const MAX_BACKOFF_MS = 30 * 60_000
const BACKOFF_STEPS_MS = [5 * 60_000, 10 * 60_000, 20 * 60_000, MAX_BACKOFF_MS]

// Shared by chats on the same account, so the endpoint sees about one request a minute however many are open. Two chats that find
// it due within the same few milliseconds can both fetch, as the fs API has no rename or lock. For the same reason a
// torn read counts as no file.
export type UsageFile = {
  fetched_at_ms: number | null
  body: Json | null
  backoff_until_ms: number
  backoff_ms: number
}

// Sent whenever this chat holds a login, empty until a body lands, so the binary never falls back to cookies for it.
export type UsageInput = { fetched_at_ms: number | null; body: Json | null }

// One chat's own share. The handle is minted once and reused, since each plugin holds at most four. `nextAt` keeps
// this chat to the shared pace when the file cannot be written. `last` stands in for a torn read.
export type UsageMemo = {
  handle: string | null
  account: string | null
  profileNextAt: number
  profileBackoffMs: number
  backoffMs: number
  reauthorizeAt: number
  off: boolean
  authFailed: boolean
  polling: boolean
  nextAt: number
  last: UsageInput
}

export const newUsageMemo = (): UsageMemo => ({
  handle: null,
  account: null,
  profileNextAt: 0,
  profileBackoffMs: 0,
  backoffMs: 0,
  reauthorizeAt: 0,
  off: false,
  authFailed: false,
  polling: false,
  nextAt: 0,
  last: { fetched_at_ms: null, body: null },
})

export const EMPTY_USAGE: UsageFile = { fetched_at_ms: null, body: null, backoff_until_ms: 0, backoff_ms: 0 }

export const usagePath = (home: string, account: string) => dataPath(home, `cache/plugin-usage-${account}.json`)

// Only identifiers from this handle's authenticated profile may select a shared cache. Global Claude settings can
// belong to a different login than an already-open chat. UUIDs also keep server data out of directory components.
export function usageAccount(text: string): string | null {
  let value
  try {
    value = obj(JSON.parse(text))
  } catch {
    return null
  }
  const account = obj(value?.account)?.uuid
  const organization = obj(value?.organization)?.uuid
  const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
  return typeof account === 'string' && uuid.test(account) && typeof organization === 'string' && uuid.test(organization)
    ? `${organization.toLowerCase()}.${account.toLowerCase()}`
    : null
}

// The binary reads these as i64 and fails the whole line on anything else.
const num = (v: unknown): number | null => (Number.isSafeInteger(v) ? (v as number) : null)

export function parseUsageFile(text: string): UsageFile | null {
  let v: unknown
  try {
    v = JSON.parse(text)
  } catch {
    return null
  }
  const o = obj(v)
  if (!o) {
    return null
  }
  return {
    fetched_at_ms: num(o.fetched_at_ms),
    body: obj(o.body) ?? null,
    backoff_until_ms: num(o.backoff_until_ms) ?? 0,
    backoff_ms: num(o.backoff_ms) ?? 0,
  }
}

// A wait further off than any this plugin sets was dated before the clock stepped back, and would hold every chat until
// the clock caught up.
export const waiting = (until: number, now: number): boolean => now < until && until - now <= MAX_BACKOFF_MS

export function fetchDue(file: UsageFile, now: number): boolean {
  const at = file.fetched_at_ms
  const stale = at === null || now - at >= FETCH_EVERY_MS || at - now > FETCH_EVERY_MS
  return stale && !waiting(file.backoff_until_ms, now)
}

// The engine's own refusals: nonessential traffic is off, the organization's policy blocks plugins' network access, or
// the session withholds its credential. Timeouts and network errors say `aborted` or `failed` and are retried.
export const REFUSED = /\$\.http\.fetch: refused: /
// The handle was evicted by newer ones the plugin minted, or the engine forgot it.
export const UNKNOWN_HANDLE = /\$\.http\.fetch: unknown auth handle/

// Written before the request, so a chat polling meanwhile waits, and a failure is not retried on every refresh.
export const claimed = (file: UsageFile, now: number): UsageFile => ({
  ...file,
  backoff_until_ms: now + FETCH_EVERY_MS,
})

const nextBackoff = (ms: number): number => BACKOFF_STEPS_MS.find(step => step > ms) ?? MAX_BACKOFF_MS

export function backedOff(file: UsageFile, now: number, retryAfter?: string): UsageFile {
  const step = nextBackoff(file.backoff_ms)
  const seconds = Number(retryAfter)
  // The endpoint has answered retry-after: 0 while still limiting, so only a positive one is believed.
  const wait = Number.isFinite(seconds) && seconds > 0 ? Math.max(step, seconds * 1000) : step
  return { ...file, backoff_until_ms: now + Math.min(wait, MAX_BACKOFF_MS), backoff_ms: step }
}

// The file a response leaves behind, or null to leave the claim standing until the next minute.
export function answered(file: UsageFile, res: HttpResponse, now: number): UsageFile | null {
  if (res.status === 429) {
    return backedOff(file, now, res.headers['retry-after'])
  }
  if (!res.ok) {
    return null
  }
  let body: Json | undefined
  try {
    body = obj(JSON.parse(res.text))
  } catch {
    return null
  }
  return body ? { fetched_at_ms: now, body, backoff_until_ms: 0, backoff_ms: 0 } : null
}

export const shown = (file: UsageFile): UsageInput => ({ fetched_at_ms: file.fetched_at_ms, body: file.body })
