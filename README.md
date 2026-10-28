# Gaṇanā (गणना)

<a href="https://github.com/sudharamachandranrajeev/brahma"><img src="https://img.shields.io/badge/part%20of-Brahma-8a5cf6" alt="Part of Brahma"></a>

Your AI usage, explained. Gaṇanā is a desktop app that reads the local logs of
your AI coding tools and shows how many tokens you used, what that would cost
at public API prices, and where it went: by day, model, project and app. Every
number has a "?" that explains it in plain words.

Everything stays on your machine. Gaṇanā reads only token counts, model names,
times and folder names; your conversations are never stored or sent anywhere.
Network requests: a daily price list download from
[models.dev](https://models.dev) (a built-in copy is used when offline), and, only
if you add provider accounts, the usage reports of those providers (see below).

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

You can also ask for a notification when a window that warned you resets, so
you know you can go on. It is off by default.

## Status bar

`ganana bar` prints a short summary of the fullest usage window, its reset
time and today's cost, read from Gaṇanā's database. Gaṇanā must have run at
least once.

```sh
ganana bar                   # JSON with every live window (default)
ganana bar --format plain    # one line, like "Claude 62%"
ganana bar --format waybar   # text, tooltip, class (ok/warning/critical)
```

- **Omarchy**: "Add to bar" on the Plans & limits page installs the
  `ganana.usage` widget (source in `integrations/omarchy/`) and adds it to the
  bar. The ring fills with the fullest window; click it for every window and
  its reset time, middle-click to open Gaṇanā. "Remove from bar" takes it out.
- **Waybar**:

  ```json
  "custom/ganana": {
    "exec": "ganana bar --format waybar",
    "return-type": "json",
    "interval": 30,
    "on-click": "ganana"
  }
  ```

## Several computers

Gaṇanā reads the AI tools on the computer it runs on. To count all your
computers, pick a folder that Syncthing, Dropbox or a similar app keeps the
same on each of them, and turn on sharing with that folder on each computer
(Accounts page, "Your other computers").

- Each computer appends its own usage to `<name>.ganana.jsonl` in the folder
  and reads the other computers' files.
- Only numbers go into the folder: token counts, model names, times, project
  folder names and limit readings. Never prompts or keys.
- A call two computers both logged (for example from synced log folders)
  counts once.
- Stopping removes the other computers' usage from this one; their files stay
  in the folder.

## Provider accounts

The Accounts page shows what providers actually billed, next to the estimate
from your local logs. The two are never added together: local logs include
subscription use no API bill covers, and a bill includes other computers.

| Source      | Needs                                    | Reads                                  |
| ----------- | ---------------------------------------- | -------------------------------------- |
| Anthropic   | Admin key (`sk-ant-admin01-`)            | Usage and cost reports, by model       |
| OpenAI      | Admin key (`sk-admin-`)                  | Completions usage and costs            |
| OpenRouter  | Management key                           | Activity (30 days) and credit balance  |
| Anything else (Gemini, TypeSafe, Qwen Cloud) | A CSV export | Date, model, tokens, cost |

- Keys are stored in the system keyring (Secret Service on Linux, Credential
  Manager on Windows), never in the database, logs or the dashboard. Only the
  last four characters are shown. Removing an account deletes its key.
- Accounts sync on start and every 15 minutes. Only usage and cost reports are
  requested, with read-only calls.
- A low OpenRouter balance (under 10% or $1) raises a desktop alert.
- CSV dates must be year-first or Unix times; importing a file replaces the days
  it covers, so overlapping exports never double count.

## Windows

Gaṇanā also builds for Windows 10 and 11 (it needs the WebView2 runtime, which
current Windows includes). The Build workflow in GitHub Actions makes an
installer. It reads the same tools from your user folder, keeps keys in
Credential Manager and shows the fullest usage window in the tray tooltip.

- Connect Claude: Claude Code runs status lines through Git Bash when it is
  installed, otherwise PowerShell. Gaṇanā writes the command with forward
  slashes and short (8.3) folder names so it works in both. If a path still has
  spaces it only works with Git Bash.
- `ganana bar` prints in a terminal, for scripts and other bars.

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
- `integrations/omarchy/`: the Omarchy bar widget.

## Roadmap

1. Local logs, prices and the overview dashboard.
2. Plan limits: Claude Pro/Max and ChatGPT usage windows, with warnings.
3. Provider accounts: Anthropic, OpenAI and OpenRouter reports, CSV import.
4. Status bar: `ganana bar`, an Omarchy widget, reset notifications.
5. Windows, and usage from several computers through a synced folder (this release).

## License

MIT
