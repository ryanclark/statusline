import { describe, expect, test } from 'claude-code/testing'

import { boot, hint } from './host'

for (const surface of ['terminal', 'desktop'] as const) {
  describe(surface, () => {
    test('below: draws every row in the hint line, links intact', async ($, on) => {
      const { seen } = await boot($, on)
      const ui = await $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: hint() })
      expect(seen.argv).toEqual([expect.any(String), '--format', 'spans', '--heartbeat-ms', '10000'])
      expect(await ui.find({ text: /12%/ })).toBeDefined()
      expect(await ui.find({ text: /second row/ })).toBeDefined()
      expect(await ui.find({ type: 'Link' })).toMatchObject({ props: { href: 'https://github.com/o/r/pull/1' } })
      expect(await ui.find({ text: /esc to interrupt/ })).toBeUndefined()
    })

    test('below: a running turn keeps the esc hint at the end of the line', async ($, on) => {
      await boot($, on)
      const ui = await $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: hint(true) })
      expect(await ui.find({ text: /12%/ })).toBeDefined()
      expect(await ui.find({ text: /esc to interrupt/ })).toBeDefined()
    })

    test('a failing binary shows its first stderr line without escapes', async ($, on) => {
      await boot($, on, {
        out: { exitCode: 1, stdout: '', stderr: '\u001b[1;31m! error:\u001b[0m reading settings\nmore' },
      })
      const ui = await $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: hint() })
      expect(await ui.find({ text: /exited 1: ! error: reading settings$/ })).toBeDefined()
    })

    test('output that is not span rows is an error, not a crash', async ($, on) => {
      await boot($, on, { out: { exitCode: 0, stdout: '{}', stderr: '' } })
      const ui = await $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: hint() })
      expect(await ui.find({ text: /did not print span rows/ })).toBeDefined()
    })

    test('output that is not JSON names the spans build, not a parse error', async ($, on) => {
      await boot($, on, { out: { exitCode: 0, stdout: '\u001b[32m12%\u001b[0m warm', stderr: '' } })
      const ui = await $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: hint() })
      expect(await ui.find({ text: /did not print span rows, it needs --format spans$/ })).toBeDefined()
    })

    test('an empty render leaves the engine its hint line', async ($, on) => {
      await boot($, on, { out: { exitCode: 0, stdout: '[[]]', stderr: '' } })
      const ui = await $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: hint() })
      expect(await ui.find({ text: 'engine' })).toBeDefined()
    })

    test(
      'above: draws in the band and leaves the hint line alone',
      { options: { placement: 'above' } },
      async ($, on) => {
        await boot($, on)
        const band = await $.ui.mount({
          plugin: 'statusline',
          surface,
          component: 'AbovePrompt',
          props: {
            hasSurvey: false,
            isWorking: false,
            maxRows: 10,
            bodyColumns: 80,
            scroll: { bodyRows: 9, offset: 0 },
            view: {},
          },
        })
        expect(await band.find({ text: /12%/ })).toBeDefined()
        const line = await $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: hint() })
        expect(await line.find({ text: /12%/ })).toBeUndefined()
        expect(await line.find({ text: 'engine' })).toBeDefined()
      },
    )
  })
}
