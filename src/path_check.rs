//! Path validation for remote-supplied names at the trust boundary.
//!
//! Branch names returned by the server's `/refs` endpoint, tree entry names
//! inside downloaded tree objects, and object hashes from download responses
//! are all **remote-supplied data**. Nothing in the Rusty server protocol is
//! authenticated beyond the token, so a malicious or compromised server must
//! not be able to make `fetch`, `checkout`, or `merge` write files anywhere
//! on disk (see AUDIT.md, "Must fix" finding 2).
//!
//! The rule enforced here:
//!
//! 1. Every remote-supplied name is validated lexically *before* it is joined
//!    onto a local path: no empty/`.`/`..` components, no path separators in
//!    single-component names, no backslashes, no absolute paths, no drive
//!    letters or colon-containing names, no control characters, and no
//!    Windows reserved device names.
//! 2. Every write derived from a remote tree additionally goes through
//!    [`safe_join`], which verifies the final path stays inside the
//!    repository root and passes through no symlink on the way down.
//!
//! Locally produced values (CLI arguments, paths derived from canonicalized
//! files on disk, index keys built by `add`) are *not* treated as remote
//! data: the user already controls their own machine. Keeping the trust
//! boundary explicit is the point of this module.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Windows device names that would silently redirect writes (e.g. `NUL`)
/// instead of creating a normal file.
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn is_windows_reserved(name: &str) -> bool {
    // "NUL.txt" is still special on Windows, so compare name-without-extension.
    let base = name.split('.').next().unwrap_or("");

    WINDOWS_RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(base))
}

/// Validate a single path component that came from a remote server
/// (a tree entry name, or one segment of a branch/ref name).
///
/// A component must be a non-empty name that means "one file or directory"
/// and nothing else: no separators, no `.`/`..`, no absolute or drive
/// syntax, no null/control characters, no reserved device names.
pub fn validate_component(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("unsafe path name \"\": empty component");
    }

    if name == "." || name == ".." {
        bail!("unsafe path name \"{}\": directory shortcut", name);
    }

    if name.contains(['/', '\\']) {
        bail!("unsafe path name \"{}\": contains a path separator", name);
    }

    // A colon would enable drive-letter paths (`C:...`), alternate data
    // streams (`file:stream`), and device access on Windows.
    if name.contains(':') || name.contains('\0') {
        bail!("unsafe path name \"{}\": illegal character", name);
    }

    if name.chars().any(|c| c.is_control()) {
        bail!("unsafe path name \"{}\": control character", name);
    }

    if is_windows_reserved(name) {
        bail!("unsafe path name \"{}\": reserved device name", name);
    }

    Ok(())
}

/// Validate a slash-separated relative name that came from a remote server,
/// such as a branch name from `/refs` (`main`, `feature/login`).
///
/// Directory structure is allowed (the CLI also reads nested remote-tracking
/// refs), but `..`, absolute paths, drive letters, and backslashes are not.
pub fn validate_ref_path(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("unsafe ref name \"\": empty");
    }

    if name.starts_with('/') || name.starts_with('\\') {
        bail!("unsafe ref name \"{}\": absolute path", name);
    }

    // A leading drive component ("C:") is rejected by the colon rule inside
    // validate_component; empty segments ("a//b", "a/") are rejected there too.
    for part in name.split('/') {
        validate_component(part)?;
    }

    Ok(())
}

/// Validate a slash-separated relative path that will be joined onto the
/// repository root (a path built from remote tree entry names).
pub fn validate_rel_path(path: &str) -> Result<()> {
    validate_ref_path(path)
}

/// Validate an object hash that will be used as a file name under `.rusty/objects`.
///
/// The hash in a download response is remote-supplied and becomes a path, so
/// it must be exactly 64 hex characters (SHA-256).
pub fn validate_object_hash(hash: &str) -> Result<()> {
    let is_valid = hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b));

    if !is_valid {
        bail!("unsafe object hash \"{}\": not a 64-char SHA-256", hash);
    }

    Ok(())
}

