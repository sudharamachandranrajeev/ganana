const compactFmt = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 })
const wholeFmt = new Intl.NumberFormat()
const moneyFmt = new Intl.NumberFormat(undefined, { style: 'currency', currency: 'USD' })
const moneyCompactFmt = new Intl.NumberFormat(undefined, {
  style: 'currency',
  currency: 'USD',
  notation: 'compact',
  maximumFractionDigits: 1,
})

/** 1,284 / 12.9K / 4.2M */
export const compact = (n: number) => (Math.abs(n) < 10_000 ? wholeFmt.format(Math.round(n)) : compactFmt.format(n))
export const whole = (n: number) => wholeFmt.format(Math.round(n))

export function money(n: number): string {
  if (n > 0 && n < 0.01) return '< $0.01'
  return Math.abs(n) >= 100_000 ? moneyCompactFmt.format(n) : moneyFmt.format(n)
}

/** Roughly 750 words, or 1.5 printed pages, per 1,000 tokens. */
export const pages = (tokens: number) => compact(tokens / 667)

export function percent(part: number, whole: number): string {
  if (whole <= 0) return '0%'
  const p = (part / whole) * 100
  return p > 0 && p < 1 ? '< 1%' : `${Math.round(p)}%`
}

export function relativeTime(ms: number, now = Date.now()): string {
  const s = Math.round((now - ms) / 1000)
  if (s < 10) return 'just now'
  if (s < 60) return `${s} seconds ago`
  const m = Math.round(s / 60)
  if (m < 60) return m === 1 ? 'a minute ago' : `${m} minutes ago`
  const h = Math.round(m / 60)
  if (h < 24) return h === 1 ? 'an hour ago' : `${h} hours ago`
  return new Date(ms).toLocaleDateString()
}

export function dayLabel(date: string, style: 'short' | 'long' = 'short'): string {
  const d = new Date(`${date}T12:00:00`)
  return style === 'short'
    ? d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
    : d.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })
}

/** Friendly names for the ids the core uses. */
const NAMES: Record<string, string> = {
  anthropic: 'Anthropic (Claude)',
  openai: 'OpenAI (GPT)',
  google: 'Google (Gemini)',
  alibaba: 'Alibaba (Qwen)',
  deepseek: 'DeepSeek',
  xai: 'xAI (Grok)',
  mistral: 'Mistral',
  openrouter: 'OpenRouter',
  typesafe: 'TypeSafe',
  other: 'Other',
  'claude-code': 'Claude Code',
  codex: 'Codex CLI',
  'gemini-cli': 'Gemini CLI',
  'qwen-code': 'Qwen Code',
  opencode: 'opencode',
}
export const nameOf = (id: string) => NAMES[id] ?? id

/** "2 h 14 min", "35 min", "3 days 4 h". */
export function duration(ms: number): string {
  const min = Math.max(0, Math.round(ms / 60_000))
  if (min < 60) return `${min} min`
  const h = Math.floor(min / 60)
  if (h < 24) return min % 60 ? `${h} h ${min % 60} min` : `${h} h`
  const d = Math.floor(h / 24)
  return `${d} day${d > 1 ? 's' : ''}${h % 24 ? ` ${h % 24} h` : ''}`
}

/** "16:40" today, "Sat 16:40" otherwise or when `day` is set. */
export function clock(ms: number, now = Date.now(), day = false): string {
  const d = new Date(ms)
  const time = d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
  return !day && d.toDateString() === new Date(now).toDateString()
    ? time
    : `${d.toLocaleDateString(undefined, { weekday: 'short' })} ${time}`
}

export function windowName(minutes: number): string {
  if (minutes === 300) return '5-hour window'
  if (minutes === 10080) return 'Weekly window'
  return minutes % 60 === 0 ? `${minutes / 60}-hour window` : `${minutes}-minute window`
}

const METERS: Record<string, string> = { claude: 'Claude Pro/Max', codex: 'ChatGPT (Codex)' }
export const meterName = (meter: string) => METERS[meter] ?? meter

/** 1st, 2nd, 3rd, 11th, 22nd... */
export function ordinal(n: number): string {
  const s = n % 100 >= 11 && n % 100 <= 13 ? 'th' : (['th', 'st', 'nd', 'rd'][n % 10] ?? 'th')
  return `${n}${s}`
}
