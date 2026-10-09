import type { AgentInfo, SessionUsage } from 'claude-code'

import type { Autocompact, CacheTtl, Live, Tracker } from '../types'
import { agentCounts } from './activity'
import { TTL_MS } from './cache'
import type { UsageInput } from './usage'

export type Sources = {
  now: number
  id: string
  cwd: string
  root: string
  model: string
  version: string
  usage: SessionUsage
  tracker: Tracker
  live: Live
  agents: readonly AgentInfo[]
  defaultTtl: CacheTtl
  autocompact: Autocompact | null
  accountUsage: UsageInput | null
  showUpdate?: boolean
}

export function autocompactOf(b: SessionUsage['context']['breakdown']): Autocompact | null {
  if (!b) {
    return null
  }
  return {
    enabled: b.isAutoCompactEnabled,
    headroom_tokens:
      b.isAutoCompactEnabled && b.autoCompactThreshold !== undefined
        ? Math.max(0, b.autoCompactThreshold - b.totalTokens)
        : null,
  }
}

// Rebuilds Claude Code's statusLine input from the mod API, which does not expose it. Fields with no source (vim,
// fast_mode, thinking, PR) are left out, and the binary saves the result as the session's snapshot.
export function inputJson(src: Sources): string {
  const { now, usage, tracker: t, live: l } = src
  const rateLimits: Record<string, { used_percentage: number; resets_at: number }> = {}
  for (const limit of usage.rateLimits) {
    const resetsAt = limit.resetsAt ? Date.parse(limit.resetsAt) : NaN
    // resets_at is required by the binary's parser, so a window without one would fail the whole input.
    if (Number.isFinite(resetsAt)) {
      rateLimits[limit.kind] = { used_percentage: limit.percentUsed, resets_at: Math.floor(resetsAt / 1000) }
    }
  }

  const tokens = usage.context.tokens ?? 0
  const ttl = t.cacheTtl ?? src.defaultTtl
  const expiresAt = t.lastRequestAt === null ? null : t.lastRequestAt + TTL_MS[ttl]
  const warm = expiresAt !== null && now < expiresAt
  const [waiting] = l.permissions

  return JSON.stringify({
    session_id: src.id,
    cwd: src.cwd,
    workspace: { current_dir: src.cwd, project_dir: src.root },
    // display_name is left empty so the binary derives it from the id, which is all $.session.model() reports.
    model: { id: src.model, display_name: '' },
    version: src.version,
    cost: { total_cost_usd: usage.cost?.usd ?? 0, total_duration_ms: Math.max(0, now - usage.startedAt) },
    context_window: {
      context_window_size: usage.context.window,
      total_input_tokens: tokens,
      total_output_tokens: t.lastUsage?.output_tokens ?? 0,
      current_usage: t.lastUsage,
      ...(usage.context.percent === undefined
        ? {}
        : { used_percentage: usage.context.percent, remaining_percentage: 100 - usage.context.percent }),
    },
    exceeds_200k_tokens: tokens > 200_000,
    rate_limits: rateLimits,
    ...(t.effort ? { effort: { level: t.effort } } : {}),
    ...(t.cacheObserved
      ? {
          prompt_cache: {
            warm,
            caching_observed: true,
            ttl,
            expires_at: warm ? Math.floor(expiresAt / 1000) : null,
            requests: t.requests,
            misses: t.misses,
            expected_rebuilds: t.expectedRebuilds,
            hit_ratio: t.promptTokens > 0 ? t.cacheReadTokens / t.promptTokens : null,
            cache_write_tokens: t.cacheWriteTokens,
            miss_recache_tokens: t.missRecacheTokens,
            last_miss_at: t.lastMissAt,
            last_miss_cause: t.lastMissCause,
            miss_causes: t.missCauses,
            miss_times: t.missTimes,
            // The compacted window caches a new, smaller prefix, so the old size says nothing until the next request.
            recache_tokens_if_cold: t.prevCached > 0 && !t.compactedSinceLast ? t.prevCached : null,
          },
        }
      : {}),
    mod: {
      show_update: src.showUpdate === true,
      tools: l.tools.map(({ tool, detail, started_at_ms }) => ({ tool, detail, started_at_ms })),
      turn: l.turn,
      permission: waiting ? { tool: waiting.tool, since_ms: waiting.since_ms } : null,
      last_error: l.lastError,
      todos: l.todos,
      agents: agentCounts(src.agents),
      background_tasks: Object.values(l.background ?? {}),
      compaction: l.compaction,
      autocompact: src.autocompact,
      ...(src.accountUsage ? { usage: src.accountUsage } : {}),
    },
  })
}
