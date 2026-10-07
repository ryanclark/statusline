import type { AgentInfo } from 'claude-code'

import type { BackgroundTask, Compaction, LastError, Live, PendingPermission, TaskItem, TodoProgress } from '../types'
import { cut, obj, plain, str } from './util'
import type { Json } from './util'

// Detail shown beside a running tool, in the order the built-in tools name their main argument.
const DETAIL_KEYS = ['command', 'file_path', 'notebook_path', 'pattern', 'url', 'query', 'description'] as const
const DETAIL_MAX = 80

export function detailOf(args: Json): string | null {
  for (const k of DETAIL_KEYS) {
    const v = str(args[k])
    const s = v === null ? '' : plain(v)
    if (s) {
      return cut(s, DETAIL_MAX, '…')
    }
  }
  return null
}

export const EMPTY_LIVE: Live = {
  tools: [],
  turn: null,
  permissions: [],
  lastError: null,
  todos: null,
  tasks: {},
  compaction: null,
  background: {},
}

export const TODO_TOOLS: ReadonlySet<string> = new Set(['TodoWrite', 'TaskCreate', 'TaskUpdate', 'TaskList', 'TaskGet'])

function progress(items: TaskItem[]): TodoProgress | null {
  if (items.length === 0) {
    return null
  }
  const active = items.find(i => i.status === 'in_progress')
  return {
    done: items.filter(i => i.status === 'completed').length,
    total: items.length,
    active: active ? (active.activeForm ?? active.subject) : null,
  }
}

// Folds one todo or task tool result into the list. Returns undefined when the result changes nothing.
export function foldTodos(l: Live, tool: string, args: Json, result: Json, main: boolean): Live | undefined {
  let tasks: Record<string, TaskItem> | undefined
  switch (tool) {
    case 'TodoWrite': {
      // A subagent's todo list is its own and would replace the one the main loop is working through.
      if (!main || !Array.isArray(result.newTodos)) {
        return undefined
      }
      const items = result.newTodos
        .map(obj)
        .filter((t): t is Json => t !== undefined)
        .map(t => ({
          subject: str(t.content) ?? '',
          status: str(t.status) ?? 'pending',
          activeForm: str(t.activeForm),
        }))
      return { ...l, todos: progress(items) }
    }
    // The task list is shared by every loop of the session, so a subagent's updates count too.
    case 'TaskCreate':
    case 'TaskGet': {
      const task = obj(result.task)
      const id = str(task?.id)
      if (id !== null) {
        tasks = {
          ...l.tasks,
          [id]: {
            subject: str(task?.subject) ?? '',
            status: tool === 'TaskCreate' ? 'pending' : (str(task?.status) ?? 'pending'),
            activeForm: str(args.activeForm) ?? l.tasks[id]?.activeForm ?? null,
          },
        }
      }
      break
    }
    case 'TaskUpdate': {
      const id = str(args.taskId)
      if (id !== null && result.success !== false) {
        tasks = { ...l.tasks }
        if (args.status === 'deleted') {
          delete tasks[id]
        } else {
          const prev = tasks[id] ?? { subject: '', status: 'pending', activeForm: null }
          tasks[id] = {
            subject: str(args.subject) ?? prev.subject,
            status: str(args.status) ?? prev.status,
            activeForm: str(args.activeForm) ?? prev.activeForm,
          }
        }
      }
      break
    }
    case 'TaskList': {
      if (!Array.isArray(result.tasks)) {
        break
      }
      // The listing carries no activeForm, so the one a create or update gave survives it.
      tasks = {}
      for (const t of result.tasks.map(obj)) {
        const id = str(t?.id)
        if (id !== null) {
          tasks[id] = {
            subject: str(t?.subject) ?? '',
            status: str(t?.status) ?? 'pending',
            activeForm: l.tasks[id]?.activeForm ?? null,
          }
        }
      }
      break
    }
  }
  return tasks === undefined ? undefined : { ...l, tasks, todos: progress(Object.values(tasks)) }
}

export const BACKGROUND_TOOLS: ReadonlySet<string> = new Set(['Bash', 'Monitor', 'Workflow', 'TaskStop'])

// The kinds a tool result can add. A remote agent or teammate in Stop's list is agent work, not a local task.
const BACKGROUND_KINDS: ReadonlySet<string> = new Set(['shell', 'monitor', 'workflow'])

// The same rule for a tool's own result and a Stop snapshot, so the line does not change when the snapshot lands.
function backgroundTask(type: string, description: unknown, command: unknown, name: unknown): BackgroundTask {
  const label = (type === 'workflow' ? str(name) : null) || str(description) || str(command)
  const text = label === null ? '' : plain(label)
  return { type, description: text ? cut(text, DETAIL_MAX, '…') : null }
}

