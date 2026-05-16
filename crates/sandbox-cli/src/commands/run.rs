//! `sandbox run [PATH]` — start (or resume) a sandbox for a project.
//!
//! Lifecycle (per ADR-0009):
//! - container running → `docker exec -it` into it
//! - container stopped → `docker start` then exec
//! - container missing → build a `Plan` from project + profile, `docker run`

use std::path::PathBuf;

use sandbox_core::{Config, LangManifest, LanguageRegistry, Meta, Paths, Profile, Project};
use sandbox_docker::{
    ExecOpts, Mount, NetworkSpec, Plan, ResourceSpec, SANDBOX_INTERNAL, SecuritySpec, UserSpec,
};

use crate::Result;
use crate::commands::dotfiles::{self, Dotfiles};

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) path: PathBuf,
    pub(crate) lang: Option<String>,
    pub(crate) profile: Option<String>,
    pub(crate) unsafe_mode: bool,
    pub(crate) network: bool,
    pub(crate) no_scan: bool,
    pub(crate) with_clamav: bool,
    /// Override port detection: each value becomes a Traefik entryPoint.
    /// When empty we run the manifest's `port_detection` heuristics.
    pub(crate) expose: Vec<u16>,
    pub(crate) print_cmd: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    let span = tracing::info_span!("run", path = %args.path.display());
    let _entered = span.enter();

    // --no-scan is the explicit "I know what I'm doing" override; it only
    // makes sense paired with --unsafe (the scan layer matches the trust
    // boundary). See SRS § run.
    if args.no_scan && !args.unsafe_mode {
        return Err(crate::Error::NoScanRequiresUnsafe);
    }

    let ctx = Context::load(&args)?;
    let plan = build_plan(&ctx);

    if args.print_cmd {
        println!("{plan}");
        return Ok(());
    }

    pre_flight_scan(&ctx, &args).await?;

    sandbox_docker::ensure_internal(SANDBOX_INTERNAL).await?;
    if !ctx.ports.is_empty() {
        // Project asked to be reachable through the proxy. The network is
        // created here (idempotent) so `docker network connect` later in
        // `lifecycle::run` can't fail with "network not found" when the
        // user hasn't run `sandbox proxy start` yet. The proxy itself is
        // still opt-in — the labels are inert until Traefik comes up.
        sandbox_docker::ensure_bridge(sandbox_proxy::PROXY_NETWORK).await?;
    }
    for vol in ctx.project.named_volumes() {
        // Newly-created Docker named volumes are owned by root inside the
        // container. We always launch the project as the host UID, so
        // first-time creation triggers a one-shot chown init container
        // (alpine, --network none) to remap ownership. Subsequent runs
        // skip the chown entirely.
        let created =
            sandbox_docker::ensure_volume_owned(vol.as_str(), ctx.user.uid, ctx.user.gid).await?;
        if created {
            tracing::info!(
                volume = vol.as_str(),
                uid = ctx.user.uid,
                gid = ctx.user.gid,
                "named volume created + chowned for host user"
            );
        }
    }
    ensure_host_mountpoints(&ctx)?;
    seed_lockfiles(&ctx)?;

    attach_or_run(&ctx, &plan).await?;
    save_state(&ctx)?;
    Ok(())
}

/// Run YARA + heuristics + compose (and optionally ClamAV) against the
/// project before docker run. Skipped in unsafe mode (the trust boundary is
/// the user's call). Blocks with exit 30 (`Error::ScanBlocked`) when any
/// finding is severity ≥ High.
async fn pre_flight_scan(ctx: &Context, args: &Args) -> Result<()> {
    if args.unsafe_mode || args.no_scan {
        tracing::info!("scan skipped (unsafe profile)");
        return Ok(());
    }
    let short = ctx.project.hash.short().to_string();
    let opts = sandbox_scan::ScanOpts {
        no_cache: false,
        cache_dir: Some(ctx.paths.scan_cache_dir()),
        ignore_file: Some(ctx.paths.scan_ignore_file()),
        project_hash: Some(short.clone()),
    };
    let mut report = sandbox_scan::scan(&ctx.project.path, &opts)?;

    if args.with_clamav {
        let clamav = run_clamav_motor(&ctx.project.path, &ctx.paths, &short).await?;
        report.findings.extend(clamav.items);
        report.findings.sort_canonical();
    }

    let blocking: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity >= sandbox_scan::Severity::High)
        .collect();
    if blocking.is_empty() {
        let total = report.findings.len();
        if total > 0 {
            tracing::info!(total, "scan clean of blocking findings");
        }
        return Ok(());
    }
    eprintln!(
        "sandbox scan blocked the run — {} finding(s) at severity ≥ {}:",
        blocking.len(),
        sandbox_scan::Severity::High
    );
    for f in &blocking {
        let location = match f.line {
            Some(line) => format!("{}:{}", f.path.display(), line),
            None => f.path.display().to_string(),
        };
        eprintln!("  [{:>8}] {:<55} {}", f.severity, f.rule_id, location);
        eprintln!("           {}", f.message);
        if let Some(rem) = &f.remediation {
            eprintln!("           ↳ {rem}");
        }
    }
    eprintln!(
        "\nReview with `sandbox scan {} --explain`, suppress with `~/.config/sandbox/scan-ignore.toml`,",
        ctx.project.path.display()
    );
    eprintln!("or override with `--unsafe` if you've audited and trust this project.");
    Err(crate::Error::ScanBlocked {
        count: blocking.len(),
        threshold: sandbox_scan::Severity::High.to_string(),
    })
}

