import { atom, read, update } from 'claude-code'
import type {
  AgentInfo,
  EngineInterface,
  PluginOptions,
  PromptSubmitInput,
  PromptSubmitResult,
  Register,
} from 'claude-code'

import type { Autocompact, CacheTtl, Compaction, ComposeShape, Rendered } from '../types'
import {
  agentStatuses,
  AGENTS_REWRITE_MS,
  BACKGROUND_TOOLS,
  backgroundSnapshot,
  detailOf,
  EMPTY_LIVE,
  endedTasks,
  foldBackground,
  foldTodos,
  NO_COMPACTION,
  RUNNING,
  TODO_TOOLS,
  turnError,
  withoutBackground,
  withoutPermission,
} from './activity'
import { isSpanRows, MISSING, NO_NEEDS, NOT_FOUND, PLUGIN_DATA_FLAG, REQUIRED_FLAGS, tooOldMessage, UNKNOWN_FLAG, WIDTH_FLAG } from './binary'
import type { Needs, Verdict } from './binary'
import { EMPTY_TRACKER, observeStep, TTL_MS } from './cache'
import { blank, draw, hintWidth } from './draw'
import { autocompactOf, inputJson } from './input'
import {
  answered,
  claimed,
  fetchDue,
  EMPTY_USAGE,
  FETCH_EVERY_MS,
  newUsageMemo,
  parseUsageFile,
  REFUSED,
  shown,
  UNKNOWN_HANDLE,
  USAGE_HEADERS,
  USAGE_URL,
  usagePath,
  waiting,
} from './usage'
import type { UsageFile, UsageInput, UsageMemo } from './usage'
import { cut, dataPath, obj, plain, str } from './util'
import type { Json } from './util'

const rendered = atom({ plugin: 'statusline', key: 'rendered' } as const, null)
const tracker = atom({ plugin: 'statusline', key: 'tracker' } as const, EMPTY_TRACKER)
const live = atom({ plugin: 'statusline', key: 'live' } as const, EMPTY_LIVE)
const mode = atom({ plugin: 'statusline', key: 'mode' } as const, null)

type State = {
  binary: string
  placement: 'below' | 'above'
  defaultTtl: CacheTtl
  intervalMs: number
  heartbeatMs: number
  inFlight: boolean
  again: boolean
  last: string
  breakdownAt: number
  autocompact: Autocompact | null
  // The last composed system prompt. The compose event names no loop, so only a main-loop request directly after it
  // claims it.
  pendingCompose: ComposeShape | null
  // A subagent request may have taken the main loop's compose or left its own.
  composeAmbiguous: boolean
  handshake: Promise<Verdict> | null
  // Whether the binary can drop whole segments to fit, and the width the line last had. A render hook may not run the
  // binary, so the next refresh passes the width on.
  fits: boolean
  reportsNeeds: boolean
  needs: Needs
  userTurns: number | null
  fitWidth: number | undefined
  tooOld: string | null
  usage: UsageMemo
  // The agents map last written for the binary and when, so an unchanged map is not written every tick.
  agentsKey: string
  agentsAt: number
}

// A session that never had a waiting agent writes no file, which the binary reads the same as an empty map.
const NO_AGENTS = '{}'

// Session ids become file names, so only the plain tokens the binary itself accepts are written.
const SESSION_ID = /^[A-Za-z0-9_-]{1,128}$/

// Outlives three missed ticks. The cap bounds how long a plugin that stops silently leaves the session with no line.
const HEARTBEAT_MAX_MS = 30_000

function fresh(options: PluginOptions): State {
  const configured = Number(options.intervalMs)
  const intervalMs = Number.isFinite(configured) && configured > 0 ? Math.max(250, Math.round(configured)) : 1000
  return {
    binary: String(options.binary || 'statusline'),
    placement: options.placement === 'above' ? 'above' : 'below',
    defaultTtl: options.cacheTtl === '5m' ? '5m' : '1h',
    intervalMs,
    heartbeatMs: Math.min(HEARTBEAT_MAX_MS, Math.max(10_000, 3 * intervalMs + 2000)),
    inFlight: false,
    again: false,
    last: '',
    breakdownAt: -Infinity,
    autocompact: null,
    pendingCompose: null,
    composeAmbiguous: false,
    handshake: null,
    fits: false,
    reportsNeeds: false,
    needs: { usage: true, autocompact: true }, // Older binaries cannot report their layout's requirements.
    userTurns: null,
    fitWidth: undefined,
    tooOld: null,
    usage: newUsageMemo(),
    agentsKey: NO_AGENTS,
    agentsAt: -Infinity,
  }
}

