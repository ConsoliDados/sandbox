//! `sandbox run [PATH]` — start (or resume) a sandbox for a project.
//!
//! Lifecycle (per ADR-0009):
//! - container running → `docker exec -it` into it
//! - container stopped → `docker start` then exec
//! - container missing → build a `Plan` from project + profile, `docker run`

use std::path::PathBuf;

use sandbox_core::{
    ComposeMeta, Config, LangManifest, LanguageRegistry, Meta, Paths, Profile, Project,
};
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
    pub(crate) with_deps: bool,
    pub(crate) compose_file: Option<PathBuf>,
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

    let mut ctx = Context::load(&args)?;

    if args.print_cmd {
        // Predict what the compose network name would be so the printed
        // Plan is faithful end-to-end without actually creating it. Safe
        // mode → sandbox-compose-<short> (--internal); --network mode →
        // <project>_default (compose-default bridge, named after the same
        // project we'd pass to `docker compose -p`).
        if args.with_deps && ctx.compose_file.is_some() {
            let net = if ctx.profile.network {
                format!("{}_default", compose_project_name(&ctx))
            } else {
                sandbox_docker::compose_internal_name(ctx.project.hash.short().as_str())
            };
            ctx.compose_state = Some(ComposeMeta {
                file: ctx.compose_file.clone().unwrap_or_default(),
                project_name: compose_project_name(&ctx),
                services: Vec::new(),
                network: net,
            });
        }
        println!("{}", build_plan(&ctx));
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
    if args.with_deps {
        ctx.compose_state = Some(compose_up_flow(&ctx).await?);
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

    let plan = build_plan(&ctx);
    ensure_running(&ctx, &plan).await?;
    // Persist state as soon as the container exists, before we hand the
    // user a shell — so `sandbox ps` / `sandbox proxy start` see the new
    // ports even if the exec attach fails (e.g. non-TTY stdin).
    save_state(&ctx)?;
    attach_shell(&ctx).await?;
    Ok(())
}

fn compose_project_name(ctx: &Context) -> String {
    // `sandbox-<short>-deps` keeps the namespace distinct from anything
    // the user might be running by hand with `docker compose` on the same
    // file. The `-deps` suffix is the differentiator; the hash keeps it
    // unique per project.
    format!("sandbox-{}-deps", ctx.project.hash.short())
}

/// Bring up the project's compose deps and rewire them so they inherit the
/// sandbox's egress policy. Idempotent: a re-run of `sandbox run --with-deps`
/// against an already-up project is a no-op for each step.
///
/// Lifecycle (ADR-0010 § Decision item 7):
///   1. (safe only) `ensure_compose_internal` creates `sandbox-compose-<hash>`.
///   2. `docker compose -p <name> -f <file> up -d`.
///   3. Read service container IDs back.
///   4. (safe only) Rewire each service to the `--internal` network with the
///      service name as a DNS alias; disconnect from compose-created
///      networks (so deps lose egress, matching the sandbox itself).
///   5. (`--network` mode) Skip the rewire; deps stay on
///      `<project>_default` (compose-managed bridge with egress).
async fn compose_up_flow(ctx: &Context) -> Result<ComposeMeta> {
    let file = ctx.compose_file.as_ref().ok_or_else(|| {
        // Defensive — Context::load already errors if --with-deps was set
        // without a file. Reaching here means the caller skipped that check.
        crate::Error::WithDepsNoComposeFile {
            project: ctx.project.path.display().to_string(),
        }
    })?;
    let project_name = compose_project_name(ctx);
    let short = ctx.project.hash.short();

    // In safe mode the deps' network must be created BEFORE we read services
    // back, so the rewire step can target it. In --network mode we don't
    // touch the network — compose creates its own.
    let target_network = if ctx.profile.network {
        format!("{project_name}_default")
    } else {
        sandbox_docker::ensure_compose_internal(short.as_str()).await?
    };

    let file_str = file.to_string_lossy().into_owned();
    tracing::info!(file = %file.display(), project = %project_name, "bringing up compose deps");
    sandbox_docker::compose_up(&file_str, &project_name).await?;

    let services = sandbox_docker::compose_services(&project_name).await?;

    if !ctx.profile.network {
        let pairs: Vec<(String, String)> = services
            .iter()
            .map(|s| (s.service.clone(), s.container_id.clone()))
            .collect();
        tracing::info!(
            target_network = %target_network,
            n = pairs.len(),
            "rewiring compose deps to --internal network"
        );
        sandbox_docker::rewire_to_internal(&target_network, &pairs).await?;
    }

    Ok(ComposeMeta {
        file: file.clone(),
        project_name,
        services: services.iter().map(|s| s.service.clone()).collect(),
        network: target_network,
    })
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
    /// Resolved compose file path. `Some` when discovery found exactly one
    /// match or `--compose-file PATH` was given; `None` when the project has
    /// no compose file. Required to be `Some` if `--with-deps` is set.
    compose_file: Option<PathBuf>,
    /// Compose lifecycle state populated by `compose_up_flow` (Stage B).
    /// `None` until the deps are actually brought up. Read by `build_plan`
    /// to add the compose network to `additional_networks` and by
    /// `save_state` to persist `Meta.compose`.
    compose_state: Option<ComposeMeta>,
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
        let compose_file = resolve_compose_file(&project.path, args)?;
        if args.with_deps && compose_file.is_none() {
            return Err(crate::Error::WithDepsNoComposeFile {
                project: project.path.display().to_string(),
            });
        }
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
            compose_file,
            compose_state: None,
        })
    }
}

