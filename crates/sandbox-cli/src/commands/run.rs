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
    pub(crate) print_cmd: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    let span = tracing::info_span!("run", path = %args.path.display());
    let _entered = span.enter();

    let ctx = Context::load(&args)?;
    let plan = build_plan(&ctx);

    if args.print_cmd {
        println!("{plan}");
        return Ok(());
    }

    sandbox_docker::ensure_internal(SANDBOX_INTERNAL).await?;
    for vol in ctx.project.named_volumes() {
        sandbox_docker::ensure_volume(vol.as_str()).await?;
    }

    attach_or_run(&ctx, &plan).await?;
    save_state(&ctx)?;
    Ok(())
}

struct Context {
    paths: Paths,
    project: Project,
    manifest: LangManifest,
    profile: Profile,
    user: UserSpec,
    dotfiles: Dotfiles,
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
        Ok(Self {
            paths,
            project,
            manifest,
            profile,
            user,
            dotfiles,
        })
    }
}

fn build_plan(ctx: &Context) -> Plan {
    let mounts = build_mounts(ctx);
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
        resources: ResourceSpec {
            cpus: ctx.profile.cpu,
            memory_mb: ctx.profile.memory_mb,
        },
        command: vec![ctx.manifest.shell.clone()],
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
        user: Some(ctx.user),
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
