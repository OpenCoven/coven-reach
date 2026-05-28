use std::path::{Path, PathBuf};
use crate::error::{ReachError, ReachResult};

pub struct Security {
    allowed: Vec<PathBuf>,
    pub read_only: bool,
}

impl Security {
    pub fn from_env() -> Self {
        let home = dirs_home();
        let raw = std::env::var("COVEN_REACH_ALLOWED_PATHS")
            .unwrap_or_else(|_| format!("{}:/tmp", home.display()));
        let allowed = raw
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| expand_tilde(s))
            .collect();
        let read_only = std::env::var("COVEN_REACH_READ_ONLY")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        Security { allowed, read_only }
    }

    /// Resolve `path` to an absolute canonical-ish path and verify it's inside
    /// an allowed root. Returns the resolved PathBuf.
    pub fn check(&self, path: &str) -> ReachResult<PathBuf> {
        let expanded = expand_tilde(path);
        // Resolve without requiring the path to exist yet (for write ops)
        let resolved = resolve_path(&expanded);
        // Check allowed
        for allowed in &self.allowed {
            let allowed_resolved = resolve_path(allowed);
            if resolved.starts_with(&allowed_resolved) {
                return Ok(resolved);
            }
        }
        Err(ReachError::PathNotAllowed(format!(
            "{} — allowed roots: {}",
            resolved.display(),
            self.allowed
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    /// Like `check` but also requires the path to exist.
    pub fn check_exists(&self, path: &str) -> ReachResult<PathBuf> {
        let resolved = self.check(path)?;
        if !resolved.exists() {
            return Err(ReachError::PathNotFound(resolved.display().to_string()));
        }
        Ok(resolved)
    }

    pub fn require_write(&self) -> ReachResult<()> {
        if self.read_only {
            Err(ReachError::ReadOnly)
        } else {
            Ok(())
        }
    }

    pub fn allowed_paths(&self) -> Vec<String> {
        self.allowed.iter().map(|p| p.display().to_string()).collect()
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn expand_tilde(path: impl AsRef<Path>) -> PathBuf {
    let p = path.as_ref();
    if let Ok(stripped) = p.strip_prefix("~") {
        dirs_home().join(stripped)
    } else {
        p.to_path_buf()
    }
}

/// Resolve to an absolute path. Resolves `..` and `.` components without
/// requiring the path to exist (unlike `canonicalize`).
pub fn resolve_path(path: impl AsRef<Path>) -> PathBuf {
    let p = path.as_ref();
    let base = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    };
    let mut out = PathBuf::new();
    for component in base.components() {
        use std::path::Component::*;
        match component {
            RootDir | Prefix(_) => out.push(component),
            CurDir => {}
            ParentDir => { out.pop(); }
            Normal(c) => out.push(c),
        }
    }
    // If path actually exists, try real canonicalize to resolve symlinks
    if out.exists() {
        std::fs::canonicalize(&out).unwrap_or(out)
    } else {
        out
    }
}
