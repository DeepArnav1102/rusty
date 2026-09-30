use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct RustyIgnore {
    patterns: Vec<String>,
}

impl RustyIgnore {
    pub fn load(repo_root: &Path) -> Self {
        let ignore_path = repo_root.join(".rustyignore");
        let mut patterns = Vec::new();

        if ignore_path.exists() {
            if let Ok(content) = fs::read_to_string(&ignore_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    patterns.push(trimmed.to_string());
                }
            }
        }

        Self { patterns }
    }

    pub fn is_ignored(&self, relative_path: &str, is_dir: bool) -> bool {
        let normalized = relative_path.replace('\\', "/");
        let normalized = normalized.trim_matches('/');

        if normalized.is_empty() {
            return false;
        }

        // Hardcoded rules: .rusty and .git are always ignored
        for comp in normalized.split('/') {
            if comp == ".rusty" || comp == ".git" {
                return true;
            }
        }

        let file_name = normalized.split('/').next_back().unwrap_or(normalized);

        for pat in &self.patterns {
            let pat = pat.trim();

            // Directory pattern (ends with '/')
            if let Some(dir_pat) = pat.strip_suffix('/') {
                let dir_pat = dir_pat.trim_matches('/');
                if dir_pat.contains('/') {
                    // Path-specific directory (e.g. "build/output/")
                    if normalized == dir_pat || normalized.starts_with(&format!("{}/", dir_pat)) {
                        return true;
                    }
                } else {
                    // Simple directory name (e.g. "target/" or "node_modules/")
                    if is_dir && (file_name == dir_pat || normalized == dir_pat) {
                        return true;
                    }
                    for comp in normalized.split('/') {
                        if comp == dir_pat {
                            return true;
                        }
                    }
                }
            }
            // Extension pattern (starts with "*.")
            else if let Some(ext) = pat.strip_prefix("*.") {
                let dot_ext = format!(".{}", ext);
                if normalized.ends_with(&dot_ext) {
                    return true;
                }
            }
            // Exact file / path pattern (e.g. ".env", "secret.txt", "docs/temp.txt")
            else {
                let pat_clean = pat.trim_matches('/');
                if pat_clean.contains('/') {
                    // Anchored / multi-component path
                    if normalized == pat_clean || normalized.starts_with(&format!("{}/", pat_clean))
                    {
                        return true;
                    }
                } else {
                    // Single filename (matches in any directory, like .env)
                    if file_name == pat_clean {
                        return true;
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_internal_ignores() {
        let ignore = RustyIgnore::default();
        assert!(ignore.is_ignored(".rusty", true));
        assert!(ignore.is_ignored(".rusty/index", false));
        assert!(ignore.is_ignored(".git", true));
        assert!(ignore.is_ignored(".git/HEAD", false));
        assert!(!ignore.is_ignored(".rustyignore", false));
        assert!(!ignore.is_ignored("a.txt", false));
    }

    #[test]
    fn test_patterns() {
        let ignore = RustyIgnore {
            patterns: vec![
                "target/".to_string(),
                "node_modules/".to_string(),
                "*.log".to_string(),
                "*.tmp".to_string(),
                ".env".to_string(),
            ],
        };

        // Directory rules
        assert!(ignore.is_ignored("target", true));
        assert!(ignore.is_ignored("target/debug/foo", false));
        assert!(ignore.is_ignored("sub/node_modules/bar.js", false));
        assert!(!ignore.is_ignored("target_file.txt", false));

        // Extension rules
        assert!(ignore.is_ignored("app.log", false));
        assert!(ignore.is_ignored("deep/nested/server.log", false));
        assert!(ignore.is_ignored("test.tmp", false));
        assert!(!ignore.is_ignored("app.logger", false));

        // Exact file rules
        assert!(ignore.is_ignored(".env", false));
        assert!(ignore.is_ignored("backend/.env", false));
        assert!(!ignore.is_ignored(".environment", false));
    }
}
