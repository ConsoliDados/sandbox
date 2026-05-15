//! `sandbox scan [PATH] [--no-cache] [--explain] [--format table|json]` —
//! run the static security scan standalone (no docker involvement).
//!
//! The same engine the pre-flight in `sandbox run` uses, exposed for users
//! who want to audit a repo without launching a container. Exits 30 when
//! the scan finds anything at severity ≥ High (matches the run pre-flight
//! threshold).

use std::path::{Path, PathBuf};

use sandbox_core::{Paths, project_hash};
use sandbox_scan::{Finding, Findings, IgnoreList, ScanOpts, ScanReport, Severity};
use serde::Serialize;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum Format {
    Table,
    Json,
}

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) path: PathBuf,
    pub(crate) no_cache: bool,
    pub(crate) explain: bool,
    pub(crate) format: Format,
    pub(crate) with_clamav: bool,
    pub(crate) update_db: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    let paths = Paths::discover()?;
    paths.ensure_dirs()?;

    if args.update_db {
        return run_update_db(&paths).await;
    }

    let canonical = std::fs::canonicalize(&args.path)?;
    let hash = project_hash(&canonical)?;
    let short = hash.short().to_string();

    let opts = ScanOpts {
        no_cache: args.no_cache,
        cache_dir: Some(paths.scan_cache_dir()),
        ignore_file: Some(paths.scan_ignore_file()),
        project_hash: Some(short.clone()),
    };
    let mut report = sandbox_scan::scan(&canonical, &opts)?;

    if args.with_clamav {
        let clamav = run_clamav(&canonical, &paths, &short).await?;
        report.findings.extend(clamav.items);
        report.findings.sort_canonical();
    }

    match args.format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&JsonReport::from(&report))?
        ),
        Format::Table => print!("{}", render_table(&report, args.explain)),
    }

    let blocking = report.findings.blocks_at(Severity::High);
    if blocking {
        return Err(Error::ScanBlocked {
            count: report
                .findings
                .iter()
                .filter(|f| f.severity >= Severity::High)
                .count(),
            threshold: Severity::High.to_string(),
        });
    }
    Ok(())
}

/// Refresh ClamAV signatures: build the scanner image if missing, then run
/// `freshclam` inside it against the persistent named volume.
async fn run_update_db(paths: &Paths) -> Result<()> {
    let scanner_dir = sandbox_scan::clamav::materialize_scanner_dockerfile(&paths.scanner_dir())?;
    eprintln!(
        "ensuring scanner image `{}` ...",
        sandbox_docker::SCANNER_IMAGE
    );
    sandbox_docker::ensure_scanner_image(&scanner_dir).await?;
    eprintln!(
        "refreshing signatures in volume `{}` (this may download ~300 MB) ...",
        sandbox_docker::SCANNER_DB_VOLUME
    );
    sandbox_docker::run_freshclam().await?;
    eprintln!("scanner DB updated.");
    Ok(())
}

/// Run the ClamAV motor as a follow-up stage after the static engine. We
/// keep this at the CLI layer (rather than inside the engine) because the
/// engine is sync and `sandbox-scan` deliberately has no Docker dependency.
async fn run_clamav(project: &Path, paths: &Paths, project_hash: &str) -> Result<Findings> {
    let scanner_dir = sandbox_scan::clamav::materialize_scanner_dockerfile(&paths.scanner_dir())?;
    sandbox_docker::ensure_scanner_image(&scanner_dir).await?;
    if !sandbox_docker::db_volume_exists().await? {
        return Err(Error::ClamavDbMissing {
            volume: sandbox_docker::SCANNER_DB_VOLUME.into(),
        });
    }
    let outcome = sandbox_docker::run_clamscan(project).await?;
    if outcome.is_error() {
        return Err(Error::ClamavScanFailed {
            code: outcome.exit_code,
            stderr: outcome.stderr,
        });
    }
    let mut findings = sandbox_scan::clamav::parse_output(&outcome.stdout);
    let list = IgnoreList::load(&paths.scan_ignore_file())?;
    list.apply(&mut findings, project_hash);
    Ok(findings)
}

#[derive(Debug, Serialize)]
struct JsonReport<'a> {
    content_hash: &'a str,
    from_cache: bool,
    worst_severity: Option<&'static str>,
    findings: &'a [Finding],
}

impl<'a> From<&'a ScanReport> for JsonReport<'a> {
    fn from(r: &'a ScanReport) -> Self {
        JsonReport {
            content_hash: &r.content_hash,
            from_cache: r.from_cache,
            worst_severity: r.findings.worst_severity().map(|s| match s {
                Severity::Info => "info",
                Severity::Warn => "warn",
                Severity::High => "high",
                Severity::Critical => "critical",
            }),
            findings: &r.findings.items,
        }
    }
}

fn render_table(report: &ScanReport, explain: bool) -> String {
    if report.findings.is_empty() {
        return format!(
            "clean — no findings (content_hash={}, cache={})\n",
            report.content_hash,
            if report.from_cache { "hit" } else { "miss" }
        );
    }
    render_findings(
        &report.findings,
        explain,
        report.from_cache,
        &report.content_hash,
    )
}

fn render_findings(
    findings: &Findings,
    explain: bool,
    from_cache: bool,
    content_hash: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} finding(s) (content_hash={}, cache={})\n\n",
        findings.len(),
        content_hash,
        if from_cache { "hit" } else { "miss" }
    ));
    for f in findings.iter() {
        let location = match f.line {
            Some(line) => format!("{}:{}", f.path.display(), line),
            None => f.path.display().to_string(),
        };
        out.push_str(&format!(
            "[{:>8}] {:<55}  {}\n",
            f.severity, f.rule_id, location
        ));
        if explain {
            out.push_str(&format!("           {}\n", f.message));
            if let Some(rem) = &f.remediation {
                out.push_str(&format!("           ↳ {rem}\n"));
            }
        }
    }
    out
}