/// Same shape as `commands::scan::run_clamav` — duplicated here to keep the
/// pre-flight self-contained and avoid pulling `scan` into the run path.
/// If we add a third caller, refactor into a shared helper.
async fn run_clamav_motor(
    project: &std::path::Path,
    paths: &Paths,
    project_hash: &str,
) -> Result<sandbox_scan::Findings> {
    let scanner_dir = sandbox_scan::clamav::materialize_scanner_dockerfile(&paths.scanner_dir())?;
    sandbox_docker::ensure_scanner_image(&scanner_dir).await?;
    if !sandbox_docker::db_volume_exists().await? {
        return Err(crate::Error::ClamavDbMissing {
            volume: sandbox_docker::SCANNER_DB_VOLUME.into(),
        });
    }
    let outcome = sandbox_docker::run_clamscan(project).await?;
    if outcome.is_error() {
        return Err(crate::Error::ClamavScanFailed {
            code: outcome.exit_code,
            stderr: outcome.stderr,
        });
    }
    let mut findings = sandbox_scan::clamav::parse_output(&outcome.stdout);
    let list = sandbox_scan::IgnoreList::load(&paths.scan_ignore_file())?;
    list.apply(&mut findings, project_hash);
    Ok(findings)
}

struct Context {
    paths: Paths,
    project: Project,
    manifest: LangManifest,
    profile: Profile,
    user: UserSpec,
    dotfiles: Dotfiles,
    /// (lockfile basename, host seed path). Empty when `unsafe_mode` is on or
    /// the manifest declares no lockfiles. See ADR-0003.
    lockfile_seeds: Vec<(String, PathBuf)>,
    /// Resolved ports (CLI overrides ∪ manifest-driven detection). Empty
    /// vec means the project doesn't request proxy routing; non-empty
    /// triggers Traefik labels + a second network on the Plan.
    ports: Vec<u16>,
    /// User-facing project slug used as the Host component of
    /// `<slug>.sandbox.localhost`. Derived from the canonical project path.
    slug: String,
}

impl Context {
    fn load(args: &Args) -> Result<Self> {
        let paths = Paths::discover()?;
        paths.ensure_dirs()?;
        let cfg = Config::load_or_default(&paths.config_file())?;

        let mut registry = LanguageRegistry::builtin()?;
        let user_dir = paths.user_languages_dir();
        if user_dir.exists() {
            registry.load_from_dir(&user_dir)?;
        }
        for d in &cfg.defaults.language_dirs {
            registry.load_from_dir(d)?;
        }

        let project = Project::resolve(&args.path, &registry, args.lang.as_deref())?;
        let manifest = registry.require(project.language.as_str())?.clone();

        let profile_name = args.profile.as_deref().unwrap_or(&cfg.defaults.profile);
        let mut profile = cfg.profile(profile_name)?.clone();
        if args.unsafe_mode {
            profile.unsafe_mode = true;
            profile.network = true;
        }
        if args.network {
            profile.network = true;
        }

        let user = UserSpec::current()?;
        let dotfiles = dotfiles::discover(&paths);
        let lockfile_seeds = lockfile_seed_paths(&paths, &project, &manifest, &profile);
        let ports = sandbox_proxy::detect_ports(&project.path, &manifest, &args.expose)?;
        let slug = sandbox_proxy::slug_from_path(&project.path);
        Ok(Self {
            paths,
            project,
            manifest,
            profile,
            user,
            dotfiles,
            lockfile_seeds,
            ports,
            slug,
        })
    }
}

