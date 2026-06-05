//! `remit logs` subcommand. Parses the CLI args into a domain
//! [`HistoryQuery`], runs it through the
//! [`ClassificationHistoryReader`] adapter, and renders the result
//! rows as tab-separated columns.

use std::path::PathBuf;
use std::time::SystemTime;

use clap::Args;
use domain::{
    ActionKind, AgentId, ClassificationHistoryReader, ClassificationRecord, HistoryQuery, Verdict,
    VerdictFilter,
};
use file_classification_history::FileClassificationHistoryReader;
use time::format_description::well_known::Rfc3339;

use crate::since;

#[derive(Args, Debug)]
pub struct LogsArgs {
    /// Filter by agent id (exact match).
    #[arg(long)]
    pub agent: Option<String>,

    /// Filter to descendants of this root agent (inclusive).
    #[arg(long)]
    pub root: Option<String>,

    /// Filter by verdict.
    #[arg(long, value_enum)]
    pub verdict: Option<VerdictArg>,

    /// Show records from the last `<DUR>` (e.g. `10m`, `1h30m`, `30s`).
    #[arg(long)]
    pub since: Option<String>,

    /// Filter by action kind (the tool name in the hook payload).
    #[arg(long)]
    pub action: Option<String>,

    /// Limit to the most recent N records (`0` for unlimited).
    #[arg(long, default_value_t = 100)]
    pub limit: usize,

    /// Display the action args column (only populated when debug_mode
    /// captured them).
    #[arg(long)]
    pub show_args: bool,

    /// Override the log path. Test/debug aid; production reads
    /// `<REMIT_HOME>/remit.log`.
    #[arg(long, hide = true)]
    pub log_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[clap(rename_all = "lower")]
pub enum VerdictArg {
    Green,
    Red,
}

impl From<VerdictArg> for VerdictFilter {
    fn from(v: VerdictArg) -> Self {
        match v {
            VerdictArg::Green => VerdictFilter::Green,
            VerdictArg::Red => VerdictFilter::Red,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LogsError {
    #[error("{0}")]
    Config(#[from] remit_config::ConfigError),
    #[error("invalid --agent value")]
    InvalidAgent,
    #[error("invalid --root value")]
    InvalidRoot,
    #[error("{0}")]
    Since(#[from] since::SinceError),
    #[error("{0}")]
    Query(#[from] domain::HistoryQueryError),
}

pub fn run(args: LogsArgs) -> Result<(), LogsError> {
    let log_path = match args.log_path.clone() {
        Some(p) => p,
        None => remit_config::log_path()?,
    };
    let log = FileClassificationHistoryReader::new(log_path);

    let query = build_query(&args)?;
    let records = log.query(&query)?;
    render(&records, args.show_args);
    Ok(())
}

fn build_query(args: &LogsArgs) -> Result<HistoryQuery, LogsError> {
    let agent_id = match &args.agent {
        Some(s) => Some(AgentId::try_new(s.clone()).map_err(|_| LogsError::InvalidAgent)?),
        None => None,
    };
    let root_agent_id = match &args.root {
        Some(s) => Some(AgentId::try_new(s.clone()).map_err(|_| LogsError::InvalidRoot)?),
        None => None,
    };
    let action = args.action.as_deref().map(ActionKind::new);
    let since_value = match &args.since {
        Some(s) => Some(SystemTime::now() - since::parse(s)?),
        None => None,
    };
    Ok(HistoryQuery {
        agent_id,
        root_agent_id,
        verdict: args.verdict.map(Into::into),
        since: since_value,
        action,
        limit: (args.limit > 0).then_some(args.limit),
    })
}

fn render(records: &[ClassificationRecord], show_args: bool) {
    if show_args {
        println!("TS\tAGENT\tACTION\tVERDICT\tREASON\tARGS");
    } else {
        println!("TS\tAGENT\tACTION\tVERDICT\tREASON");
    }
    for r in records {
        print_row(r, show_args);
    }
}

fn print_row(r: &ClassificationRecord, show_args: bool) {
    let ts = r.ts.format(&Rfc3339).unwrap_or_default();
    let agent = r.session_id.as_ref();
    let action = r.tool_name.as_ref();
    let (verdict, reason) = match &r.verdict {
        Verdict::Green => ("green", ""),
        Verdict::Red { reason, .. } => ("red", reason.as_str()),
    };
    if show_args {
        let args = r.tool_input.as_deref().unwrap_or("-");
        println!("{ts}\t{agent}\t{action}\t{verdict}\t{reason}\t{args}");
    } else {
        println!("{ts}\t{agent}\t{action}\t{verdict}\t{reason}");
    }
}
