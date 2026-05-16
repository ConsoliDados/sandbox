//! `sandbox ps [--all] [--format json|table]` — list managed sandboxes.
//!
//! Source of truth for "what sandboxes exist" is the per-project state under
//! `$XDG_DATA_HOME/sandbox/containers/<hash>/meta.toml`. Live container info
//! (status, network, uptime) is enriched from `docker ps`. State without a
//! matching container shows STATUS=`absent`; a Docker container with no state
//! is ignored (would be a foreign `sandbox-*` name).
//!
//! `DEPS` is a Phase 6 column (compose deps); rendered as `—` for now.

use std::collections::HashMap;

use sandbox_core::{Meta, Paths};
use sandbox_docker::{ContainerInfo, list_sandboxes, list_sandboxes_args};
use serde::Serialize;

use crate::Result;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum Format {
    Table,
    Json,
}

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) all: bool,
    pub(crate) format: Format,
    pub(crate) print_cmd: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Row {
    name: String,
    hash: String,
    lang: String,
    path: String,
    status: String,
    network: String,
    uptime: String,
    deps: String,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    if args.print_cmd {
        let cmd = list_sandboxes_args().join(" ");
        println!("docker {cmd}");
        return Ok(());
    }

    let paths = Paths::discover()?;
    let metas = Meta::load_all(&paths.containers_dir())?;
    let infos = list_sandboxes().await?;
    let rows = build_rows(&metas, &infos, args.all);

    match args.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
        Format::Table => print!("{}", render_table(&rows)),
    }
    Ok(())
}

fn build_rows(metas: &[Meta], infos: &[ContainerInfo], include_all: bool) -> Vec<Row> {
    let by_name: HashMap<&str, &ContainerInfo> =
        infos.iter().map(|i| (i.names.as_str(), i)).collect();

    let mut rows = Vec::new();
    for meta in metas {
        let info = by_name.get(meta.container_name.as_str()).copied();
        let (state, network, uptime) = match info {
            Some(i) => (i.state.clone(), i.networks.clone(), i.running_for.clone()),
            None => ("absent".into(), "—".into(), "—".into()),
        };
        if !include_all && state != "running" {
            continue;
        }
        rows.push(Row {
            name: meta.container_name.clone(),
            hash: meta.project_hash.clone(),
            lang: meta.language.clone(),
            path: meta.project_path.display().to_string(),
            status: state,
            network,
            uptime,
            deps: "—".into(),
        });
    }
    rows
}

const HEADERS: [&str; 8] = [
    "NAME", "HASH", "LANG", "PATH", "STATUS", "NETWORK", "UPTIME", "DEPS",
];

fn render_table(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "no sandbox containers\n".to_string();
    }
    let mut widths: [usize; 8] = HEADERS.map(str::len);
    for row in rows {
        for (slot, cell) in widths.iter_mut().zip(row_cells(row).iter()) {
            *slot = (*slot).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    write_line(&mut out, HEADERS.iter().copied(), &widths);
    for row in rows {
        let cells = row_cells(row);
        write_line(&mut out, cells.iter().map(String::as_str), &widths);
    }
    out
}

fn row_cells(row: &Row) -> [String; 8] {
    [
        row.name.clone(),
        row.hash.clone(),
        row.lang.clone(),
        row.path.clone(),
        row.status.clone(),
        row.network.clone(),
        row.uptime.clone(),
        row.deps.clone(),
    ]
}

fn write_line<'a>(out: &mut String, cells: impl Iterator<Item = &'a str>, widths: &[usize; 8]) {
    for (i, (cell, width)) in cells.zip(widths.iter()).enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(cell);
        let pad = width.saturating_sub(cell.chars().count());
        for _ in 0..pad {
            out.push(' ');
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn meta(name: &str, hash: &str, lang: &str, path: &str) -> Meta {
        Meta {
            container_name: name.into(),
            project_path: PathBuf::from(path),
            project_hash: hash.into(),
            language: lang.into(),
            created_at: None,
            last_run_at: None,
            named_volumes: vec![],
            lockfiles: vec![],
            ports: vec![],
        }
    }

    fn info(name: &str, state: &str, network: &str, running_for: &str) -> ContainerInfo {
        ContainerInfo {
            names: name.into(),
            status: "x".into(),
            state: state.into(),
            networks: network.into(),
            image: "img".into(),
            running_for: running_for.into(),
        }
    }

    #[test]
    fn build_rows_filters_non_running_by_default() {
        let metas = vec![
            meta("sandbox-aaa", "aaa", "node", "/p/a"),
            meta("sandbox-bbb", "bbb", "rust", "/p/b"),
        ];
        let infos = vec![
            info(
                "sandbox-aaa",
                "running",
                "sandbox-internal",
                "5 minutes ago",
            ),
            info("sandbox-bbb", "exited", "bridge", "1 hour ago"),
        ];
        let rows = build_rows(&metas, &infos, false);
        let summary: Vec<_> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.status.as_str()))
            .collect();
        assert_eq!(summary, vec![("sandbox-aaa", "running")]);
    }

    #[test]
    fn build_rows_with_all_includes_stopped_and_absent() {
        let metas = vec![
            meta("sandbox-aaa", "aaa", "node", "/p/a"),
            meta("sandbox-bbb", "bbb", "rust", "/p/b"),
            meta("sandbox-ccc", "ccc", "bun", "/p/c"),
        ];
        let infos = vec![
            info(
                "sandbox-aaa",
                "running",
                "sandbox-internal",
                "5 minutes ago",
            ),
            info("sandbox-bbb", "exited", "bridge", "1 hour ago"),
            // sandbox-ccc has state but no container — should render as absent.
        ];
        let rows = build_rows(&metas, &infos, true);
        let summary: Vec<_> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.status.as_str(), r.network.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("sandbox-aaa", "running", "sandbox-internal"),
                ("sandbox-bbb", "exited", "bridge"),
                ("sandbox-ccc", "absent", "—"),
            ]
        );
    }

    #[test]
    fn render_table_pads_columns_and_includes_headers() {
        let rows = vec![Row {
            name: "sandbox-aaa".into(),
            hash: "aaa".into(),
            lang: "node".into(),
            path: "/p/a".into(),
            status: "running".into(),
            network: "sandbox-internal".into(),
            uptime: "5 minutes ago".into(),
            deps: "—".into(),
        }];
        let out = render_table(&rows);
        assert!(out.starts_with("NAME"));
        assert!(out.contains("HASH"));
        assert!(out.contains("DEPS"));
        assert!(out.contains("sandbox-aaa"));
        assert!(out.contains("sandbox-internal"));
        assert!(out.contains("5 minutes ago"));
    }

    #[test]
    fn render_table_says_empty_when_no_rows() {
        assert_eq!(render_table(&[]), "no sandbox containers\n");
    }
}