let state = fresh({})

const BREAKDOWN_EVERY_MS = 10_000

async function buildInput($: EngineInterface): Promise<string> {
  const now = await $.clock.now()
  const due = state.needs.autocompact && now - state.breakdownAt >= BREAKDOWN_EVERY_MS
  if (due) {
    state.breakdownAt = now
  }
  const [id, cwd, root, model, version, usage, t, l, agents, turns] = await Promise.all([
    $.session.id(),
    $.session.cwd(),
    $.session.root(),
    $.session.model(),
    $.session.version(),
    due ? $.session.usage({ breakdown: 'summary' }) : $.session.usage(),
    read($, tracker),
    read($, live),
    $.agent.list(),
    state.userTurns === null ? Promise.resolve().then(() => $.session.turns()).catch(() => 1) : state.userTurns,
  ])
  // A prompt submitted while the first reading was pending must never be undone by that reading.
  state.userTurns = Math.max(state.userTurns ?? 0, turns)
  if (due) {
    state.autocompact = autocompactOf(usage.context.breakdown)
  }
  await writeAgents($, id, now, agents)
  return inputJson({
    now,
    id,
    cwd,
    root,
    model,
    version: version.version,
    usage,
    tracker: t,
    live: l,
    agents,
    defaultTtl: state.defaultTtl,
    autocompact: state.autocompact,
    accountUsage: state.reportsNeeds ? accountUsage(state.usage) : pollUsage($),
    showUpdate: state.userTurns === 0,
  })
}

async function tooOld($: EngineInterface, binary: string): Promise<string> {
  const [stat, out] = await Promise.all([
    $.fs.stat(binary, { resolve: true }).catch(() => undefined),
    $.process.run([binary, '--version'], { timeoutMs: 5000 }).catch(() => undefined),
  ])
  return tooOldMessage(out?.exitCode === 0 ? out.stdout : null, stat?.realPath ?? '')
}

// Checks `--help` for the flags rather than the version, since a source build can carry a release's number without
// that release's flags. Any other exit is left for the refresh to report.
async function probe($: EngineInterface, binary: string): Promise<Verdict> {
  let out
  try {
    out = await $.process.run([binary, '--help'], { timeoutMs: 5000 })
  } catch (err) {
    return MISSING.test(String(err))
      ? { error: NOT_FOUND, final: true }
      : { error: `statusline: ${String(err)}`, final: false }
  }
  const help = out.stdout
  state.fits = WIDTH_FLAG.test(help)
  state.reportsNeeds = PLUGIN_DATA_FLAG.test(help)
  if (state.reportsNeeds) {
    state.needs = { ...NO_NEEDS }
  }
  if (out.exitCode === 0 && !REQUIRED_FLAGS.every(flag => flag.test(help))) {
    return { error: await tooOld($, binary), final: true }
  }
  return null
}

// The file the binary's own heartbeat writes (session.rs), holding the expiry in epoch ms. Best effort, since every
// refresh writes it again through the binary.
async function writeHeartbeat($: EngineInterface, sessionId: string, expiresMs: number) {
  if (!SESSION_ID.test(sessionId)) {
    return
  }
  try {
    const home = await $.env.get('HOME')
    if (home) {
      await $.fs.write(dataPath(home, `sessions/${sessionId}.plugin`), String(Math.floor(expiresMs)))
    }
  } catch {
    // A missed write leaves the native line drawing until the next refresh, as if the plugin were not installed.
  }
}

