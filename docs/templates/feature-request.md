# Feature Request Template

> Copy this template when proposing a new feature or enhancement. Fill in each section and delete the guidance comments (`<!-- ... -->`). Remove optional sections if they don't apply.

---

## Summary

<!-- One paragraph: What capability does this add? Why does it matter? -->

## Problem Statement

<!-- What pain point, gap, or limitation does this address?
     Reference existing issues if this evolves from prior work. -->

> **Evolves from:** <!-- #NNN (issue title), or "N/A" -->
> **Affects:** <!-- files, crates, workflows impacted -->

## Proposed Solution

<!-- Detailed design. Include as many of the following as applicable:
     - Architecture overview (ASCII diagrams welcome)
     - Data structures / schemas
     - API surface changes
     - Code examples or pseudocode
     - Configuration changes -->

### Architecture Overview

<!-- ASCII diagram or description of how components interact. -->

```
<!-- diagram here -->
```

### Detailed Design

<!-- Walk through the implementation. Reference crate names, file paths,
     and existing patterns in the codebase. -->

## Acceptance Criteria

<!-- Prioritized checklist. Every item must be verifiable. -->

### Must Have (P0)

- [ ] <!-- criterion -->

### Should Have (P1)

- [ ] <!-- criterion -->

### Nice to Have (P2)

- [ ] <!-- criterion -->

## Testing Strategy

<!-- How will this be verified? Cover:
     - Unit tests (which crates?)
     - Integration tests (what scenarios?)
     - Manual validation steps (if any) -->

### Unit Tests

<!-- e.g., "Add tests in crates/pi-daemon-kernel/tests/ covering..." -->

### Integration Tests

<!-- e.g., "New integration test using FullTestServer that verifies..." -->

## Impact on Existing Systems

<!-- What existing components are affected? Use this table format: -->

| Component | Impact | Changes Required |
|-----------|--------|-----------------|
| `<!-- component -->` | **<!-- None / Modified / Replaced -->** | <!-- description --> |

## Non-Goals

<!-- Explicitly state what this feature does NOT do. Prevents scope creep. -->

- <!-- non-goal 1 -->

## Migration Path *(optional)*

<!-- For breaking changes or phased rollouts. Describe how to get from
     the current state to the new state without disruption. -->

### Phase 1: <!-- description -->
### Phase 2: <!-- description -->

## References

<!-- Related issues, PRs, docs, or external resources. -->

- <!-- #NNN — description -->
- <!-- docs/Architecture.md — relevant section -->
