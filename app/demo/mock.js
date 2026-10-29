// Stands in for the Tauri backend when the dashboard runs in a plain browser
// for the README screenshots. Answers come from demo.json, which
// core/examples/demo.rs computes from made-up usage with the real reports.
// URL parameters pick the page and theme: ?page=limits&theme=dark&days=30
;(() => {
  const params = new URLSearchParams(location.search)
  for (const key of ['page', 'theme', 'days', 'metric']) {
    if (params.has(key)) localStorage.setItem(key, params.get(key))
  }

  const demo = fetch('demo.json').then((r) => r.json())
  let callbacks = 0
  const answers = {
    overview: (d, a) => d.overview[a.days] ?? d.overview['30'],
    accounts_report: (d, a) => d.accounts[a.days] ?? d.accounts['30'],
    limits: (d) => d.limits,
    price_models: (d) => d.price_models,
    aliases: () => ({}),
    status: (d) => ({
      scanning: false,
      last_scan: d.now - 40_000,
      last_report: { files_read: 214, events: 0, limits: 0, errors: [] },
      prices_fetched_at: d.now - 3 * 3600_000,
      price_error: null,
      watched: ['~/.claude/projects', '~/.codex/sessions', '~/.gemini/tmp', '~/.qwen/usage', 'opencode'],
      watch_error: null,
    }),
    sharing: () => ({ folder: null, device: 'desk', devices: [], last_sync: null, last_report: null }),
    bar_widget_status: () => ({ available: true, installed: true }),
    relay_status: () => ({ settings: '~/.claude/settings.json', state: 'on', wraps: null }),
    'plugin:event|listen': () => ++callbacks,
  }

  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args = {}) => {
      const answer = answers[cmd]
      return answer ? answer(await demo, args) : null
    },
    transformCallback: () => ++callbacks,
    unregisterCallback: () => {},
    convertFileSrc: (path) => path,
  }
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
})()
