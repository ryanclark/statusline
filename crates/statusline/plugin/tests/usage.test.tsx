import { describe, expect, test } from 'claude-code/testing'
import type { HttpResponse, SessionAuthorization } from 'claude-code'

import { fetchDue, parseUsageFile, USAGE_URL } from '../hooks/usage'
import type { UsageFile } from '../hooks/usage'
import { boot, gate, modOf, T0 } from './host'

const PATH = '/home/me/.statusline/cache/plugin-usage.json'
const MIN = 60_000

// The endpoint's top-level keys, with a Fable row among the limits.
const BODY = {
  five_hour: { utilization: 12, resets_at: '2026-10-06T13:00:00+00:00' },
  seven_day: { utilization: 40, resets_at: '2026-10-10T09:00:00+00:00' },
  extra_usage: { is_enabled: true, monthly_limit: 10000, used_credits: 2500, utilization: 25, currency: 'USD' },
  limits: [
    { kind: 'session', percent: 12, resets_at: '2026-10-06T13:00:00+00:00', scope: null },
    { kind: 'weekly_all', percent: 40, resets_at: '2026-10-10T09:00:00+00:00', scope: null },
    {
      kind: 'weekly_scoped',
      percent: 63,
      resets_at: '2026-10-10T09:00:00+00:00',
      scope: { model: { id: null, display_name: 'Fable' }, surface: null },
    },
  ],
  spend: { balance: { amount_minor: 4210, currency: 'USD', exponent: 2 } },
}

const reply = (status: number, body: unknown = BODY, headers: Record<string, string> = {}): HttpResponse => ({
  status,
  ok: status >= 200 && status < 300,
  headers,
  text: typeof body === 'string' ? body : JSON.stringify(body),
})

const bearer = (handle = 'h1'): (() => SessionAuthorization) => () => ({ handle, kind: 'bearer' })

const shared = (files: Map<string, string>): UsageFile => parseUsageFile(files.get(PATH) ?? '') as UsageFile

const file = (f: Partial<UsageFile>) =>
  JSON.stringify({ fetched_at_ms: null, body: null, backoff_until_ms: 0, backoff_ms: 0, ...f })

