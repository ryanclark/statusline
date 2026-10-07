export type Span = {
  text: string
  fg?: string
  bg?: string
  bold?: boolean
  dim?: boolean
  italic?: boolean
  underline?: boolean
  strikethrough?: boolean
  inverse?: boolean
  href?: string
}

export type Rendered = { rows: Span[][]; error?: string }

export type LastUsage = {
  input_tokens: number
  output_tokens: number
  cache_creation_input_tokens: number
  cache_read_input_tokens: number
}

export type MissCause = {
  causes: string[]
  tools_added?: number
  tools_removed?: number
  system_char_delta?: number
}

// Only the difference between two main-loop system prompts is reported, so the text itself is not kept.
export type ComposeShape = { tools: string[]; chars: number }

export type CacheTtl = '5m' | '1h'

// What the plugin learns from watching main-loop requests, since no API reports it directly. It fills the input's
// `prompt_cache`, which mirrors the object Claude Code documents for a statusLine command, so some fields go unread.
export type Tracker = {
  lastUsage: LastUsage | null
  lastRequestAt: number | null
  cacheObserved: boolean
  cacheTtl: CacheTtl | null
  requests: number
  cacheReadTokens: number
  cacheWriteTokens: number
  promptTokens: number
  effort: string | null
  // The previous main-loop request, kept across a model switch or compaction unlike `lastRequestAt` and `lastUsage`.
  prevSentAt: number | null
  prevModel: string | null
  prevCached: number
  compactedSinceLast: boolean
  compose: ComposeShape | null
  misses: number
  expectedRebuilds: number
  missTimes: number[]
  missRecacheTokens: number
  lastMissAt: number | null
  lastMissCause: MissCause | null
  missCauses: Record<string, number>
  lastStopReason: string | null
}

export type ToolInFlight = { id: string; tool: string; detail: string | null; started_at_ms: number }

export type TaskItem = { subject: string; status: string; activeForm: string | null }

// PermissionRequest carries no tool_use_id, so a wait is known only by its tool and loop.
export type PendingPermission = { tool: string; since_ms: number; agent: string | null }

export type LastError = { kind: string; detail: string | null; at_ms: number }

export type TurnInfo = { started_at_ms: number | null; last_duration_ms: number | null; ended_at_ms: number | null }

export type TodoProgress = { done: number; total: number; active: string | null }

export type Compaction = {
  count: number
  last_at_ms: number | null
  tokens_before: number | null
  tokens_after: number | null
  running_since_ms: number | null
  trigger: 'auto' | 'manual' | null
}

// What a background shell, monitor or workflow is shown as. Subagents are left to the agents segment.
export type BackgroundTask = { type: string; description: string | null }

export type Autocompact = { enabled: boolean; headroom_tokens: number | null }

// Sources of the input's `mod` object, apart from the agents and autocompact figures polled on each refresh.
export type Live = {
  tools: ToolInFlight[]
  turn: TurnInfo | null
  // Oldest first. Parallel calls and subagents can each wait on the user at once.
  permissions: PendingPermission[]
  lastError: LastError | null
  todos: TodoProgress | null
  // Task tools report one task per call, so the list is rebuilt here to count it.
  tasks: Record<string, TaskItem>
  compaction: Compaction | null
  // Keyed by task id. Tool results add and remove tasks as they happen, and each Stop's snapshot replaces the lot.
  background: Record<string, BackgroundTask>
}

declare module 'claude-code' {
  interface PluginState {
    statusline: { rendered: Rendered | null; tracker: Tracker; live: Live }
  }
}
