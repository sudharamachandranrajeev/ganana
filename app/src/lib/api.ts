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
  limits: number
  errors: string[]
}

/** One plan usage window, such as Claude's 5-hour window. */
export interface LimitWindow {
  meter: string
  provider: string
  window_minutes: number
  used_pct: number
  /** Unix seconds. */
  resets_at: number
  /** Unix ms of the latest reading. */
  seen_at: number
  reset_passed: boolean
  rate_per_hour: number | null
  /** Unix seconds when it would be full at the recent pace, if before reset. */
  full_at: number | null
  /** [unix ms, used %] readings, oldest first. */
  history: [number, number][]
}

export interface Plan {
  id: number
  provider: string
  name: string
  monthly_usd: number
  renews_day: number
}

export interface PlanValue extends Plan {
  source: string | null
  period_start: string
  period_end: string
  usage: Totals
}

export interface Limits {
  windows: LimitWindow[]
  plans: PlanValue[]
  thresholds: number[]
}

export type Relay = { settings: string } & (
  | { state: 'on'; wraps: string | null }
  | { state: 'off'; existing: string | null }
)

/** A provider account: synced with a key kept in the system keyring, or filled from CSV files. */
export interface Account {
  id: number
  kind: 'anthropic' | 'openai' | 'openrouter' | 'csv'
  provider: string
  name: string
  /** Last four characters of the key. */
  key_hint: string | null
  /** Unix ms of the last successful sync or import. */
  synced_at: number | null
  error: string | null
  credit_total: number | null
  credit_used: number | null
}

export interface AccountView extends Account {
  /** `cost` is the billed amount; `calls` counts requests where reported. */
  totals: Totals
  models: Slice[]
  items: Slice[]
  local_cost: number
  local_tokens: number
}

export interface AccountsReport {
  days: number
  /** UTC days. */
  daily: Day[]
  accounts: AccountView[]
  billed: number
}

/** Column index for each field of a CSV import. */
export interface CsvMapping {
  date: number | null
  model: number | null
  input: number | null
  output: number | null
  cost: number | null
}

export interface CsvInspect {
  headers: string[]
  guess: CsvMapping
  sample: string[][]
  rows: number
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

export const limits = () => invoke<Limits>('limits')
export const savePlan = (plan: Plan) => invoke<number>('save_plan', { plan })
export const deletePlan = (id: number) => invoke<void>('delete_plan', { id })
export const setThresholds = (thresholds: number[]) => invoke<void>('set_thresholds', { thresholds })
export const relayStatus = () => invoke<Relay>('relay_status')
export const relayInstall = () => invoke<Relay>('relay_install')
export const relayUninstall = () => invoke<Relay>('relay_uninstall')

export const accountsReport = (days: number) => invoke<AccountsReport>('accounts_report', { days })
export const addAccount = (kind: string, name: string, key: string) => invoke<Account>('add_account', { kind, name, key })
export const removeAccount = (id: number) => invoke<void>('remove_account', { id })
export const syncNow = () => invoke<void>('sync_now')
export const csvInspect = (text: string) => invoke<CsvInspect>('csv_inspect', { text })
export const csvImport = (account: number | null, name: string, provider: string, text: string, mapping: CsvMapping) =>
  invoke<Account>('csv_import', { account, name, provider, text, mapping })

export const onUsageUpdated = (f: () => void): Promise<UnlistenFn> => listen('usage-updated', f)
export const onStatus = (f: (s: Status) => void): Promise<UnlistenFn> => listen<Status>('status', (e) => f(e.payload))
export const onAccountsUpdated = (f: () => void): Promise<UnlistenFn> => listen('accounts-updated', f)
