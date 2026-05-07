use clap::{Parser, Subcommand};

mod error;

use error::{Error, Result};

#[derive(Parser, Debug)]
#[command(
    name = "sandbox",
    version,
    about = "Secure-by-default isolated dev environments in Docker",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Increase logging verbosity (repeat for more)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress non-error output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Print the underlying docker commands instead of running them
    #[arg(long, global = true)]
    print_cmd: bool,

    /// Override config file location
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<std::path::PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start (or resume) a sandbox for a project
    Run {
        /// Project path (defaults to current directory)
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
    },
    /// Stop a sandbox container; keep state
    Down {
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Remove container, named volumes, and per-project state
    Nuke {
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// List sandbox containers
    Ps,
    /// Tail sandbox container logs
    Logs { project: String },
    /// Run a command inside a running sandbox
    Exec {
        project: String,
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Toggle internet egress at runtime
    Net {
        #[command(subcommand)]
        op: NetOp,
    },
    /// Run security scan without launching a container
    Scan {
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
    },
    /// Manage language manifests
    Lang,
    /// Control reverse proxy sidecar
    Proxy,
    /// Edit or show config
    Config,
}

#[derive(Subcommand, Debug)]
enum NetOp {
    On { project: String },
    Off { project: String },
    Status { project: String },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose, cli.quiet);

    match cli.command {
        None => {
            <Cli as clap::CommandFactory>::command().print_help()?;
            println!();
            Ok(())
        }
        Some(cmd) => {
            // Phase 0: stub — no command bodies yet. See docs/sandbox/roadmap.md.
            tracing::info!(?cmd, "command parsed (Phase 0 stub — not implemented)");
            Err(Error::NotImplemented)
        }
    }
}

fn init_logging(verbose: u8, quiet: bool) {
    let default_level = if quiet {
        "error"
    } else {
        match verbose {
            0 => "info",
            1 => "debug",
            _ => "trace",
        }
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}
