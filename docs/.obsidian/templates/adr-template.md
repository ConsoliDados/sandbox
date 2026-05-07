<%*
const id = await tp.system.prompt("ADR ID (e.g. 0011)");
const title = await tp.system.prompt("ADR title");
const phase = await tp.system.prompt("Phase (e.g. 2)");
const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
await tp.file.rename(`${id}-${slug}`);
await tp.file.move(`/adrs/${id}-${slug}`);
const date = tp.date.now("YYYY-MM-DD");
-%>
# ADR-<% id %> — <% title %>

- **Status:** Draft
- **Date:** <% date %>
- **Phase:** <% phase %>

## Context

<!-- What problem are we solving? What constraints exist? -->

## Decision

<!-- What did we decide. State it as a sentence: "We will use X." -->

## Alternatives considered

- **(a)** ... — rejected because ...
- **(b)** ... — rejected because ...

## Consequences

<!-- Positive and negative. What becomes easier? What becomes harder? What needs follow-up? -->

## References

<!-- Links to issues, prior art, related ADRs. -->
