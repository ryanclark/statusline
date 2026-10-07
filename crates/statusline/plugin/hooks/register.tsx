import { atom, read, update } from 'claude-code'
import type { EngineInterface, PluginOptions, Register } from 'claude-code'

import type { Autocompact, CacheTtl, Compaction, ComposeShape, Rendered } from '../types'
import {
  detailOf,
  EMPTY_LIVE,
  foldTodos,
  NO_COMPACTION,
  RUNNING,
  TODO_TOOLS,
  turnError,
  withoutPermission,
} from './activity'
import { isSpanRows, MISSING, NOT_FOUND, REQUIRED_FLAGS, tooOldMessage, UNKNOWN_FLAG } from './binary'
import type { Verdict } from './binary'
import { EMPTY_TRACKER, observeStep, TTL_MS } from './cache'
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
import { parsePills } from './pills'
import { cut, obj, plain, str } from './util'
import type { Json } from './util'

const rendered = atom({ plugin: 'statusline', key: 'rendered' } as const, null)
const tracker = atom({ plugin: 'statusline', key: 'tracker' } as const, EMPTY_TRACKER)
const live = atom({ plugin: 'statusline', key: 'live' } as const, EMPTY_LIVE)

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
  tooOld: string | null
  usage: UsageMemo
}

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
    tooOld: null,
    usage: newUsageMemo(),
  }
}

let state = fresh({})

const BREAKDOWN_EVERY_MS = 10_000

async function buildInput($: EngineInterface): Promise<string> {
  const now = await $.clock.now()
  const due = now - state.breakdownAt >= BREAKDOWN_EVERY_MS
  if (due) {
    state.breakdownAt = now
  }
  const [id, cwd, root, model, version, usage, t, l, agents, shared] = await Promise.all([
    $.session.id(),
    $.session.cwd(),
    $.session.root(),
    $.session.model(),
    $.session.version(),
    due ? $.session.usage({ breakdown: 'summary' }) : $.session.usage(),
    read($, tracker),
    read($, live),
    $.agent.list(),
    pollUsage($, now),
  ])
  if (due) {
    state.autocompact = autocompactOf(usage.context.breakdown)
  }
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
    accountUsage: shared,
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
  if (out.exitCode === 0 && !REQUIRED_FLAGS.every(flag => flag.test(help))) {
    return { error: await tooOld($, binary), final: true }
  }
  return null
}

