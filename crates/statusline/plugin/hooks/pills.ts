export type Pills = { pills: string[]; selected: boolean }

export const NO_PILLS: Pills = { pills: [], selected: false }

// The nouns the footer counts, taken from the Claude Code 2.1.290 binary. The hint reaches a plugin read the way a
// screen reader reads it, so the parts may be joined by " · " or by one space and a pill is found by its shape alone.
const PILL = /\b\d+ (?:(?:local|cloud|mcp) )?(?:shell|monitor|workflow|agent|teammate|task|dream)s?\b/gi

// The engine only words the selection as "Enter to view ...". It does not say which pill holds the focus, so `selected`
// is a fact about the whole row and the caller lights every pill.
const VIEW = /\benter to view\b/i

export function parsePills(hint: unknown): Pills {
  if (typeof hint !== 'string' || hint.length > 500) {
    return NO_PILLS
  }
  const pills = hint.match(PILL) ?? []
  if (pills.length === 0) {
    return NO_PILLS
  }
  return { pills, selected: VIEW.test(hint) }
}
