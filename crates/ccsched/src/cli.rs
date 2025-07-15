use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "ccsched")]
#[command(about = "Claude Code Scheduler - Intelligent task scheduling for Claude Code")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the scheduler service (alias: s)
    #[command(alias = "s")]
    Start(StartArgs),
    /// Submit a task to the scheduler
    Submit(SubmitArgs),
    /// Add a file as a task (aliases: a) - filename becomes task command, file content becomes prompt
    #[command(alias = "a")]
    Add(AddArgs),
    /// List all tasks and their status (aliases: ls, l)
    #[command(alias = "ls", alias = "l")]
    List(ListArgs),
    /// Show detailed information about a specific task (alias: sh)
    #[command(alias = "sh")]
    Show(ShowArgs),
    /// Resume a task session with Claude Code (alias: r)
    #[command(alias = "r")]
    Resume(ResumeArgs),
    /// Delete a task (aliases: rm, d)
    #[command(alias = "rm", alias = "d")]
    Delete(DeleteArgs),
    /// Rename a task (alias: mv)
    #[command(alias = "mv")]
    Rename(RenameArgs),
    /// Edit a task's prompt (alias: e)
    #[command(alias = "e")]
    Edit(EditArgs),
}

#[derive(Parser)]
pub struct StartArgs {
    /// Host address to bind to (default: "localhost")
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Port to listen on (default: 39512)
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Path to Claude Code executable (default: "claude")
    #[arg(short, long)]
    pub claude_path: Option<String>,

    /// Environment file to load (default: ".env")
    #[arg(short, long)]
    pub env: Option<String>,
}

#[derive(Parser)]
pub struct SubmitArgs {
    /// Task name
    pub name: String,

    /// Prompt file (if not provided, will read from stdin if piped/redirected, or open an editor)
    pub prompt_file: Option<String>,

    /// Working directory for the task
    #[arg(short, long)]
    pub cwd: Option<String>,

    /// Comma-separated list of task IDs this task depends on
    #[arg(short, long)]
    pub depends: Option<String>,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,
}

#[derive(Parser)]
pub struct AddArgs {
    /// Filename to use as task command (file content will be used as prompt)
    pub filename: String,

    /// Working directory for the task
    #[arg(short, long)]
    pub cwd: Option<String>,

    /// Comma-separated list of task IDs this task depends on
    #[arg(short, long)]
    pub depends: Option<String>,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Show detailed information including timestamps and session IDs
    #[arg(short, long)]
    pub detail: bool,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,
}

#[derive(Parser)]
pub struct ShowArgs {
    /// Task ID to show details for
    pub task_id: i64,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,
}

#[derive(Parser)]
pub struct ResumeArgs {
    /// Task ID or Session ID to resume
    pub task_or_session_id: String,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Additional arguments to pass to Claude Code
    #[arg(last = true)]
    pub claude_args: Vec<String>,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// Task ID to delete
    pub task_id: i64,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,
}

#[derive(Parser)]
pub struct RenameArgs {
    /// Task ID to rename
    pub task_id: i64,

    /// New name for the task
    pub new_name: String,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,
}

#[derive(Parser)]
pub struct EditArgs {
    /// Task ID to edit
    pub task_id: i64,

    /// Prompt file (if not provided, will read from stdin if piped/redirected, or open an editor)
    pub prompt_file: Option<String>,

    /// Scheduler host
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Scheduler port
    #[arg(short, long)]
    pub port: Option<u16>,
}