describe('usage', () => {
  test('is fetched with the session login and passed to the binary', async ($, on) => {
    const files = new Map<string, string>()
    const { seen } = await boot($, on, { authorize: bearer(), files, fetch: () => reply(200) })
    const init = { auth: 'h1', headers: { 'anthropic-beta': 'oauth-2025-04-20' } }
    expect(seen.fetches).toEqual([{ url: USAGE_URL, init }])
    expect(shared(files)).toEqual({ fetched_at_ms: T0, body: BODY, backoff_until_ms: 0, backoff_ms: 0 })
    // A new body refreshes the line at once rather than on the next tick.
    expect(modOf(seen).usage).toEqual({ fetched_at_ms: T0, body: BODY })
  })

  test('two chats sharing the file fetch once a minute between them', async ($, on) => {
    const other = { ...BODY, extra_usage: { ...BODY.extra_usage, used_credits: 9000 } }
    // The other chat fetched 30s ago.
    const files = new Map([[PATH, file({ fetched_at_ms: T0 - 30_000, body: other })]])
    const { seen, clock } = await boot($, on, { authorize: bearer(), files, fetch: () => reply(200) })
    expect(modOf(seen).usage).toEqual({ fetched_at_ms: T0 - 30_000, body: other })
    await clock.advance(29_000)
    expect(seen.fetches ?? []).toHaveLength(0)
    await clock.advance(1000)
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage.fetched_at_ms).toBe(T0 + 30_000)

    // The other chat fetches next, 50s on, which pushes this chat's next fetch out by as much.
    await clock.advance(50_000)
    files.set(PATH, file({ fetched_at_ms: clock.now(), body: other }))
    await clock.advance(59_000)
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage.body).toEqual(other)
    await clock.advance(1000)
    expect(seen.fetches).toHaveLength(2)
    // One handle, minted once and reused.
    expect(seen.authorizes).toBe(1)
    expect(seen.fetches?.every(f => f.init?.auth === 'h1')).toBe(true)
  })

  test('a fetch under way claims the minute, so another chat does not fetch beside it', async ($, on) => {
    const files = new Map<string, string>()
    const held = gate()
    const { seen, clock } = await boot($, on, {
      authorize: bearer(),
      files,
      fetch: async () => {
        await held.wait
        return reply(200)
      },
    })
    expect(seen.fetches).toHaveLength(1)
    expect(fetchDue(shared(files), clock.now())).toBe(false)
    await clock.advance(5000)
    expect(seen.fetches).toHaveLength(1)
    held.open()
    await clock.settle()
    expect(shared(files).fetched_at_ms).toBe(T0 + 5000)
  })

  test('a failure waits a minute rather than retrying every refresh', async ($, on) => {
    const files = new Map<string, string>()
    const { seen, clock } = await boot($, on, { authorize: bearer(), files, fetch: () => reply(500, 'oops') })
    await clock.advance(MIN - 1000)
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage).toBeUndefined()
    await clock.advance(1000)
    expect(seen.fetches).toHaveLength(2)
  })

  test(
    'a 429 backs every chat off 5, 10, 20, then 30 minutes, and a success resets it',
    { options: { intervalMs: MIN } },
    async ($, on) => {
      const files = new Map([[PATH, file({ fetched_at_ms: T0 - 2 * MIN, body: BODY })]])
      let limited = true
      const fetch = () => (limited ? reply(429, { error: 'rate_limited' }, { 'retry-after': '0' }) : reply(200))
      const { seen, clock } = await boot($, on, { authorize: bearer(), files, fetch })
      let count = 1
      for (const minutes of [5, 10, 20, 30, 30]) {
        expect(seen.fetches).toHaveLength(count)
        const f = shared(files)
        expect(f.backoff_ms).toBe(minutes * MIN)
        expect(f.backoff_until_ms).toBe(clock.now() + minutes * MIN)
        // The last body stays shown while backing off.
        expect(f.fetched_at_ms).toBe(T0 - 2 * MIN)
        expect(modOf(seen).usage.body).toEqual(BODY)
        await clock.advance(minutes * MIN - MIN)
        expect(seen.fetches).toHaveLength(count)
        if (minutes === 30 && count === 5) {
          limited = false
        }
        await clock.advance(MIN)
        count += 1
      }
      expect(seen.fetches).toHaveLength(6)
      expect(shared(files)).toMatchObject({ fetched_at_ms: clock.now(), backoff_until_ms: 0, backoff_ms: 0 })
    },
  )

  test('a positive retry-after longer than the step is honoured', { options: { intervalMs: MIN } }, async ($, on) => {
    const files = new Map<string, string>()
    const fetch = () => reply(429, {}, { 'retry-after': '900' })
    await boot($, on, { authorize: bearer(), files, fetch })
    expect(shared(files)).toMatchObject({ backoff_until_ms: T0 + 15 * MIN, backoff_ms: 5 * MIN })
  })

  for (const [name, authorize] of [
    ['no login', undefined],
    ['an API key', () => ({ handle: 'k1', kind: 'api-key' as const })],
  ] as const) {
    test(`${name} fetches nothing and leaves the binary its cookies`, async ($, on) => {
      const files = new Map<string, string>()
      const { seen, clock } = await boot($, on, { authorize, files, fetch: () => reply(200) })
      await clock.advance(2 * MIN)
      expect(seen.fetches ?? []).toHaveLength(0)
      expect(files.has(PATH)).toBe(false)
      expect(modOf(seen).usage).toBeUndefined()
      expect(seen.authorizes).toBe(1)
    })
  }

  test('another chat’s body is not shown without a login of this chat’s own', async ($, on) => {
    const files = new Map([[PATH, file({ fetched_at_ms: T0, body: BODY })]])
    const { seen } = await boot($, on, { files })
    expect(modOf(seen).usage).toBeUndefined()
  })

  test('a 401 authorizes once more and retries with the new handle', async ($, on) => {
    const handles = ['h1', 'h2']
    const authorize = (): SessionAuthorization => ({ handle: handles.shift() ?? 'h3', kind: 'bearer' })
    const fetch = (_url: string, init: { auth?: string } | undefined) => reply(init?.auth === 'h1' ? 401 : 200)
    const files = new Map<string, string>()
    const { seen, clock } = await boot($, on, { authorize, files, fetch })
    expect(seen.fetches?.map(f => f.init?.auth)).toEqual(['h1', 'h2'])
    expect(modOf(seen).usage.body).toEqual(BODY)
    await clock.advance(MIN)
    expect(seen.fetches?.map(f => f.init?.auth)).toEqual(['h1', 'h2', 'h2'])
    expect(seen.authorizes).toBe(2)
  })

  test('a 401 that survives the new handle waits for the next minute', async ($, on) => {
    const files = new Map<string, string>()
    const { seen, clock } = await boot($, on, { authorize: bearer(), files, fetch: () => reply(401, {}) })
    expect(seen.fetches).toHaveLength(2)
    await clock.advance(MIN - 1000)
    expect(seen.fetches).toHaveLength(2)
    // Each handle counts against the plugin's four, so a streak of 401s mints only one more.
    await clock.advance(1000)
    expect(seen.fetches).toHaveLength(3)
    expect(seen.authorizes).toBe(2)
  })

  test('a refused request stops this chat fetching', async ($, on) => {
    const files = new Map<string, string>()
    const fetch = () => ({ deny: 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC is set' })
    const { seen, clock } = await boot($, on, { authorize: bearer(), files, fetch })
    await clock.advance(3 * MIN)
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage).toBeUndefined()
  })

  for (const [age, fetches] of [
    [1000, 0],
    [2 * MIN, 1],
  ] as const) {
    test(`a torn file written ${age}ms ago ${fetches ? 'is fetched over' : 'is left to its writer'}`, async ($, on) => {
      const files = new Map([[PATH, '{"fetched_at_ms": 17912']])
      const { seen } = await boot($, on, { authorize: bearer(), files, mtimeMs: T0 - age, fetch: () => reply(200) })
      expect(seen.fetches ?? []).toHaveLength(fetches)
    })
  }
})
