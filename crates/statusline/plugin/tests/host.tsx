import { mock } from 'claude-code/testing'
import type { MockClock, TestBody } from 'claude-code/testing'
import type { AgentInfo, HttpInit, HttpResponse, On, SessionAuthorization, SessionUsage } from 'claude-code'

import type { LastUsage, Span } from '../types'
import type { Needs } from '../hooks/binary'

const rows: Span[][] = [
  [
    { text: '12%', fg: '#76c86e', bold: true },
    { text: ' ' },
    { text: '#1', fg: '#50c878', href: 'https://github.com/o/r/pull/1' },
    { text: ' warm', dim: true },
  ],
  [{ text: 'second row' }],
]

const ok = { exitCode: 0, stdout: JSON.stringify(rows), stderr: '' }

export const capableHelp = {
  exitCode: 0,
  stdout:
    'Usage: statusline [OPTIONS] [COMMAND]\n\nOptions:\n      --format <FORMAT>  How the default render is printed\n' +
    '      --heartbeat-ms <MS>  How long the native line stays silent\n  -h, --help  Print help\n',
  stderr: '',
}
export const oldHelp = {
  exitCode: 0,
  stdout: 'Usage: statusline [OPTIONS] [COMMAND]\n\nOptions:\n  -f <FIVE_HOUR_RESET_THRESHOLD>\n  -h, --help\n',
  stderr: '',
}
export const missing = (argv: readonly string[]) => `ENOENT: no such file or directory, posix_spawn '${argv[0]}'`

type Out = { exitCode: number; stdout: string; stderr: string }

const liveUsage = {
  startedAt: 0,
  context: { tokens: 1000, window: 200000, percent: 1 },
  rateLimits: [
    { kind: 'five_hour', percentUsed: 12, resetsAt: '2026-10-06T13:00:00Z' },
    { kind: 'seven_day', percentUsed: 40 },
  ],
  cost: { usd: 1.5 },
}

export const T0 = 1791280000000
export const S0 = T0 / 1000
export const quiet = { intervalMs: 3_600_000 }
export const opus = 'claude-opus-5-5'

type Usage = LastUsage & { model: string }

export const usage = (read: number, write: number, model = opus): Usage => ({
  input_tokens: 2,
  output_tokens: 7,
  cache_creation_input_tokens: write,
  cache_read_input_tokens: read,
  model,
})

type Engine = Parameters<TestBody>[0]

type StopReason = 'end_turn' | 'max_tokens' | 'tool_use'

// A promise the test resolves by hand, to hold a call in flight.
export function gate() {
  let open = () => {}
  const wait = new Promise<void>(resolve => {
    open = resolve
  })
  return { wait, open }
}

type Host = {
  out?: Out | ((argv: readonly string[]) => Out)
  // What `--help` answers, whose flags say what the build can do.
  help?: Out
  version?: Out
  // The message a run rejects with, as the engine's does for a binary that cannot start or outlasts its timeout.
  reject?: (argv: readonly string[]) => string | undefined
  realPath?: string
  // Responses to main and subagent requests alike, in the order they are sent, then the default.
  steps?: { usage: Usage; stopReason?: StopReason }[]
  agents?: AgentInfo[]
  breakdown?: { autoCompactThreshold?: number; isAutoCompactEnabled: boolean; totalTokens: number } | null
  // Only the named tool waits on `wait`, so other calls in the same test still answer.
  tool?: { holds?: string; wait?: Promise<void>; result?: (e: { tool: string }) => unknown }
  compact?: { wait?: Promise<void>; result?: { skip: string } }
  system?: () => string
  // What `$.session.authorize()` answers each time it is asked, null (no claude.ai login) when not given.
  authorize?: () => SessionAuthorization | Promise<SessionAuthorization>
  needs?: () => Needs
  updateNotice?: string
  turns?: number
  runWait?: () => Promise<void>
  // The files beneath the plugin. A test holding the same map stands in for another chat reading and writing them.
  files?: Map<string, string>
  // The modification time `fs.stat` reports for any of `files`.
  mtimeMs?: number
  // Answers `$.http.fetch`, or rejects it with `{ reject }`, the message the engine's own fetch throws.
  fetch?: (
    url: string,
    init: HttpInit | undefined,
  ) => HttpResponse | { reject: string } | Promise<HttpResponse | { reject: string }>
  // Paths whose `fs.write` fails, as on a full disk or a read-only home.
  unwritable?: Set<string>
  // The message `$.session.model()` rejects with.
  modelError?: string
}

