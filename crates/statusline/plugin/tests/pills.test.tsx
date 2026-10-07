import { describe, expect, test } from 'claude-code/testing'

import { parsePills } from '../hooks/pills'
import { boot } from './host'

const props = (hint: string, isWorking = false) => ({ isDraft: false, isWorking, hint })

describe('parsePills', () => {
  test('idle with pills, nothing selected', () => {
    expect(parsePills('3 shells · ← for agents · ↓ to manage')).toEqual({ pills: ['3 shells'], selected: false })
  })

  test('a selected pill', () => {
    expect(parsePills('3 shells · Enter to view tasks')).toEqual({ pills: ['3 shells'], selected: true })
  })

  test('no background tasks', () => {
    expect(parsePills('(shift+tab to cycle) · ← for agents')).toEqual({ pills: [], selected: false })
  })

  test('while a turn runs', () => {
    expect(parsePills('1 shell · esc to interrupt · ← for agents · ↓ to manage')).toEqual({
      pills: ['1 shell'],
      selected: false,
    })
  })

  test('several nouns', () => {
    expect(parsePills('1 shell · 2 monitors · 1 local agent · Enter to view tasks')).toEqual({
      pills: ['1 shell', '2 monitors', '1 local agent'],
      selected: true,
    })
  })

  test('unknown segments are ignored', () => {
    expect(parsePills('3 widgets · shell · 2 files changed · ← for agents')).toEqual({ pills: [], selected: false })
  })

  test('garbage', () => {
    expect(parsePills('')).toEqual({ pills: [], selected: false })
    expect(parsePills(undefined)).toEqual({ pills: [], selected: false })
    expect(parsePills(42)).toEqual({ pills: [], selected: false })
    expect(parsePills(' · · ')).toEqual({ pills: [], selected: false })
    expect(parsePills('x'.repeat(10_000))).toEqual({ pills: [], selected: false })
  })
})

for (const surface of ['terminal', 'desktop'] as const) {
  describe(surface, () => {
    const mount = ($: Parameters<typeof boot>[0], p: ReturnType<typeof props>) =>
      $.ui.mount({ plugin: 'statusline', surface, component: 'PromptHint', props: p })

    test('pills and the statusline share the first row', async ($, on) => {
      await boot($, on)
      const ui = await mount($, props('1 shell · ← for agents · ↓ to manage'))
      expect(await ui.find({ type: 'Text', text: /^1 shell$/ })).toMatchObject({ props: { color: 'cyan' } })
      expect(await ui.find({ text: /12%/ })).toBeDefined()
      expect(await ui.find({ text: /second row/ })).toBeDefined()
      expect(await ui.find({ text: /for agents/ })).toBeUndefined()
    })

    test('a selected pill is cyan background with dark text', async ($, on) => {
      await boot($, on)
      const ui = await mount($, props('1 shell · Enter to view tasks'))
      expect(await ui.find({ type: 'Text', text: /^1 shell$/ })).toMatchObject({
        props: { color: 'black', backgroundColor: 'cyan' },
      })
    })

    test('several pills with a selection highlight none', async ($, on) => {
      await boot($, on)
      const ui = await mount($, props('1 shell · 2 monitors · Enter to view tasks'))
      expect(await ui.find({ type: 'Text', text: /^ 2 monitors$/ })).toMatchObject({ props: { color: 'cyan' } })
      expect(await ui.find({ type: 'Text', text: /^1 shell$/ })).toMatchObject({ props: { color: 'cyan' } })
    })

    test('no pills leaves the line as before', async ($, on) => {
      await boot($, on)
      const ui = await mount($, props('(shift+tab to cycle) · ← for agents'))
      expect(await ui.find({ text: /12%/ })).toBeDefined()
      expect(await ui.find({ text: /shell/ })).toBeUndefined()
      expect(await ui.find({ text: / · $/ })).toBeUndefined()
    })

    test('the interrupt hint is added once, and not when the engine hint has it', async ($, on) => {
      await boot($, on)
      const bare = await mount($, props('1 shell', true))
      expect(await bare.find({ text: /esc to interrupt/ })).toBeDefined()
      const full = await mount($, props('1 shell · esc to interrupt · ← for agents', true))
      expect(await full.find({ text: /esc to interrupt/ })).toBeUndefined()
      expect(await full.find({ text: /1 shell/ })).toBeDefined()
    })
  })
}
