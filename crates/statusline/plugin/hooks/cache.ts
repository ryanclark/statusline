import type { CacheTtl, ComposeShape, LastUsage, MissCause, Tracker } from '../types'

export const TTL_MS: Record<CacheTtl, number> = { '5m': 5 * 60_000, '1h': 60 * 60_000 }

// A system prompt change keeps the tools prefix cached, so a miss still reads some cache. Half allows for the
// breakpoint moving.
const MISS_RATIO = 0.5
// Expected rebuilds, which Claude Code counts separately from misses.
const EXPECTED_CAUSES = new Set(['model_changed', 'compacted'])
// Bounds the session state on a pathological session. The binary only windows the recent ones.
const MISS_TIMES_CAP = 1000

export type Step = {
  sentAt: number
  model: string
  usage: LastUsage
  compose: ComposeShape | null
  effort: string | null
}

export const EMPTY_TRACKER: Tracker = {
  lastUsage: null,
  lastRequestAt: null,
  cacheObserved: false,
  cacheTtl: null,
  requests: 0,
  cacheReadTokens: 0,
  cacheWriteTokens: 0,
  promptTokens: 0,
  effort: null,
  prevSentAt: null,
  prevModel: null,
  prevCached: 0,
  compactedSinceLast: false,
  compose: null,
  misses: 0,
  expectedRebuilds: 0,
  missTimes: [],
  missRecacheTokens: 0,
  lastMissAt: null,
  lastMissCause: null,
  missCauses: {},
  lastStopReason: null,
}

function diffCompose(before: ComposeShape | null, after: ComposeShape | null): MissCause {
  const out: MissCause = { causes: [] }
  if (!before || !after) {
    return out
  }
  const old = new Set(before.tools)
  const now = new Set(after.tools)
  const added = after.tools.filter(n => !old.has(n)).length
  const removed = before.tools.filter(n => !now.has(n)).length
  if (added + removed > 0) {
    out.causes.push('tools_changed')
    out.tools_added = added
    out.tools_removed = removed
  }
  if (after.chars !== before.chars) {
    out.causes.push('system_changed')
    out.system_char_delta = after.chars - before.chars
  }
  return out
}

// `ambiguous` means a subagent sent a request since the last main one or is still running, so the compose this request
// claimed may not be the main loop's.
export function observeStep(t: Tracker, s: Step, ttlMs: number, ambiguous: boolean): Tracker {
  const u = s.usage
  const read = u.cache_read_input_tokens
  const write = u.cache_creation_input_tokens
  const next: Tracker = {
    ...t,
    // Copied field by field, since the step's usage is the engine's own and carries more than the binary reads.
    lastUsage: {
      input_tokens: u.input_tokens,
      output_tokens: u.output_tokens,
      cache_creation_input_tokens: write,
      cache_read_input_tokens: read,
    },
    // Send time rather than arrival, so the expiry errs early by at most one request's duration.
    lastRequestAt: s.sentAt,
    cacheObserved: t.cacheObserved || read + write > 0,
    requests: t.requests + 1,
    cacheReadTokens: t.cacheReadTokens + read,
    cacheWriteTokens: t.cacheWriteTokens + write,
    promptTokens: t.promptTokens + u.input_tokens + read + write,
    effort: s.effort ?? t.effort,
    prevSentAt: s.sentAt,
    prevModel: s.model,
    prevCached: read + write,
    compactedSinceLast: false,
    // Left unset after an ambiguous request, so the next diff waits for a compose the main loop surely claimed.
    compose: ambiguous ? null : (s.compose ?? t.compose),
  }
  if (t.prevCached <= 0 || read >= t.prevCached * MISS_RATIO) {
    return next
  }
  const seen: string[] = []
  if (t.prevSentAt !== null && s.sentAt - t.prevSentAt > ttlMs) {
    seen.push('ttl_expired')
  }
  if (t.prevModel !== null && s.model !== t.prevModel) {
    seen.push('model_changed')
  }
  if (t.compactedSinceLast) {
    seen.push('compacted')
  }
  const diff: MissCause = ambiguous ? { causes: [] } : diffCompose(t.compose, s.compose)
  const cause: MissCause = { ...diff, causes: [...seen, ...diff.causes] }
  if (cause.causes.some(c => EXPECTED_CAUSES.has(c))) {
    return { ...next, expectedRebuilds: t.expectedRebuilds + 1 }
  }
  const at = Math.floor(s.sentAt / 1000)
  const missCauses = { ...t.missCauses }
  for (const c of cause.causes) {
    missCauses[c] = (missCauses[c] ?? 0) + 1
  }
  return {
    ...next,
    misses: t.misses + 1,
    missTimes: [...t.missTimes, at].slice(-MISS_TIMES_CAP),
    missRecacheTokens: t.missRecacheTokens + write,
    lastMissAt: at,
    lastMissCause: cause,
    missCauses,
  }
}
