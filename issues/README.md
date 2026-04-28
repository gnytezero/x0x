# x0x Issue Database

This directory is the git-committed issue tracker for the non-Linear x0x Symphony workflow.

The initial tracker source is `issues/issues.jsonl`: one JSON object per line, designed to match Symphony's normalized issue model closely enough that an `x0x-symphony` runner can treat it like any other tracker adapter.

## Why JSONL in git?

- **No external tracker dependency**: works offline and in forks.
- **Reviewable history**: issue creation, state changes, and handoffs are normal git diffs.
- **Bridge to x0x later**: each JSONL issue maps naturally to a CRDT task item for the full x0x-symphony architecture.
- **Simple implementation**: a runner can parse, filter, lock, update, and commit records without a database server.

## Files

- `issues.jsonl` — canonical active issue database.
- `schema.md` — record schema, states, and update rules.

## State model

Agent-dispatchable states:

- `todo`
- `in_progress`

Human handoff state:

- `review`

Terminal states:

- `done`
- `cancelled`
- `duplicate`

Blocked state:

- `blocked`

Agents should normally move completed work to `review`, not `done`.
