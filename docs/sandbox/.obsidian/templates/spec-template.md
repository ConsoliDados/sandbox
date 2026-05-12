<%*
const id = await tp.system.prompt("Spec / epic ID (e.g. 002)");
const slug = await tp.system.prompt("Slug (kebab-case)");
await tp.file.rename(`spec-${id}-${slug}`);
-%>
---
id: SPEC-<% id %>
slug: <% slug %>
status: planned
type: epic
children: []
---

# SPEC-<% id %> — <% slug.replace(/-/g, " ") %>

## Why

<!-- Strategic narrative. Why does this exist? -->

## Outcome / acceptance

- [ ]

## Children (sequenced)

| ID | Slug | Status | Dependency |
|----|------|--------|------------|
|    |      | planned |           |

## Cut order if time runs out

1.
2.

## Branch strategy

<!-- Per playbook §commits — usually feat/<slug> off dev as the integration branch. -->
