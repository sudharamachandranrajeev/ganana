import type { Overview } from './api'
import { compact, money, pages, percent } from './format'

/** One or two plain sentences that say what the numbers mean. No AI call. */
export function summarise(o: Overview): string[] {
  const p = o.period
  if (p.calls === 0) {
    return [
      `No AI usage found in the last ${o.days} days yet.`,
      'Gaṇanā watches the usage logs of Claude Code, Codex, Gemini CLI, Qwen Code and opencode on this computer, and updates by itself when they change.',
    ]
  }
  const lines = [
    `In the last ${o.days} days your AI tools made ${compact(p.calls)} model calls and handled ${compact(p.total_tokens)} tokens, about ${pages(p.total_tokens)} pages of text.`,
  ]
  const top = o.models.find((m) => m.cost > 0)
  let cost = `At public API prices that is worth ${money(p.cost)}`
  if (p.unpriced_tokens > 0) cost += `, plus ${percent(p.unpriced_tokens, p.total_tokens)} of tokens from models without a known price`
  cost += '.'
  if (top) cost += ` ${top.name} did the most expensive work (${money(top.cost)}).`
  lines.push(cost)
  if (p.savings > 0) {
    lines.push(`Re-reading from the cache saved about ${money(p.savings)} compared with reading everything fresh.`)
  }
  const today = o.today
  if (today.calls > 0) {
    lines.push(`Today so far: ${compact(today.total_tokens)} tokens in ${compact(today.calls)} calls, worth ${money(today.cost)}.`)
  }
  return lines
}
