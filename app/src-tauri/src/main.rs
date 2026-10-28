// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ganana_core::claude_relay;

fn main() {
    // Claude Code runs this binary as its status line to pass on plan limits.
    let mut args = std::env::args_os().skip(1);
    match args.next() {
        Some(a) if a == claude_relay::FLAG => {
            if let Some(dir) = args.next() {
                claude_relay::run(dir.as_ref());
            }
            return;
        }
        // Status bars run `ganana bar` for a one-line summary.
        Some(a) if a == "bar" => std::process::exit(ganana_lib::bar(args.collect())),
        _ => {}
    }
    ganana_lib::run()
}