/// Compute the (name, host_path) pairs for lockfile bind mounts.
///
/// In `unsafe_mode` we do not isolate lockfiles: the source bind is RW and any
/// changes flow straight to the host project tree. In `safe`/`paranoid` each
/// declared lockfile is mapped to a writable file under the per-project state
/// dir, which we'll bind on top of `/app/<name>`.
///
/// Selection rules:
///
/// 1. Every manifest-declared lockfile that already exists on the host
///    source **or** in the state dir is bound (the common case once the
///    project has been initialized).
/// 2. If after rule 1 the result is empty, fall back to the manifest's
///    `primary_lock_file` so a fresh project can still let its package
///    manager *create* a real lockfile on first run. `seed_lockfiles`
///    later touches an empty stub on the host so Docker can mount over it
///    inside the `/app:ro` bind (mount-on-RO doesn't allow `mkdirat`).
///    Without this, `npm install` on a fresh project hits EROFS — exactly
///    the issue Phase 5 smoke surfaced.
///
/// See ADR-0003 § Lockfile mount mechanics.
fn lockfile_seed_paths(
    paths: &Paths,
    project: &Project,
    manifest: &LangManifest,
    profile: &Profile,
) -> Vec<(String, PathBuf)> {
    if profile.unsafe_mode {
        return Vec::new();
    }
    let dir = paths.lockfiles_dir(&project.hash.short());
    let mut seeds: Vec<(String, PathBuf)> = project
        .lock_files
        .iter()
        .filter(|name| {
            let seed = dir.join(name);
            let host = project.path.join(name);
            seed.is_file() || host.is_file()
        })
        .map(|name| (name.clone(), dir.join(name)))
        .collect();
    if seeds.is_empty()
        && let Some(primary) = manifest.primary_lock()
    {
        seeds.push((primary.to_string(), dir.join(primary)));
    }
    seeds
}

/// Create the lockfiles seed dir and ensure each filtered lockfile exists as a
/// regular file under the state dir so Docker performs a file bind. On first
/// sight we copy the project's current lockfile from the host; subsequent runs
/// preserve the seed (state-dir is the source of truth in safe/paranoid).
///
/// When the manifest's `primary_lock_file` falls through (no lockfile on host
/// AND no seed yet), this function ALSO touches an empty stub on the host
/// — Docker cannot create the mountpoint file inside the `/app:ro` bind, so
/// the bind target must already exist on the source side. The stub is empty
/// (zero bytes); the real lockfile contents live in the state-dir bind and
/// get written there by the in-container package manager.
fn seed_lockfiles(ctx: &Context) -> Result<()> {
    if ctx.lockfile_seeds.is_empty() {
        return Ok(());
    }
    let dir = ctx.paths.lockfiles_dir(&ctx.project.hash.short());
    std::fs::create_dir_all(&dir)?;
    for (name, seed) in &ctx.lockfile_seeds {
        let host_source = ctx.project.path.join(name);
        if !host_source.is_file() {
            // Primary-lock fallback path. Touch an empty file on the host so
            // the `:ro` bind has a mountpoint, log loudly so the user knows
            // we wrote into their tree.
            tracing::info!(
                path = %host_source.display(),
                "touching empty primary lockfile on host (mount-on-RO requires the target to exist)"
            );
            std::fs::File::create(&host_source)?;
        }
        if seed.exists() {
            continue;
        }
        std::fs::copy(&host_source, seed)?;
    }
    Ok(())
}

/// Create empty `package_dir`s on the host project tree when they're missing.
///
/// Required in safe/paranoid: `/app` is bind-mounted `:ro`, and Docker cannot
/// `mkdirat` the inner mountpoint (`/app/node_modules`, …) inside a read-only
/// bind. Creating the directory on the host first means Docker mounts the
/// named volume *over* an existing path instead of trying to create one.
///
/// Under `--unsafe` the source bind is RW so Docker handles this itself; we
/// skip the pre-create to avoid touching the source tree unnecessarily.
fn ensure_host_mountpoints(ctx: &Context) -> Result<()> {
    if ctx.profile.unsafe_mode {
        return Ok(());
    }
    for dir in &ctx.project.package_dirs {
        let path = ctx.project.path.join(dir);
        if !path.exists() {
            tracing::info!(?path, "pre-creating package_dir on host for RO bind");
            std::fs::create_dir_all(&path)?;
        }
    }
    Ok(())
}

fn build_plan(ctx: &Context) -> Plan {
    let mounts = build_mounts(ctx);
    let (labels, additional_networks) = if ctx.ports.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            sandbox_proxy::labels_for_project(&ctx.slug, &ctx.ports, sandbox_proxy::DEFAULT_DOMAIN),
            vec![sandbox_proxy::PROXY_NETWORK.to_string()],
        )
    };
    Plan {
        image: ctx.manifest.image.clone(),
        container_name: ctx.project.container_name.clone(),
        user: ctx.user,
        workdir: ctx.manifest.workdir.clone(),
        mounts,
        env: build_env(ctx),
        network: if ctx.profile.network {
            NetworkSpec::Bridge
        } else {
            NetworkSpec::Internal(SANDBOX_INTERNAL.to_string())
        },
        security: SecuritySpec {
            cap_drop_all: ctx.profile.cap_drop == "ALL",
            no_new_privileges: ctx.profile.no_new_privileges,
        },
        additional_networks,
        resources: ResourceSpec {
            cpus: ctx.profile.cpu,
            memory_mb: ctx.profile.memory_mb,
        },
        labels,
        entrypoint: Some(ctx.manifest.shell.clone()),
        command: vec![],
        interactive: true,
        tty: true,
        remove_on_exit: false,
        detach: false,
    }
}

