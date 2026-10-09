import type { EngineInterface } from 'claude-code'

import type { Rendered } from '../types'
import { parsePills } from './pills'

// Claude Code draws a statusLine command's uncoloured text in the theme's muted grey, not the terminal foreground,
// so spans without their own colour take that theme key to match the native line.
const DEFAULT_FG = 'inactive'

// The components `$.ui.resolve` hands back for the surface being drawn.
type Ui = ReturnType<EngineInterface['ui']['resolve']>

// The hint row's padding, the "⏵⏵ " before the mode label, and the " · " Claude Code draws between it and this
// line.
const ROW_PADDING = 4
const MODE_GLYPHS = 3
const SEPARATOR = 3
// Terminals disagree on whether ⏵ and ⏸ take one cell or two.
const GLYPH_SLACK = 2

// The label Claude Code draws for each permission mode. Default mode is reserved too, since what it draws there is
// unconfirmed and a few spare columns beat a wrapped row.
const MODE_LABELS: Record<string, string> = {
  default: 'manual mode on',
  acceptEdits: 'accept edits on',
  plan: 'plan mode on',
  auto: 'auto mode on',
  dontAsk: "don't ask on",
  bypassPermissions: 'bypass permissions on',
}
// Assumed for a mode not seen yet or not in the table, so the line never wraps.
const LONGEST_LABEL = Math.max(...Object.values(MODE_LABELS).map(label => label.length))

// Claude Code lays the mode label and this line out in one row and shrinks both when the line overflows, which wraps
// the label's " · " onto a second row. A fixed width keeps the line to what the label leaves.
export function hintWidth(columns: number | undefined, mode: string | null): number | undefined {
  if (columns === undefined) {
    return undefined
  }
  const label = (mode === null ? undefined : MODE_LABELS[mode])?.length ?? LONGEST_LABEL
  return Math.max(1, columns - ROW_PADDING - MODE_GLYPHS - label - SEPARATOR - GLYPH_SLACK)
}

// A line with no text and no error leaves the slot to Claude Code.
export const blank = (r: Rendered): boolean => !r.error && !r.update && r.rows.every(row => row.length === 0)

export function draw(ui: Ui, r: Rendered, working: boolean, hint?: unknown, width?: number) {
  const { Box, Text, Link } = ui
  if (r.error) {
    return (
      <Text color="error" wrap="truncate-end">
        {r.error}
      </Text>
    )
  }
  const lastRow = r.rows.length - 1
  const { pills, selected } = parsePills(hint)
  return (
    <Box flexDirection="column" width={width}>
      {r.update ? <Text color="success" wrap="truncate-end">{r.update}</Text> : null}
      {r.rows.map((row, i) => (
        // One Text per row so an overflowing row is cut once at its end rather than every span shrinking on its own.
        <Text key={`row${i}`} wrap="truncate-end">
          {i === 0 && pills.length > 0 ? (
            <Text key="pills">
              {pills.map((p, k) => (
                <Text key={`pill${k}`} color={selected ? 'inverseText' : 'cyan'} backgroundColor={selected ? 'cyan' : undefined}>
                  {k > 0 ? ` ${p}` : p}
                </Text>
              ))}
              <Text dimColor>{' · '}</Text>
            </Text>
          ) : null}
          {row.map((s, j) => {
            const text = (
              <Text
                key={`s${j}`}
                color={s.fg ?? DEFAULT_FG}
                backgroundColor={s.bg}
                bold={s.bold}
                dimColor={s.dim}
                italic={s.italic}
                underline={s.underline}
                strikethrough={s.strikethrough}
                inverse={s.inverse}
              >
                {s.text}
              </Text>
            )
            return s.href ? (
              <Link key={`l${j}`} href={s.href}>
                {text}
              </Link>
            ) : (
              text
            )
          })}
          {working && i === lastRow ? (
            <Text key="esc" dimColor>
              {' · esc to interrupt'}
            </Text>
          ) : null}
        </Text>
      ))}
    </Box>
  )
}
