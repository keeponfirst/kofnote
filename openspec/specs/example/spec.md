# Example Spec: Dashboard Graph -> Records Filter Linkage

## Background

KOF Note already provides an interactive dashboard force graph and a records workspace.  
This spec demonstrates how to formalize one cross-page interaction so UX and behavior are testable and repeatable.

## Requirements

### Requirement 1: Node click routes to Records with aligned filters

When a user clicks a graph node on Dashboard:

- `type` node -> open Records tab and set record type filter.
- `tag` node -> open Records tab and set keyword filter to selected tag.
- `record` node -> open Records tab and focus the selected record in editor/list.

### Requirement 2: Focus state should be explicit and resettable

- UI must visually indicate focused node and linked neighbors.
- User can clear focus to return to full graph context.

## Scope

- Dashboard graph interaction behavior.
- Records filter synchronization behavior.
- Toast feedback for routing actions.

## Non-goals

- Replacing graph rendering library.
- Reworking record search ranking algorithm.
- Introducing server-side storage.

## API / Interface

### Frontend interaction contract (logical)

- `handleDashboardGraphNodeActivate(node)`:
  - input: graph node descriptor (`kind`, optional `recordType`, optional `tag`, optional `jsonPath`)
  - effect: update local UI state (`activeTab`, filters, selected record)

### Backend API impact

- No new backend command required.
- Existing search/list commands remain unchanged.

## Data Model (if applicable)

Graph node shape (already in app):

- `kind`: `core | type | tag | record`
- `recordType?`: enum for `type` nodes
- `tag?`: string for `tag` nodes
- `jsonPath?`: unique record path for `record` nodes

## Acceptance Criteria

1. Clicking a `type` node opens Records tab and type filter is not `all`.
2. Clicking a `tag` node opens Records tab and keyword filter equals selected tag.
3. Clicking a `record` node opens Records tab and selected record path matches node payload.
4. Clearing focus returns graph to unfocused visual state.
5. User receives toast feedback for each routing action.

## Edge Cases

- Node payload missing expected field (e.g., `tag` node without tag) should not crash.
- Record node points to missing/deleted path: UI shows no crash and remains interactive.
- Empty dataset: graph renders empty state gracefully; no routing attempted.

## Testing Strategy

### Automated

- Playwright smoke for type-node routing to Records filter.
- Unit/integration tests for activation handler logic (state transitions by node kind).

### Manual

- Validate drag + click behavior does not block routing.
- Validate behavior in both languages (`en`, `zh-TW`) for toasts and labels.
- Validate with large dataset (200+ records) that routing remains responsive.
