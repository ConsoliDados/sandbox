# Branch protection — canonical configs

These two JSON files are the **source of truth** for the GitHub branch protection
rules on `ConsoliDados/sandbox`. They match the
[GitHub branch-protection schema](https://docs.github.com/en/rest/branches/branch-protection?apiVersion=2022-11-28#update-branch-protection)
as the API expects it (nested objects, no extras).

| File | Branch | Gate |
|---|---|---|
| [`main.json`](main.json) | `main` | heavy — full CI battery (lint + test ubuntu + msrv + **test-docker**), enforce admins, signed history |
| [`dev.json`](dev.json)   | `dev`  | light — cheap CI only (lint + test ubuntu + msrv), admin bypass allowed |

## Apply

```sh
gh api -X PUT /repos/ConsoliDados/sandbox/branches/main/protection --input docs/sandbox/branch-protection/main.json
gh api -X PUT /repos/ConsoliDados/sandbox/branches/dev/protection  --input docs/sandbox/branch-protection/dev.json
```

Both should return a verbose JSON describing the active protection (with `url`,
`required_status_checks`, etc). Anything else (especially HTTP `422 Invalid request`)
means a schema drift — re-check the file against the
[current API docs](https://docs.github.com/en/rest/branches/branch-protection).

## Why `--input <file>` instead of `-f key.nested=val`

`gh api -f` does not construct nested JSON objects from dot-notation keys.
`-f required_status_checks.strict=true` sends the literal string
`required_status_checks.strict=true`, which the API ignores — you then get
`"required_status_checks" wasn't supplied` back, even though you "passed" it.
The branch-protection endpoint requires a nested-object body, so use `--input`.

## Notes on the current values

- **`required_approving_review_count: 0`** on `main` is intentional for
  solo-dev phase. Solo dev cannot self-approve their own PR — combined with
  `enforce_admins: true`, an approval requirement of ≥1 would deadlock every
  release PR. CI gates are doing the real work; promote to `1` when the project
  takes a regular contributor.
- **`enforce_admins: true`** on `main` (admin bypass disabled) is kept on
  purpose so the gate above isn't optional even for the owner.
- **`required_linear_history: true`** on both branches means `release/* → main`
  and `main → dev` (back-merge) PRs must use **squash** merge. No merge commits.
- **`enforce_admins: false`** on `dev` lets the owner bypass when something
  truly time-sensitive needs to land — `dev` is the integration buffer, not the
  release.
- **`restrictions: null`** = no allowlist of users/teams who can push;
  protection comes from PR-required + status checks, not from an identity gate.

## When to change

Any time you add a required status check (e.g. a new `cargo-deny` job, or
promoting `test (macos-latest)` from informational to required), update the
relevant JSON in this directory and reapply. Keep the file matching live
state — the JSON is the canonical config, the live setting is a copy.

Branch protection state lives **server-side** in GitHub; it's not enforced by
this repo until applied. CI alone does not block direct pushes — protection
does. Both layers are required.
