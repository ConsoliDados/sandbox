# Architecture Decision Records (ADRs)

ADRs capture decisions that have lasting structural consequences. They are short, dated, and linked from the playbook / SAD when relevant.

## Status legend

- **Draft** — title and context noted; decision not yet made or not yet written down.
- **Proposed** — content drafted, awaiting review.
- **Accepted** — decision is in force; implementation reflects it.
- **Superseded** — replaced by a later ADR (link to successor).

## Index

| # | Title | Status | Phase to write |
|---|---|---|---|
| 0001 | Rewrite as Rust binary CLI (vs evolving the shell script) | Accepted | Phase 1 |
| 0002 | Docker integration via shell-out instead of bollard | Accepted | Phase 1 |
| 0003 | Volume strategy: read-only source + named volumes for package dirs | Accepted | Phase 2 |
| 0004 | Network isolation by default (`sandbox-internal`); runtime toggle | Accepted | Phase 2 |
| 0005 | Reverse proxy via Traefik sidecar | Accepted | Phase 5 |
| 0006 | Language manifests as TOML (with future YAML opt-in) | Accepted | Phase 1 |
| 0007 | State storage follows XDG Base Directory spec | Accepted | Phase 1 |
| 0008 | Scan pipeline: YARA → heuristics → ClamAV → (deferred) LLM | Accepted | Phase 4 |
| 0009 | Container reuse semantics for `run` / `down` / `nuke` | Accepted | Phase 1 |
| 0010 | Project compose deps: `--with-deps`, three networks, egress mirrors profile | Accepted | Phase 6 |
| 0011 | Typed errors throughout (no anyhow) | Accepted | Phase 1 |

## Template

```markdown
# ADR-NNNN — Title

- **Status:** Draft | Proposed | Accepted | Superseded by ADR-XXXX
- **Date:** YYYY-MM-DD
- **Phase:** N

## Context

What problem are we solving? What constraints exist?

## Decision

What did we decide. State it as a sentence: "We will use X."

## Alternatives considered

- (a) ... — rejected because ...
- (b) ... — rejected because ...

## Consequences

Positive and negative. What becomes easier? What becomes harder? What needs follow-up?

## References

Links to issues, prior art, related ADRs.
```

## When to write a new ADR

See `../playbook.md` § 9. In short: changing a default that affects security posture, adding a new external dep, changing the CLI surface, or picking between approaches with lasting trade-offs.
