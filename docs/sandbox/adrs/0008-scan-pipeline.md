# ADR-0008 — Scan pipeline: YARA → heuristics → ClamAV (deferred LLM)

- **Status:** Draft
- **Date:** 2026-05-07
- **Phase:** 4

## Context

The pre-flight scan is the load-bearing defense for `safe` and `paranoid` profiles. It must catch:

- Known bad patterns (DPRK Lazarus / Contagious Interview JS, common stagers, exfil beacons) — best handled by **YARA** signatures.
- Suspicious shapes that aren't yet in any rule database (e.g. `Function.constructor` evals, `child_process.exec` of base64 blobs, autorun `tasks.json`) — best handled by **language-aware heuristics** in Rust.
- Generic malware files (Windows binaries dropped into a Node project, packed payloads) — best handled by a maintained **AV signature DB** like ClamAV.

The user explicitly requested AV coverage during the 2026-05-07 design checkpoint.

A separate consideration: future **LLM-assisted analysis**. Out of scope for v0.1 (no LLMs in the binary), but the pipeline should make it easy to slot in as a fourth motor later. The user plans to keep using Claude as a manual second-pass review of any project flagged green by the scan.

## Decision

**We will run a three-motor scan pipeline, in order, with short-circuit on any blocking finding:**

```
YARA  ─►  heuristics (regex/AST)  ─►  ClamAV  ─►  verdict
                                                        │
                                                        ▼
                                                   (block | warn | clean)
```

### Motor placement

- **YARA** runs in-process inside the `sandbox-scan` crate (via `yara-rust` bindings). Rules live in `crates/sandbox-scan/rules/` and `~/.config/sandbox/scan-rules/`.
- **Heuristics** are pure Rust — regex first; AST-aware checks for shapes that benefit from it (e.g. `Function.constructor` calls in JS).
- **ClamAV** runs **inside an ephemeral scan container**, never on the host and never inside the project container.
  - Image: `sandbox/scanner:latest` (built and published by the project; pulled-on-first-use).
  - Project source bind-mounted **read-only** into the scan container at a fixed path.
  - Signature DB persists in a dedicated named volume (`sandbox-scanner-db`) mounted RW into the scan container.
  - Output: structured JSON on stdout; container is removed after each scan.

### When each motor runs

| Profile | YARA | Heuristics | ClamAV |
|---|---|---|---|
| `safe` (default) | yes (block on critical) | yes (block on critical) | opt-in via `[scan] clamav = true` in config |
| `paranoid` | yes (block on critical) | yes (block on critical) | **mandatory** |
| `unsafe` | skipped | skipped | skipped |

The `--no-scan` flag still requires `--unsafe` (per SRS).

### Signature DB updates

Explicit, never automatic:

```
sandbox scan --update-db
```

This subcommand starts the scan container with `--network bridge`, runs `freshclam` against the named volume, and exits. Users opt into when they want fresh signatures (controlling the rare moment the scanner has internet).

### Cache

Per ADR-0009, container identity is path-based. Scan cache is keyed by **content hash** (`git ls-files` hash) at `$XDG_CACHE_HOME/sandbox/scan/<content-hash>.toml`. Cache hit skips all motors unless `--no-cache`.

## Alternatives considered

- **(a) ClamAV installed on the host.** Rejected: most Linux/macOS users do not run host AV; forcing the install is heavy and leaks the dependency outside the sandbox tool.
- **(b) ClamAV inside the project container (one of the language images).** Rejected: ~500 MB binary + ~300 MB signatures inflates every language image; worse, it puts the AV inside the same trust boundary as the (possibly hostile) project, where the project could disrupt the scan.
- **(c) Single motor (YARA only).** Rejected: YARA misses generic malware that AV catches (and vice versa). The motors are complementary.
- **(d) Run all motors in parallel.** Considered. Rejected for v0.1: the linear pipeline is simpler and the latency cost is small; revisit if scans become slow on large repos.
- **(e) Bundle an LLM call as a fourth motor now.** Deferred: cost, latency, and a hard requirement for network egress make this a Phase 7+ concern. Manual Claude review by the user covers the gap until then.

## Consequences

Positive:

- Coverage stacks: signature-based (YARA, ClamAV) + shape-based (heuristics).
- Zero footprint on the host (no AV install) and on the project image (no AV bloat).
- The pipeline is extensible: a future LLM motor slots in as a fourth stage with the same `Verdict` interface.

Negative / open:

- **First scan downloads an image.** `sandbox/scanner:latest` is pulled on first use; the user sees a one-time delay. Mitigation: `sandbox scan --warmup` (or document `docker pull` upfront).
- **Signature freshness** is the user's responsibility (`--update-db`). Stale signatures = missed detections. Mitigation: the scan output prints the DB age and warns if older than 30 days.
- **Suppression UX** (false positives) — see OQ-007 (project-local vs. user-global suppression keys). Decide before this ADR is accepted.
- **Compose validation** (T6 in threat model) is a separate scan path that runs against `docker-compose.yml`. Lives in `sandbox-scan` but is not part of this three-motor pipeline. Documented in a follow-up ADR if it grows.

## References

- `../threat-model.md` T1, T2, T8 (scan-cached re-runs)
- `../srs.md` § `scan` and `run` `--no-scan`
- `crates/sandbox-scan/AGENTS.md`
- `../open-questions.md` OQ-007 (scan suppression syntax)