async function writeAgents($: EngineInterface, sessionId: string, now: number, agents: readonly AgentInfo[]) {
  const statuses = agentStatuses(agents)
  const key = JSON.stringify(statuses)
  const unchanged = key === state.agentsKey && (key === NO_AGENTS || now - state.agentsAt < AGENTS_REWRITE_MS)
  if (unchanged || !SESSION_ID.test(sessionId)) {
    return
  }
  try {
    const home = await $.env.get('HOME')
    if (home) {
      const file = { written_at_ms: now, agents: statuses }
      await $.fs.write(dataPath(home, `sessions/${sessionId}.agents.json`), JSON.stringify(file))
      state.agentsKey = key
      state.agentsAt = now
    }
  } catch {
    // Left unmarked so the next refresh tries again. Until then the panel shows Claude Code's own status.
  }
}

// Null when nothing should be fetched yet, as a file that does not parse and was just written is another chat's
// write still under way.
async function readUsage($: EngineInterface, path: string, now: number): Promise<UsageFile | null> {
  let text: string
  try {
    text = await $.fs.read(path)
  } catch {
    return EMPTY_USAGE
  }
  const file = parseUsageFile(text)
  if (file) {
    return file
  }
  const stat = await $.fs.stat(path).catch(() => undefined)
  return stat && now - stat.mtimeMs < FETCH_EVERY_MS ? null : EMPTY_USAGE
}

async function writeUsage($: EngineInterface, path: string, file: UsageFile) {
  try {
    await $.fs.write(path, JSON.stringify(file))
  } catch {
    // The next chat to find the file due fetches again.
  }
}

async function authorizeUsage($: EngineInterface, memo: UsageMemo): Promise<string | null> {
  let auth
  try {
    auth = await $.session.authorize()
  } catch {
    // A failed call must allow the cookie fallback, without retrying authorization on every render.
    memo.handle = null
    memo.authFailed = true
    memo.nextAt = await $.clock.now() + FETCH_EVERY_MS
    return null
  }
  memo.authFailed = false
  // An API key or a third-party provider has no claude.ai usage, and the binary keeps its cookie path for those.
  memo.handle = auth?.kind === 'bearer' ? auth.handle : null
  memo.off = memo.handle === null
  return memo.handle
}

async function fetchUsage($: EngineInterface, memo: UsageMemo, path: string, file: UsageFile): Promise<boolean> {
  let handle = memo.handle
  if (handle === null || !state.needs.usage) {
    return false
  }
  const start = await $.clock.now()
  if (!state.needs.usage) {
    return false
  }
  memo.nextAt = start + FETCH_EVERY_MS
  await writeUsage($, path, claimed(file, start))
  if (!state.needs.usage) {
    return false
  }
  let res
  try {
    res = await $.http.fetch(USAGE_URL, { auth: handle, headers: USAGE_HEADERS })
    // A handle keeps the token it was minted with, so one minted before the session refreshed its login fails until
    // it is minted again. Once per fetch keeps that to a handle a minute.
    if (res.status === 401) {
      handle = await authorizeUsage($, memo)
      if (handle === null || !state.needs.usage) {
        return false
      }
      res = await $.http.fetch(USAGE_URL, { auth: handle, headers: USAGE_HEADERS })
    }
  } catch (err) {
    const message = String(err)
    if (REFUSED.test(message)) {
      memo.off = true
    } else if (UNKNOWN_HANDLE.test(message)) {
      memo.handle = null
    }
    return false
  }
  const next = answered(file, res, await $.clock.now())
  if (next === null) {
    return false
  }
  memo.nextAt = Math.max(memo.nextAt, next.backoff_until_ms)
  memo.last = shown(next)
  await writeUsage($, path, next)
  return res.ok
}

// Authorization, cache reads and HTTP all run outside the render. The empty value reserves ownership while login
// is pending, so the binary cannot start its cookie fallback at the same time.
function accountUsage(memo: UsageMemo): UsageInput | null {
  return memo.off || memo.authFailed ? null : memo.last
}

function pollUsage($: EngineInterface): UsageInput | null {
  const memo = state.usage
  if (memo.off) {
    return null
  }
  if (state.needs.usage && !memo.polling) {
    const owned = accountUsage(memo) !== null
    memo.polling = true
    void refreshUsage($, memo)
      .catch(() => {})
      .finally(() => {
        memo.polling = false
        if (owned !== (accountUsage(memo) !== null) && state.usage === memo) {
          void refresh($)
        }
      })
  }
  return accountUsage(memo)
}

