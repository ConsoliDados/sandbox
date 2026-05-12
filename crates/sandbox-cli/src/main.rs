use clap::{Parser, Subcommand};

mod commands;
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

    /// Print the underlying docker command instead of running it
    #[arg(long, global = true)]
    print_cmd: bool,

    /// Override config file location (not yet wired)
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

        /// Force a language (default: auto-detect)
        #[arg(long)]
        lang: Option<String>,

        /// Use a named profile from config (default: from config)
        #[arg(long)]
        profile: Option<String>,

        /// Disable paranoid defaults: r/w volume, full network, skip scan
        #[arg(long = "unsafe")]
        unsafe_mode: bool,

        /// Allow internet egress
        #[arg(long)]
        network: bool,
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
        /// Remove container only; keep named volumes
        #[arg(long)]
        keep_volumes: bool,
        /// Keep state directory
        #[arg(long)]
        keep_state: bool,
    },
    /// List sandbox containers
    Ps {
        /// Include stopped containers
        #[arg(long)]
        all: bool,
        /// Output format
        #[arg(long, value_enum, default_value_t = commands::ps::Format::Table)]
        format: commands::ps::Format,
    },
    /// Tail sandbox container logs
    Logs {
        /// Project path (defaults to current directory)
        project: Option<String>,
        /// Stream new log lines until interrupted
        #[arg(short, long)]
        follow: bool,
        /// Number of lines from the end of the logs to show
        #[arg(long, value_name = "N")]
        tail: Option<u32>,
        /// Only show logs since DURATION (e.g. 5m, 1h) or RFC3339 timestamp
        #[arg(long, value_name = "DURATION")]
        since: Option<String>,
    },
    /// Run a command inside a running sandbox
    Exec {
        /// Project path (defaults to current directory). Anything after `--`
        /// is the command to run.
        project: Option<String>,
        /// Override the user (default: container's default user)
        #[arg(long, value_name = "USER")]
        user: Option<String>,
        /// Override the working directory (default: /app)
        #[arg(long, value_name = "PATH")]
        workdir: Option<String>,
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Toggle internet egress at runtime (Phase 6)
    Net {
        #[command(subcommand)]
        op: NetOp,
    },
    /// Run security scan without launching a container (Phase 4)
    Scan {
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
    },
    /// Manage language manifests (Phase 3)
    Lang,
    /// Control reverse proxy sidecar (Phase 5)
    Proxy,
    /// Edit or show config (Phase 3)
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
        std::process::exit(err.exit_code());
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose, cli.quiet);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(dispatch(cli))
}

async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        None => {
            <Cli as clap::CommandFactory>::command().print_help()?;
            println!();
            Ok(())
        }
        Some(Command::Run {
            path,
            lang,
            profile,
            unsafe_mode,
            network,
        }) => {
            commands::run::execute(commands::run::Args {
                path,
                lang,
                profile,
                unsafe_mode,
                network,
                print_cmd: cli.print_cmd,
            })
            .await
        }
        Some(Command::Down { project, all }) => {
            commands::down::execute(commands::down::Args { project, all }).await
        }
        Some(Command::Nuke {
            project,
            all,
            keep_volumes,
            keep_state,
        }) => {
            commands::nuke::execute(commands::nuke::Args {
                project,
                all,
                keep_volumes,
                keep_state,
            })
            .await
        }
        Some(Command::Ps { all, format }) => {
            commands::ps::execute(commands::ps::Args {
                all,
                format,
                print_cmd: cli.print_cmd,
            })
            .await
        }
        Some(Command::Logs {
            project,
            follow,
            tail,
            since,
        }) => {
            commands::logs::execute(commands::logs::Args {
                project,
                follow,
                tail,
                since,
                print_cmd: cli.print_cmd,
            })
            .await
        }
        Some(Command::Exec {
            project,
            user,
            workdir,
            cmd,
        }) => {
            commands::exec::execute(commands::exec::Args {
                project,
                cmd,
                user,
                workdir,
                print_cmd: cli.print_cmd,
            })
            .await
        }
        Some(other) => {
            tracing::info!(?other, "command not implemented in Phase 1");
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
