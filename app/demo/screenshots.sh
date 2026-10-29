#!/usr/bin/env bash
# Renders the README screenshots from made-up usage: no real logs, keys or
# accounts are read. Needs Rust, Node, Python 3 and Chromium.
#
#   app/demo/screenshots.sh [out-dir]      # default: docs/screenshots
#
# core/examples/demo.rs fills a throwaway database and prints the
# dashboard's data; the built dashboard then runs in headless Chromium with
# app/demo/mock.js answering in place of the Tauri backend.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
out=$(realpath -m "${1:-$root/docs/screenshots}")
chrome=${CHROME:-$(command -v chromium || command -v chromium-browser || command -v google-chrome)}
work=$(mktemp -d)
trap 'kill "${server:-0}" 2>/dev/null || true; rm -rf "$work"' EXIT

cargo run -q --manifest-path "$root/Cargo.toml" -p ganana-core --example demo -- "$work/demo.db" > "$work/demo.json"
(cd "$root/app" && npm run -s build -- --outDir "$work/site" --emptyOutDir > /dev/null)
cp "$root/app/demo/mock.js" "$work/demo.json" "$work/site/"
# The mock must be in place before the dashboard's own script runs.
sed -i 's|<script type="module"|<script src="/mock.js"></script><script type="module"|' "$work/site/index.html"

# A free port, so the shots never come from some other server.
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -m http.server "$port" --bind 127.0.0.1 --directory "$work/site" > /dev/null 2>&1 &
server=$!
for _ in $(seq 20); do curl -sf "http://127.0.0.1:$port/mock.js" > /dev/null && break; sleep 0.25; done
curl -sf "http://127.0.0.1:$port/mock.js" > /dev/null || { echo "the demo server did not start" >&2; exit 1; }

mkdir -p "$out"
shot() { # name width height query
  "$chrome" --headless=new --no-sandbox --disable-gpu --hide-scrollbars --force-prefers-reduced-motion --force-device-scale-factor=1 \
    --user-data-dir="$work/profile-$1" --window-size="$2,$3" --virtual-time-budget=8000 \
    --screenshot="$out/$1.png" "http://127.0.0.1:$port/?$4" > /dev/null 2>&1
  echo "✓ $1"
}
shot hero 1180 820 'page=overview&theme=light&days=30'
shot overview-dark 1180 1400 'page=overview&theme=dark&days=30'
shot limits-light 1180 1200 'page=limits&theme=light'
shot accounts-light 1180 1200 'page=accounts&theme=light&days=30'
