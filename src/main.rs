mod app;
mod cache;
mod codex;
mod graph;
mod layout;
mod ledger;
mod overview;
mod parse;
mod theme;
mod transcript;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Browse a Codex conversation's fork tree: the session and every session that shares its history.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Session to focus: uuid, uuid prefix, thread name, or rollout path.
    /// Default: the newest session in the current project.
    session: Option<String>,
    /// Project to open (default: the current directory's git root, or the directory itself).
    #[arg(long)]
    project: Option<PathBuf>,
    /// Consider only the N most recently active sessions (default 1). With N = 1, open that
    /// session's tree directly; otherwise show only the trees that contain one of them.
    #[arg(long, value_name = "N", num_args = 0..=1, default_missing_value = "1")]
    last: Option<usize>,
    /// Color theme. `auto` picks Mocha or Latte on truecolor terminals, else the 16 ANSI colors.
    #[arg(long, value_enum, default_value_t = theme::ThemeName::Auto)]
    theme: theme::ThemeName,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the fork tree once, without colors.
    Tree {
        session: Option<String>,
        #[arg(long, default_value_t = 100)]
        width: usize,
        /// Also show archived, inferred and empty sessions.
        #[arg(long)]
        all: bool,
        /// No spacer rows between siblings.
        #[arg(long)]
        compact: bool,
    },
    /// List the project's fork trees (and, with --all, standalone sessions).
    List {
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        all: bool,
        /// Only trees containing one of the N most recently active sessions.
        #[arg(long, value_name = "N")]
        last: Option<usize>,
    },
    /// Warm or rebuild the project cache without opening the UI.
    Index {
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        rebuild: bool,
    },
    /// Print every fork edge in the project: child parent k kind.
    #[command(hide = true)]
    DebugEdges {
        #[arg(long)]
        project: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home = codex::codex_home();
    match cli.cmd {
        Some(Cmd::Tree {
            session,
            width,
            all,
            compact,
        }) => app::print_tree(&home, session.as_deref(), width, all, compact),
        Some(Cmd::List { project, all, last }) => app::list(&home, project, all, last),
        Some(Cmd::Index { project, rebuild }) => app::index(&home, project, rebuild),
        Some(Cmd::DebugEdges { project }) => app::debug_edges(&home, project),
        None => app::run(
            &home,
            cli.session.as_deref(),
            cli.project.as_deref(),
            cli.last,
            cli.theme,
        ),
    }
}
