# Git Issue Database Schema

`issues/issues.jsonl` contains one UTF-8 JSON object per line. The file is intentionally line-oriented so agents and humans can update individual records with small diffs.

## Required fields

Each record should contain these fields, mirroring Symphony's normalized issue model:

```json
{
  "id": "X0X-0001",
  "identifier": "X0X-0001",
  "title": "Short imperative title",
  "description": "Markdown-capable issue description",
  "priority": 2,
  "state": "todo",
  "branch_name": null,
  "url": null,
  "labels": ["x0x-symphony"],
  "blocked_by": [],
  "created_at": "2026-04-28T00:00:00Z",
  "updated_at": "2026-04-28T00:00:00Z"
}
```

## Optional fields

Runners and agents may preserve or add:

- `acceptance` — list of acceptance criteria strings.
- `validation` — list of expected validation commands or checks.
- `assignee` — human or agent identifier.
- `estimate` — size estimate, implementation-defined.
- `handoff` — final/most recent handoff summary from an agent.
- `links` — related docs, PRs, commits, or external references.

## Symphony extensions

Two top-level fields are added by the x0x-symphony orchestrator. Both are
optional in M1 of x0x-symphony and required from M2 onward. They are
written by the orchestrator, not hand-edited.

### `shard`

Frozen at task creation. See
`../x0x-symphony/docs/adr/0002-sharded-claim-ttl.md`.

```json
{
  "shard": {
    "primary":            "<agent_id_hex>",
    "backups":            ["<agent_id_hex>", "<agent_id_hex>"],
    "claim_ttl_ms":       3600000,
    "created_view_epoch": 17
  }
}
```

### `claim`

Present once a worker holds the issue. Updated on heartbeat.

```json
{
  "claim": {
    "by":           "<agent_id_hex>",
    "at":           "2026-04-28T12:00:00Z",
    "heartbeat_at": "2026-04-28T12:14:00Z",
    "shard_role":   "primary",
    "signature":    "<ml-dsa-65 sig hex>"
  }
}
```

`handoff` should use this shape:

```json
{
  "summary": "What changed and why",
  "files_changed": ["path/to/file.rs"],
  "validation": [
    {"command": "just fmt-check", "status": "passed"}
  ],
  "follow_up": ["Anything humans or later agents should know"],
  "proofs_dir": "proofs/X0X-0001/2026-04-28T12-15-00Z"
}
```

`proofs_dir` is optional and points at a relative directory containing
large validation artefacts (full stdout, stderr, runner traces, fmt
diffs). Small status only lives inside `validation`.

## State values

| State | Meaning | Agent dispatch? |
|---|---|---:|
| `todo` | Ready for an agent to start if blockers are clear. | yes |
| `in_progress` | Claimed or actively being worked. | yes, limited concurrency |
| `review` | Agent completed useful work; human review required. | no |
| `blocked` | Not dispatchable until blockers are resolved. | no |
| `done` | Human accepted and closed. | no |
| `cancelled` | No longer planned. | no |
| `duplicate` | Superseded by another issue. | no |

## Priority

Lower numbers are dispatched first:

- `1` — urgent / release blocking
- `2` — high
- `3` — normal
- `4` — low
- `null` — unsorted backlog

## Blockers

`blocked_by` is a list of issue refs:

```json
[
  {"id": "X0X-0002", "identifier": "X0X-0002", "state": "todo"}
]
```

A `todo` issue with any non-terminal blocker must not be dispatched.

## Update rules

1. Keep `id` and `identifier` stable.
2. Use lowercase labels.
3. Use ISO-8601 UTC timestamps.
4. Agents may move their issue to `review`; humans move reviewed work to `done`.
5. Preserve unknown fields so future x0x-symphony adapters can extend the model.
6. Prefer append/edit commits that include both code changes and the issue record handoff.

## x0x-symphony tracker mapping

This JSONL model is the M1–M2 bootstrap tracker for x0x-symphony. At M3
the runner switches to the `x0x_crdt` adapter that reads/writes x0x's
TaskList CRDT through x0xd's REST API, after which this JSONL file is
removed (per `../x0x-symphony/docs/adr/0001-tracker-abstraction.md` and
`../x0x-symphony/docs/adr/0003-no-external-tracker-v1.md`).

| JSONL field | x0x TaskList representation                                  |
|-------------|--------------------------------------------------------------|
| `state == todo` / `in_progress` / `done` | TaskItem checkbox (Empty / Claimed / Done) |
| `state == review` / `blocked` / `cancelled` / `duplicate` | LWW metadata `state` |
| `id` / `identifier` | TaskItem id + metadata `identifier`                  |
| `priority` / `labels` / `blocked_by` / `branch_name` / `url` | LWW metadata fields |
| `shard`     | LWW metadata, written once at creation                       |
| `claim`     | LWW metadata, refreshed on heartbeat                         |
| `handoff` (small) | LWW metadata field                                     |
| `handoff` (large blobs) | KvStore entry referenced from metadata           |

See `../x0x-symphony/docs/design/symphony.md` §7.3 for the full mapping
and `../x0x-symphony/docs/adr/0004-x0x-tasklist-as-backbone.md` for the
choice of backbone.