async function refreshUsage($: EngineInterface, memo: UsageMemo) {
  if (memo.handle === null && waiting(memo.nextAt, await $.clock.now())) {
    return
  }
  if (memo.handle === null && (await authorizeUsage($, memo)) === null) {
    return
  }
  if (!state.needs.usage || state.usage !== memo) {
    return
  }
  const home = await $.env.get('HOME')
  if (!home) {
    return
  }
  const path = usagePath(home)
  const now = await $.clock.now()
  const file = await readUsage($, path, now)
  if (file) {
    const changed = JSON.stringify(memo.last) !== JSON.stringify(shown(file))
    memo.last = shown(file)
    if (changed) {
      void refresh($)
    }
    if (!waiting(memo.nextAt, now) && fetchDue(file, now) && await fetchUsage($, memo, path, file)) {
      void refresh($)
    }
  }
}

async function run($: EngineInterface): Promise<Rendered> {
  state.handshake ??= probe($, state.binary)
  const verdict = await state.handshake
  if (verdict) {
    if (!verdict.final) {
      state.handshake = null
    }
    return { rows: [], error: verdict.error }
  }
  const { binary } = state
  const stdin = await buildInput($)
  let out
  try {
    const argv = [binary, '--format', 'spans', '--heartbeat-ms', String(state.heartbeatMs)]
    if (state.reportsNeeds) {
      argv.push('--plugin-data')
    }
    if (state.fits && state.fitWidth !== undefined) {
      argv.push('--width', String(state.fitWidth))
    }
    out = await $.process.run(argv, { stdin, timeoutMs: 5000 })
  } catch (err) {
    return { rows: [], error: MISSING.test(String(err)) ? NOT_FOUND : `statusline: ${String(err)}` }
  }
  const stderr = plain(out.stderr)
  if (out.exitCode === 2 && state.reportsNeeds && /unexpected argument '--plugin-data'/.test(stderr)) {
    // A running chat can outlive a binary downgrade. Optional capabilities may be dropped without losing the line.
    state.reportsNeeds = false
    state.needs = { usage: true, autocompact: true }
    return run($)
  }
  if (out.exitCode === 2 && state.fits && /unexpected argument '--width'/.test(stderr)) {
    state.fits = false
    return run($)
  }
  if (out.exitCode === 2 && UNKNOWN_FLAG.test(stderr)) {
    state.tooOld ??= await tooOld($, binary)
    return { rows: [], error: state.tooOld }
  }
  if (out.exitCode !== 0) {
    return { rows: [], error: `statusline: ${binary} exited ${out.exitCode}: ${stderr}` }
  }
  let rows: unknown = null
  try {
    rows = JSON.parse(out.stdout || 'null')
  } catch {
    // Plain text means an ANSI-only build or another binary, which a JSON parse error would not say.
  }
  let updateNotice: string | undefined
  if (state.reportsNeeds) {
    const envelope = obj(rows)
    const needs = obj(envelope?.needs)
    if (!needs || typeof needs.usage !== 'boolean' || typeof needs.autocompact !== 'boolean') {
      return { rows: [], error: 'statusline: invalid plugin data from binary' }
    }
    const nextNeeds = { usage: needs.usage, autocompact: needs.autocompact }
    if (nextNeeds.autocompact && !state.needs.autocompact) {
      state.again = true
    }
    state.needs = nextNeeds
    // The binary has just resolved settings and account overrides. Checking after its reply also prevents an old
    // layout from starting a due request on the very tick the user removes the usage segment.
    pollUsage($)
    updateNotice = typeof envelope?.update === 'string' && state.userTurns === 0 ? envelope.update : undefined
    rows = envelope?.rows
  }
  return isSpanRows(rows)
    ? { rows, ...(updateNotice ? { update: updateNotice } : {}) }
    : { rows: [], error: `statusline: ${binary} did not print span rows, it needs --format spans` }
}

