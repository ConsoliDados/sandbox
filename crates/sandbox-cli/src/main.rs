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

        /// Skip the pre-flight security scan (requires --unsafe)
        #[arg(long = "no-scan")]
        no_scan: bool,

        /// Add the ClamAV motor to the pre-flight scan (requires `sandbox scan --update-db` first)
        #[arg(long = "with-clamav")]
        with_clamav: bool,

        /// Override port detection for the reverse proxy. Repeat to expose
        /// multiple ports: `--expose 3000 --expose 5007`. When omitted, the
        /// language manifest's `port_detection` rules run.
        #[arg(long, value_name = "PORT")]
        expose: Vec<u16>,

        /// Bring up the project's `docker-compose` deps alongside the
        /// sandbox container (ADR-0010). Deps inherit the sandbox's egress
        /// policy: in safe mode they're moved to a `--internal` network and
        /// cannot reach the internet; with `--network` they keep the
        /// compose-default bridge.
        #[arg(long = "with-deps")]
        with_deps: bool,

        /// Explicit path to a compose file. Overrides discovery; required
        /// when discovery finds more than one candidate.
        #[arg(long = "compose-file", value_name = "PATH")]
        compose_file: Option<std::path::PathBuf>,
    },
    /// Stop a sandbox container; keep state
    #[command(visible_alias = "stop")]
    Down {
        project: Option<String>,
        #[arg(long)]
        all: bool,
        /// Also stop and remove the compose deps brought up by `--with-deps`.
        #[arg(long = "with-deps")]
        with_deps: bool,
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
        /// Skip the confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,
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
    /// Re-enter the shell of a running sandbox (no scan; container must be up)
    #[command(alias = "shell")]
    Attach {
        /// Project path (defaults to current directory)
        project: Option<String>,
        /// Force a language (default: auto-detect)
        #[arg(long)]
        lang: Option<String>,
    },
    /// Toggle internet egress at runtime (Phase 6)
    Net {
        #[command(subcommand)]
        op: NetOp,
    },
    /// Run security scan without launching a container
    Scan {
        /// Project path (defaults to current directory)
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
        /// Bypass the cache and rerun every motor
        #[arg(long)]
        no_cache: bool,
        /// Print message + remediation under each finding
        #[arg(long)]
        explain: bool,
        /// Output format
        #[arg(long, value_enum, default_value_t = commands::scan::Format::Table)]
        format: commands::scan::Format,
        /// Run the ClamAV motor on top of YARA + heuristics + compose
        #[arg(long = "with-clamav")]
        with_clamav: bool,
        /// Refresh the ClamAV signature DB and exit (ignores PATH and other flags)
        #[arg(long = "update-db", conflicts_with_all = ["no_cache", "explain", "with_clamav"])]
        update_db: bool,
    },
    /// Manage language manifests (Phase 3)
    Lang,
    /// Control the Traefik reverse proxy sidecar
    Proxy {
        #[command(subcommand)]
        op: ProxyOp,
    },
    /// Edit or show config (Phase 3)
    Config,
}

#[derive(Subcommand, Debug)]
enum NetOp {
    /// Attach the default Docker bridge — grants internet egress
    On {
        #[arg(default_value = ".")]
        project: String,
    },
    /// Detach the bridge — restores the egress-restricted default
    Off {
        #[arg(default_value = ".")]
        project: String,
    },
    /// Report which networks the container is attached to + egress state
    Status {
        #[arg(default_value = ".")]
        project: String,
        #[arg(long, value_enum, default_value_t = commands::net::Format::Table)]
        format: commands::net::Format,
    },
}

impl From<NetOp> for commands::net::Args {
    fn from(op: NetOp) -> Self {
        match op {
            NetOp::On { project } => Self::On { project },
            NetOp::Off { project } => Self::Off { project },
            NetOp::Status { project, format } => Self::Status { project, format },
        }
    }
}

#[derive(Subcommand, Debug)]
enum ProxyOp {
    /// Render compose+config and bring the Traefik sidecar up
    Start {
        /// Enable the Traefik dashboard (port 8090)
        #[arg(long)]
        dashboard: bool,
    },
    /// Stop the Traefik sidecar (keeps generated config)
    Stop,
    /// Print sidecar status (docker compose ps)
    Status,
    /// Tail sidecar logs
    Logs {
        #[arg(short, long)]
        follow: bool,
    },
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
            no_scan,
            with_clamav,
            expose,
            with_deps,
            compose_file,
        }) => {
            commands::run::execute(commands::run::Args {
                path,
                lang,
                profile,
                unsafe_mode,
                network,
                no_scan,
                with_clamav,
                expose,
                with_deps,
                compose_file,
                print_cmd: cli.print_cmd,
            })
            .await
        }
        Some(Command::Down {
            project,
            all,
            with_deps,
        }) => {
            commands::down::execute(commands::down::Args {
                project,
                all,
                with_deps,
            })
            .await
        }
        Some(Command::Nuke {
            project,
            all,
            keep_volumes,
            keep_state,
            yes,
        }) => {
            commands::nuke::execute(commands::nuke::Args {
                project,
                all,
                keep_volumes,
                keep_state,
                yes,
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
        Some(Command::Attach { project, lang }) => {
            commands::attach::execute(commands::attach::Args {
                project,
                lang,
                print_cmd: cli.print_cmd,
            })
            .await
        }
        Some(Command::Net { op }) => commands::net::execute(op.into()).await,
        Some(Command::Proxy { op }) => commands::proxy::execute(op.into()).await,
        Some(Command::Scan {
            path,
            no_cache,
            explain,
            format,
            with_clamav,
            update_db,
        }) => {
            commands::scan::execute(commands::scan::Args {
                path,
                no_cache,
                explain,
                format,
                with_clamav,
                update_db,
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
