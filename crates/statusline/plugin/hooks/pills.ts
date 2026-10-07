export type Pills = { pills: string[]; selected: boolean }

export const NO_PILLS: Pills = { pills: [], selected: false }

// The nouns the footer counts, taken from the Claude Code 2.1.290 binary. A segment that is not a count plus one of these
// (with an optional kind word such as "local") is a hint and never a pill.
const PILL = /^\d+ (?:(?:local|cloud|mcp) )?(?:shell|monitor|workflow|agent|teammate|task|dream)s?$/i

// The engine only words the selection as "Enter to view ...". With several pills the hint does not say which one holds
// the focus, so `selected` is a fact about the row and the caller highlights nothing rather than guess.
const VIEW = /^enter to view\b/i

export function parsePills(hint: unknown): Pills {
  if (typeof hint !== 'string' || hint.length > 500) {
    return NO_PILLS
  }
  const segments = hint.split(' · ').map(s => s.trim())
  const pills = segments.filter(s => PILL.test(s))
  if (pills.length === 0) {
    return NO_PILLS
  }
  return { pills, selected: segments.some(s => VIEW.test(s)) }
}
