import { describe, expect, test } from 'claude-code/testing'
import type { Plugin } from 'claude-code/testing'

import { boot, modOf, quiet } from './host'

// What each tool answers once its work is off in the background, as the engine's tool results carry it.
const results: Record<string, (e: { tool: string }) => unknown> = {
  Bash: e => {
    const args = e as unknown as { command: string; run_in_background?: boolean }
    return args.run_in_background
      ? { stdout: '', stderr: '', interrupted: false, backgroundTaskId: `b-${args.command}` }
      : { stdout: 'ok', stderr: '', interrupted: false }
  },
  Monitor: () => ({ taskId: 'm1', timeoutMs: 300000 }),
  Workflow: () => ({ status: 'async_launched', taskId: 'w1', taskType: 'local_workflow', workflowName: 'review' }),
  TaskStop: e => {
    const id = (e as unknown as { task_id: string }).task_id
    return { message: `Stopped ${id}`, task_id: id, task_type: 'local_bash' }
  },
}
const result = (e: { tool: string }) => results[e.tool]?.(e) ?? {}

// A task's end as Claude reads it, which the engine submits as a prompt of the notification's own origin.
const notification = (id: string, status?: string) =>
  [
    '<task-notification>',
    `<task-id>${id}</task-id>`,
    '<tool-use-id>toolu_1</tool-use-id>',
    ...(status ? [`<status>${status}</status>`] : []),
    `<summary>Background command "${id}" ${status ?? 'wrote a line'}</summary>`,
    '</task-notification>',
  ].join('\n')
const fromTask = { kind: 'task-notification' } as const

const summary = (id: string, type: string, extra: Record<string, string> = {}) => ({
  id,
  type,
  status: 'running',
  description: `${type} ${id}`,
  ...extra,
})

