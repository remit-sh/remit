//! `remit` — query the Remit classification history.
//!
//! Composition root for the read side. Wires the
//! [`file_classification_history::FileClassificationHistoryReader`]
//! adapter into the domain [`domain::ClassificationHistoryReader`]
//! port and renders results to stdout for the operator.

use std::process::ExitCode;

use clap::Parser;

mod logs;
mod since;

#[cfg(test)]
mod tests;

#[derive(Parser)]
#[command(name = "remit", version, about = "Query the Remit classification log")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Query classifications
    Logs(logs::LogsArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Logs(args) => match logs::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("remit: {e}");
                ExitCode::from(1)
            }
        },
    }
}
