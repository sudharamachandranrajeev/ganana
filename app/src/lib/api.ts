import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface Tokens {
  input: number
  output: number
  cache_read: number
  cache_write: number
  cache_write_1h: number
  reasoning: number
}

export interface Totals {
  tokens: Tokens
  total_tokens: number
  cost: number
  savings: number
  calls: number
  unpriced_tokens: number
}

export interface Day {
  date: string
  cost: number
  total_tokens: number
  cost_by_provider: Record<string, number>
  tokens_by_provider: Record<string, number>
}

export interface Slice extends Totals {
  name: string
  provider: string | null
}

export interface Unpriced {
  provider: string
  model: string
  total_tokens: number
  calls: number
}

export interface Overview {
  generated_at: number
  days: number
  today: Totals
  week: Totals
  period: Totals
  daily: Day[]
  providers: Slice[]
  models: Slice[]
  projects: Slice[]
  sources: Slice[]
  unpriced: Unpriced[]
  events_stored: number
}

export interface ScanReport {
  files_read: number
  events: number
  errors: string[]
}

export interface Status {
  scanning: boolean
  last_scan: number | null
  last_report: ScanReport | null
  prices_fetched_at: number | null
  price_error: string | null
  watched: string[]
  watch_error: string | null
}

export const overview = (days: number) => invoke<Overview>('overview', { days })
export const status = () => invoke<Status>('status')
export const rescan = () => invoke<void>('rescan')
export const priceModels = () => invoke<string[]>('price_models')
export const aliases = () => invoke<Record<string, string>>('aliases')
export const setAlias = (alias: string, target: string | null) => invoke<void>('set_alias', { alias, target })

export const onUsageUpdated = (f: () => void): Promise<UnlistenFn> => listen('usage-updated', f)
export const onStatus = (f: (s: Status) => void): Promise<UnlistenFn> => listen<Status>('status', (e) => f(e.payload))
