import type { EngineInterface } from 'claude-code'

import type { Rendered } from '../types'
import { parsePills } from './pills'

// Claude Code draws a statusLine command's uncoloured text in the theme's muted grey, not the terminal foreground,
// so spans without their own colour take that theme key to match the native line.
const DEFAULT_FG = 'inactive'

// The components `$.ui.resolve` hands back for the surface being drawn.
type Ui = ReturnType<EngineInterface['ui']['resolve']>

// A line with no text and no error leaves the slot to Claude Code.
export const blank = (r: Rendered): boolean => !r.error && r.rows.every(row => row.length === 0)

export function draw(ui: Ui, r: Rendered, working: boolean, hint?: unknown) {
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
    <Box flexDirection="column">
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
