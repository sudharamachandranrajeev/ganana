// Colour follows the provider, never its rank: each provider owns a fixed
// categorical slot (validated for colour-blind separation in this order).
// Providers past the eighth share the last slot as "Other".
export const PROVIDER_ORDER = ['anthropic', 'openai', 'google', 'alibaba', 'deepseek', 'xai', 'mistral']

export function providerSlot(provider: string): number {
  const i = PROVIDER_ORDER.indexOf(provider)
  return i < 0 ? 8 : i + 1
}

export const providerColor = (provider: string) => `var(--series-${providerSlot(provider)})`

/** Stack order: fixed slots first, then any others. */
export function sortProviders(ids: Iterable<string>): string[] {
  return [...new Set(ids)].sort((a, b) => providerSlot(a) - providerSlot(b) || a.localeCompare(b))
}