// Folds one tool result into the background tasks. Returns undefined when the result changes nothing.
export function foldBackground(l: Live, tool: string, args: Json, result: Json): Live | undefined {
  // A session that ran an older build of the plugin keeps its live value across the reload, without this field.
  const background = l.background ?? {}
  let id: string | null
  let task: BackgroundTask
  switch (tool) {
    case 'Bash':
      id = str(result.backgroundTaskId)
      // A synchronous subagent's shell is killed with that agent's answer, so it is not work the session waits on.
      if (result.backgroundEndsWithFinalResponse === true) {
        return undefined
      }
      task = backgroundTask('shell', args.description, args.command, null)
      break
    case 'Monitor':
      id = str(result.taskId)
      task = backgroundTask('monitor', args.description, args.command, null)
      break
    case 'Workflow':
      if (result.taskType === 'remote_agent' || result.status === 'remote_launched') {
        return undefined
      }
      id = str(result.taskId)
      task = backgroundTask('workflow', null, null, result.workflowName ?? args.name)
      break
    case 'TaskStop': {
      const stopped = str(result.task_id)
      return stopped === null ? undefined : withoutBackground(l, [stopped])
    }
    default:
      return undefined
  }
  return id === null ? undefined : { ...l, background: { ...background, [id]: task } }
}

// Stop's list of the session's work still in flight. Undefined when the event carries none, as an older engine's.
export function backgroundSnapshot(list: unknown): Record<string, BackgroundTask> | undefined {
  if (!Array.isArray(list)) {
    return undefined
  }
  const background: Record<string, BackgroundTask> = {}
  for (const t of list.map(obj)) {
    const id = str(t?.id)
    const type = str(t?.type)
    if (t && id !== null && type !== null && BACKGROUND_KINDS.has(type)) {
      background[id] = backgroundTask(type, t.description, t.command, t.name)
    }
  }
  return background
}

export function withoutBackground(l: Live, ids: readonly string[]): Live | undefined {
  const background = l.background ?? {}
  const gone = ids.filter(id => id in background)
  if (gone.length === 0) {
    return undefined
  }
  const rest = { ...background }
  for (const id of gone) {
    delete rest[id]
  }
  return { ...l, background: rest }
}

const NOTIFICATION = /<task-notification>([\s\S]*?)<\/task-notification>/g
const ENDED = new Set(['completed', 'failed', 'killed'])

// The tasks a notification prompt reports ended. Several queued notifications can arrive as one prompt, and one with
// any other status, or none, may come from a monitor that keeps running.
export function endedTasks(text: string): string[] {
  const ids: string[] = []
  for (const [, body = ''] of text.matchAll(NOTIFICATION)) {
    const id = /<task-id>([^<]+)<\/task-id>/.exec(body)?.[1]?.trim()
    const status = /<status>([^<]+)<\/status>/.exec(body)?.[1]?.trim()
    if (id && status && ENDED.has(status)) {
      ids.push(id)
    }
  }
  return ids
}

// PermissionRequest carries no tool_use_id, so a call's end settles the oldest wait on the same tool in the same loop.
export function withoutPermission(list: PendingPermission[], tool: string, agent: string | null): PendingPermission[] {
  const i = list.findIndex(p => p.tool === tool && p.agent === agent)
  return i < 0 ? list : [...list.slice(0, i), ...list.slice(i + 1)]
}

// StopFailure and turn.complete come in no documented order, so whichever is second keeps the more specific word.
export function turnError(
  l: Live,
  reason: 'answer' | 'aborted' | 'refusal' | 'error',
  refusal: string | null,
  stopReason: string | null,
  now: number,
  durationMs: number,
): LastError | null {
  switch (reason) {
    case 'answer':
      return stopReason === 'max_tokens' ? { kind: 'max_tokens', detail: null, at_ms: now } : null
    case 'aborted':
      return { kind: 'aborted', detail: null, at_ms: now }
    case 'refusal':
      return { kind: 'refusal', detail: refusal, at_ms: now }
    case 'error': {
      const startedAt = l.turn?.started_at_ms ?? now - durationMs
      return l.lastError && l.lastError.at_ms >= startedAt ? l.lastError : { kind: 'error', detail: null, at_ms: now }
    }
  }
}

export const NO_COMPACTION: Compaction = {
  count: 0,
  last_at_ms: null,
  tokens_before: null,
  tokens_after: null,
  running_since_ms: null,
  trigger: null,
}

// Waiting agents still hold work, so they count as running. Ended ones linger in the list for a while and are left out.
export const RUNNING: ReadonlySet<AgentInfo['status']> = new Set(['pending', 'running', 'waiting'])

export function agentCounts(list: readonly AgentInfo[]): { running: number; idle: number } | null {
  // A waiting agent is held on its own background work or an approval, which Claude Code's panel shows as done.
  const running = list.filter(a => a.status === 'pending' || a.status === 'running').length
  const idle = list.filter(a => a.status === 'idle' || a.status === 'waiting').length
  return running + idle === 0 ? null : { running, idle }
}

// The agent panel's command is told an agent held on its own background work has completed, so the binary takes the
// waiting ones from this map instead (subagent.rs). Only waiting agents are kept, as the binary reads nothing else and
// a finished agent stays listed for the rest of the session. Sorted so a reordered list is not a change.
export function agentStatuses(list: readonly AgentInfo[]): Record<string, AgentInfo['status']> {
  const out: Record<string, AgentInfo['status']> = {}
  for (const a of [...list].sort((x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0))) {
    if (a.status === 'waiting') {
      out[a.id] = a.status
    }
  }
  return out
}

// The binary trusts the map for 30s, so an unchanged one is written again well before then.
export const AGENTS_REWRITE_MS = 10_000
