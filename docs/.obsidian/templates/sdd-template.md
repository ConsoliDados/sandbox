<%*
const id = await tp.system.prompt("SDD ID (e.g. 003)");
const title = await tp.system.prompt("Crate or area name");
const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
await tp.file.rename(`sdd-${slug}`);
await tp.file.move(`/sdds/sdd-${slug}`);
const date = tp.date.now("YYYY-MM-DD");
-%>
---
id: SDD-<% id %>
title: <% title %>
status: draft
date: <% date %>
---

# SDD-<% id %> — <% title %>

## 1. Scope

<!-- Which crate or sub-area does this SDD cover? Boundaries with neighbors. -->

## 2. Public types

<!-- Exported structs, enums, traits. Field tables for the non-trivial ones. -->

## 3. Operations

<!-- Public functions and methods. Inputs, outputs, error variants. -->

| Function | Inputs | Outputs | Errors |
|----------|--------|---------|--------|
|          |        |         |        |

## 4. Invariants

<!-- Each invariant gets at least one explicit test. Number them. -->

1.

## 5. Errors

<!-- Variants of the crate's `Error` enum. -->

## 6. External dependencies

<!-- Crates, daemons, FS layout we rely on. -->

## 7. Open items

<!-- Tracked in docs/open-questions.md. Reference by ID. -->
