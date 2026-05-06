# ADR-0006 — Language manifests as TOML (with future YAML opt-in)

- **Status:** Accepted
- **Date:** 2026-05-06
- **Phase:** 1

## Context

Adding a new language stack to `sandbox` (e.g. Python, Go, Deno) must not require code changes. The tool needs a declarative manifest format describing image, detection rules, package directories, port detection patterns, and image extras. Candidates: TOML, YAML, JSON, Rust source structs.

The schema is also user-editable: users will drop manifests in `~/.config/sandbox/languages/`. The format must be human-friendly while strict enough to fail closed on typos that affect security.

## Decision

We will use **TOML** as the primary manifest format. The schema is documented in `languages/README.md` and lives in `sandbox-core::lang`.

YAML support is a future opt-in (post-v0.1). Same schema, different parser. Selected by file extension.

## Alternatives considered

- **(a) YAML primary.** Rejected: YAML's whitespace sensitivity and implicit type coercion (`yes`/`no` → bool) are footguns in security-relevant config. TOML's strictness is desirable here.
- **(b) JSON.** Rejected: no comments, verbose for humans.
- **(c) Hybrid (TOML user-facing, YAML internal).** Rejected: inconsistency burden.
- **(d) Code-defined manifests (Rust structs).** Rejected: defeats the purpose of "no code change to add a stack" — every new stack would require recompiling the binary.

## Consequences

Positive:
- Native serde + toml support; no extra deps.
- Strict by default: typos in field names fail at parse time, not silently ignored.
- Comments allowed; users can document their custom manifests.
- The same parser code path works for both bundled defaults and user overrides.

Negative:
- Users who prefer YAML must wait or convert. Mitigated by future YAML loader (same schema).
- Multi-line strings less ergonomic than YAML for things like long regex lists. Acceptable for this domain.

## Schema versioning

The schema is versioned implicitly by the binary's release. Once we hit v0.2 and the schema needs a breaking change, we add a `schema_version = 1` field at the top. Loader rejects manifests with unknown versions; migration tool ships alongside.

## References

- `languages/README.md` (canonical schema)
- `crates/sandbox-core/AGENTS.md`
- `docs/open-questions.md` OQ-005 (multi-match priority — resolved by `priority` field)
