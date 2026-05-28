use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC 2.0 ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<Value>, code: i64, message: String, data: Option<Value>) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message, data }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// Standard JSON-RPC error codes
pub const ERR_PARSE: i64 = -32700;
pub const ERR_INVALID_REQUEST: i64 = -32600;
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;
pub const ERR_INTERNAL: i64 = -32603;

// ── MCP types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ToolResult {
    pub fn text(s: impl Into<String>) -> Self {
        ToolResult {
            content: vec![ToolContent::text(s)],
            is_error: None,
        }
    }

    pub fn json(v: Value) -> Self {
        let s = serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string());
        ToolResult {
            content: vec![ToolContent::text(s)],
            is_error: None,
        }
    }

    pub fn error(s: impl Into<String>) -> Self {
        ToolResult {
            content: vec![ToolContent::text(s)],
            is_error: Some(true),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
}

impl ToolContent {
    pub fn text(s: impl Into<String>) -> Self {
        ToolContent { kind: "text".into(), text: s.into() }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