describe('background tasks', () => {
  test('a backgrounded Bash, a Monitor and a Workflow each add one', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { tool: { result } })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'cargo test', description: 'Run tests' })
    await $.tool.call({
      tool: 'Bash',
      tool_use_id: 'toolu_2',
      command: 'npm run dev',
      description: 'Start the dev server',
      run_in_background: true,
    })
    await $.tool.call({
      tool: 'Monitor',
      tool_use_id: 'toolu_3',
      description: 'CI on #42',
      command: 'gh pr checks 42 --watch',
      timeout_ms: 300000,
    })
    await $.tool.call({ tool: 'Workflow', tool_use_id: 'toolu_4', name: 'review' })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([
      { type: 'shell', description: 'Start the dev server' },
      { type: 'monitor', description: 'CI on #42' },
      { type: 'workflow', description: 'review' },
    ])
  })

  test('a shell with no description shows its command, cut to 80', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { tool: { result } })
    const long = `tail -f ${'x'.repeat(100)}`
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: long, run_in_background: true })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([{ type: 'shell', description: `${long.slice(0, 79)}…` }])
  })

  test("a synchronous subagent's shell, which ends with it, is not added", { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, {
      tool: {
        result: () => ({
          stdout: '',
          stderr: '',
          interrupted: false,
          backgroundTaskId: 'b1',
          backgroundEndsWithFinalResponse: true,
        }),
      },
    })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'sleep 60', agentId: 'agent-1' } as never)
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([])
  })

  test('TaskStop removes the task it stopped', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { tool: { result } })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'a', run_in_background: true })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_2', command: 'b', run_in_background: true })
    await $.tool.call({ tool: 'TaskStop', tool_use_id: 'toolu_3', task_id: 'b-a' })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([{ type: 'shell', description: 'b' }])
  })

  test("Stop's snapshot replaces the set and leaves subagents out", { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { tool: { result } })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'finished', run_in_background: true })
    await $.classic.Stop({
      stop_hook_active: false,
      background_tasks: [
        summary('b2', 'shell', { description: '', command: 'npm run dev' }),
        summary('a1', 'subagent', { agent_type: 'Explore' }),
        summary('m1', 'monitor', { server: 'ci', tool: 'watch' }),
        summary('w1', 'workflow', { description: 'Reviews the diff', name: 'review' }),
      ],
    })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([
      { type: 'shell', description: 'npm run dev' },
      { type: 'monitor', description: 'monitor m1' },
      { type: 'workflow', description: 'review' },
    ])
    await $.classic.Stop({ stop_hook_active: false, background_tasks: [] })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([])
  })

  test('a Stop without a snapshot keeps the set, and a SubagentStop snapshot settles it', async ($, on) => {
    const { seen, clock } = await boot($, on, { tool: { result } })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'a', run_in_background: true })
    await $.classic.Stop({ stop_hook_active: false })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([{ type: 'shell', description: 'a' }])
    await $.classic.SubagentStop({
      stop_hook_active: false,
      agent_id: 'agent-1',
      agent_type: 'Explore',
      agent_transcript_path: '/t/agent-1.jsonl',
      background_tasks: [summary('a1', 'subagent')],
    })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([])
  })

  // The test engine cannot reload the module, so a second plugin reads the value where a reload would find it and
  // hands it back in the only free text a Notification answer carries.
  const reader: Plugin = {
    name: 'reader',
    register: on => {
      on('classic.Notification', async ($, e, next) => {
        const { value } = await $.state.get({ plugin: 'statusline', key: 'live' })
        return { ...(await next(e)), stopReason: JSON.stringify(value?.background) }
      })
    },
  }

  test('the set is kept in session state, which a reload of the module keeps', { plugins: [reader] }, async ($, on) => {
    on('classic.Notification', () => ({}))
    const { clock } = await boot($, on, { tool: { result } })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'a', run_in_background: true })
    await clock.settle()
    const read = await $.classic.Notification({ message: 'idle', notification_type: 'idle_prompt' })
    expect(JSON.parse(read.stopReason ?? 'null')).toEqual({ 'b-a': { type: 'shell', description: 'a' } })
  })

  test("a task's notification of its end removes it, several at once", { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { tool: { result } })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_1', command: 'a', run_in_background: true })
    await $.tool.call({ tool: 'Bash', tool_use_id: 'toolu_2', command: 'b', run_in_background: true })
    await $.tool.call({ tool: 'Monitor', tool_use_id: 'toolu_3', description: 'CI', command: 'ci', timeout_ms: 1 })
    await $.prompt.submit({
      text: `${notification('b-a', 'completed')}\n${notification('m1', 'killed')}`,
      wait: false,
      origin: fromTask,
    })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([{ type: 'shell', description: 'b' }])
  })

  test('a notification that is not an end, or a typed one, keeps the task', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, { tool: { result } })
    await $.tool.call({ tool: 'Monitor', tool_use_id: 'toolu_1', description: 'CI', command: 'ci', timeout_ms: 1 })
    await $.prompt.submit({ text: notification('m1'), wait: false, origin: fromTask })
    await $.prompt.submit({ text: notification('m1', 'blocked'), wait: false, origin: fromTask })
    await $.prompt.submit({ text: notification('m1', 'completed'), wait: false, origin: { kind: 'composer' } })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([{ type: 'monitor', description: 'CI' }])
  })

  test('a remote Workflow and a remote agent in a Stop snapshot are not shown', { options: quiet }, async ($, on) => {
    const { seen, clock } = await boot($, on, {
      tool: {
        result: () => ({ status: 'remote_launched', taskId: 'r1', taskType: 'remote_agent', workflowName: 'review' }),
      },
    })
    await $.tool.call({ tool: 'Workflow', tool_use_id: 'toolu_1', name: 'review', remote: true } as never)
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([])
    await $.classic.Stop({
      stop_hook_active: false,
      background_tasks: [summary('r1', 'remote_agent'), summary('b1', 'shell', { command: 'make' })],
    })
    await clock.settle()
    expect(modOf(seen).background_tasks).toEqual([{ type: 'shell', description: 'shell b1' }])
  })
})
