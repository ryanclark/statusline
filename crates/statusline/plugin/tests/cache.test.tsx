import { describe, expect, test } from 'claude-code/testing'
import type { AgentInfo } from 'claude-code'

import { boot, compose, inputOf, quiet, S0, step, transcript, usage } from './host'

describe('prompt cache misses', () => {
  test('the first cache write is neither a miss nor a rebuild', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { steps: [{ usage: usage(0, 50000) }] })
    await step($, clock)
    expect(inputOf(seen).prompt_cache).toMatchObject({
      requests: 1,
      misses: 0,
      expected_rebuilds: 0,
      miss_times: [],
      last_miss_at: null,
      last_miss_cause: null,
      miss_causes: {},
      recache_tokens_if_cold: 50000,
    })
  })

  test(
    'a request after the TTL that reads nothing is a ttl miss',
    { options: { ...quiet, cacheTtl: '5m' } },
    async ($, on) => {
      const steps = [{ usage: usage(0, 50000) }, { usage: usage(50000, 2000) }, { usage: usage(0, 52500) }]
      const { seen, clock } = await boot($, on, { steps })
      await step($, clock)
      await step($, clock)
      expect(inputOf(seen).prompt_cache.misses).toBe(0)
      await clock.advance(6 * 60_000)
      await step($, clock)
      expect(inputOf(seen).prompt_cache).toMatchObject({
        requests: 3,
        misses: 1,
        expected_rebuilds: 0,
        miss_times: [S0 + 360],
        last_miss_at: S0 + 360,
        last_miss_cause: { causes: ['ttl_expired'] },
        miss_causes: { ttl_expired: 1 },
        miss_recache_tokens: 52500,
      })
    },
  )

  test(
    'a read under half the cached prefix is a miss, with the tool and system changes',
    { options: quiet },
    async ($, on) => {
      let system = 'x'.repeat(100)
      const steps = [{ usage: usage(0, 50000) }, { usage: usage(48000, 3000) }, { usage: usage(20000, 33000) }]
      const { seen, clock } = await boot($, on, { steps, system: () => system })
      await compose($, ['Bash', 'Read'])
      await step($, clock)
      await compose($, ['Read', 'Bash'])
      await step($, clock)
      // Most of the prefix was read, so the breakpoint moving is not a miss.
      expect(inputOf(seen).prompt_cache.misses).toBe(0)
      system = 'x'.repeat(140)
      await compose($, ['Bash', 'Read', 'Grep'])
      await step($, clock)
      expect(inputOf(seen).prompt_cache).toMatchObject({
        misses: 1,
        last_miss_cause: {
          causes: ['tools_changed', 'system_changed'],
          tools_added: 1,
          tools_removed: 0,
          system_char_delta: 40,
        },
        miss_causes: { tools_changed: 1, system_changed: 1 },
        miss_recache_tokens: 33000,
      })
    },
  )

  test('a model switch and a compaction are expected rebuilds, not misses', { options: quiet }, async ($, on) => {
    const sonnet = 'claude-sonnet-5'
    const steps = [{ usage: usage(0, 50000) }, { usage: usage(0, 50000, sonnet) }, { usage: usage(0, 21000, sonnet) }]
    const { seen, clock } = await boot($, on, { steps })
    await step($, clock)
    await step($, clock, { model: sonnet })
    expect(inputOf(seen).prompt_cache).toMatchObject({ misses: 0, expected_rebuilds: 1 })
    await $.session.compact({ trigger: 'manual', messages: transcript })
    await step($, clock, { model: sonnet })
    expect(inputOf(seen).prompt_cache).toMatchObject({
      misses: 0,
      expected_rebuilds: 2,
      miss_times: [],
      last_miss_cause: null,
      miss_causes: {},
      miss_recache_tokens: 0,
    })
  })

  test('a miss with no cause seen is reported unexplained', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { steps: [{ usage: usage(0, 50000) }, { usage: usage(0, 50000) }] })
    await step($, clock)
    await step($, clock)
    expect(inputOf(seen).prompt_cache).toMatchObject({ misses: 1, last_miss_cause: { causes: [] }, miss_causes: {} })
  })

  test("a subagent's prompt and requests are not the main loop's", { options: quiet }, async ($, on) => {
    const steps = [{ usage: usage(0, 50000) }, { usage: usage(0, 9000) }, { usage: usage(0, 50000) }]
    const { seen, clock } = await boot($, on, { steps })
    await compose($, ['Bash', 'Read', 'Agent'])
    await step($, clock)
    // A subagent request may have claimed either loop's prompt, so the next main miss names no prompt change.
    await compose($, ['Read'])
    await step($, clock, { agentId: 'agent-1' })
    expect(inputOf(seen).prompt_cache).toMatchObject({ requests: 1, misses: 0 })
    await step($, clock)
    expect(inputOf(seen).prompt_cache).toMatchObject({ requests: 2, misses: 1, last_miss_cause: { causes: [] } })
  })

  test('a running subagent leaves a main miss without a prompt cause', { options: quiet }, async ($, on) => {
    const steps = [{ usage: usage(0, 50000) }, { usage: usage(0, 52000) }]
    const agents: AgentInfo[] = [{ id: 'a', description: 'a', type: 'Explore', status: 'running' }]
    const { seen, clock } = await boot($, on, { steps, agents })
    await compose($, ['Bash', 'Read'])
    await step($, clock)
    // Not yet sent a request, but it may have composed the prompt the main loop takes next.
    await compose($, ['Bash', 'Read', 'Grep'])
    await step($, clock)
    expect(inputOf(seen).prompt_cache).toMatchObject({ misses: 1, last_miss_cause: { causes: [] }, miss_causes: {} })
  })

  test(
    'a compose another loop may have claimed is not diffed against the next main prompt',
    { options: quiet },
    async ($, on) => {
      const steps = [
        { usage: usage(0, 50000) },
        { usage: usage(50000, 2000) },
        { usage: usage(0, 9000) },
        { usage: usage(0, 52000) },
      ]
      const { seen, clock } = await boot($, on, { steps })
      await compose($, ['Bash', 'Read', 'Agent'])
      await step($, clock)
      // Composed for the subagent but taken by the main loop's next request, so it is not the main prompt.
      await compose($, ['Read'])
      await step($, clock)
      await step($, clock, { agentId: 'agent-1' })
      await compose($, ['Bash', 'Read', 'Agent'])
      await step($, clock)
      expect(inputOf(seen).prompt_cache).toMatchObject({ misses: 1, last_miss_cause: { causes: [] } })
    },
  )

  test(
    'a main-loop compaction leaves no cold recache figure until the next request',
    { options: quiet },
    async ($, on) => {
      const { seen, clock } = await boot($, on, { steps: [{ usage: usage(0, 50000) }, { usage: usage(0, 21000) }] })
      await step($, clock)
      expect(inputOf(seen).prompt_cache.recache_tokens_if_cold).toBe(50000)
      await $.session.compact({ trigger: 'manual', messages: transcript })
      await clock.settle()
      expect(inputOf(seen).prompt_cache.recache_tokens_if_cold).toBeNull()
      await step($, clock)
      expect(inputOf(seen).prompt_cache).toMatchObject({ expected_rebuilds: 1, recache_tokens_if_cold: 21000 })
    },
  )
})
