import { describe, expect, test } from 'claude-code/testing'

import { usageAccount, usagePath } from '../hooks/usage'
import { boot, gate, modOf, mountHint, PROFILE, T0, USAGE_PATH } from './host'

const MIN = 60_000
const otherProfile = { ...PROFILE, account: { uuid: '33333333-3333-4333-8333-333333333333' } }
const otherPath = usagePath('/home/me', usageAccount(JSON.stringify(otherProfile))!)
const first = { extra_usage: { monthly_limit: 10000, used_credits: 9900 } }
const second = { extra_usage: { monthly_limit: 10000, used_credits: 200 } }
const response = (body: unknown, status = 200) => ({ status, ok: status === 200, headers: {}, text: JSON.stringify(body) })
const file = (body: unknown, at = T0) => JSON.stringify({ fetched_at_ms: at, body, backoff_until_ms: 0, backoff_ms: 0 })
const bearer = () => ({ handle: 'h1', kind: 'bearer' as const })
const needs = () => ({ usage: true, autocompact: false })

describe('usage account isolation', () => {
  test('the authenticated organization and account both form the cache key', () => {
    const key = usageAccount(JSON.stringify(PROFILE))
    expect(key).toBe('22222222-2222-4222-8222-222222222222.11111111-1111-4111-8111-111111111111')
    expect(usageAccount(JSON.stringify(otherProfile)) === key).toBe(false)
    expect(usageAccount(JSON.stringify({ ...PROFILE, organization: otherProfile.account })) === key).toBe(false)
    for (const text of ['', '{}', 'null', '{', JSON.stringify({ ...PROFILE, account: { uuid: '../elsewhere' } })]) {
      expect(usageAccount(text)).toBeNull()
    }
  })

  test('each login reads only its own cache', async ($, on) => {
    const files = new Map([[USAGE_PATH, file(first)], [otherPath, file(second)]])
    const { seen } = await boot($, on, {
      needs, authorize: bearer, files, profile: () => response(otherProfile),
    })
    expect(modOf(seen).usage.body).toEqual(second)
    expect(seen.fetches ?? []).toHaveLength(0)
  })

  test('the legacy unscoped cache is never adopted', async ($, on) => {
    const files = new Map([['/home/me/.statusline/cache/plugin-usage.json', file(first)]])
    const { seen } = await boot($, on, { needs, authorize: bearer, files, fetch: () => response(second) })
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage.body).toEqual(second)
    expect(JSON.parse(files.get(USAGE_PATH)!).body).toEqual(second)
  })

  test('a failed identity lookup uses private memory and retries without reading another account', { options: { intervalMs: MIN } }, async ($, on) => {
    const files = new Map([[USAGE_PATH, file(first)]])
    const { seen, clock } = await boot($, on, {
      needs, authorize: bearer, files, profile: () => response({}, 503), fetch: () => response(second),
    })
    expect(modOf(seen).usage.body).toEqual(second)
    expect(files.get(USAGE_PATH)).toBe(file(first))
    await clock.advance(5 * MIN - 1000)
    expect(seen.profiles).toHaveLength(1)
    expect(seen.fetches).toHaveLength(1)
    await clock.advance(1000)
    expect(seen.profiles).toHaveLength(2)
    expect(seen.fetches).toHaveLength(2)
    expect(files.size).toBe(2) // Existing usage and the session heartbeat.
    await clock.advance(5 * MIN)
    expect(seen.profiles).toHaveLength(2)
  })

  test('a slow profile never holds up the line and removal cancels subsequent usage work', async ($, on) => {
    const held = gate()
    let enabled = true
    const { seen, clock } = await boot($, on, {
      needs: () => ({ usage: enabled, autocompact: false }), authorize: bearer,
      profile: async () => { await held.wait; return response(PROFILE) }, fetch: () => response(first),
    })
    expect(seen.profiles).toHaveLength(1)
    expect(seen.fetches ?? []).toHaveLength(0)
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
    enabled = false
    await clock.advance(5000)
    held.open()
    await clock.settle()
    expect(seen.fetches ?? []).toHaveLength(0)
  })

  test('reauthorizing as a different account cannot write its result into the old cache', async ($, on) => {
    let changed = false
    const files = new Map<string, string>()
    const { seen, clock } = await boot($, on, {
      needs, files,
      authorize: () => ({ handle: changed ? 'new' : 'old', kind: 'bearer' }),
      profile: auth => response(auth === 'new' ? otherProfile : PROFILE),
      fetch: (_url, init) => init?.auth === 'old' ? response(first, changed ? 401 : 200) : response(second),
    })
    expect(modOf(seen).usage.body).toEqual(first)
    changed = true
    await clock.advance(MIN)
    expect(modOf(seen).usage.body).toEqual(second)
    expect(JSON.parse(files.get(USAGE_PATH)!).body).toEqual(first)
    expect(JSON.parse(files.get(otherPath)!).body).toEqual(second)
    expect(seen.fetches?.map(f => f.init?.auth)).toEqual(['old', 'old', 'new'])
    expect(seen.profiles).toHaveLength(2)
  })

  test('profile rate limits back off without blocking private usage refreshes', { options: { intervalMs: MIN } }, async ($, on) => {
    const { seen, clock } = await boot($, on, {
      needs, authorize: bearer, files: new Map(), profile: () => response({}, 429), fetch: () => response(second),
    })
    await clock.advance(5 * MIN - 1000)
    expect(seen.profiles).toHaveLength(1)
    expect(modOf(seen).usage.body).toEqual(second)
    await clock.advance(1000)
    expect(seen.profiles).toHaveLength(2)
    await clock.advance(9 * MIN)
    expect(seen.profiles).toHaveLength(2)
  })

  test('periodic reauthorization catches an account change while the old token still works', { options: { intervalMs: MIN } }, async ($, on) => {
    let changed = false
    const files = new Map<string, string>()
    const { seen, clock } = await boot($, on, {
      needs, files,
      authorize: () => ({ handle: changed ? 'new' : 'old', kind: 'bearer' }),
      profile: auth => response(auth === 'new' ? otherProfile : PROFILE),
      fetch: (_url, init) => response(init?.auth === 'old' ? first : second),
    })
    changed = true
    await clock.advance(14 * MIN)
    expect(seen.profiles).toHaveLength(1)
    await clock.advance(MIN)
    expect(seen.profiles).toHaveLength(2)
    expect(modOf(seen).usage.body).toEqual(second)
    expect(JSON.parse(files.get(USAGE_PATH)!).body).toEqual(first)
    expect(JSON.parse(files.get(otherPath)!).body).toEqual(second)
  })

  test('malformed successful profiles also back off progressively', { options: { intervalMs: MIN } }, async ($, on) => {
    const { seen, clock } = await boot($, on, {
      needs, authorize: bearer, files: new Map(), profile: () => response({ organization: null }), fetch: () => response(second),
    })
    await clock.advance(5 * MIN)
    expect(seen.profiles).toHaveLength(2)
    await clock.advance(10 * MIN)
    expect(seen.profiles).toHaveLength(3)
    await clock.advance(19 * MIN)
    expect(seen.profiles).toHaveLength(3)
    await clock.advance(MIN)
    expect(seen.profiles).toHaveLength(4)
  })
})
