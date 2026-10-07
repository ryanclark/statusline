import type { Span } from '../types'
import { obj } from './util'

export const NOT_FOUND = 'statusline not found: brew install ryanclark/tap/statusline, then statusline install --plugin'
// process.run also rejects on timeout or when the binary exists but cannot start, so only ENOENT means missing.
export const MISSING = /ENOENT|not found|No such file/i
export const REQUIRED_FLAGS = [/--format\b/, /--heartbeat-ms\b/]
// clap's wording for a flag a build predates. The probe sees it first, and a refresh catches a binary swapped later.
export const UNKNOWN_FLAG = /unexpected argument '--(format|heartbeat-ms)'/

// A `final` error holds until reload. Others show for one refresh, then the binary is probed again.
export type Verdict = { error: string; final: boolean } | null

export const isSpanRows = (v: unknown): v is Span[][] =>
  Array.isArray(v) && v.every(row => Array.isArray(row) && row.every(s => typeof obj(s)?.text === 'string'))

const parseVersion = (s: string): string | null => /statusline (\d+\.\d+\.\d+)/.exec(s)?.[1] ?? null

// Matches how the binary was installed, since upgrading the wrong install fails or adds a second copy.
// statusline is not on crates.io, so cargo's bin dir only holds a `just install` build.
function upgradeFor(realPath: string): string {
  if (/[\\/](Cellar|homebrew)[\\/]/i.test(realPath)) {
    return 'brew upgrade ryanclark/tap/statusline'
  }
  if (/[\\/]\.local[\\/]bin[\\/]/.test(realPath)) {
    return 'curl -fsSL https://raw.githubusercontent.com/ryanclark/statusline/main/install.sh | sh'
  }
  if (/[\\/]\.cargo[\\/]bin[\\/]/.test(realPath)) {
    return 'just install (from a checkout)'
  }
  return 'update statusline (brew upgrade ryanclark/tap/statusline, or rerun install.sh)'
}

// The version only appears in the message, so a build too old for `--version` still gets one.
export function tooOldMessage(versionOutput: string | null, realPath: string): string {
  const version = versionOutput === null ? null : parseVersion(versionOutput)
  return `statusline ${version ?? 'binary'} is too old for this plugin: ${upgradeFor(realPath)}`
}