async function refresh($: EngineInterface) {
  if (state.inFlight) {
    state.again = true
    return
  }
  state.inFlight = true
  state.again = false
  try {
    let next: Rendered
    try {
      next = await run($)
    } catch (err) {
      next = { rows: [], error: `statusline: ${String(err)}` }
    }
    const key = JSON.stringify(next)
    if (key !== state.last) {
      await update($, rendered, () => next)
      state.last = key
    }
  } catch {
    // Left unmarked so the next refresh stores the line again.
  } finally {
    state.inFlight = false
    if (state.again) {
      void refresh($)
    }
  }
}

async function drawLine(
  $: EngineInterface,
  e: Parameters<EngineInterface['ui']['resolve']>[0],
  working: boolean,
  hint?: unknown,
  width?: number,
) {
  const r = await read($, rendered)
  const visible = r && state.userTurns !== 0 ? { ...r, update: undefined } : r
  return visible && !blank(visible) ? draw($.ui.resolve(e), visible, working, hint, width) : null
}

async function noteTodos($: EngineInterface, tool: string, args: Json, result: Json, main: boolean) {
  if (!TODO_TOOLS.has(tool)) {
    return
  }
  await update($, live, l => foldTodos(l, tool, args, result, main) ?? l)
  void refresh($)
}

async function noteBackground($: EngineInterface, tool: string, args: Json, result: Json) {
  if (!BACKGROUND_TOOLS.has(tool)) {
    return
  }
  await update($, live, l => foldBackground(l, tool, args, result) ?? l)
  void refresh($)
}

// A task's end reaches Claude as a prompt, delivered into the running turn or starting one once the session is idle.
async function noteTaskEnd(
  $: EngineInterface,
  e: PromptSubmitInput,
  next: (e: PromptSubmitInput) => Promise<PromptSubmitResult>,
): Promise<PromptSubmitResult> {
  if (e.origin.kind !== 'task-notification') {
    state.userTurns = Math.max(1, state.userTurns ?? 0)
    await update($, rendered, r => r?.update ? { ...r, update: undefined } : r)
    void refresh($)
  }
  const ended = e.origin.kind === 'task-notification' ? endedTasks(e.text) : []
  if (ended.length > 0) {
    await update($, live, l => withoutBackground(l, ended) ?? l)
    void refresh($)
  }
  return next(e)
}

// Stop and SubagentStop both list the whole session's work in flight, which settles the tasks whose end went unseen.
async function settleBackground<E extends { background_tasks?: unknown }, R>(
  $: EngineInterface,
  e: E,
  next: (e: E) => Promise<R>,
): Promise<R> {
  const background = backgroundSnapshot(e.background_tasks)
  if (background) {
    await update($, live, l => ({ ...l, background }))
    void refresh($)
  }
  return next(e)
}

async function clearPermission($: EngineInterface, tool: string, agent: string | undefined) {
  const who = agent ?? null
  if (!(await read($, live)).permissions.some(p => p.tool === tool && p.agent === who)) {
    return
  }
  await update($, live, l => ({ ...l, permissions: withoutPermission(l.permissions, tool, who) }))
  void refresh($)
}

// No event marks the user approving a call, so a wait ends when its call does.
async function settlePermission<E extends { tool_name: string; agent_id?: string }, R>(
  $: EngineInterface,
  e: E,
  next: (e: E) => Promise<R>,
): Promise<R> {
  await clearPermission($, e.tool_name, e.agent_id)
  return next(e)
}

// No event reports a shift+tab, so the mode is as of the last hook that carried it.
async function noteMode<E extends { permission_mode?: string; agent_id?: string }, R>(
  $: EngineInterface,
  e: E,
  next: (e: E) => Promise<R>,
): Promise<R> {
  const seen = e.permission_mode
  if (seen && !e.agent_id && seen !== (await read($, mode))) {
    await update($, mode, () => seen)
  }
  return next(e)
}

function passThrough<E, R>(_$: EngineInterface, e: E, next: (e: E) => R): R {
  return next(e)
}

