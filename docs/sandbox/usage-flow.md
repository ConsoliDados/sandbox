# Usage flow — progressive trust

The sandbox is designed for the moment when a repository's trustworthiness is **unknown**: an interview challenge, a recruiter's "test project", a consulting client's codebase, an OSS PR you're reviewing. The defaults assume hostile until proven otherwise.

The trust dial has four notches. You start at the most restrictive and **relax progressively** as you validate.

## The four notches

```
Repo unknown
    │
    ▼
sandbox run .                       ← safe (default)
    │   • scan runs (YARA + heuristics; ClamAV opt-in)
    │   • /app mounted read-only
    │   • node_modules / target / lockfiles in named volumes (host can't see them)
    │   • $HOME is tmpfs, no host secrets reachable
    │   • no internet egress
    │   • capabilities dropped, no-new-privileges, ephemeral container
    │
    ├── pre-flight scan flagged something? → read findings, decide.
    │
    ▼
Human review
    • Read the code that scared the scanner.
    • For deeper analysis, copy the source and ask Claude (or any LLM) for a
      second opinion. Out-of-band review is independent of the sandbox.
    │
    ▼
sandbox run . --network             ← let the project reach the internet
    │   • everything from `safe`, plus internet egress on `bridge`
    │   • use this when the project legitimately needs to fetch things
    │     (npm install with private registry, API calls during dev) and
    │     you've reviewed the code enough to permit the C2 risk
    │
    ▼
sandbox run . --unsafe              ← treat as a normal dev container
    │   • /app is read-write — `git status` shows your edits from inside
    │   • node_modules / target / lockfiles are bind-mounted into the
    │     host project tree (visible to your editor and git on the host)
    │   • internet on
    │   • scan skipped
    │   • caps and ephemeral $HOME still in place
    │
    ▼
Trusted persistence (planned, OQ-003)
    ~/.config/sandbox/trusted.toml stores hash → trust level
    so frequently-used projects skip the dial without --unsafe each time.
```

## When to step the dial

| Situation | Recommended notch |
|---|---|
| First run on any new repo | `safe` (default) |
| Recruiter sent you a "take-home challenge" | `safe`. Stay there. Use `paranoid` if you're paranoid. |
| You read the code, scan is clean, you want to install deps | `safe` (install inside the container) |
| The project legitimately needs internet at runtime | `safe --network` |
| You're a maintainer of this project | `--unsafe` (or persistent trust once OQ-003 lands) |
| You're inspecting a forensic capture and refuse to execute anything | `paranoid` + don't run a shell — only `sandbox scan .` |

## Pitfall: `bun i` on the host

A frequent reflex is to run `bun i` / `npm i` / `cargo build` on the host shell because that's how everyone develops. **In safe/paranoid this defeats the sandbox** — the host package manager runs `postinstall` with your privileges and writes `node_modules` directly into the project on disk. The whole point of the named volume is to keep that out of host reach.

The rule: package managers run **inside** the container. Use `sandbox exec PROJECT -- bun i`, or run them from the container shell that `sandbox run` drops you into.

In `unsafe` mode the bind mount makes the host equivalent to the container for these directories, so running on the host is fine once the project is trusted.

## Pitfall: lockfile not in `git status`

In `safe` / `paranoid`, lockfiles live in the named volume — the host doesn't see them. If you need to commit a lockfile change, either:

- Run with `--unsafe` and commit from the host (`bun.lockb` is in your tree),
- Or wait for OQ-002 to be resolved (commit-from-container path).

This is intentional: a malicious lockfile change should not be committed to the host's checkout silently.

## What never relaxes

Even `--unsafe` keeps:

- Linux capabilities dropped (`--cap-drop=ALL`)
- `--security-opt=no-new-privileges`
- `$HOME` as `tmpfs` (host secrets are never inside the container)
- Numeric `--user $(id -u):$(id -g)` (no root inside)

These are not part of the trust dial. To turn them off you'd need to edit a profile in `~/.config/sandbox/config.toml` and you'd be choosing to do it with full knowledge.

## See also

- `threat-model.md` — what each notch defends against
- `srs.md` — the flags and subcommands
- ADR-0003 — volume strategy per profile
- ADR-0008 — scan pipeline
- `open-questions.md` — OQ-002 (commits in `/app` RO), OQ-003 (trust persistence)
