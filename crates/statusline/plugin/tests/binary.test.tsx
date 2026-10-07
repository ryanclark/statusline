import { describe, expect, test } from 'claude-code/testing'

import { boot, capableHelp, missing, mountHint, oldHelp, quiet, T0, unknownFlag } from './host'

describe('binary', () => {
  test('the heartbeat outlives three missed ticks', { options: quiet }, async ($, on) => {
    const { seen } = await boot($, on)
    expect(seen.argv).toEqual([expect.any(String), '--format', 'spans', '--heartbeat-ms', '30000'])
  })

  // The binary writes it only once a refresh runs, and until then the native line draws beside the plugin's.
  test('the heartbeat is written as the session starts, before the binary is asked anything', async ($, on) => {
    const { seen } = await boot($, on)
    const heartbeat = `write /home/me/.statusline/sessions/abc.plugin ${T0 + 10_000}`
    expect(seen.events?.slice(0, 2)).toEqual([heartbeat, 'run --help'])
  })

  test('the session ending expires its heartbeat, so the native line draws again at once', async ($, on) => {
    const { seen } = await boot($, on)
    await $.session.end({ reason: 'prompt_input_exit', sessionId: 'abc', resume: { id: 'abc' } })
    expect(seen.events?.at(-1)).toBe('write /home/me/.statusline/sessions/abc.plugin 0')
    // The id names a file, so one that is not a plain token could reach outside the sessions directory.
    const before = seen.events?.length
    await $.session.end({ reason: 'other', sessionId: '../escape', resume: { id: '../escape' } })
    expect(seen.events).toHaveLength(before ?? 0)
  })

  // clap parses --heartbeat-ms as u64, so a fraction or an exponent fails every refresh with exit 2.
  for (const [intervalMs, heartbeat] of [
    [3000.5, '11003'],
    [1e21, '30000'],
  ] as const) {
    test(`an interval of ${intervalMs} passes a whole heartbeat`, { options: { intervalMs } }, async ($, on) => {
      const { seen } = await boot($, on)
      expect(seen.argv).toEqual([expect.any(String), '--format', 'spans', '--heartbeat-ms', heartbeat])
    })
  }

  test('the build is probed once, and one with both flags draws', async ($, on) => {
    const { seen, clock } = await boot($, on)
    await clock.advance(3000)
    expect(seen.runs?.filter(argv => argv[1] === '--help')).toHaveLength(1)
    expect(seen.runs?.[0]?.[1]).toBe('--help')
    // The version only words a too-old message, so a capable build is never asked it.
    expect(seen.runs?.filter(argv => argv[1] === '--version')).toHaveLength(0)
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
  })

  // Checks `--help` for the flags rather than the version, since a source build can carry a release's number without
  // that release's flags.
  test('a build without --heartbeat-ms is too old whatever its version, and is not run', async ($, on) => {
    const help = { ...capableHelp, stdout: capableHelp.stdout.replace(/.*--heartbeat-ms.*\n/, '') }
    const version = { exitCode: 0, stdout: 'statusline 1.2.0\n', stderr: '' }
    const realPath = '/opt/homebrew/Cellar/statusline/1.2.0/bin/statusline'
    const { seen, clock } = await boot($, on, { help, version, realPath })
    await clock.advance(3000)
    const ui = await mountHint($)
    const line = 'statusline 1.2.0 is too old for this plugin: brew upgrade ryanclark/tap/statusline'
    expect(await ui.find({ text: line })).toBeDefined()
    expect(seen.runs?.map(argv => argv[1])).toEqual(['--help', '--version'])
  })

  // statusline is not on crates.io, so a binary in cargo's bin dir came from `just install` in a checkout.
  for (const [realPath, hint] of [
    ['/Users/me/.cargo/bin/statusline', 'just install (from a checkout)'],
    [
      '/home/me/.local/bin/statusline',
      'curl -fsSL https://raw.githubusercontent.com/ryanclark/statusline/main/install.sh | sh',
    ],
    ['/usr/bin/statusline', 'update statusline (brew upgrade ryanclark/tap/statusline, or rerun install.sh)'],
  ] as const) {
    test(`a build at ${realPath} that predates --version names its own install`, async ($, on) => {
      await boot($, on, { help: oldHelp, version: unknownFlag('--version'), realPath })
      const line = `statusline binary is too old for this plugin: ${hint}`
      expect(await (await mountHint($)).find({ text: line })).toBeDefined()
    })
  }

  test('a binary that rejects a refresh flag is too old', async ($, on) => {
    const realPath = '/usr/local/Cellar/statusline/1.1.0/bin/statusline'
    await boot($, on, { out: unknownFlag('--heartbeat-ms'), realPath })
    const ui = await mountHint($)
    const line = 'statusline 1.1.0 is too old for this plugin: brew upgrade ryanclark/tap/statusline'
    expect(await ui.find({ text: line })).toBeDefined()
  })

  test('a missing binary says how to install it', async ($, on) => {
    await boot($, on, { reject: missing })
    const ui = await mountHint($)
    const line = 'statusline not found: brew install ryanclark/tap/statusline, then statusline install --plugin'
    expect(await ui.find({ text: line })).toBeDefined()
  })

  test('a probe that outlasts its timeout is reported, then retried on the next refresh', async ($, on) => {
    let slow = true
    const reject = (argv: readonly string[]) =>
      argv[1] === '--help' && slow ? 'process.run: still running after 5000 ms, killed' : undefined
    const { seen, clock } = await boot($, on, { reject })
    expect(await (await mountHint($)).find({ text: /^statusline: .*still running after 5000 ms/ })).toBeDefined()
    slow = false
    await clock.advance(1000)
    expect(seen.runs?.filter(argv => argv[1] === '--help')).toHaveLength(2)
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
  })

  test('a binary that cannot start for another reason is not called missing', async ($, on) => {
    const reject = (argv: readonly string[]) => `EACCES: permission denied, posix_spawn '${argv[0]}'`
    const { seen, clock } = await boot($, on, { reject })
    expect(await (await mountHint($)).find({ text: /^statusline: .*EACCES: permission denied/ })).toBeDefined()
    await clock.advance(1000)
    expect(seen.runs?.filter(argv => argv[1] === '--help')).toHaveLength(2)
  })

  test('a probe that exits non-zero is left to the refresh', async ($, on) => {
    const help = { exitCode: 1, stdout: '', stderr: 'boom' }
    await boot($, on, { help, out: { exitCode: 3, stdout: '', stderr: 'bad input' } })
    expect(await (await mountHint($)).find({ text: /^statusline: .* exited 3: bad input$/ })).toBeDefined()
  })

  test('a line that could not be stored is stored again on the next refresh', async ($, on) => {
    let broken = true
    let fail = false
    on('state.set', (_$, e, next) => {
      if (e.key === 'rendered' && fail) {
        fail = false
        return { deny: 'state.set: store unavailable' }
      }
      return next(e)
    })
    const reject = (argv: readonly string[]) =>
      broken ? `EACCES: permission denied, posix_spawn '${argv[0]}'` : undefined
    const { clock } = await boot($, on, { reject })
    broken = false
    fail = true
    await clock.advance(1000)
    await clock.advance(1000)
    expect(await (await mountHint($)).find({ text: /12%/ })).toBeDefined()
  })

  test('a failure gathering the input is not taken for a missing binary', async ($, on) => {
    await boot($, on, { modelError: 'model not found' })
    expect(await (await mountHint($)).find({ text: /^statusline: .*model not found/ })).toBeDefined()
  })
})
