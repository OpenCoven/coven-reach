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
