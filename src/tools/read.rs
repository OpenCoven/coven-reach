use serde_json::Value;
use crate::error::{ReachError, ReachResult};
use crate::security::Security;
use std::path::Path;

pub fn handle(args: &Value, sec: &Security) -> ReachResult<Value> {
    let op = args["operation"].as_str().unwrap_or("");
    let sources: Vec<&str> = args["sources"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    match op {
        "content" => read_content(args, sources, sec),
        "metadata" => read_metadata(sources, sec),
        "diff" => read_diff(sources, sec),
        "checksum" => read_checksum(args, sources, sec),
        _ => Err(ReachError::InvalidArgument(format!("unknown reach_read operation: '{op}'"))),
    }
}

fn read_content(args: &Value, sources: Vec<&str>, sec: &Security) -> ReachResult<Value> {
    let format = args["format"].as_str().unwrap_or("text");
    let offset = args["offset"].as_u64().map(|v| v as usize);
    let length = args["length"].as_u64().map(|v| v as usize);

    let mut results = Vec::new();
    for src in sources {
        let r = if src.starts_with("http://") || src.starts_with("https://") {
            fetch_url(src, format)
        } else {
            read_file(src, format, offset, length, sec)
        };
        results.push(match r {
            Ok(v) => v,
            Err(e) => e.to_json(),
        });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn read_file(
    path: &str,
    format: &str,
    offset: Option<usize>,
    length: Option<usize>,
    sec: &Security,
) -> ReachResult<Value> {
    let resolved = sec.check_exists(path)?;
    let bytes = std::fs::read(&resolved).map_err(|e| ReachError::Io {
        path: resolved.display().to_string(),
        source: e,
    })?;

    let bytes = if let Some(off) = offset {
        let off = off.min(bytes.len());
        let end = length.map(|l| (off + l).min(bytes.len())).unwrap_or(bytes.len());
        bytes[off..end].to_vec()
    } else if let Some(len) = length {
        bytes[..len.min(bytes.len())].to_vec()
    } else {
        bytes
    };

    let content: Value = match format {
        "base64" => {
            use std::io::Read;
            // Simple base64 encoding
            let encoded = base64_encode(&bytes);
            serde_json::json!({ "encoding": "base64", "data": encoded })
        }
        "text" | _ => {
            let text = String::from_utf8_lossy(&bytes).to_string();
            serde_json::json!({ "encoding": "text", "content": text })
        }
    };

    Ok(serde_json::json!({
        "ok": true,
        "path": resolved.display().to_string(),
        "size": bytes.len(),
        "content": content,
    }))
}

fn fetch_url(url: &str, format: &str) -> ReachResult<Value> {
    let response = ureq::get(url).call().map_err(|e| ReachError::Fetch {
        url: url.to_string(),
        message: e.to_string(),
    })?;

    let status = response.status();
    let content_type = response.content_type().to_string();
    let body = response.into_string().map_err(|e| ReachError::Fetch {
        url: url.to_string(),
        message: e.to_string(),
    })?;

    let content: Value = match format {
        "markdown" => {
            let md = htmd::convert(&body).unwrap_or_else(|_| body.clone());
            serde_json::json!({ "encoding": "markdown", "content": md })
        }
        _ => serde_json::json!({ "encoding": "text", "content": body }),
    };

    Ok(serde_json::json!({
        "ok": true,
        "url": url,
        "status": status,
        "contentType": content_type,
        "content": content,
    }))
}

fn read_metadata(sources: Vec<&str>, sec: &Security) -> ReachResult<Value> {
    let mut results = Vec::new();
    for src in sources {
        let r = if src.starts_with("http://") || src.starts_with("https://") {
            url_metadata(src)
        } else {
            file_metadata(src, sec)
        };
        results.push(match r {
            Ok(v) => v,
            Err(e) => e.to_json(),
        });
    }
    Ok(serde_json::json!({ "ok": true, "results": results }))
}

fn file_metadata(path: &str, sec: &Security) -> ReachResult<Value> {
    let resolved = sec.check_exists(path)?;
    let meta = std::fs::metadata(&resolved).map_err(|e| ReachError::Io {
        path: resolved.display().to_string(),
        source: e,
    })?;
    let mime = mime_guess::from_path(&resolved)
        .first_or_octet_stream()
        .to_string();
    use std::time::UNIX_EPOCH;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let created = meta
        .created()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    Ok(serde_json::json!({
        "ok": true,
        "path": resolved.display().to_string(),
        "size": meta.len(),
        "isDir": meta.is_dir(),
        "isFile": meta.is_file(),
        "isSymlink": meta.file_type().is_symlink(),
        "mimeType": mime,
        "modifiedUnix": modified,
        "createdUnix": created,
        "readonly": meta.permissions().readonly(),
    }))
}

fn url_metadata(url: &str) -> ReachResult<Value> {
    let response = ureq::head(url).call().map_err(|e| ReachError::Fetch {
        url: url.to_string(),
        message: e.to_string(),
    })?;
    let status = response.status();
    let content_type = response.content_type().to_string();
    let content_length = response.header("content-length").map(|s| s.to_string());

    Ok(serde_json::json!({
        "ok": true,
        "url": url,
        "status": status,
        "contentType": content_type,
        "contentLength": content_length,
    }))
}

fn read_diff(sources: Vec<&str>, sec: &Security) -> ReachResult<Value> {
    if sources.len() != 2 {
        return Err(ReachError::InvalidArgument(
            "diff requires exactly 2 sources".into(),
        ));
    }
    let a = sec.check_exists(sources[0])?;
    let b = sec.check_exists(sources[1])?;
    let text_a = std::fs::read_to_string(&a).map_err(|e| ReachError::Io {
        path: a.display().to_string(),
        source: e,
    })?;
    let text_b = std::fs::read_to_string(&b).map_err(|e| ReachError::Io {
        path: b.display().to_string(),
        source: e,
    })?;

    let diff = similar::TextDiff::from_lines(&text_a, &text_b);
    let unified = diff
        .unified_diff()
        .header(&a.display().to_string(), &b.display().to_string())
        .to_string();

    Ok(serde_json::json!({
        "ok": true,
        "sources": [a.display().to_string(), b.display().to_string()],
        "format": "unified",
        "diff": unified,
        "changed": !unified.is_empty(),
    }))
}

fn read_checksum(args: &Value, sources: Vec<&str>, sec: &Security) -> ReachResult<Value> {
    let algo = args["checksum_algorithm"].as_str().unwrap_or("sha256");
    let mut results = Vec::new();
    for src in &sources {
        let r = compute_checksum(src, algo, sec);
        results.push(match r {
            Ok(v) => v,
            Err(e) => e.to_json(),
        });
    }
    Ok(serde_json::json!({ "ok": true, "algorithm": algo, "results": results }))
}

fn compute_checksum(path: &str, algo: &str, sec: &Security) -> ReachResult<Value> {
    let resolved = sec.check_exists(path)?;
    let bytes = std::fs::read(&resolved).map_err(|e| ReachError::Io {
        path: resolved.display().to_string(),
        source: e,
    })?;

    let hash = match algo {
        "sha256" | "sha-256" => {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(&bytes))
        }
        "sha512" | "sha-512" => {
            use sha2::{Digest, Sha512};
            hex::encode(Sha512::digest(&bytes))
        }
        "md5" => {
            use md5::Digest;
            hex::encode(md5::Md5::digest(&bytes))
        }
        _ => {
            return Err(ReachError::InvalidArgument(format!(
                "unknown algorithm '{algo}' — use sha256, sha512, or md5"
            )))
        }
    };

    Ok(serde_json::json!({
        "ok": true,
        "path": resolved.display().to_string(),
        "algorithm": algo,
        "checksum": hash,
        "size": bytes.len(),
    }))
}

fn base64_encode(bytes: &[u8]) -> String {
    // Simple base64 without external dep
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(CHARS[(b0 >> 2)] as char);
        out.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[b2 & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}