/// Resolve the compose file via `--compose-file` override or the
/// `sandbox-docker::compose::discover` heuristic. Multi-match bubbles up as
/// the docker-layer error, asking the user to pass `--compose-file`.
fn resolve_compose_file(project_root: &std::path::Path, args: &Args) -> Result<Option<PathBuf>> {
    let outcome = sandbox_docker::discover_compose(project_root, args.compose_file.as_deref())?;
    Ok(match outcome {
        sandbox_docker::ComposeOutcome::None => None,
        sandbox_docker::ComposeOutcome::Found(path) => Some(path),
    })
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
    let mut additional_networks: Vec<String> = Vec::new();
    let labels = if ctx.ports.is_empty() {
        Vec::new()
    } else {
        additional_networks.push(sandbox_proxy::PROXY_NETWORK.to_string());
        sandbox_proxy::labels_for_project(&ctx.slug, &ctx.ports, sandbox_proxy::DEFAULT_DOMAIN)
    };
    if let Some(compose) = &ctx.compose_state {
        additional_networks.push(compose.network.clone());
    }
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
        // PID 1 is `sleep infinity` (keepalive), not the shell. The
        // interactive shell is layered via `docker exec -it <shell>` in
        // attach_or_run. Keeping the entrypoint as the shell would tie
        // the container's lifetime to the user's session — `node srv & exit`
        // would kill PID 1 (bash), and the container (and the node) with it.
        // The keepalive entrypoint is the standard devcontainer pattern.
        entrypoint: Some("sleep".into()),
        command: vec!["infinity".into()],
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

/// Make sure the container is up and running. PID 1 is the keepalive
/// (`sleep infinity` per `build_plan`); the user's interactive shell is
/// layered on top via [`attach_shell`], so exiting the shell does not
/// terminate the container.
async fn ensure_running(ctx: &Context, plan: &Plan) -> Result<()> {
    let name = &ctx.project.container_name;
    if !sandbox_docker::exists(name).await? {
        tracing::info!(container = %name, "creating new container");
        sandbox_docker::run(plan).await?;
    } else if !sandbox_docker::is_running(name).await? {
        tracing::info!(container = %name, "starting stopped container");
        sandbox_docker::start(name).await?;
    } else {
        tracing::info!(container = %name, "container already running");
    }
    Ok(())
}

/// Open an interactive shell inside the running container via `docker exec`.
/// The container survives `exit` because PID 1 stays alive.
async fn attach_shell(ctx: &Context) -> Result<()> {
    let opts = exec_opts(ctx);
    sandbox_docker::exec(
        &ctx.project.container_name,
        &opts,
        std::slice::from_ref(&ctx.manifest.shell),
    )
    .await?;
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
        // Prefer the freshly-populated `ctx.compose_state` (set when
        // `--with-deps` ran the lifecycle this turn); otherwise keep the
        // previous block intact so a plain `sandbox run` doesn't blow away
        // the record of deps brought up by a prior `--with-deps`.
        compose: ctx
            .compose_state
            .clone()
            .or_else(|| existing.as_ref().and_then(|m| m.compose.clone())),
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
