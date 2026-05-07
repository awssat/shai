mod adapters;
mod cli_commands;
mod context;
mod discovery;
mod search_output;
mod semantic;
mod status_output;
mod storage;
mod verbalizer;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "shai",
    version,
    about = "Shadow AI — project-local memory for any LLM"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum AdapterCommands {
    /// Show built-in adapters plus any project-local custom adapters
    List,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a CLI agent inside a Shai container to auto-audit traffic
    Run {
        /// The agent command to execute (e.g. claude, copilot)
        command: String,
        /// Arguments to pass to the agent
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Print project session history
    History {
        #[arg(short, long, default_value_t = 10)]
        limit: u32,
        /// Filter to sessions that touched this file (fuzzy)
        #[arg(long)]
        file: Option<String>,
    },
    /// Show change timeline for a specific file across all sessions
    Log {
        /// File path or partial name (fuzzy match)
        file: String,
        #[arg(short, long, default_value_t = 20)]
        limit: u32,
    },
    /// Restore a file to a previous saved state
    Rollback {
        /// File path to restore
        file: String,
        /// Steps back (default 1 = last save)
        #[arg(short, long, default_value_t = 1)]
        steps: u32,
    },
    /// Preview what rollback would change before writing anything
    Diff {
        /// File path to compare against recorded history
        file: String,
        /// Steps back (default 1 = last save)
        #[arg(short, long, default_value_t = 1)]
        steps: u32,
    },
    /// Search prompts, file paths, and change summaries
    Search {
        query: String,
        #[arg(short, long, default_value_t = 20)]
        limit: u32,
        #[arg(long, default_value = "all")]
        mode: String,
    },
    /// Show a concise project summary from recent history
    Summary,
    /// Explain why a file/path mattered in recent work
    Why { path: String },
    /// Show project statistics
    Status,
    /// Show normalized activity analytics by file, subsystem, and missing prompts
    Analytics {
        /// Filter recent touch activity to a specific file path or substring
        #[arg(long)]
        file: Option<String>,
        /// Filter top-tool and missing-prompt analytics to a path/subsystem substring
        #[arg(long)]
        subsystem: Option<String>,
        /// Max rows per analytics section
        #[arg(short, long, default_value_t = 10)]
        limit: u32,
    },
    /// Archive or delete old blobs to reclaim disk space. Keeps all summaries.
    Gc {
        /// Days threshold — blobs older than this are processed (default 30)
        #[arg(long, default_value_t = 30)]
        days: u32,
        /// Permanently delete blobs instead of archiving to blobs_archive.redb
        #[arg(long)]
        delete: bool,
        /// Show what would be done without making any changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Export this project's memory to a portable archive (for team sharing or backup).
    Export {
        /// Output file path (e.g. shai-memory.ndjson)
        output: String,
    },
    /// Import sessions from an archive previously produced by `shai export`.
    Import {
        /// Input archive file path
        input: String,
    },
    /// Inspect adapter support and project-local custom adapter overrides
    Adapters {
        #[command(subcommand)]
        command: AdapterCommands,
    },
}

use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), String> {
    let filter = EnvFilter::builder()
        .with_env_var("SHAI_LOG")
        .with_default_directive(tracing_subscriber::filter::LevelFilter::WARN.into())
        .from_env_lossy();

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    match Cli::parse().command {
        Commands::Run { command, args } => cli_commands::cmd_run(command, args).await?,
        Commands::History { limit, file } => cli_commands::cmd_history(limit, file),
        Commands::Log { file, limit } => cli_commands::cmd_log(&file, limit),
        Commands::Rollback { file, steps } => cli_commands::cmd_rollback(&file, steps),
        Commands::Diff { file, steps } => cli_commands::cmd_diff(&file, steps),
        Commands::Search { query, limit, mode } => cli_commands::cmd_search(&query, limit, &mode),
        Commands::Summary => cli_commands::cmd_summary(),
        Commands::Why { path } => cli_commands::cmd_why(&path),
        Commands::Status => cli_commands::cmd_status(),
        Commands::Analytics {
            file,
            subsystem,
            limit,
        } => cli_commands::cmd_analytics(file.as_deref(), subsystem.as_deref(), limit),
        Commands::Gc {
            days,
            delete,
            dry_run,
        } => cli_commands::cmd_gc(days, delete, dry_run),
        Commands::Export { output } => cli_commands::cmd_export(&output),
        Commands::Import { input } => cli_commands::cmd_import(&input),
        Commands::Adapters { command } => match command {
            AdapterCommands::List => cli_commands::cmd_adapters_list(),
        },
    }

    Ok(())
}