export const register: Register = (on, options) => {
  state = fresh(options)

  on('session.start', async ($, e, next) => {
    state.userTurns = null
    // The first render waits on the probe and a run of the binary, and the native line would draw beside it until then.
    const [id, now] = await Promise.all([$.session.id(), $.clock.now()])
    await writeHeartbeat($, id, now + state.heartbeatMs)
    // The engine drops this timer when the module reloads, so a new interval never stacks on an old one.
    $.clock.every(state.intervalMs, () => void refresh($))
    void refresh($)
    return next(e)
  })
  // No event marks this module unloading, so an ending session is the one chance to hand the line back before the
  // heartbeat runs out.
  on('session.end', async ($, e, next) => {
    await writeHeartbeat($, e.sessionId, 0)
    return next(e)
  })
  on('session.measure', async ($, e, next) => {
    const result = await next(e)
    void refresh($)
    return result
  })
  on('turn.start', async ($, e, next) => {
    state.userTurns = Math.max(1, state.userTurns ?? 0)
    const now = await $.clock.now()
    // A turn that ends before its first response would otherwise report the previous turn's stop reason.
    await update($, tracker, t => ({ ...t, lastStopReason: null }))
    await update($, live, l => ({
      ...l,
      turn: {
        started_at_ms: now,
        last_duration_ms: l.turn?.last_duration_ms ?? null,
        ended_at_ms: l.turn?.ended_at_ms ?? null,
      },
    }))
    void refresh($)
    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    if (!e.agentId) {
      const [now, t] = await Promise.all([$.clock.now(), read($, tracker)])
      const refusal = e.reason === 'refusal' ? (e.refusal.explanation ?? e.refusal.category) : null
      await update($, live, l => ({
        ...l,
        // A call cut short by an interrupt may never report its end, and a rejected permission raises no PostToolUse.
        // A subagent's waits outlive the main turn, as a background agent keeps running.
        tools: [],
        permissions: l.permissions.filter(p => p.agent !== null),
        turn: { started_at_ms: null, last_duration_ms: e.durationMs, ended_at_ms: now },
        lastError: turnError(l, e.reason, refusal, t.lastStopReason, now, e.durationMs),
      }))
    } else if ((await read($, live)).permissions.some(p => p.agent === e.agentId)) {
      // The loop events' agentId is the classic events' agent_id. A loop that ended no longer waits on anything.
      await update($, live, l => ({ ...l, permissions: l.permissions.filter(p => p.agent !== e.agentId) }))
    }
    void refresh($)
    return result
  })

  // Main-loop requests only: a subagent's usage would overwrite the last response the context segments describe.
  on('turn.step', async function* ($, e, next) {
    const composed = state.pendingCompose
    state.pendingCompose = null
    if (e.agentId) {
      state.composeAmbiguous = true
      return yield* next(e)
    }
    // A running subagent may compose a prompt before its first request, so the one claimed here may be its own.
    const agents = await $.agent.list()
    const ambiguous = state.composeAmbiguous || agents.some(a => RUNNING.has(a.status))
    state.composeAmbiguous = false
    const sentAt = await $.clock.now()
    const r = yield* next(e)
    const u = r.usage
    await update($, tracker, t => {
      const stepped = { ...t, lastStopReason: r.stopReason }
      if (!u) {
        return stepped
      }
      const effort = typeof e.effort === 'string' ? e.effort : null
      const s = { sentAt, model: u.model || e.model, usage: u, compose: composed, effort }
      return observeStep(stepped, s, TTL_MS[t.cacheTtl ?? state.defaultTtl], ambiguous)
    })
    void refresh($)
    return r
  })

  on('prompt.compose', async ($, e, next) => {
    const result = await next(e)
    state.pendingCompose = {
      tools: [...e.tools].sort(),
      chars: result.sections.reduce((n, s) => n + s.text.length, 0),
    }
    return result
  })

  on('tool.call', async ($, e, next) => {
    const args = e as unknown as Json
    const id = str(args.tool_use_id)
    // A subagent's tools run inside the main loop's Agent call, which is already shown.
    const tracked = !e.agentId && id !== null
    if (tracked) {
      const now = await $.clock.now()
      await update($, live, l => ({
        ...l,
        tools: [...l.tools, { id, tool: e.tool, detail: detailOf(args), started_at_ms: now }],
      }))
      void refresh($)
    }
    try {
      const result = await next(e)
      const out = obj(result.result)
      if (!result.deny && !result.isError && out) {
        await noteTodos($, e.tool, args, out, !e.agentId)
        await noteBackground($, e.tool, args, out)
      }
      return result
    } finally {
      if (tracked) {
        await update($, live, l => ({ ...l, tools: l.tools.filter(t => t.id !== id) }))
        void refresh($)
      }
    }
  }).catch(passThrough)

  on('classic.PermissionRequest', async ($, e, next) => {
    const now = await $.clock.now()
    const waiting = { tool: e.tool_name, since_ms: now, agent: e.agent_id ?? null }
    await update($, live, l => ({ ...l, permissions: [...l.permissions, waiting] }))
    void refresh($)
    return next(e)
  }).catch(passThrough)
  on('classic.PostToolUse', ($, e, next) => noteMode($, e, seen => settlePermission($, seen, next))).catch(passThrough)
  on('classic.UserPromptSubmit', noteMode).catch(passThrough)
  on('classic.SessionStart', noteMode).catch(passThrough)
  on('prompt.submit', noteTaskEnd).catch(passThrough)
  // A notification can end a task before this plugin loaded or while a build without the prompt hook ran.
  on('classic.Stop', ($, e, next) => noteMode($, e, seen => settleBackground($, seen, next))).catch(passThrough)
  on('classic.SubagentStop', settleBackground).catch(passThrough)
  on('classic.PostToolUseFailure', settlePermission).catch(passThrough)
  on('classic.PermissionDenied', settlePermission).catch(passThrough)

  // Carries the API's own word for the failure, which turn.complete reduces to 'error'.
  on('classic.StopFailure', async ($, e, next) => {
    if (!e.agent_id) {
      const now = await $.clock.now()
      const detail = e.error_details ? cut(plain(e.error_details), 200) || null : null
      await update($, live, l => ({ ...l, lastError: { kind: e.error, detail, at_ms: now } }))
      void refresh($)
    }
    return next(e)
  }).catch(passThrough)

  // A model switch is the only place the engine reports the cache TTL, and the new model starts with a cold cache.
  on('classic.PostModelSwitch', async ($, e, next) => {
    await update($, tracker, t => ({ ...t, cacheTtl: e.cache_ttl, lastRequestAt: null }))
    void refresh($)
    return next(e)
  }).catch(passThrough)

  // Main conversation only: precompute installs nothing and a subagent's compaction leaves the main window alone.
  on('session.compact', async ($, e, next) => {
    if (e.trigger === 'precompute' || e.agentId) {
      return next(e)
    }
    const trigger = e.trigger === 'auto' || e.trigger === 'manual' ? e.trigger : null
    const [start, before] = await Promise.all([$.clock.now(), read($, live)])
    const prior = before.compaction
    await update($, live, l => ({
      ...l,
      compaction: { ...(l.compaction ?? NO_COMPACTION), running_since_ms: start, trigger },
    }))
    void refresh($)
    let done: Compaction | null = null
    try {
      const result = await next(e)
      if (result.messages) {
        const now = await $.clock.now()
        done = {
          count: (prior?.count ?? 0) + 1,
          last_at_ms: now,
          tokens_before: result.tokensBefore ?? null,
          tokens_after: result.tokensAfter ?? null,
          running_since_ms: null,
          trigger,
        }
        // Compaction starts a new window, so the last response no longer describes the context.
        await update($, tracker, t => ({ ...t, lastUsage: null, compactedSinceLast: true }))
      }
      return result
    } finally {
      const settled = done ?? (prior ? { ...prior, running_since_ms: null } : null)
      await update($, live, l => ({ ...l, compaction: settled }))
      void refresh($)
    }
  }).catch(passThrough)

  on('ui.render', { component: 'PromptHint' }, async ($, e, next) => {
    if (state.placement !== 'below') {
      return next(e)
    }
    const width = hintWidth(e.viewport?.columns, await read($, mode))
    state.fitWidth = width
    // The hint line is the only place the engine says how to interrupt, so it is carried over while a turn runs.
    return (await drawLine($, e, e.props.isWorking, e.props.hint, width)) ?? next(e)
  })


  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    if (state.placement !== 'above' || e.props.hasSurvey) {
      return next(e)
    }
    return (await drawLine($, e, false)) ?? next(e)
  })
}