export const transcript = [{ role: 'user' as const, text: 'Summary of the conversation so far', toolUses: [] }]

type Seen = {
  stdin?: string
  argv?: readonly string[]
  breakdowns?: number
  runs?: (readonly string[])[]
  // Heartbeat writes and binary runs, in the order the plugin made them.
  events?: string[]
  fetches?: { url: string; init: HttpInit | undefined }[]
  authorizes?: number
}

// Stands in for the engine beneath the plugin: a fixed clock, the session's live figures, and the binary.
function host(on: On, cfg: Host, seen: Seen = {}): MockClock {
  on('session.start', (_$, e) => ({ cwd: e.cwd }))
  on('session.end', (_$, e) => ({ sessionId: e.sessionId }))
  const clock = mock.clock(on, { now: T0 })
  mock.env(on, { HOME: '/home/me' })
  on('fs.write', (_$, e) => {
    if (cfg.unwritable?.has(e.path)) {
      throw new Error(`EROFS: ${e.path}`)
    }
    seen.events = [...(seen.events ?? []), `write ${e.path} ${e.text}`]
    cfg.files?.set(e.path, e.text)
    return { value: undefined }
  })
  on('fs.read', (_$, e) => {
    const text = cfg.files?.get(e.path)
    if (text === undefined) {
      throw new Error(`ENOENT: ${e.path}`)
    }
    return { value: text }
  })
  on('session.authorize', async () => {
    seen.authorizes = (seen.authorizes ?? 0) + 1
    return { value: await cfg.authorize?.() ?? null }
  })
  on('http.fetch', async (_$, e) => {
    seen.fetches = [...(seen.fetches ?? []), { url: e.url, init: e.init }]
    if (!cfg.fetch) {
      throw new Error(`no network in tests: ${e.url}`)
    }
    const res = await cfg.fetch(e.url, e.init)
    return 'reject' in res ? { deny: res.reject } : { value: res }
  })
  on('ui.render', ($, e) => {
    const { Text } = $.ui.resolve(e)
    return <Text>engine</Text>
  })
  on('session.id', () => ({ value: 'abc' }))
  on('session.turns', () => ({ value: cfg.turns ?? 0 }))
  on('session.cwd', () => ({ value: '/work' }))
  on('session.root', () => ({ value: '/repo-root' }))
  on('session.model', () => (cfg.modelError ? { deny: cfg.modelError } : { value: opus }))
  on('session.usage', (_$, e) => {
    if (!e?.breakdown) {
      return { value: liveUsage }
    }
    seen.breakdowns = (seen.breakdowns ?? 0) + 1
    const breakdown =
      cfg.breakdown === null
        ? undefined
        : (cfg.breakdown ?? { autoCompactThreshold: 167000, isAutoCompactEnabled: true, totalTokens: 129000 })
    return { value: { ...liveUsage, context: { ...liveUsage.context, breakdown } } as unknown as SessionUsage }
  })
  on('session.version', () => ({ value: { version: '2.1.290', base: '2.1.290' } }))
  on('agent.list', () => ({ value: cfg.agents ?? [] }))
  const steps = [...(cfg.steps ?? [])]
  on('turn.step', async function* (_$, e) {
    const s = steps.shift() ?? { usage: usage(234480, 6430) }
    return {
      turnId: e.turnId,
      index: e.index,
      answer: '',
      toolUses: [],
      stopReason: s.stopReason ?? ('end_turn' as const),
      usage: s.usage,
    }
  })
  on('turn.start', (_$, e) => ({ turnId: e.turnId }))
  on('turn.complete', (_$, e) => ({ text: e.answer }))
  on('prompt.submit', (_$, e) => ({ text: e.text, origin: e.origin }))
  on('prompt.compose', () => ({
    sections: [{ id: 'main', text: cfg.system?.() ?? 'x'.repeat(100), scope: 'session' as const }],
  }))
  on('tool.call', async (_$, e) => {
    if (e.tool === cfg.tool?.holds) {
      await cfg.tool.wait
    }
    return { result: (cfg.tool?.result?.(e) ?? { stdout: '', stderr: '', interrupted: false }) as never }
  })
  on('session.compact', async () => {
    await cfg.compact?.wait
    return cfg.compact?.result ?? { messages: transcript, tokensBefore: 182000, tokensAfter: 21000 }
  })
  on('classic.PermissionRequest', () => ({}))
  on('classic.PermissionDenied', () => ({}))
  on('classic.PostToolUse', () => ({}))
  on('classic.PostToolUseFailure', () => ({}))
  on('classic.StopFailure', () => ({}))
  on('classic.Stop', () => ({}))
  on('classic.SubagentStop', () => ({}))
  on('classic.UserPromptSubmit', () => ({}))
  on('classic.SessionStart', () => ({}))
  on('fs.stat', (_$, e) => {
    const file = cfg.files?.get(e.path)
    if (file !== undefined) {
      return { value: { kind: 'file' as const, size: file.length, mtimeMs: cfg.mtimeMs ?? 0, isLink: false } }
    }
    if (cfg.realPath === undefined) {
      throw new Error(`ENOENT: ${e.path}`)
    }
    return { value: { kind: 'file' as const, size: 1, mtimeMs: 0, isLink: true, realPath: cfg.realPath } }
  })
  on('process.run', async (_$, e) => {
    seen.runs = [...(seen.runs ?? []), e.argv]
    seen.events = [...(seen.events ?? []), `run ${e.argv[1]}`]
    const rejected = cfg.reject?.(e.argv)
    if (rejected !== undefined) {
      return { deny: rejected }
    }
    let out = typeof cfg.out === 'function' ? cfg.out(e.argv) : cfg.out ?? ok
    if (e.argv[1] === '--help') {
      out = cfg.help ?? capableHelp
      if (cfg.needs) {
        out = { ...out, stdout: out.stdout + '  --plugin-data  Include plugin metadata\n' }
      }
    } else if (e.argv[1] === '--version') {
      out = cfg.version ?? { exitCode: 0, stdout: 'statusline 1.1.0\n', stderr: '' }
    } else {
      seen.stdin = e.init?.stdin
      seen.argv = e.argv
      await cfg.runWait?.()
      if (cfg.needs && e.argv.includes('--plugin-data') && out.exitCode === 0) {
        out = { ...out, stdout: JSON.stringify({ rows: JSON.parse(out.stdout), needs: cfg.needs(), update: cfg.updateNotice ?? null }) }
      }
    }
    return { value: { ...out, isStdoutTruncated: false, isStderrTruncated: false } }
  })
  return clock
}

