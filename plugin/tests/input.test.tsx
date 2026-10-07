import { describe, expect, test } from 'claude-code/testing'

import { boot, inputOf, opus, step } from './host'

describe('input', () => {
  test('is built from the live session alone', async ($, on) => {
    const { seen } = await boot($, on)
    const input = inputOf(seen)
    expect(input.session_id).toBe('abc')
    expect(input.model).toEqual({ id: opus, display_name: '' })
    expect(input.version).toBe('2.1.290')
    expect(input.workspace).toEqual({ current_dir: '/work', project_dir: '/repo-root' })
    expect(input.cost.total_cost_usd).toBe(1.5)
    // seven_day has no reset time, and the binary rejects a window without one.
    expect(input.rate_limits).toEqual({ five_hour: { used_percentage: 12, resets_at: 1791291600 } })
    expect(input.context_window).toMatchObject({
      context_window_size: 200000,
      total_input_tokens: 1000,
      current_usage: null,
      used_percentage: 1,
      remaining_percentage: 99,
    })
    expect(input.prompt_cache).toBeUndefined()
  })

  test('a main-loop response fills current usage, effort and a warm cache', async ($, on) => {
    const { seen, clock } = await boot($, on)
    await step($, clock)
    const input = inputOf(seen)
    expect(input.context_window.current_usage).toEqual({
      input_tokens: 2,
      output_tokens: 7,
      cache_creation_input_tokens: 6430,
      cache_read_input_tokens: 234480,
    })
    expect(input.context_window.total_output_tokens).toBe(7)
    expect(input.effort).toEqual({ level: 'high' })
    expect(input.prompt_cache).toMatchObject({ warm: true, caching_observed: true, ttl: '1h', requests: 1 })
    expect(input.prompt_cache.expires_at).toBeGreaterThan(0)
  })

  test('a subagent response is not the main context', async ($, on) => {
    const { seen, clock } = await boot($, on)
    await step($, clock, { agentId: 'agent-1' })
    const input = inputOf(seen)
    expect(input.context_window.current_usage).toBeNull()
    expect(input.prompt_cache).toBeUndefined()
  })

  test(
    'the configured TTL is assumed until a model switch reports one',
    { options: { cacheTtl: '5m' } },
    async ($, on) => {
      const { seen, clock } = await boot($, on)
      await step($, clock)
      expect(inputOf(seen).prompt_cache.ttl).toBe('5m')
    },
  )
})
