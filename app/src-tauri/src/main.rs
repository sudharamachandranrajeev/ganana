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
        Some(a) if a == "bar" => {
            #[cfg(windows)]
            attach_console();
            std::process::exit(ganana_lib::bar(args.collect()))
        }
        _ => {}
    }
    ganana_lib::run()
}

/// A release build on Windows starts without a console, so `ganana bar` run
/// from a terminal would print nothing. Use the terminal's console unless
/// the output already goes somewhere (a pipe or a file).
#[cfg(windows)]
fn attach_console() {
    unsafe extern "system" {
        fn GetStdHandle(id: u32) -> isize;
        fn AttachConsole(pid: u32) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 calls with constant arguments.
    unsafe {
        if matches!(GetStdHandle(STD_OUTPUT_HANDLE), 0 | -1) {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
