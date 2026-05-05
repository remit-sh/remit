//! Tiny user-facing CLI. The agent calls
//! `remit prepare-next-scope "<text>"` from a Bash tool call; the **hook**
//! intercepts that bash invocation and writes the pending row using the parent
//! session id from the hook payload. This binary's only job is to exit 0 (and
//! print confirmation) so the bash command itself succeeds — the hook does the
//! real work.
//!
//! Spike-only. Future: native IPC / hook-output protocol replaces the sentinel.

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next();
    match cmd.as_deref() {
        Some("prepare-next-scope") => {
            let text = args.next().unwrap_or_default();
            if text.is_empty() {
                eprintln!("remit prepare-next-scope: missing scope text");
                std::process::exit(1);
            }
            println!("scope prepared for next agent: {text}");
        }
        Some(other) => {
            eprintln!("remit: unknown subcommand {other:?}");
            std::process::exit(1);
        }
        None => {
            eprintln!("usage: remit prepare-next-scope <text>");
            std::process::exit(1);
        }
    }
}
