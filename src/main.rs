use std::io::{self, BufRead, Write};
use serde_json::Value;

use coven_reach::protocol::*;
use coven_reach::{security, tools, error};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: &str = "2024-11-05";

fn main() {
    let sec = security::Security::from_env();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };

        let response = handle_line(&line, &sec);
        let json = serde_json::to_string(&response).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"serialization error"}}"#.into()
        });
        let _ = writeln!(out, "{json}");
        let _ = out.flush();
    }
}

fn handle_line(line: &str, sec: &security::Security) -> JsonRpcResponse {
    let req: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return JsonRpcResponse::err(None, ERR_PARSE, format!("Parse error: {e}"), None);
        }
    };

    let id = req.id.clone();

    match req.method.as_str() {
        "initialize" => handle_initialize(id),
        "notifications/initialized" | "initialized" => {
            // No response for notifications
            JsonRpcResponse::ok(id, serde_json::json!({}))
        }
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tool_call(id, req.params, sec),
        "ping" => JsonRpcResponse::ok(id, serde_json::json!({})),
        _ => JsonRpcResponse::err(
            id,
            ERR_METHOD_NOT_FOUND,
            format!("Method not found: {}", req.method),
            None,
        ),
    }
}

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::ok(
        id,
        serde_json::to_value(InitializeResult {
            protocol_version: PROTOCOL_VERSION.into(),
            capabilities: ServerCapabilities {
                tools: serde_json::json!({ "listChanged": false }),
            },
            server_info: ServerInfo {
                name: "coven-reach".into(),
                version: VERSION.into(),
            },
        })
        .unwrap_or(serde_json::json!({})),
    )
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let tools: Vec<ToolDefinition> = vec![
        ToolDefinition {
            name: "reach_read".into(),
            description: "Read files or fetch URLs. Operations: content (read file/URL as text, base64, or markdown), metadata (stat/HEAD), diff (unified diff between two files), checksum (sha256/sha512/md5). Supports batch sources array.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["operation"],
                "properties": {
                    "operation": { "type": "string", "enum": ["content", "metadata", "diff", "checksum"] },
                    "sources": { "type": "array", "items": { "type": "string" }, "description": "File paths or URLs" },
                    "format": { "type": "string", "enum": ["text", "base64", "markdown"], "description": "Output format for content operation" },
                    "checksum_algorithm": { "type": "string", "enum": ["sha256", "sha512", "md5"] },
                    "offset": { "type": "integer", "description": "Byte offset for partial read" },
                    "length": { "type": "integer", "description": "Byte length for partial read" }
                }
            }),
        },
        ToolDefinition {
            name: "reach_write".into(),
            description: "Filesystem write operations. Operations: put (write file), mkdir, copy, move, delete, touch, archive (zip/tar.gz), unarchive. Disabled when COVEN_REACH_READ_ONLY=true.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["operation"],
                "properties": {
                    "operation": { "type": "string", "enum": ["put", "mkdir", "copy", "move", "delete", "touch", "archive", "unarchive"] },
                    "entries": { "type": "array", "description": "Array of operation entries (for put/mkdir/copy/move/delete/touch)" },
                    "archive_path": { "type": "string" },
                    "source_paths": { "type": "array", "items": { "type": "string" } },
                    "destination_path": { "type": "string" },
                    "format": { "type": "string", "enum": ["zip", "tar.gz", "tgz"] }
                }
            }),
        },
        ToolDefinition {
            name: "reach_list".into(),
            description: "List directory contents or get server/system info. Operations: entries (directory listing with optional recursion and size calculation), system_info (server capabilities, config).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["operation"],
                "properties": {
                    "operation": { "type": "string", "enum": ["entries", "system_info"] },
                    "path": { "type": "string" },
                    "recursive_depth": { "type": "integer", "default": 0 },
                    "calculate_recursive_size": { "type": "boolean", "default": false },
                    "info_type": { "type": "string", "enum": ["server_capabilities", "filesystem_stats"] }
                }
            }),
        },
        ToolDefinition {
            name: "reach_find".into(),
            description: "Advanced file search with glob patterns, content search (text/regex), size/date filters, and MIME type filtering.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "operation": { "type": "string", "enum": ["search"], "default": "search" },
                    "path": { "type": "string" },
                    "recursive": { "type": "boolean", "default": true },
                    "name_pattern": { "type": "string", "description": "Glob pattern e.g. *.rs or *.{js,ts}" },
                    "case_sensitive": { "type": "boolean", "default": false },
                    "content_pattern": { "type": "string" },
                    "content_is_regex": { "type": "boolean", "default": false },
                    "content_case_sensitive": { "type": "boolean", "default": false },
                    "file_extensions": { "type": "array", "items": { "type": "string" } },
                    "size_min": { "type": "integer" },
                    "size_max": { "type": "integer" },
                    "modified_after": { "type": "string", "description": "ISO 8601" },
                    "modified_before": { "type": "string" },
                    "created_after": { "type": "string" },
                    "created_before": { "type": "string" },
                    "entry_type": { "type": "string", "enum": ["file", "directory", "any"], "default": "any" },
                    "mime_type": { "type": "string" },
                    "max_results": { "type": "integer", "default": 200 }
                }
            }),
        },
        ToolDefinition {
            name: "reach_familiar".into(),
            description: "List OpenCoven familiars from ~/.openclaw/workspace. Returns name, creature, vibe, workspace path, session count, and memory status for each familiar. Read-only, no credentials required.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "reach_secret_check".into(),
            description: "Scan files or directories for common secret patterns (API keys, tokens, private keys). Reports file:line locations only — secret values are NEVER included in output. Useful for pre-commit checks.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["sources"],
                "properties": {
                    "sources": { "type": "array", "items": { "type": "string" }, "description": "File paths or directories to scan" },
                    "recursive": { "type": "boolean", "default": true },
                    "file_extensions": { "type": "array", "items": { "type": "string" }, "description": "Limit scan to these extensions e.g. [\".env\", \".ts\"]" }
                }
            }),
        },
    ];

    JsonRpcResponse::ok(
        id,
        serde_json::json!({ "tools": serde_json::to_value(tools).unwrap_or(serde_json::json!([])) }),
    )
}

