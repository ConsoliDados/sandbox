<%*
const id = await tp.system.prompt("Feature ID (e.g. 001)");
const slug = await tp.system.prompt("Feature slug (kebab-case)");
await tp.file.rename(`feat-${id}-${slug}`);
-%>
---
id: FEAT-<% id %>
slug: <% slug %>
status: planned
phase: ?
depends-on: []
blocks: []
---

# FEAT-<% id %> — <% slug.replace(/-/g, " ") %>

## Goal

<!-- One paragraph. Why does this feature exist? What outcome does it produce? -->

## Acceptance criteria

<!-- Checkboxes. Each one is testable. -->

- [ ]
- [ ]

## Scope

**In:**

**Out:**

## Open questions

<!-- Surface to docs/open-questions.md if they affect the plan. -->

## Branch

`feat/<% slug %>` off `dev` (Git Flow).
