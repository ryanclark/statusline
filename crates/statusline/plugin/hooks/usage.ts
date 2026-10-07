import type { HttpResponse } from 'claude-code'

import { obj } from './util'
import type { Json } from './util'

export const USAGE_URL = 'https://api.anthropic.com/api/oauth/usage'
export const USAGE_HEADERS = { 'anthropic-beta': 'oauth-2025-04-20' }
// Claude Code keeps its own snapshot for a minute too. Polling every 30s drew 429s within minutes.
export const FETCH_EVERY_MS = 60_000
// After a 429 the endpoint kept answering 429 while it was polled, so the wait grows to half an hour.
const MAX_BACKOFF_MS = 30 * 60_000
export const BACKOFF_STEPS_MS = [5 * 60_000, 10 * 60_000, 20 * 60_000, MAX_BACKOFF_MS]

// Shared by every open chat, so the endpoint sees one request a minute however many are open. The fs API has no rename
// to write it atomically, so a torn read counts as no file.
export type UsageFile = {
  fetched_at_ms: number | null
  body: Json | null
  backoff_until_ms: number
  backoff_ms: number
}

export type UsageInput = { fetched_at_ms: number; body: Json }

// One chat's own share. The handle is minted once and reused, since each plugin holds at most four.
export type UsageMemo = { handle: string | null; off: boolean; polling: boolean; reauthorized: boolean }

export const newUsageMemo = (): UsageMemo => ({ handle: null, off: false, polling: false, reauthorized: false })

export const EMPTY_USAGE: UsageFile = { fetched_at_ms: null, body: null, backoff_until_ms: 0, backoff_ms: 0 }

export const usagePath = (home: string) => `${home}/.statusline/cache/plugin-usage.json`

const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null)

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

export function fetchDue(file: UsageFile, now: number): boolean {
  return now >= file.backoff_until_ms && (file.fetched_at_ms === null || now - file.fetched_at_ms >= FETCH_EVERY_MS)
}

// Written before the request, so a chat polling meanwhile waits, and a failure is not retried on every refresh.
export const claimed = (file: UsageFile, now: number): UsageFile => ({
  ...file,
  backoff_until_ms: now + FETCH_EVERY_MS,
})

export const nextBackoff = (ms: number): number =>
  BACKOFF_STEPS_MS.find(step => step > ms) ?? MAX_BACKOFF_MS

// The file a response leaves behind, or null to leave the claim standing until the next minute.
export function answered(file: UsageFile, res: HttpResponse, now: number): UsageFile | null {
  if (res.status === 429) {
    const step = nextBackoff(file.backoff_ms)
    const retryAfter = Number(res.headers['retry-after'])
    // The endpoint has answered `retry-after: 0` while still limiting, so only a positive one is believed.
    const wait = Number.isFinite(retryAfter) && retryAfter > 0 ? Math.max(step, retryAfter * 1000) : step
    return { ...file, backoff_until_ms: now + wait, backoff_ms: step }
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

export const shown = (file: UsageFile | null): UsageInput | null =>
  file?.body && file.fetched_at_ms !== null ? { fetched_at_ms: file.fetched_at_ms, body: file.body } : null
