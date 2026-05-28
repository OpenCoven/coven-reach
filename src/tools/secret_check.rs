use serde_json::Value;
use regex::Regex;
use std::path::Path;
use once_cell::sync::Lazy;
use crate::error::{ReachError, ReachResult};
use crate::security::Security;

/// Secret patterns to detect — matches categories from coven privacy.rs.
/// Reports locations but NEVER includes the matched value in output.
static SECRET_PATTERNS: Lazy<Vec<(&'static str, Regex)>> = Lazy::new(|| {
    let patterns: &[(&str, &str)] = &[
        ("private_key", r"(?is)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----"),
        ("bearer_token", r"(?im)\bAuthorization\s*:\s*(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]{8,}"),
        ("openai_key", r"\bsk-[A-Za-z0-9]{20,}\b"),
        ("anthropic_key", r"\bsk-ant-[A-Za-z0-9_-]{20,}\b"),
        ("github_token", r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"),
        ("github_pat", r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
        ("env_secret", r#"(?i)\b(?:OPENAI_API_KEY|ANTHROPIC_API_KEY|GITHUB_TOKEN|GH_TOKEN|API[_-]?KEY|ACCESS[_-]?TOKEN|REFRESH[_-]?TOKEN|AUTH[_-]?TOKEN|SECRET|PASSWORD|PRIVATE[_-]?KEY)\s*=\s*["']?[^"'\s]{8,}"#),
        ("inline_secret", r#"(?i)\b(?:api[_-]?key|access[_-]?token|refresh[_-]?token|auth[_-]?token|secret|password|private[_-]?key)\s*[:=]\s*["']?[^"',\s}]{8,}"#),
        ("url_token", r#"(?i)([?&](?:token|key|secret|api_key|access_token)=)[^&\s"']{8,}"#),
    ];
    patterns.iter()
        .filter_map(|(name, pat)| Regex::new(pat).ok().map(|r| (*name, r)))
        .collect()
});

pub fn handle(args: &Value, sec: &Security) -> ReachResult<Value> {
    let sources: Vec<&str> = args["sources"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let recursive = args["recursive"].as_bool().unwrap_or(true);
    let extensions: Option<Vec<String>> = args["file_extensions"].as_array().map(|a| {
        a.iter().filter_map(|v| v.as_str()).map(|s| {
            if s.starts_with('.') { s.to_string() } else { format!(".{s}") }
        }).collect()
    });

    let mut results = Vec::new();
    let mut total_findings = 0usize;

    for src in sources {
        let resolved = sec.check_exists(src)?;
        if resolved.is_file() {
            let r = scan_file(&resolved, &extensions)?;
            total_findings += r.len();
            results.push(serde_json::json!({
                "path": resolved.display().to_string(),
                "findings": r,
                "clean": r.is_empty(),
            }));
        } else if resolved.is_dir() {
            let walker = if recursive {
                walkdir::WalkDir::new(&resolved)
            } else {
                walkdir::WalkDir::new(&resolved).max_depth(1)
            };
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() { continue; }
                let path = entry.path();
                if let Some(exts) = &extensions {
                    let ext = path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| format!(".{e}"))
                        .unwrap_or_default();
                    if !exts.iter().any(|e| e.eq_ignore_ascii_case(&ext)) { continue; }
                }
                if sec.check(path.to_str().unwrap_or("")).is_err() { continue; }
                let r = scan_file(path, &extensions)?;
                total_findings += r.len();
                if !r.is_empty() {
                    results.push(serde_json::json!({
                        "path": path.display().to_string(),
                        "findings": r,
                        "clean": false,
                    }));
                }
            }
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "clean": total_findings == 0,
        "totalFindings": total_findings,
        "filesScanned": results.len(),
        "results": results,
        "note": "Secret values are never included in output — only locations are reported.",
    }))
}

fn scan_file(path: &Path, _extensions: &Option<Vec<String>>) -> ReachResult<Vec<Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Ok(vec![]), // binary file — skip
    };

    let mut findings = Vec::new();
    for (pattern_name, regex) in SECRET_PATTERNS.iter() {
        for mat in regex.find_iter(&text) {
            // Find line number
            let line_no = text[..mat.start()].chars().filter(|&c| c == '\n').count() + 1;
            let col = mat.start() - text[..mat.start()].rfind('\n').map(|p| p + 1).unwrap_or(0);
            findings.push(serde_json::json!({
                "pattern": pattern_name,
                "line": line_no,
                "col": col,
                "length": mat.end() - mat.start(),
                // Never include the matched text
            }));
        }
    }

    Ok(findings)
}
