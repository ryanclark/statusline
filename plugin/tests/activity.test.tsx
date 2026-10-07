import { describe, expect, test } from 'claude-code/testing'
import type { AgentInfo } from 'claude-code'

import { boot, compose, gate, inputOf, modOf, quiet, S0, step, T0, transcript, usage } from './host'

describe('activity', () => {
  test('is empty but for the autocompact headroom while nothing happens', async ($, on) => {
    const { seen } = await boot($, on)
    expect(modOf(seen)).toEqual({
      tools: [],
      turn: null,
      permission: null,
      last_error: null,
      todos: null,
      agents: null,
      compaction: null,
      autocompact: { enabled: true, headroom_tokens: 38000 },
    })
  })

  test('autocompact off has no headroom', async ($, on) => {
    const { seen } = await boot($, on, { breakdown: { isAutoCompactEnabled: false, totalTokens: 129000 } })
    expect(modOf(seen).autocompact).toEqual({ enabled: false, headroom_tokens: null })
  })

  test('autocompact is unknown without a breakdown', async ($, on) => {
    const { seen } = await boot($, on, { breakdown: null })
    expect(modOf(seen).autocompact).toBeNull()
  })

  test('the breakdown is asked for at most every 10s, not every tick', async ($, on) => {
    const { seen, clock } = await boot($, on)
    expect(seen.breakdowns).toBe(1)
    await clock.advance(9000)
    expect(seen.breakdowns).toBe(1)
    await clock.advance(1000)
    expect(seen.breakdowns).toBe(2)
    await clock.advance(3000)
    expect(seen.breakdowns).toBe(2)
  })

  test('main-loop tools in flight, oldest first, with their main argument', { options: quiet }, async ($, on) => {
    const bash = gate()
    const { seen, clock } = await boot($, on, { tool: { holds: 'Bash', wait: bash.wait } })
    const call = $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'cargo test -p core\necho done' })
    await clock.settle()
    await clock.advance(2000)
    const held = $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_2', command: '' })
    // Read answers at once, so it is gone by the time the binary is next asked.
    await $.tool.call({ tool: 'Read', tool_use_id: 'toolu_3', file_path: '/repo/README.md' })
    await clock.settle()
    expect(modOf(seen).tools).toEqual([
      { tool: 'Bash', detail: 'cargo test -p core', started_at_ms: T0 },
      { tool: 'Bash', detail: null, started_at_ms: T0 + 2000 },
    ])
    bash.open()
    await call
    await held
    await clock.settle()
    expect(modOf(seen).tools).toEqual([])
  })

  test(
    "a long argument is cut to 80 characters, and a subagent's tools are not shown",
    { options: quiet },
    async ($, on) => {
      const read = gate()
      const { seen, clock } = await boot($, on, { tool: { holds: 'Read', wait: read.wait } })
      const long = `/repo/${'d'.repeat(100)}/main.rs`
      const main = $.tool.call({ tool: 'Read', tool_use_id: 'toolu_1', file_path: long })
      const sub = $.tool.call({ tool: 'Read', tool_use_id: 'toolu_2', file_path: '/sub', agentId: 'agent-1' } as never)
      await clock.settle()
      const tools = modOf(seen).tools
      expect(tools).toHaveLength(1)
      expect(tools[0].detail).toHaveLength(80)
      expect(tools[0].detail).toBe(`${long.slice(0, 79)}…`)
      read.open()
      await main
      await sub
      await clock.settle()
    },
  )

  test('text is cut between code points, never inside a surrogate pair', { options: quiet }, async ($, on) => {
    const read = gate()
    const { seen, clock } = await boot($, on, { tool: { holds: 'Read', wait: read.wait } })
    // The emoji's two code units straddle the cut at the 79th unit.
    const path = `${'a'.repeat(78)}😀${'b'.repeat(10)}`
    const call = $.tool.call({ tool: 'Read', tool_use_id: 'toolu_1', file_path: path })
    await clock.settle()
    // A lone surrogate is escaped by JSON.stringify, and the binary's parser rejects the whole input over it.
    expect(seen.stdin).not.toMatch(/\\ud[89a-f][0-9a-f]{2}/i)
    expect(modOf(seen).tools[0].detail).toBe(`${'a'.repeat(78)}😀…`)
    read.open()
    await call
    await $.classic.StopFailure({ error: 'overloaded', error_details: `${'x'.repeat(199)}😀 more` })
    await clock.settle()
    expect(seen.stdin).not.toMatch(/\\ud[89a-f][0-9a-f]{2}/i)
    expect(modOf(seen).last_error.detail).toBe(`${'x'.repeat(199)}😀`)
  })

  test('turn timing: running, then the last duration once it completes', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on)
    await $.turn.start({ text: 'fix it', turnId: 't1' })
    await clock.settle()
    expect(modOf(seen).turn).toEqual({ started_at_ms: T0, last_duration_ms: null, ended_at_ms: null })
    await clock.advance(130_000)
    // A subagent's run ends inside the main turn and says nothing of it.
    await $.turn.complete({
      answer: '',
      durationMs: 5000,
      isAborted: false,
      turnId: 's1',
      agentId: 'agent-1',
      reason: 'answer',
    })
    await clock.settle()
    expect(modOf(seen).turn.started_at_ms).toBe(T0)
    await $.turn.complete({ answer: 'done', durationMs: 130_000, isAborted: false, turnId: 't1', reason: 'answer' })
    await clock.settle()
    expect(modOf(seen).turn).toEqual({ started_at_ms: null, last_duration_ms: 130_000, ended_at_ms: T0 + 130_000 })
  })

  test(
    'a permission wait shows until its tool ends, is denied, or the turn ends',
    { options: quiet },
    async ($, on) => {
      const { seen, clock } = await boot($, on)
      const bash = { tool_name: 'Bash', tool_input: { command: 'rm -rf target' } }
      await $.classic.PermissionRequest(bash)
      await clock.settle()
      expect(modOf(seen).permission).toEqual({ tool: 'Bash', since_ms: T0 })
      // The same tool finishing in a subagent is not the call waiting on the user.
      await $.classic.PostToolUse({ ...bash, tool_response: {}, tool_use_id: 'toolu_9', agent_id: 'agent-1' })
      await clock.settle()
      expect(modOf(seen).permission).not.toBeNull()
      await $.classic.PostToolUse({ ...bash, tool_response: {}, tool_use_id: 'toolu_1' })
      await clock.settle()
      expect(modOf(seen).permission).toBeNull()

      await $.classic.PermissionRequest({ tool_name: 'Edit', tool_input: {} })
      await $.classic.PermissionDenied({
        tool_name: 'Edit',
        tool_input: {},
        tool_use_id: 'toolu_2',
        reason: 'user rejected',
      })
      await clock.settle()
      expect(modOf(seen).permission).toBeNull()

      await $.classic.PermissionRequest(bash)
      await $.classic.PostToolUseFailure({ ...bash, tool_use_id: 'toolu_3', error: 'exit 1' })
      await clock.settle()
      expect(modOf(seen).permission).toBeNull()

      await $.classic.PermissionRequest(bash)
      await $.turn.complete({ answer: '', durationMs: 10, isAborted: true, turnId: 't1', reason: 'aborted' })
      await clock.settle()
      expect(modOf(seen).permission).toBeNull()
    },
  )

  test(
    'every pending permission is kept, the oldest shown, each ended by its own call',
    { options: quiet },
    async ($, on) => {
      const { seen, clock } = await boot($, on)
      const bash = { tool_name: 'Bash', tool_input: { command: 'make' } }
      const edit = { tool_name: 'Edit', tool_input: {} }
      await $.classic.PermissionRequest(bash)
      await clock.advance(1000)
      await $.classic.PermissionRequest(edit)
      await clock.advance(1000)
      await $.classic.PermissionRequest(bash)
      await clock.settle()
      expect(modOf(seen).permission).toEqual({ tool: 'Bash', since_ms: T0 })
      await $.classic.PostToolUse({ ...bash, tool_response: {}, tool_use_id: 'toolu_1' })
      await clock.settle()
      expect(modOf(seen).permission).toEqual({ tool: 'Edit', since_ms: T0 + 1000 })
      await $.classic.PermissionDenied({ ...edit, tool_use_id: 'toolu_2', reason: 'user rejected' })
      await clock.settle()
      expect(modOf(seen).permission).toEqual({ tool: 'Bash', since_ms: T0 + 2000 })
      await $.classic.PostToolUseFailure({ ...bash, tool_use_id: 'toolu_3', error: 'exit 1' })
      await clock.settle()
      expect(modOf(seen).permission).toBeNull()
    },
  )

  test("a main turn ending leaves a subagent's permission wait standing", { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on)
    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: { command: 'make' }, agent_id: 'agent-1' })
    await $.classic.PermissionRequest({ tool_name: 'Edit', tool_input: {} })
    await $.turn.complete({ answer: '', durationMs: 10, isAborted: true, turnId: 't1', reason: 'aborted' })
    await clock.settle()
    expect(modOf(seen).permission).toEqual({ tool: 'Bash', since_ms: T0 })
    // A subagent stopped mid-wait raises no PostToolUse, so its own run ending settles its waits.
    await $.turn.complete({
      answer: '',
      durationMs: 10,
      isAborted: true,
      turnId: 's1',
      agentId: 'agent-1',
      reason: 'aborted',
    })
    await clock.settle()
    expect(modOf(seen).permission).toBeNull()
  })

  test('a turn that sends no request does not inherit the last stop reason', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { steps: [{ usage: usage(0, 100), stopReason: 'max_tokens' }] })
    const done = (turnId: string) =>
      $.turn.complete({ answer: '', durationMs: 1000, isAborted: false, turnId, reason: 'answer' })
    await $.turn.start({ text: 'go', turnId: 't1' })
    await step($, clock)
    await done('t1')
    await clock.settle()
    expect(modOf(seen).last_error).toMatchObject({ kind: 'max_tokens' })
    await $.turn.start({ text: 'again', turnId: 't2' })
    await done('t2')
    await clock.settle()
    expect(modOf(seen).last_error).toBeNull()
  })

  test("the last error keeps the API's word, and a good turn clears it", { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { steps: [{ usage: usage(0, 100), stopReason: 'max_tokens' }] })
    const done = (reason: 'answer' | 'aborted' | 'error') =>
      $.turn.complete({ answer: '', durationMs: 1000, isAborted: reason === 'aborted', turnId: 't1', reason })

    await $.turn.start({ text: 'go', turnId: 't1' })
    // A subagent's failure is reported to its spawner, not the session.
    await $.classic.StopFailure({ error: 'rate_limit', agent_id: 'agent-1' })
    await $.classic.StopFailure({ error: 'overloaded', error_details: '529 Overloaded' })
    await done('error')
    await clock.settle()
    expect(modOf(seen).last_error).toEqual({ kind: 'overloaded', detail: '529 Overloaded', at_ms: T0 })

    await clock.advance(1000)
    await $.turn.start({ text: 'again', turnId: 't2' })
    await done('error')
    await clock.settle()
    expect(modOf(seen).last_error).toEqual({ kind: 'error', detail: null, at_ms: T0 + 1000 })

    await done('aborted')
    await clock.settle()
    expect(modOf(seen).last_error).toMatchObject({ kind: 'aborted' })

    await $.turn.complete({
      answer: '',
      durationMs: 1000,
      isAborted: false,
      turnId: 't3',
      reason: 'refusal',
      refusal: { category: 'cyber', explanation: 'Declined to help' },
    })
    await clock.settle()
    expect(modOf(seen).last_error).toMatchObject({ kind: 'refusal', detail: 'Declined to help' })

    await step($, clock)
    await done('answer')
    await clock.settle()
    expect(modOf(seen).last_error).toMatchObject({ kind: 'max_tokens' })

    await step($, clock)
    await done('answer')
    await clock.settle()
    expect(modOf(seen).last_error).toBeNull()
  })

  test("todos follow the main loop's TodoWrite", { options: quiet }, async ($, on) => {
    const todo = (content: string, status: 'pending' | 'in_progress' | 'completed') => ({
      content,
      status,
      activeForm: `${content}ing`,
    })
    const newTodos = [todo('Build', 'completed'), todo('Test', 'in_progress'), todo('Ship', 'pending')]
    const { seen, clock } = await boot($, on, { tool: { result: () => ({ oldTodos: [], newTodos }) } })
    await $.tool.call({ tool: 'TodoWrite', tool_use_id: 'toolu_1', todos: newTodos })
    await clock.settle()
    expect(modOf(seen).todos).toEqual({ done: 1, total: 3, active: 'Testing' })
  })

  test(
    'todos follow the task tools, keeping the active form a listing leaves out',
    { options: quiet },
    async ($, on) => {
      let created = 0
      const listed = [
        { id: '1', subject: 'Run tests', status: 'in_progress', blockedBy: [] },
        { id: '2', subject: 'Fix lint', status: 'completed', blockedBy: [] },
        { id: '3', subject: 'Write docs', status: 'pending', blockedBy: [] },
      ]
      const results: Record<string, (e: { tool: string }) => unknown> = {
        TaskCreate: e => ({ task: { id: String(++created), subject: (e as unknown as { subject: string }).subject } }),
        TaskUpdate: e => ({
          success: true,
          taskId: (e as unknown as { taskId: string }).taskId,
          updatedFields: ['status'],
        }),
        TaskList: () => ({ tasks: listed }),
      }
      const { seen, clock } = await boot($, on, { tool: { result: e => results[e.tool]?.(e) } })
      await $.tool.call({
        tool: 'TaskCreate',
        tool_use_id: 'a',
        subject: 'Run tests',
        description: '',
        activeForm: 'Running tests',
      })
      await $.tool.call({ tool: 'TaskCreate', tool_use_id: 'b', subject: 'Fix lint', description: '' })
      await clock.settle()
      expect(modOf(seen).todos).toEqual({ done: 0, total: 2, active: null })
      await $.tool.call({ tool: 'TaskUpdate', tool_use_id: 'c', taskId: '1', status: 'in_progress' })
      await $.tool.call({ tool: 'TaskUpdate', tool_use_id: 'd', taskId: '2', status: 'completed' })
      await clock.settle()
      expect(modOf(seen).todos).toEqual({ done: 1, total: 2, active: 'Running tests' })
      await $.tool.call({ tool: 'TaskList', tool_use_id: 'e' })
      await clock.settle()
      expect(modOf(seen).todos).toEqual({ done: 1, total: 3, active: 'Running tests' })
      await $.tool.call({ tool: 'TaskUpdate', tool_use_id: 'f', taskId: '3', status: 'deleted' })
      await clock.settle()
      expect(modOf(seen).todos).toEqual({ done: 1, total: 2, active: 'Running tests' })
    },
  )

  test('agents are counted by whether they still work', async ($, on) => {
    const agent = (id: string, status: AgentInfo['status']): AgentInfo => ({
      id,
      description: id,
      type: 'Explore',
      status,
    })
    const agents = [
      agent('a', 'running'),
      agent('b', 'waiting'),
      agent('c', 'idle'),
      agent('d', 'completed'),
      agent('e', 'killed'),
    ]
    const { seen } = await boot($, on, { agents })
    expect(modOf(seen).agents).toEqual({ running: 1, idle: 2 })
  })

  test('agents that have all ended are not shown', async ($, on) => {
    const { seen } = await boot($, on, {
      agents: [{ id: 'a', description: 'a', type: 'Explore', status: 'completed' }],
    })
    expect(modOf(seen).agents).toBeNull()
  })

  test('a main-loop compaction shows while it runs, then its counts', { options: quiet }, async ($, on) => {
    const summarising = gate()
    const { seen, clock } = await boot($, on, { compact: { wait: summarising.wait } })
    const run = $.session.compact({ trigger: 'auto', messages: transcript })
    await clock.settle()
    expect(modOf(seen).compaction).toEqual({
      count: 0,
      last_at_ms: null,
      tokens_before: null,
      tokens_after: null,
      running_since_ms: T0,
      trigger: 'auto',
    })
    await clock.advance(18_000)
    summarising.open()
    await run
    await clock.settle()
    const after = {
      count: 1,
      last_at_ms: T0 + 18_000,
      tokens_before: 182000,
      tokens_after: 21000,
      running_since_ms: null,
      trigger: 'auto',
    }
    expect(modOf(seen).compaction).toEqual(after)
    await $.session.compact({ trigger: 'precompute', messages: transcript })
    await $.session.compact({ trigger: 'manual', messages: transcript, agentId: 'agent-1' })
    await clock.settle()
    expect(modOf(seen).compaction).toEqual(after)
  })

  test('a skipped compaction leaves nothing behind', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { compact: { result: { skip: 'blocked by PreCompact' } } })
    await $.session.compact({ trigger: 'manual', messages: transcript })
    await clock.settle()
    expect(modOf(seen).compaction).toBeNull()
  })

  test('a representative session', { options: { ...quiet, cacheTtl: '5m' } }, async ($, on) => {
    const bash = gate()
    const todos = [
      ...['Read', 'Plan', 'Build'].map(c => ({ content: c, status: 'completed' as const, activeForm: c })),
      { content: 'Run tests', status: 'in_progress' as const, activeForm: 'Running tests' },
      ...['Lint', 'Docs', 'Ship'].map(c => ({ content: c, status: 'pending' as const, activeForm: c })),
    ]
    const agents: AgentInfo[] = ['a', 'b', 'c'].map(id => ({ id, description: id, type: 'Explore', status: 'running' }))
    agents.push({ id: 'mate', teammateId: 'mate@team', description: 'mate', type: 'teammate', status: 'idle' })
    const { seen, clock } = await boot($, on, {
      steps: [{ usage: usage(0, 50000) }, { usage: usage(0, 52000) }],
      agents,
      tool: { holds: 'Bash', wait: bash.wait, result: () => ({ oldTodos: [], newTodos: todos }) },
    })
    await $.session.compact({ trigger: 'manual', messages: transcript })
    await $.turn.start({ text: 'go', turnId: 't1' })
    await compose($, ['Bash', 'Read'])
    await step($, clock)
    await $.classic.StopFailure({ error: 'overloaded', error_details: '529 Overloaded' })
    await $.turn.complete({ answer: '', durationMs: 130_000, isAborted: false, turnId: 't1', reason: 'error' })
    await clock.advance(6 * 60_000)
    await $.turn.start({ text: 'retry', turnId: 't2' })
    await compose($, ['Bash', 'Read', 'Grep'])
    await step($, clock)
    await $.tool.call({ tool: 'TodoWrite', tool_use_id: 'toolu_1', todos })
    await $.classic.PermissionRequest({ tool_name: 'Bash', tool_input: { command: 'cargo test -p core' } })
    const call = $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_2', command: 'cargo test -p core' })
    await clock.settle()
    const input = inputOf(seen)
    const T1 = T0 + 360_000
    expect(input.prompt_cache).toEqual({
      warm: true,
      caching_observed: true,
      ttl: '5m',
      expires_at: S0 + 660,
      requests: 2,
      misses: 1,
      expected_rebuilds: 0,
      hit_ratio: 0,
      cache_write_tokens: 102000,
      miss_recache_tokens: 52000,
      last_miss_at: S0 + 360,
      // The running agents may have composed the prompt the main loop took, so the tools change is not named.
      last_miss_cause: { causes: ['ttl_expired'] },
      miss_causes: { ttl_expired: 1 },
      miss_times: [S0 + 360],
      recache_tokens_if_cold: 52000,
    })
    expect(input.mod).toEqual({
      tools: [{ tool: 'Bash', detail: 'cargo test -p core', started_at_ms: T1 }],
      turn: { started_at_ms: T1, last_duration_ms: 130_000, ended_at_ms: T0 },
      permission: { tool: 'Bash', since_ms: T1 },
      last_error: { kind: 'overloaded', detail: '529 Overloaded', at_ms: T0 },
      todos: { done: 3, total: 7, active: 'Running tests' },
      agents: { running: 3, idle: 1 },
      compaction: {
        count: 1,
        last_at_ms: T0,
        tokens_before: 182000,
        tokens_after: 21000,
        running_since_ms: null,
        trigger: 'manual',
      },
      autocompact: { enabled: true, headroom_tokens: 38000 },
    })
    bash.open()
    await call
    await clock.settle()
  })
})
