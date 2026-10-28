// Plain sentences for the top of the Plans & limits page, built from the
// data with no AI call.

import type { LimitWindow } from './api'
import { clock, duration, meterName } from './format'

const short = (minutes: number) => (minutes === 300 ? '5-hour window' : minutes === 10080 ? 'week' : `${minutes}-minute window`)

export function summariseLimits(windows: LimitWindow[], now: number): string[] {
  const live = windows.filter((w) => !w.reset_passed)
  if (!live.length) {
    return windows.length
      ? ['Every usage window you have used has reset since, so you start from zero.']
      : ['No plan limits seen yet. Connect Claude below, or use Codex CLI, and they appear here.']
  }
  const lines: string[] = []
  for (const meter of [...new Set(live.map((w) => w.meter))]) {
    const ws = live.filter((w) => w.meter === meter).sort((a, b) => a.window_minutes - b.window_minutes)
    const parts = ws.map((w) => `${Math.round(w.used_pct)}% of the ${short(w.window_minutes)}`)
    lines.push(`${meterName(meter)}: ${parts.join(' and ')} used.`)
    const full = ws.find((w) => w.used_pct >= 100)
    const filling = ws.filter((w) => w.full_at).sort((a, b) => a.full_at! - b.full_at!)[0]
    // A nearly full window is worth naming even when another one fills first.
    const almost = ws.find((w) => w.used_pct >= 90 && w !== filling)
    if (full) {
      lines.push(`The ${short(full.window_minutes)} is full until ${clock(full.resets_at * 1000, now)}, in ${duration(full.resets_at * 1000 - now)}.`)
      continue
    }
    if (filling) {
      lines.push(
        `At this pace the ${short(filling.window_minutes)} fills around ${clock(filling.full_at! * 1000, now)}, before it resets at ${clock(filling.resets_at * 1000, now)}.`,
      )
    }
    if (almost) {
      lines.push(
        `The ${short(almost.window_minutes)} is almost used up: ${100 - Math.round(almost.used_pct)}% left until it resets at ${clock(almost.resets_at * 1000, now)}.`,
      )
    }
    if (!filling && !almost && ws.some((w) => w.rate_per_hour !== null)) {
      lines.push('At this pace you stay under the limits.')
    }
  }
  return lines
}
