# Gaṇanā (गणना)

Your AI usage, explained. Gaṇanā is a desktop app that reads the local logs of
your AI coding tools and shows how many tokens you used, what that would cost
at public API prices, and where it went: by day, model, project and app. Every
number has a "?" that explains it in plain words.

Everything stays on your machine. Gaṇanā reads only token counts, model names,
times and folder names; your conversations are never stored or sent anywhere.
The only network request is a daily price list download from
[models.dev](https://models.dev) (a built-in copy is used when offline).

## What it reads

| App          | Where                                              |
| ------------ | -------------------------------------------------- |
| Claude Code  | `~/.claude/projects`, `~/.config/claude/projects`, `$CLAUDE_CONFIG_DIR` |
| Codex CLI    | `~/.codex/sessions` (or `$CODEX_HOME`)             |
| Gemini CLI   | `~/.gemini/tmp/*/chats`                            |
| Qwen Code    | `~/.qwen/usage`                                    |
| opencode     | `~/.local/share/opencode/opencode.db` (read-only)  |

Logs are read incrementally: only new lines are parsed after the first scan,
and the folders are watched so the dashboard updates by itself.

Models with no public price (custom aliases, routers) are listed under
"Models without a price", where you can map each one to a real model.

## Plans and limits

The "Plans & limits" page shows how full each usage window is, when it resets,
and when it will fill at your recent pace.

- **ChatGPT (Codex)**: the 5-hour and weekly windows come from the Codex CLI
  logs. Nothing to set up.
- **Claude Pro/Max**: Claude Code reports these limits only to its status
  line. "Connect Claude" sets Gaṇanā as that status line in
  `~/.claude/settings.json`, after saving a backup next to Gaṇanā's data. If
  you already had a status line, Gaṇanā runs it and shows its output, so
  nothing changes on screen. Gaṇanā keeps only the two limit percentages and
  their reset times. "Disconnect" puts your old setting back.

  Disconnect before you uninstall Gaṇanā, or Claude Code will point at a
  program that no longer exists.

You can add your plans (name, monthly price, renewal day) to compare what you
pay with what the same usage would cost at API prices.

Desktop alerts fire once per window when it reaches 75%, 90% and 100% (you can
change the levels). Closing the window keeps Gaṇanā running in the tray so
alerts still arrive; use "Quit" in the tray menu to stop it. Starting it again
shows the running window.

## Build

Needs Rust (stable), Node 20.19+ or 22.12+ and the
[Tauri 2 system dependencies](https://tauri.app/start/prerequisites/).

```sh
cd app
npm install
npx tauri dev            # run in development
npx tauri build          # build installers
```

Core library tests and a command-line report:

```sh
cargo test
cargo run --release -p ganana-core --example scan -- /tmp/ganana-test.db 30
```

## Layout

- `core/`: Rust library: log parsers, SQLite store, price catalogue, reports.
- `app/`: Tauri 2 shell (`src-tauri/`) and Svelte 5 dashboard (`src/`).

## Roadmap

1. Local logs, prices and the overview dashboard.
2. Plan limits: Claude Pro/Max and ChatGPT usage windows, with warnings (this release).
3. API accounts: Anthropic, OpenAI and OpenRouter usage and cost reports.
4. A status-bar widget.
5. Optional sync between your own machines.

## License

MIT
