// Contagious Interview / DPRK Lazarus IoCs.
//
// Reference: incident-2026-05-06-ctrading (BeaverTail + InvisibleFerret JS
// payload, distributed via Bitbucket repo `0xmvpmintlabs/ctrading.git`).
//
// Severity convention (consumed by sandbox-scan's `YaraEngine`):
//   meta.severity: "critical" | "high" | "warn" | "info"
//   meta.description: shown in `sandbox scan --explain`
//   meta.remediation: shown when the finding fires
//
// Bump `RULESET_VERSION` in cache.rs when these change so existing scan
// caches re-evaluate.

rule contagious_interview_profile_js {
    meta:
        description = "Profile.js backdoor pattern (Function.constructor eval + base64 C2 + /api/service/token endpoint)"
        severity    = "critical"
        reference   = "incident-2026-05-06-ctrading"
        remediation = "Treat the project as hostile. Do not run; do not connect to a network. Discard the clone."
    strings:
        $func_constructor = "new (Function.constructor)('require'"
        $b64_chainlink    = "Y2hhaW5saW5rLWFwaS12My5saXY="
        $endpoint         = "api/service/token"
    condition:
        $func_constructor and $b64_chainlink and $endpoint
}

rule contagious_interview_vscode_autorun {
    meta:
        description = "VSCode tasks.json silent autorun of a payload under .vscode/"
        severity    = "critical"
        reference   = "incident-2026-05-06-ctrading"
        remediation = "Inspect .vscode/tasks.json and remove any non-developer-authored task. The payload file (typically .vscode/cancel) should be deleted."
    strings:
        $run_on        = "\"runOn\": \"folderOpen\""
        $hide          = "\"hide\": true"
        $reveal_never  = "\"reveal\": \"never\""
        $node_payload  = /node \.vscode\/(cancel|temp|aux|hidden|legit)/
    condition:
        $run_on and $hide and $reveal_never and $node_payload
}

rule contagious_interview_c2_domain {
    meta:
        description = "Reference to the chainlink-api-v3 C2 domain family (plaintext or base64)"
        severity    = "high"
        reference   = "incident-2026-05-06-ctrading"
        remediation = "Search the repository for surrounding context. The domain has no legitimate use in a developer's source tree."
    strings:
        $plain_live = "chainlink-api-v3.live" nocase
        $plain_xyz  = "chainlink-api-v3.xyz"  nocase
        $plain_com  = "chainlink-api-v3.com"  nocase
        $b64_live   = "Y2hhaW5saW5rLWFwaS12My5saXY="
    condition:
        any of them
}
