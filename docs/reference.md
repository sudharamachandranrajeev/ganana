# Gaṇanā in detail

The README shows what Gaṇanā does and how to start. This page has the details: which logs it
reads, how plan limits reach it, the status bar command, sharing between computers and provider
accounts.

## Which logs it reads

| App | Where |
| --- | --- |
| Claude Code | `~/.claude/projects`, `~/.config/claude/projects`, `$CLAUDE_CONFIG_DIR` |
| Codex CLI | `~/.codex/sessions` (or `$CODEX_HOME`) |
| Gemini CLI | `~/.gemini/tmp/*/chats` |
| Qwen Code | `~/.qwen/usage` |
| opencode | `~/.local/share/opencode/opencode.db` (read-only) |

- Gaṇanā reads only token counts, model names, times and folder names. It never stores your
  conversations.
- After the first scan it reads only new lines. It watches the folders, so the dashboard updates
  by itself.
- Models with no public price (custom aliases, routers) are listed under **Models without a
  price**, where you can map each one to a real model.
- Prices come from [models.dev](https://models.dev), downloaded once a day. A built-in copy is
  used when you are offline.

## Plans and limits

The **Plans & limits** page shows how full each usage window is, when it resets, and when it will
fill at your recent pace.

- **ChatGPT (Codex)**: the 5-hour and weekly windows come from the Codex CLI logs. Nothing to set
  up.
- **Claude Pro/Max**: Claude Code reports these limits only to its status line. **Connect Claude**
  sets Gaṇanā as that status line in `~/.claude/settings.json`, after saving a backup next to
  Gaṇanā's data. If you already had a status line, Gaṇanā runs it and shows its output, so nothing
  changes on screen. Gaṇanā keeps only the two limit percentages and their reset times.
  **Disconnect and restore the old setting** puts your old setting back.

> [!WARNING]
> Disconnect Claude before you uninstall Gaṇanā. Otherwise Claude Code points at a program that no
> longer exists.

You can add your plans (name, monthly price, renewal day) to compare what you pay with what the
same usage would cost at API prices.

**Alerts.** A desktop alert comes once per window at 75%, 90% and 100%. You can change the levels.
You can also ask for a notice when a window that warned you resets. That one is off by default.

**Tray.** Closing the window keeps Gaṇanā running in the tray, so alerts still arrive. Use
**Quit** in the tray menu to stop it. Starting it again shows the running window. On Windows the
tray tooltip names the fullest window.

## Status bar

`ganana bar` prints a short summary of the fullest usage window, its reset time and today's cost,
read from Gaṇanā's database. Gaṇanā must have run at least once.

```sh
ganana bar                   # JSON with every live window (default)
ganana bar --format plain    # one line, like "Claude 62%"
ganana bar --format waybar   # text, tooltip, class (ok/warning/critical)
```

- **Omarchy**: **Add to bar** on the Plans & limits page installs the `ganana.usage` widget
  (source in `integrations/omarchy/`) and adds it to the bar. The ring fills with the fullest
  window. Click it for every window and its reset time; middle-click to open Gaṇanā. **Remove from
  bar** takes it out.
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

Gaṇanā reads the AI tools on the computer it runs on. To count all your computers, pick a folder
that Syncthing or a similar sync app keeps the same on each of them. Then, on each computer,
open **Accounts → Your other computers → Share usage between computers**.

- Each computer appends its own usage to `<name>.ganana.jsonl` in the folder, and reads the other
  computers' files.
- Only numbers go into the folder: token counts, model names, times, project folder names and
  limit readings. Never prompts or keys.
- A call two computers both logged (for example from synced log folders) counts once.
- **Stop sharing** removes the other computers' usage from this one. Their files stay in the
  folder.

## Provider accounts

The **Accounts** page shows what providers actually billed, next to the estimate from your local
logs. The two are never added together: local logs include subscription use that no API bill
covers, and a bill includes other computers.

| Source | Needs | Reads |
| --- | --- | --- |
| Anthropic | Admin key (`sk-ant-admin01-`) | Usage and cost reports, by model |
| OpenAI | Admin key (`sk-admin-`) | Completions usage and costs |
| OpenRouter | Management key | Activity (30 days) and credit balance |
| Any other provider | A CSV export | Date, model, tokens, cost |

- Keys are stored in the system keyring (Secret Service on Linux, Credential Manager on Windows),
  never in the database, logs or the dashboard. Only the last four characters are shown. Removing
  an account deletes its key.
- Accounts sync on start and every 15 minutes, with read-only calls for usage and cost reports
  only.
- A low OpenRouter balance (under 10%, or under $1) raises a desktop alert.
- CSV dates must be year-first or Unix times. Importing a file replaces the days it covers, so
  overlapping exports never count twice.

## Windows notes

Gaṇanā runs on Windows 10 and 11. It needs the WebView2 runtime, which current Windows includes.
It reads the same tools from your user folder and keeps keys in Credential Manager.

- **Connect Claude**: Claude Code runs status lines through Git Bash when it is installed,
  otherwise PowerShell. Gaṇanā writes the command with forward slashes and short (8.3) folder
  names, so it works in both. If a path still has spaces, it only works with Git Bash.
- `ganana bar` prints in a terminal, for scripts and other bars.

## Where Gaṇanā keeps its data

- Linux: `~/.local/share/dev.ganana.app/ganana.db`
- Windows: `%APPDATA%\dev.ganana.app\ganana.db`

To start again from nothing, first **Disconnect** Claude (the backup of your old status line lives
in this folder), then quit Gaṇanā and delete the folder. On the next start it reads whatever logs
your AI tools still keep. Plans, accounts and settings are gone.
