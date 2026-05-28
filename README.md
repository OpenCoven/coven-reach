# coven-reach 🕯️

**A Coven-native MCP server for filesystem and web operations.**

`coven-reach` is an original Rust implementation of a [Model Context Protocol](https://modelcontextprotocol.io) server designed for the OpenCoven ecosystem. It gives any familiar — Charm, Kitty, Cody, or any agent — access to the local filesystem and the web through a clean, harness-agnostic interface.

> **Harness-agnostic.** `coven-reach` is a standalone binary. It speaks MCP over stdio and works with any MCP client — OpenClaw, Claude Code, Cursor, or anything else that speaks JSON-RPC 2.0.

---

## Features

| Tool | Operations |
|---|---|
| `reach_read` | Read files (text/base64), fetch URLs as markdown/text, metadata, file diff, checksums |
| `reach_write` | Put, mkdir, copy, move, delete, touch, archive (zip/tar.gz), unarchive |
| `reach_list` | Directory listing with recursive depth, size totals, server capabilities |
| `reach_find` | Advanced search: glob, content regex, size/date/MIME filters |
| `reach_familiar` | 🆕 List OpenCoven familiars from `~/.openclaw/workspace` |
| `reach_secret_check` | 🆕 Scan files for leaked secrets — reports locations, never values |

**Coven-specific additions over a generic MCP filesystem tool:**

- `reach_familiar` — introspect your Coven workspace: familiars, their identity, session counts, memory state
- `reach_secret_check` — pre-commit/pre-push secret scanning using the same pattern categories as `coven-cli`'s privacy redaction; values are **never** included in output, only file:line locations
- Batch operations — pass arrays of sources/entries, get per-item results
- Path allowlist enforcement via `COVEN_REACH_ALLOWED_PATHS`
- Read-only mode via `COVEN_REACH_READ_ONLY`

---

## Quick start

### Install

```sh
cargo install --git https://github.com/OpenCoven/coven-reach
```

Or build from source:

```sh
git clone https://github.com/OpenCoven/coven-reach
cd coven-reach
cargo build --release
# binary at target/release/coven-reach
```

### Configure with OpenClaw

Add to your `openclaw.toml` or the MCP client config:

```json
{
  "mcpServers": {
    "reach": {
      "command": "coven-reach",
      "env": {
        "COVEN_REACH_ALLOWED_PATHS": "~/Documents:~/Projects:/tmp",
        "COVEN_REACH_READ_ONLY": "false"
      }
    }
  }
}
```

---

## Configuration

| Environment variable | Default | Description |
|---|---|---|
| `COVEN_REACH_ALLOWED_PATHS` | `~:/tmp` | Colon-separated list of allowed root paths. Paths outside this list are rejected. |
| `COVEN_REACH_READ_ONLY` | `false` | Set to `true` to disable all write operations. |

Paths are resolved to their canonical form before checking. Symlinks are followed and the resolved target must also be within allowed roots.

---

## Tool reference

### `reach_read`

Read files or fetch URLs.

```json
{
  "operation": "content",
  "sources": ["~/notes.md", "https://example.com/article"],
  "format": "markdown"
}
```

**Operations:**
- `content` — read file as `text` (default), `base64`, or `markdown` (URL: HTML → markdown); supports `offset`/`length` for partial reads
- `metadata` — stat a file or HEAD a URL
- `diff` — unified diff between exactly two file paths
- `checksum` — `sha256` (default), `sha512`, or `md5`

---

### `reach_write`

Filesystem mutations. Disabled when `COVEN_REACH_READ_ONLY=true`.

```json
{
  "operation": "put",
  "entries": [
    { "path": "~/notes/todo.md", "content": "# TODO\n", "write_mode": "overwrite" }
  ]
}
```

**Operations:** `put`, `mkdir`, `copy`, `move`, `delete`, `touch`, `archive`, `unarchive`

Archive formats: `zip`, `tar.gz` / `tgz`

---

### `reach_list`

```json
{ "operation": "entries", "path": "~/Projects", "recursive_depth": 2, "calculate_recursive_size": true }
```

**Operations:**
- `entries` — directory listing with optional recursion and size totals
- `system_info` — server capabilities, allowed paths, version info

---

### `reach_find`

Advanced file search with AND logic across all criteria.

```json
{
  "path": "~/Projects",
  "name_pattern": "*.{rs,ts}",
  "content_pattern": "TODO|FIXME",
  "content_is_regex": true,
  "content_case_sensitive": false,
  "modified_after": "2026-01-01T00:00:00Z",
  "max_results": 50
}
```

**Parameters:** `path`, `recursive`, `name_pattern` (glob with `{a,b}` support), `case_sensitive`, `content_pattern`, `content_is_regex`, `content_case_sensitive`, `file_extensions`, `size_min`, `size_max`, `modified_after`, `modified_before`, `created_after`, `created_before`, `entry_type` (`file`/`directory`/`any`), `mime_type`, `max_results`

---

### `reach_familiar` 🕯️

List all OpenCoven familiars from your workspace. No auth required, read-only.

```json
{}
```

Returns: name, creature, vibe, emoji, workspace path, session count, memory state, last memory update.

---

### `reach_secret_check` 🔐

Scan for leaked secrets. Patterns cover: private keys, bearer tokens, OpenAI/Anthropic/GitHub keys, env var assignments, URL tokens.

```json
{
  "sources": ["~/Projects/my-app"],
  "recursive": true,
  "file_extensions": [".env", ".ts", ".rs", ".json"]
}
```

**Important:** Secret values are **never** included in output. Only `file:line:col` locations and the pattern category are returned.

---

## Security

- All paths are resolved to absolute form and checked against `COVEN_REACH_ALLOWED_PATHS` before any operation
- Symlinks are followed and the resolved target must also be within allowed roots
- `reach_secret_check` reports secret locations only — matched values are never serialized
- No unsafe code

---

## Development

```sh
cargo build
cargo test --test integration -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Part of OpenCoven

`coven-reach` is part of the [OpenCoven](https://opencoven.ai) ecosystem — an open framework for persistent AI familiars with memory, identity, and tools.

- [`coven`](https://github.com/OpenCoven/coven) — Coven daemon + harness substrate
- [`@opencoven/channels`](https://github.com/OpenCoven/coven/tree/main/packages/channels) — Channel connectors (Discord, etc.)
- [`cast-codes`](https://github.com/OpenCoven/cast-codes) — CastCodes editor

---

MIT License © OpenCoven Contributors