// Starts the session and lets its first refresh, started unawaited, run to the end.
export async function boot($: Engine, on: On, cfg: Host = {}) {
  const seen: Seen = {}
  const clock = host(on, cfg, seen)
  await $.session.start({ cwd: '/work', surface: 'terminal', isInteractive: true })
  await clock.settle()
  return { seen, clock }
}

export const hint = (isWorking = false) => ({ isDraft: false, isWorking, hint: '? for shortcuts' })

// Runs one model request through the chain, as the engine does for each step of a turn.
export async function step(
  $: Engine,
  clock: MockClock,
  { agentId, model = opus }: { agentId?: string; model?: string } = {},
) {
  const stream = $.turn.step({ turnId: 't1', index: 0, model, effort: 'high', messageCount: 3, agentId })
  for await (const _ of stream) {
    // Drained only so the step settles.
  }
  await stream.result
  await clock.settle()
}

// Renders a system prompt as the engine does before a request, offering these tools.
export async function compose($: Engine, tools: string[]) {
  await $.prompt.compose({
    model: opus,
    promptModel: opus,
    surfaces: ['terminal'],
    tools,
    outputStyle: null,
    traits: [],
  })
}

export const inputOf = (seen: Seen) => JSON.parse(seen.stdin ?? '{}')

export const modOf = (seen: Seen) => inputOf(seen).mod

export const unknownFlag = (flag: string): Out => ({
  exitCode: 2,
  stdout: '',
  stderr: `\u001b[1;31merror:\u001b[0m unexpected argument '\u001b[33m${flag}\u001b[0m' found\n\nUsage: statusline`,
})

export const mountHint = ($: Engine) =>
  $.ui.mount({ plugin: 'statusline', surface: 'terminal', component: 'PromptHint', props: hint() })
