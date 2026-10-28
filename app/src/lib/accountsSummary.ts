// Plain sentences for the top of the Accounts page, built from the data with
// no AI call.

import type { AccountsReport } from './api'
import { compact, money } from './format'

export function summariseAccounts(r: AccountsReport): string[] {
  const accounts = r.accounts
  if (!accounts.length) {
    return [
      'No provider accounts added yet.',
      'Add an admin key to see what Anthropic, OpenAI or OpenRouter actually billed, or import a usage file from any other provider.',
    ]
  }
  const lines = [
    `Your ${accounts.length === 1 ? 'account was' : `${accounts.length} accounts were`} billed ${money(r.billed)} in the last ${r.days} days (UTC days, as providers count them).`,
  ]
  const top = accounts.toSorted((a, b) => b.totals.cost - a.totals.cost)[0]
  if (accounts.length > 1 && top.totals.cost > 0) {
    lines.push(`${top.name} cost the most: ${money(top.totals.cost)} for ${compact(top.totals.total_tokens)} tokens.`)
  }
  for (const a of accounts) {
    if (a.credit_total === null || a.credit_used === null) continue
    const left = a.credit_total - a.credit_used
    if (left < Math.max(1, a.credit_total / 10)) lines.push(`${a.name} is low on credit: ${money(Math.max(0, left))} left.`)
  }
  const failed = accounts.filter((a) => a.error)
  if (failed.length) {
    lines.push(`${failed.map((a) => a.name).join(', ')} could not be read at the last try. The card below says why.`)
  }
  return lines
}