// The file the binary's own heartbeat writes (session.rs), holding the expiry in epoch ms. Best effort, since every
// refresh writes it again through the binary.
async function writeHeartbeat($: EngineInterface, sessionId: string, expiresMs: number) {
  // The id becomes a file name, so only the plain tokens the binary itself accepts are written.
  if (!/^[A-Za-z0-9_-]{1,128}$/.test(sessionId)) {
    return
  }
  try {
    const home = await $.env.get('HOME')
    if (home) {
      await $.fs.write(`${home}/.statusline/sessions/${sessionId}.plugin`, String(Math.floor(expiresMs)))
    }
  } catch {
    // A missed write leaves the native line drawing until the next refresh, as if the plugin were not installed.
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
  const auth = await $.session.authorize().catch(() => null)
  // An API key or a third-party provider has no claude.ai usage, and the binary keeps its cookie path for those.
  memo.handle = auth?.kind === 'bearer' ? auth.handle : null
  memo.off = memo.handle === null
  return memo.handle
}

async function fetchUsage($: EngineInterface, memo: UsageMemo, path: string, file: UsageFile): Promise<boolean> {
  let handle = memo.handle
  if (handle === null) {
    return false
  }
  const start = await $.clock.now()
  memo.nextAt = start + FETCH_EVERY_MS
  await writeUsage($, path, claimed(file, start))
  let res
  try {
    res = await $.http.fetch(USAGE_URL, { auth: handle, headers: USAGE_HEADERS })
    // A handle keeps the token it was minted with, so one minted before the session refreshed its login fails until
    // it is minted again. Once per fetch keeps that to a handle a minute.
    if (res.status === 401) {
      handle = await authorizeUsage($, memo)
      if (handle === null) {
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
  await writeUsage($, path, next)
  return res.ok
}

// The shared usage for this refresh, starting a fetch when it is due. The fetch is not awaited, since the engine gives
// it up to 30s and the line should not wait on it.
async function pollUsage($: EngineInterface, now: number): Promise<UsageInput | null> {
  const memo = state.usage
  if (memo.off || (memo.handle === null && (await authorizeUsage($, memo)) === null)) {
    return null
  }
  const home = await $.env.get('HOME')
  if (!home) {
    return null
  }
  const path = usagePath(home)
  const file = await readUsage($, path, now)
  if (file && !memo.polling && !waiting(memo.nextAt, now) && fetchDue(file, now)) {
    memo.polling = true
    void fetchUsage($, memo, path, file)
      .then(fetched => {
        if (fetched) {
          void refresh($)
        }
      })
      .catch(() => {})
      .finally(() => {
        memo.polling = false
      })
  }
  if (file) {
    memo.last = shown(file)
  }
  return memo.last
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
  let out
  try {
    out = await $.process.run([binary, '--format', 'spans', '--heartbeat-ms', String(state.heartbeatMs)], {
      stdin: await buildInput($),
      timeoutMs: 5000,
    })
  } catch (err) {
    return { rows: [], error: MISSING.test(String(err)) ? NOT_FOUND : `statusline: ${String(err)}` }
  }
  const stderr = plain(out.stderr)
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
  return isSpanRows(rows)
    ? { rows }
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
      state.last = key
      await update($, rendered, () => next)
    }
  } finally {
    state.inFlight = false
    if (state.again) {
      void refresh($)
    }
  }
}

// Claude Code draws a statusLine command's uncoloured text in the theme's muted grey, not the terminal foreground,
// so spans without their own colour take that theme key to match the native line.
const DEFAULT_FG = 'inactive'

async function draw(
  $: EngineInterface,
  e: Parameters<EngineInterface['ui']['resolve']>[0],
  working: boolean,
  hint?: unknown,
) {
  const r = await read($, rendered)
  if (!r || (!r.error && r.rows.every(row => row.length === 0))) {
    return null
  }
  const { Box, Text, Link } = $.ui.resolve(e)
  if (r.error) {
    return (
      <Text color="error" wrap="truncate-end">
        {r.error}
      </Text>
    )
  }
  const lastRow = r.rows.length - 1
  const { pills, selected } = parsePills(hint)
  return (
    <Box flexDirection="column">
      {r.rows.map((row, i) => (
        // One Text per row so an overflowing row is cut once at its end rather than every span shrinking on its own.
        <Text key={`row${i}`} wrap="truncate-end">
          {i === 0 && pills.length > 0 ? (
            <Text key="pills">
              {pills.map((p, k) => (
                <Text key={`pill${k}`} color={selected ? 'inverseText' : 'cyan'} backgroundColor={selected ? 'cyan' : undefined}>
                  {k > 0 ? ` ${p}` : p}
                </Text>
              ))}
              <Text dimColor>{' · '}</Text>
            </Text>
          ) : null}
          {row.map((s, j) => {
            const text = (
              <Text
                key={`s${j}`}
                color={s.fg ?? DEFAULT_FG}
                backgroundColor={s.bg}
                bold={s.bold}
                dimColor={s.dim}
                italic={s.italic}
                underline={s.underline}
                strikethrough={s.strikethrough}
                inverse={s.inverse}
              >
                {s.text}
              </Text>
            )
            return s.href ? (
              <Link key={`l${j}`} href={s.href}>
                {text}
              </Link>
            ) : (
              text
            )
          })}
          {working && i === lastRow ? (
            <Text key="esc" dimColor>
              {' · esc to interrupt'}
            </Text>
          ) : null}
        </Text>
      ))}
    </Box>
  )
}

async function noteTodos($: EngineInterface, tool: string, args: Json, result: Json, main: boolean) {
  if (!TODO_TOOLS.has(tool)) {
    return
  }
  await update($, live, l => foldTodos(l, tool, args, result, main) ?? l)
  void refresh($)
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

function passThrough<E, R>(_$: EngineInterface, e: E, next: (e: E) => R): R {
  return next(e)
}

export const register: Register = (on, options) => {
  state = fresh(options)

  on('session.start', async ($, e, next) => {
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
      const usage = {
        input_tokens: u.input_tokens,
        output_tokens: u.output_tokens,
        cache_creation_input_tokens: u.cache_creation_input_tokens,
        cache_read_input_tokens: u.cache_read_input_tokens,
      }
      const effort = typeof e.effort === 'string' ? e.effort : null
      const s = { sentAt, model: u.model || e.model, usage, compose: composed, effort }
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
  on('classic.PostToolUse', settlePermission).catch(passThrough)
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
    // The hint line is the only place the engine says how to interrupt, so it is carried over while a turn runs.
    return (await draw($, e, e.props.isWorking, e.props.hint)) ?? next(e)
  })

  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    if (state.placement !== 'above' || e.props.hasSurvey) {
      return next(e)
    }
    return (await draw($, e, false)) ?? next(e)
  })
}
