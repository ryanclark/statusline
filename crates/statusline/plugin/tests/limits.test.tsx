import { describe, expect, test } from 'claude-code/testing'

import { rateLimits } from '../hooks/limits'
import { boot, gate, inputOf, mountHint, T0, USAGE_PATH } from './host'

const reset = (ms: number) => new Date(ms).toISOString()
const five = reset(T0 + 3_600_000)
const week = reset(T0 + 5 * 24 * 3_600_000)
const session = [
  { kind: 'five_hour' as const, percentUsed: 97, resetsAt: five },
  { kind: 'seven_day' as const, percentUsed: 66, resetsAt: week },
]
const body = {
  five_hour: { utilization: 100, resets_at: five },
  seven_day: { utilization: 67, resets_at: week },
}
const shared = (value: unknown = body, at: number | null = T0) => ({ fetched_at_ms: at, body: value as typeof body })
const bearer = () => ({ handle: 'h1', kind: 'bearer' as const })
const needs = () => ({ usage: true, autocompact: false })
const reply = (status = 200) => ({ status, ok: status === 200, headers: {}, text: JSON.stringify(body) })

describe('account rate limits', () => {
  test('the same shared reading replaces different per-chat observations', () => {
    const older = session.map(limit => ({ ...limit, percentUsed: limit.percentUsed - 2 }))
    const first = rateLimits(session, shared(), T0)
    expect(rateLimits(older, shared(), T0)).toEqual(first)
    expect(first).toEqual({
      five_hour: { used_percentage: 100, resets_at: (T0 + 3_600_000) / 1000 },
      seven_day: { used_percentage: 67, resets_at: (T0 + 5 * 24 * 3_600_000) / 1000 },
    })
  })

  test('missing, stale and implausibly future snapshots leave the session fallback', () => {
    const fallback = rateLimits(session, null, T0)
    for (const at of [null, T0 - 300_001, T0 + 60_001, NaN, Infinity, T0 + 0.5]) {
      expect(rateLimits(session, shared(body, at), T0)).toEqual(fallback)
    }
    expect(rateLimits(session, shared(null), T0)).toEqual(fallback)
    expect(rateLimits(session, shared(body, T0 + 5000), T0).five_hour.used_percentage).toBe(100)
  })

  test('bad fields fall back independently and do not overwrite gateway limits', () => {
    const native = [...session, { kind: 'spend_limit' as const, percentUsed: 12, resetsAt: five }]
    for (const invalid of [
      {}, { utilization: '99', resets_at: five }, { utilization: -1, resets_at: five },
      { utilization: Infinity, resets_at: five }, { utilization: 99, resets_at: 'invalid' },
    ]) {
      const limits = rateLimits(native, shared({ ...body, five_hour: invalid, spend_limit: body.five_hour }), T0)
      expect(limits.five_hour.used_percentage).toBe(97)
      expect(limits.seven_day.used_percentage).toBe(67)
      expect(limits.spend_limit.used_percentage).toBe(12)
    }
  })

  test('expired windows disappear even when both sources still report 100%', () => {
    const now = T0 + 3_600_000
    const limits = rateLimits(session, shared(body, now), now)
    expect(limits.five_hour).toBeUndefined()
    expect(limits.seven_day.used_percentage).toBe(67)
  })

  test('a newer session window is not replaced by an older shared window', () => {
    const newer = [{ kind: 'five_hour' as const, percentUsed: 2, resetsAt: reset(T0 + 5 * 3_600_000) }]
    expect(rateLimits(newer, shared(), T0).five_hour.used_percentage).toBe(2)
  })

  test('real API timestamps with microseconds match headers rounded to the next second', () => {
    const now = Date.parse('2026-10-09T12:00:00Z')
    const native = [{ kind: 'five_hour' as const, percentUsed: 97, resetsAt: '2026-10-09T12:40:01Z' }]
    const limits = rateLimits(native, shared({
      five_hour: { utilization: 100, resets_at: '2026-10-09T12:40:00.187907+00:00' },
      seven_day: { utilization: 67, resets_at: '2026-10-11T23:00:00.187931+00:00' },
    }, now), now)
    expect(limits.five_hour.used_percentage).toBe(100)
    expect(limits.seven_day.used_percentage).toBe(67)
    expect(limits.five_hour.resets_at).toBe(Date.parse('2026-10-09T12:40:00Z') / 1000)
  })

  test('a cached null does not hide a session window opened after the snapshot', () => {
    const native = [
      { kind: 'five_hour' as const, percentUsed: 2, resetsAt: reset(T0 + 5 * 3_600_000 + 30_000) },
      { kind: 'seven_day' as const, percentUsed: 1, resetsAt: reset(T0 + 7 * 24 * 3_600_000 + 30_000) },
    ]
    const limits = rateLimits(native, shared({ five_hour: null, seven_day: null }), T0 + 60_000)
    expect(limits.five_hour.used_percentage).toBe(2)
    expect(limits.seven_day.used_percentage).toBe(1)
  })

  test('a new shared window can correctly decrease usage to zero', () => {
    const next = shared({ five_hour: { utilization: 0, resets_at: reset(T0 + 5 * 3_600_000) } })
    expect(rateLimits(session, next, T0).five_hour.used_percentage).toBe(0)
    expect(rateLimits(session, shared({ five_hour: null }), T0).five_hour.used_percentage).toBe(97)
  })

  test('an idle plugin uses freshly fetched account limits in the binary input', async ($, on) => {
    const { seen, clock } = await boot($, on, {
      needs, authorize: bearer, files: new Map(), rateLimits: () => session, fetch: () => reply(),
    })
    expect(inputOf(seen).rate_limits.five_hour.used_percentage).toBe(100)
    expect(inputOf(seen).rate_limits.seven_day.used_percentage).toBe(67)
    await clock.advance(59_000)
    expect(seen.fetches).toHaveLength(1)
    expect(seen.profiles).toHaveLength(1)
    expect(inputOf(seen).rate_limits.five_hour.used_percentage).toBe(100)
  })

  test('cached account limits draw while the next HTTP request is stalled', async ($, on) => {
    const held = gate()
    const files = new Map([[USAGE_PATH, JSON.stringify({ ...shared(body, T0 - 120_000), backoff_until_ms: 0, backoff_ms: 0 })]])
    const { seen, clock } = await boot($, on, {
      needs, authorize: bearer, files, rateLimits: () => session,
      fetch: async () => { await held.wait; return reply() },
    })
    expect(inputOf(seen).rate_limits.five_hour.used_percentage).toBe(100)
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
    await clock.advance(5000)
    expect(seen.fetches).toHaveLength(1)
    expect(inputOf(seen).rate_limits.seven_day.used_percentage).toBe(67)
    held.open()
    await clock.settle()
  })

  test('failed background requests leave the per-chat limits visible', async ($, on) => {
    const { seen } = await boot($, on, {
      needs, authorize: bearer, files: new Map(), rateLimits: () => session, fetch: () => reply(503),
    })
    expect(inputOf(seen).rate_limits.five_hour.used_percentage).toBe(97)
    expect(inputOf(seen).rate_limits.seven_day.used_percentage).toBe(66)
  })
})