fn handle_tool_call(id: Option<Value>, params: Option<Value>, sec: &security::Security) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => return JsonRpcResponse::err(id, ERR_INVALID_PARAMS, "tools/call requires params".into(), None),
    };

    let call: ToolCallParams = match serde_json::from_value(params) {
        Ok(c) => c,
        Err(e) => return JsonRpcResponse::err(id, ERR_INVALID_PARAMS, format!("Invalid params: {e}"), None),
    };

    let args = call.arguments.unwrap_or(serde_json::json!({}));

    let result = match call.name.as_str() {
        "reach_read" => tools::read::handle(&args, sec).map(ToolResult::json),
        "reach_write" => tools::write::handle(&args, sec).map(ToolResult::json),
        "reach_list" => tools::list::handle(&args, sec).map(ToolResult::json),
        "reach_find" => tools::find::handle(&args, sec).map(ToolResult::json),
        "reach_familiar" => tools::familiar::handle(&args).map(ToolResult::json),
        "reach_secret_check" => tools::secret_check::handle(&args, sec).map(ToolResult::json),
        name => Err(error::ReachError::Other(format!("Unknown tool: {name}"))),
    };

    match result {
        Ok(tool_result) => JsonRpcResponse::ok(
            id,
            serde_json::to_value(tool_result).unwrap_or(serde_json::json!({})),
        ),
        Err(e) => JsonRpcResponse::ok(
            id,
            serde_json::to_value(ToolResult::error(e.to_string())).unwrap_or(serde_json::json!({})),
        ),
    }
}