/// Build the absolute path for `rel` inside `repo_root` for a **write or
/// delete**, refusing to return a path that could escape the repository.
///
/// Two independent checks:
///
/// 1. Lexical: `rel` (already validated by [`validate_rel_path`]) may never
///    contain `..`, absolute, or drive syntax — enforced again here so a
///    call site cannot bypass validation by construction.
/// 2. Link traversal: if any directory on the way down is an existing
///    symlink, the final write can land outside the repository even though
///    the path text is valid, so that is refused as well.
///
/// `repo_root` must be the repository root (`.rusty`'s parent); callers
/// normally pass the canonicalized root. This function does NOT create
/// directories — it only computes and validates the path.
pub fn safe_join(repo_root: &Path, rel: &str) -> Result<PathBuf> {
    validate_rel_path(rel)?;

    let canonical_root = repo_root
        .canonicalize()
        .ok()
        .unwrap_or_else(|| repo_root.to_path_buf());

    let joined = canonical_root.join(rel);

    // Belt-and-braces: the joined result must stay under the canonical root.
    if !joined.starts_with(&canonical_root) {
        bail!(
            "unsafe path \"{}\": resolves outside the repository",
            joined.display()
        );
    }

    // Refuse to traverse an existing symlink while descending, because it
    // would silently retarget the write/delete outside the repository.
    let mut current = canonical_root.clone();

    for part in rel.split('/') {
        current = current.join(part);

        if let Ok(meta) = fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() {
                bail!(
                    "unsafe path \"{}\": traverses a symlink",
                    joined.display()
                );
            }
        }
    }

    Ok(joined)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Single-component names
    // ------------------------------------------------------------------

    #[test]
    fn accepts_normal_component() {
        assert!(validate_component("main.rs").is_ok());
        assert!(validate_component("src").is_ok());
        assert!(validate_component("file name.txt").is_ok());
        assert!(validate_component("üñîçø∂é").is_ok());
    }

    #[test]
    fn rejects_traversal_component() {
        assert!(validate_component("").is_err());
        assert!(validate_component(".").is_err());
        assert!(validate_component("..").is_err());
    }

    #[test]
    fn rejects_separator_component() {
        assert!(validate_component("a/b").is_err());
        assert!(validate_component("a\\b").is_err());
        assert!(validate_component("a/../b").is_err());
    }

    #[test]
    fn rejects_drive_and_absolute_component() {
        assert!(validate_component("C:").is_err());
        assert!(validate_component("C:evil").is_err());
    }

    #[test]
    fn rejects_hidden_dangerous_names() {
        assert!(validate_component("\0").is_err());
        assert!(validate_component("a\nb").is_err());
        assert!(validate_component("NUL").is_err());
        assert!(validate_component("nul.txt").is_err());
        assert!(validate_component("CON").is_err());

        // A lone ".." hidden behind a normal-looking name is still normal
        // text ("a..b" is a legal file name), only exact "." / ".." are bad.
        assert!(validate_component("a..b").is_ok());
    }

    // ------------------------------------------------------------------
    // Slash-separated ref names from /refs
    // ------------------------------------------------------------------

    #[test]
    fn accepts_usual_branch_names() {
        assert!(validate_ref_path("main").is_ok());
        assert!(validate_ref_path("feature/login-page").is_ok());
        assert!(validate_ref_path("release/v2.0").is_ok());
    }

    #[test]
    fn reject_ref_traversal() {
        assert!(validate_ref_path("").is_err());
        assert!(validate_ref_path("../evil").is_err());
        assert!(validate_ref_path("a/../b").is_err());
        assert!(validate_ref_path("feature/../../pwned").is_err());
        assert!(validate_ref_path("/abs/branch").is_err());
        assert!(validate_ref_path("a//b").is_err());
        assert!(validate_ref_path("a\\b").is_err());
        assert!(validate_ref_path("C:").is_err());
        assert!(validate_ref_path("branch:stream").is_err());
        assert!(validate_ref_path("feature/").is_err());
        assert!(validate_ref_path(".").is_err());
    }

    // ------------------------------------------------------------------
    // Object hashes used as object file names
    // ------------------------------------------------------------------

    #[test]
    fn accepts_real_sha256() {
        let hash = "a591a6d40bf420404a011733cf0a31b236d9c3a8c9c4bfa80b7a4cbef0ce5da8";
        assert_eq!(hash.len(), 64);
        assert!(validate_object_hash(hash).is_ok());
    }

    #[test]
    fn rejects_non_hashes() {
        assert!(validate_object_hash("").is_err());
        assert!(validate_object_hash("../pwned").is_err());
        assert!(validate_object_hash("d41d8cd98f00b204e9800998ecf8427e").is_err()); // md5-style
        assert!(validate_object_hash("no-slash-here/../../HEAD").is_err());
        // 63 chars — wrong length corrupts the object file name scheme.
        assert!(validate_object_hash(&"a".repeat(63)).is_err());
    }

    // ------------------------------------------------------------------
    // safe_join: the end-to-end guard for writes
    // ------------------------------------------------------------------

    #[test]
    fn safe_join_accepts_normal_relative_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("work");
        fs::create_dir_all(&root).unwrap();

        let joined = safe_join(&root, "src/main.rs").unwrap();
        assert!(joined.starts_with(fs::canonicalize(&root).unwrap()));
    }

    #[test]
    fn safe_join_rejects_traversal_even_directly() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("work");
        fs::create_dir_all(&root).unwrap();

        assert!(safe_join(&root, "../pwned.txt").is_err());
        assert!(safe_join(&root, "..\\pwned.txt").is_err());
        assert!(safe_join(&root, "src/../../../pwned.txt").is_err());
        assert!(safe_join(&root, "C:pwned.txt").is_err());
        assert!(safe_join(&root, "").is_err());
    }

    #[test]
    fn safe_join_rejects_symlink_traversal() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("work");
        let outside = dir.path().join("outside");

        fs::create_dir_all(&root).unwrap();

        fs::create_dir_all(&outside).unwrap();

        // Create a benign directory in the repo that is replaced by (or born
        // as) a symlink pointing outside.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("evil")).unwrap();

            assert!(safe_join(&root, "evil/pwned.txt").is_err());
            assert!(safe_join(&root, "normalfile.txt").is_ok());
        }
    }
}
