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

1. Local logs, prices and the overview dashboard (this release).
2. Plan limits: Claude Pro/Max and ChatGPT usage windows, with warnings.
3. API accounts: Anthropic, OpenAI and OpenRouter usage and cost reports.
4. Notifications and a status-bar widget.
5. Optional sync between your own machines.

## License

MIT
