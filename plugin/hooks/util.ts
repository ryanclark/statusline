export type Json = Record<string, unknown>

export const obj = (v: unknown): Json | undefined => (v && typeof v === 'object' ? (v as Json) : undefined)
export const str = (v: unknown): string | null => (typeof v === 'string' ? v : null)

// Cut by code point: a lone surrogate from a UTF-16 cut is escaped by JSON.stringify, and serde_json then rejects the
// whole input.
export function cut(s: string, max: number, mark = ''): string {
  const points = Array.from(s)
  return points.length > max ? points.slice(0, max - mark.length).join('') + mark : s
}

// Text refuses control characters, and the binary colours its stderr.
export const plain = (s: string) =>
  s
    .replace(/\x1b\[[0-9;:]*[A-Za-z]/g, '')
    .replace(/\t/g, ' ')
    .replace(/[\x00-\x08\x0b-\x1f\x7f]/g, '')
    .trim()
    .split('\n')[0] ?? ''