fn build_mounts(ctx: &Context) -> Vec<Mount> {
    let mut mounts = Vec::new();

    mounts.push(Mount::Bind {
        src: ctx.project.path.clone(),
        dst: ctx.manifest.workdir.clone(),
        read_only: !ctx.profile.unsafe_mode,
    });

    let workdir = ctx.manifest.workdir.trim_end_matches('/');
    for (volume, dir) in ctx
        .project
        .named_volumes()
        .into_iter()
        .zip(ctx.project.package_dirs.iter())
    {
        mounts.push(Mount::Volume {
            name: volume.as_str().to_string(),
            dst: format!("{workdir}/{dir}"),
            read_only: false,
        });
    }

    for (name, seed) in &ctx.lockfile_seeds {
        mounts.push(Mount::Bind {
            src: seed.clone(),
            dst: format!("{workdir}/{name}"),
            read_only: false,
        });
    }

    if ctx.profile.ephemeral_home {
        mounts.push(Mount::Tmpfs {
            dst: "/home/sandbox".to_string(),
        });
    }

    if let Some(zshrc) = &ctx.dotfiles.zshrc {
        mounts.push(Mount::Bind {
            src: zshrc.clone(),
            dst: "/home/sandbox/.zshrc".to_string(),
            read_only: true,
        });
    }
    if let Some(starship) = &ctx.dotfiles.starship {
        mounts.push(Mount::Bind {
            src: starship.clone(),
            dst: "/home/sandbox/.config/starship.toml".to_string(),
            read_only: true,
        });
    }

    mounts
}

fn build_env(ctx: &Context) -> Vec<(String, String)> {
    vec![
        ("HOME".into(), "/home/sandbox".into()),
        (
            "SANDBOX_PROJECT_HASH".into(),
            ctx.project.hash.short().to_string(),
        ),
        ("SANDBOX_PROFILE".into(), ctx.profile.name.clone()),
    ]
}

async fn attach_or_run(ctx: &Context, plan: &Plan) -> Result<()> {
    let name = &ctx.project.container_name;
    if sandbox_docker::is_running(name).await? {
        tracing::info!(container = %name, "exec into running container");
        let opts = exec_opts(ctx);
        sandbox_docker::exec(name, &opts, std::slice::from_ref(&ctx.manifest.shell)).await?;
        return Ok(());
    }
    if sandbox_docker::exists(name).await? {
        tracing::info!(container = %name, "starting stopped container");
        sandbox_docker::start(name).await?;
        let opts = exec_opts(ctx);
        sandbox_docker::exec(name, &opts, std::slice::from_ref(&ctx.manifest.shell)).await?;
        return Ok(());
    }
    tracing::info!(container = %name, "creating new container");
    sandbox_docker::run(plan).await?;
    Ok(())
}

fn exec_opts(ctx: &Context) -> ExecOpts {
    ExecOpts {
        user: Some(format!("{}:{}", ctx.user.uid, ctx.user.gid)),
        workdir: Some(ctx.manifest.workdir.clone()),
        interactive: true,
        tty: true,
    }
}

fn save_state(ctx: &Context) -> Result<()> {
    let state_dir = ctx.paths.container_state_dir(&ctx.project.hash.short());
    let now = now_unix();
    let existing = Meta::exists_at(&state_dir)
        .then(|| Meta::load(&state_dir).ok())
        .flatten();
    let created_at = existing
        .as_ref()
        .and_then(|m| m.created_at.clone())
        .or_else(|| Some(now.clone()));
    let meta = Meta {
        container_name: ctx.project.container_name.as_str().to_string(),
        project_path: ctx.project.path.clone(),
        project_hash: ctx.project.hash.short().to_string(),
        language: ctx.project.language.as_str().to_string(),
        created_at,
        last_run_at: Some(now),
        named_volumes: ctx
            .project
            .named_volumes()
            .iter()
            .map(|v| v.as_str().to_string())
            .collect(),
        lockfiles: ctx
            .lockfile_seeds
            .iter()
            .map(|(name, _)| name.clone())
            .collect(),
        ports: ctx.ports.clone(),
    };
    meta.save(&state_dir)?;
    Ok(())
}

fn now_unix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
