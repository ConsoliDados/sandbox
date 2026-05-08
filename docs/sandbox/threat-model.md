# Threat Model

## What this tool defends against (in scope)

| # | Threat | Defense |
|---|---|---|
| T1 | Malware in untrusted source code (e.g. Contagious Interview / DPRK / supply chain) executing arbitrary code on the host | Run code only inside a Docker container with non-root user, dropped capabilities, no `new privileges`, ephemeral `$HOME` |
| T2 | Persistent malware writing to the project source tree via volume mount | Project mounted **read-only** in default mode; package directories (`node_modules`, `target`, `.venv`, `dist`) are **named volumes** separate from the source |
| T3 | C2 callback / data exfiltration to the internet | Container joins `sandbox-internal` network with no internet egress by default. Egress is runtime opt-in via `sandbox net on` or boot opt-in via `--network` |
| T4 | Theft of host credentials (`~/.ssh`, `~/.aws`, `~/.config/gh`, `~/.npmrc`, `~/.gnupg`, etc.) | These paths are **never** mounted. Container `$HOME` is a `tmpfs`. Only the project directory and explicitly-opted dotfiles (zshrc, starship) are mounted, read-only |
| T5 | Malicious `tasks.json` / `devcontainer.json` / `.idea` autorun when opening project in editor on host | The user is expected to open the project **only inside the container** (e.g. `sandbox exec PROJECT -- $EDITOR`). The CLI warns when these vector files exist in default mode and refuses without `--unsafe` if scan finds known-bad patterns |
| T6 | Malicious `docker-compose.yml` declaring `privileged`, `network_mode: host`, host bind mounts outside project, `cap_add=SYS_ADMIN`, etc. | Compose file is parsed and validated against an allowlist before any service is started. Violations block in default mode; `--unsafe` permits |
| T7 | Resource exhaustion (cryptominer in the project) | CPU and memory limits applied per container (configurable via profile) |
| T8 | Re-running a previously-cleaned project that was modified to be malicious | Hash of source tree (via `git ls-files`) is recorded. On run, hash is compared; mismatch triggers re-scan even if cached |
| T9 | Generic malware (Windows binary, packed payload) committed to a Linux/Node project | ClamAV motor in the scan pipeline runs against project source via an ephemeral scan container. Mandatory in `paranoid`, opt-in in `safe`. See ADR-0008 |

## Out of scope (not defended against)

| # | Non-goal | Mitigation suggestion |
|---|---|---|
| N1 | Linux kernel exploits / container escape via 0-days | Use a VM (Firecracker, QEMU) or a microVM-based runtime if you need kernel boundary |
| N2 | Side-channel CPU attacks (Spectre, etc.) | Same as N1 |
| N3 | Compromise of the `sandbox` binary itself | Compile from source you trust. Treat the binary as a privileged tool |
| N4 | Compromise of upstream Docker images (e.g. malicious `node:latest`) | Pin versions in language manifests. Future: optional image hash verification |
| N5 | Malicious Rust dependencies in **this** project's `Cargo.lock` | Standard mitigation: `cargo audit`, `cargo deny`, dependency review on PRs |
| N6 | Network-level attacks against the host (DNS poisoning, ARP, etc.) | Out of scope for a dev tool |
| N7 | Phishing / social engineering of the user themselves | Tooling can warn (recruiter red flags), but the human is the boundary |

## Operator pitfall — running package managers on the host

Even with the sandbox running, executing `npm i` / `bun i` / `pnpm i` / `cargo build` **on the host** (outside the container) bypasses every protection in this document. The host's package manager runs `postinstall` scripts and `build.rs` with the operator's privileges and writes `node_modules` / `target` directly into the project tree on disk.

The rule is: **package managers and build commands run only inside the container** (`sandbox exec` / inside the project shell). The host's job is to start the sandbox, not to operate on the project. `unsafe` mode relaxes the volume mount so the operator may choose to run them on the host once the project is trusted; in `safe` and `paranoid` this is a misuse pattern.

## Trust assumptions

- The user trusts the **`sandbox` binary** they have installed.
- The user trusts the **language manifest files** in `~/.config/sandbox/languages/` (locally edited) and the bundled defaults at first install.
- The user trusts the **Docker daemon** and the **base images** referenced in language manifests (pinned by digest in future iteration).
- The user does **not** trust the **project source code** until they explicitly mark it trusted (`--unsafe`).

## Real-world incident

This project is a direct response to a 2026-05-06 incident where the user was targeted by the Contagious Interview / DPRK Lazarus campaign via a fake Gala Games recruiter. The malicious repo had two coordinated payloads:

1. `.vscode/cancel` (~105 KB obfuscated JS) auto-executed via `.vscode/tasks.json runOn=folderOpen`.
2. `server/routes/api/profile.js` with `Function.constructor` eval beaconing to `chainlink-api-v3.live`.

The previous `docker-sandbox` (volume mount, no other restrictions) **would have allowed the malware to write to the project source tree and exfiltrate via DNS** if the C2 had been live. This rewrite is what should have existed before that incident.

Full report: `~/Dev/projects/studies/gala-chain/challenges/incident-2026-05-06-ctrading/`.

## Defense-in-depth layers (summary)

```
Project source on host
   ↓ [bind mount, read-only]
Container /app (read-only)
   + named volumes for package_dirs (writable, scoped)
   + tmpfs for $HOME (no host secrets)
   + non-root user (uid mapping to host user)
   + dropped Linux capabilities
   + no-new-privileges
   + sandbox-internal network (no internet egress)
   + cpu/memory limits
   + scan must pass before launch (default mode)
   ↓
[explicit user action: --unsafe or --network or sandbox net on]
   ↓
Relaxed posture for trusted runs
```
