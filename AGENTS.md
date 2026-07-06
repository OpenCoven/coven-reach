# AGENTS.md — coven-reach workspace

## What this is

`coven-reach` is an original Rust MCP server for the OpenCoven ecosystem. It is NOT a port of any existing project — it is a purpose-built tool designed to integrate natively with Coven.

## Structure

```
src/
  main.rs           — MCP stdio loop and dispatcher
  lib.rs            — public crate exports
  protocol.rs       — JSON-RPC 2.0 + MCP types
  security.rs       — allowed path enforcement, tilde expansion, symlink resolution
  error.rs          — ReachError enum with MCP-friendly codes
  tools/
    mod.rs
    read.rs         — reach_read: content, metadata, diff, checksum
    write.rs        — reach_write: put, mkdir, copy, move, delete, touch, archive, unarchive
    list.rs         — reach_list: entries, system_info
    find.rs         — reach_find: glob + content + size/date/MIME filters
    familiar.rs     — reach_familiar: OpenCoven workspace introspection
    secret_check.rs — reach_secret_check: secret pattern scanner (locations only)
tests/
  integration.rs    — tool round-trip tests against real temp dirs
```

## Key invariants

- **No unsafe code.** Keep it that way.
- **Paths are always checked.** Every operation goes through `Security::check()` or `Security::check_exists()` before touching the filesystem.
- **Secret values never leave `secret_check`.** Pattern matches report only location (file, line, col, length, pattern name). Never serialize the matched text.
- **Batch-friendly.** Tools that accept `sources` or `entries` arrays return per-item `ok`/`error` rather than failing the whole call.
- **Tests use `--test-threads=1`.** The tests share the `COVEN_REACH_ALLOWED_PATHS` env var; run sequentially to avoid interference.

## Env vars

- `COVEN_REACH_ALLOWED_PATHS` — colon-separated allowed roots (default: `~:/tmp`)
- `COVEN_REACH_READ_ONLY` — `true` disables all write operations

## Coven integration

```json
{
  "mcpServers": {
    "reach": {
      "command": "coven-reach",
      "env": {
        "COVEN_REACH_ALLOWED_PATHS": "~/Documents:~/Projects:/tmp"
      }
    }
  }
}
```

## Contributing as an agent (branch, PR, CI)

- **Never push to `main`.** Every change lands via a PR with green CI. Branch from current `origin/main`.
- **Fresh branch per task**; use a worktree if multiple sessions may touch this repo:
  ```sh
  git fetch origin main
  git worktree add -b <branch> /tmp/reach-<branch> origin/main
  ```
- Keep the diff scoped to one concern; conventional-commit subjects (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`).
- Run the gates locally before opening the PR:
  ```sh
  cargo fmt --all --check
  cargo clippy --all-targets -- -D warnings
  cargo test -- --test-threads=1   # tests share COVEN_REACH_ALLOWED_PATHS; run sequentially
  ```
- After merge: delete the remote branch, remove your local worktree/branch.

## Attribution — credit contributors correctly

When you re-land or build on someone else's work (a fork PR, an issue author's proposal, a co-author), **credit the human contributor with a working GitHub-linked trailer** so they appear in the contributors graph and on their profile:

```
Co-authored-by: Full Name <ID+username@users.noreply.github.com>
```

- Use the **numeric-id no-reply form**. Get the id with `gh api users/<login> --jq .id`.
- **Never** use a machine or `.local` email (e.g. `name@Someones-Mac.local`) in a co-author trailer — it links to no account and gives **zero** credit.
- When a squash-merge folds a contributor's PR into an internal branch, preserve their `Co-authored-by:` line in the squash commit message.
- Credit **people**, not AI tools.

## Claude Code

`CLAUDE.md` points here — this file is the source of truth for both.
