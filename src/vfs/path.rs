/*
 *
 *       src/vfs/path.rs
 *       POSIX path normalization and parent/basename splitting
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Path canonicalization utilities (`normalize_path`, `split_parent_name`).

use alloc::{string::String, vec::Vec};

/// Normalize a path relative to `base_dir` (which must be an absolute path starting with `/`).
pub fn normalize_path(base_dir: &str, input: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();

    if !input.starts_with('/') {
        for part in base_dir.split('/') {
            if !part.is_empty() && part != "." {
                if part == ".." {
                    stack.pop();
                } else {
                    stack.push(part);
                }
            }
        }
    }

    for part in input.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            stack.pop();
        } else {
            stack.push(part);
        }
    }

    if stack.is_empty() {
        return String::from("/");
    }

    let mut out = String::with_capacity(input.len() + 1);
    for seg in stack {
        out.push('/');
        out.push_str(seg);
    }
    out
}

/// Split a normalized absolute path into `(parent_dir, file_name)`.
/// Returns `None` if `abs_path == "/"`.
pub fn split_parent_name(abs_path: &str) -> Option<(String, String)> {
    let trimmed = abs_path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let idx = trimmed.rfind('/')?;
    let parent = if idx == 0 {
        String::from("/")
    } else {
        String::from(&trimmed[..idx])
    };
    let name = String::from(&trimmed[idx + 1..]);
    if name.is_empty() {
        return None;
    }
    Some((parent, name))
}
