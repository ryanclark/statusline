import { describe, expect, test } from 'claude-code/testing'

import { boot, gate, modOf, mountHint, T0, unknownFlag } from './host'

const noNeeds = () => ({ usage: false, autocompact: false })
const bearer = () => ({ handle: 'h1', kind: 'bearer' as const })
const body = { extra_usage: { monthly_limit: 10000, used_credits: 2500 } }
const reply = () => ({ status: 200, ok: true, headers: {}, text: JSON.stringify(body) })

describe('requested data', () => {

  test('authorization errors allow cookies and retry at most once a minute', async ($, on) => {
    let failing = true
    const { seen, clock } = await boot($, on, {
      needs: () => ({ usage: true, autocompact: false }), files: new Map(), fetch: reply,
      authorize: () => { if (failing) throw new Error('authorization unavailable'); return bearer() },
    })
    expect(modOf(seen).usage).toBeUndefined()
    await clock.advance(59_000)
    expect(seen.authorizes).toBe(1)
    expect(seen.fetches ?? []).toHaveLength(0)
    await clock.advance(1000)
    expect(seen.authorizes).toBe(2)
    expect(modOf(seen).usage).toBeUndefined()
    failing = false
    await clock.advance(60_000)
    expect(seen.authorizes).toBe(3)
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage.body).toEqual(body)
  })

  test('a binary downgrade can drop optional plugin data without breaking the line', async ($, on) => {
    let downgraded = false
    const { seen, clock } = await boot($, on, {
      needs: noNeeds,
      out: argv => downgraded && argv.includes('--plugin-data')
        ? unknownFlag('--plugin-data')
        : { exitCode: 0, stdout: JSON.stringify([[{ text: 'still drawing' }]]), stderr: '' },
    })
    expect(seen.argv).toContain('--plugin-data')
    downgraded = true
    await clock.advance(1000)
    expect(seen.argv?.includes('--plugin-data')).toBe(false)
    expect(await (await mountHint($)).find({ text: /still drawing/ })).toBeDefined()
  })
  test('unused usage and compaction segments do no authorization, HTTP or breakdown work', async ($, on) => {
    const { seen, clock } = await boot($, on, { needs: noNeeds, authorize: bearer, fetch: reply })
    await clock.advance(61_000)
    expect(seen.authorizes ?? 0).toBe(0)
    expect(seen.fetches ?? []).toHaveLength(0)
    expect(seen.breakdowns ?? 0).toBe(0)
    expect(seen.argv).toContain('--plugin-data')
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
  })

  test('enabling a usage segment starts a fetch; removing it stops subsequent requests', async ($, on) => {
    let enabled = false
    const { seen, clock } = await boot($, on, {
      needs: () => ({ usage: enabled, autocompact: false }),
      authorize: bearer, fetch: reply, files: new Map(),
    })
    expect(seen.authorizes ?? 0).toBe(0)
    enabled = true
    await clock.advance(1000)
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage.body).toEqual(body)
    await clock.advance(59_000)
    enabled = false
    await clock.advance(1000)
    await clock.advance(61_000)
    expect(seen.fetches).toHaveLength(1)
  })

  test('a slow authorization never holds up the first line or later renders', async ($, on) => {
    const held = gate()
    const { seen, clock } = await boot($, on, {
      needs: () => ({ usage: true, autocompact: false }),
      authorize: async () => { await held.wait; return bearer() },
      fetch: reply, files: new Map(),
    })
    expect(seen.authorizes).toBe(1)
    expect(seen.fetches ?? []).toHaveLength(0)
    expect(modOf(seen).usage).toEqual({ fetched_at_ms: null, body: null })
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
    const runs = seen.runs?.length ?? 0
    await clock.advance(5000)
    expect((seen.runs?.length ?? 0) > runs).toBe(true)
    expect(seen.authorizes).toBe(1)
    held.open()
    await clock.settle()
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage.body).toEqual(body)
  })

  test('removing a segment while authorization is pending prevents the HTTP request', async ($, on) => {
    const held = gate()
    let enabled = true
    const { seen, clock } = await boot($, on, {
      needs: () => ({ usage: enabled, autocompact: false }),
      authorize: async () => { await held.wait; return bearer() },
      fetch: reply,
    })
    enabled = false
    await clock.advance(1000)
    held.open()
    await clock.settle()
    expect(seen.fetches ?? []).toHaveLength(0)
  })

  test('cached usage is displayed while its background HTTP request is stalled', async ($, on) => {
    const held = gate()
    const files = new Map([['/home/me/.statusline/cache/plugin-usage.json', JSON.stringify({
      fetched_at_ms: T0 - 120_000, body, backoff_until_ms: 0, backoff_ms: 0,
    })]])
    const { seen, clock } = await boot($, on, {
      needs: () => ({ usage: true, autocompact: false }), authorize: bearer, files,
      fetch: async () => { await held.wait; return reply() },
    })
    expect(seen.fetches).toHaveLength(1)
    expect(modOf(seen).usage.body).toEqual(body)
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
    held.open()
    await clock.settle()
  })
})
