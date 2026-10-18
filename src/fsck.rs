use anyhow::Result;
use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::index::Index;
use crate::tree::Tree;

enum FileBytes {
    Mmap(Mmap),
    Vec(Vec<u8>),
}

impl Deref for FileBytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            FileBytes::Mmap(m) => m,
            FileBytes::Vec(v) => v,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ObjectType {
    Blob,
    Tree,
    Commit,
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectType::Blob => write!(f, "blob"),
            ObjectType::Tree => write!(f, "tree"),
            ObjectType::Commit => write!(f, "commit"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    pub severity: ProblemSeverity,
    pub code: &'static str,
    pub message: String,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProblemSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct DanglingObject {
    pub object_type: ObjectType,
    pub hash: String,
}

#[derive(Debug, Default, Serialize)]
pub struct FsckStats {
    pub total_objects_scanned: usize,
    pub commits_checked: usize,
    pub trees_checked: usize,
    pub blobs_checked: usize,
    pub refs_checked: usize,
    pub unreachable_objects_count: usize,
    pub dangling_objects_count: usize,
    pub errors_count: usize,
    pub warnings_count: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct FsckReport {
    pub is_clean: bool,
    pub stats: FsckStats,
    pub problems: Vec<Problem>,
    pub unreachable: Vec<DanglingObject>,
    pub dangling: Vec<DanglingObject>,
    pub lost_found_written: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FsckOptions {
    pub unreachable: bool,
    pub dangling: bool,
    pub connectivity_only: bool,
    pub strict: bool,
    pub lost_found: bool,
    pub json: bool,
}

impl Default for FsckOptions {
    fn default() -> Self {
        Self {
            unreachable: false,
            dangling: true,
            connectivity_only: false,
            strict: false,
            lost_found: false,
            json: false,
        }
    }
}

pub fn run_fsck(repo_path: &Path, options: &FsckOptions) -> Result<FsckReport> {
    let mut problems: Vec<Problem> = Vec::new();

    // 1. Discover all root references (branches, tags, remotes, HEAD, MERGE_HEAD, index)
    let root_refs = discover_roots(repo_path, &mut problems);

    // 2. Discover all objects physically stored on disk: (hash, type, path)
    let disk_objects = scan_disk_objects(repo_path, &mut problems);

    // Build fast lookup table of disk objects
    let mut disk_map: HashMap<String, (ObjectType, PathBuf)> = HashMap::with_capacity(disk_objects.len());
    for (hash, obj_type, path) in &disk_objects {
        disk_map.insert(hash.clone(), (*obj_type, path.clone()));
    }

    // 3. Parallel verification of objects on disk
    let total_scanned = disk_objects.len();
    let commits_checked = AtomicUsize::new(0);
    let trees_checked = AtomicUsize::new(0);
    let blobs_checked = AtomicUsize::new(0);

    // Store outgoing edges: parent -> children (references to other objects)
    // and reverse edges: child -> parents (incoming references)
    struct ParsedObject {
        hash: String,
        obj_type: ObjectType,
        children: Vec<(String, ObjectType)>,
        problems: Vec<Problem>,
    }

    let parsed_results: Vec<ParsedObject> = disk_objects
        .par_iter()
        .map(|(hash, obj_type, path)| {
            let mut obj_problems = Vec::new();
            let mut children = Vec::new();

            // Hash verification (unless connectivity_only is on and it's a blob)
            // Even under connectivity_only, checking sha256 of metadata (commits, trees) is vital.
            let check_hash = !options.connectivity_only || *obj_type != ObjectType::Blob;

            let file_data: Option<FileBytes> = match File::open(path) {
                Ok(file) => {
                    let mmap_opt = unsafe { Mmap::map(&file).ok() };
                    match mmap_opt {
                        Some(mmap) => Some(FileBytes::Mmap(mmap)),
                        None => {
                            // Empty file or mmap fallback (e.g. empty blobs)
                            fs::read(path).ok().map(FileBytes::Vec)
                        }
                    }
                }
                Err(e) => {
                    obj_problems.push(Problem {
                        severity: ProblemSeverity::Error,
                        code: "OBJECT_READ_ERROR",
                        message: format!("Failed to read object {}: {}", path.display(), e),
                        target: hash.clone(),
                    });
                    None
                }
            };

            if let Some(bytes) = file_data.as_deref() {
                if check_hash {
                    let mut hasher = Sha256::new();
                    hasher.update(bytes);
                    let actual_hash = hex::encode(hasher.finalize());
                    if actual_hash != *hash {
                        obj_problems.push(Problem {
                            severity: ProblemSeverity::Error,
                            code: "CORRUPT_OBJECT_HASH_MISMATCH",
                            message: format!(
                                "SHA-256 mismatch for {}: expected {}, computed {}",
                                obj_type, hash, actual_hash
                            ),
                            target: hash.clone(),
                        });
                    }
                }

                match obj_type {
                    ObjectType::Blob => {
                        blobs_checked.fetch_add(1, Ordering::Relaxed);
                    }
                    ObjectType::Tree => {
                        trees_checked.fetch_add(1, Ordering::Relaxed);
                        match serde_json::from_slice::<Tree>(bytes) {
                            Ok(tree) => {
                                let mut seen_names = HashSet::new();
                                for entry in &tree.entries {
                                    if options.strict {
                                        if entry.name.is_empty() || entry.name == "." || entry.name == ".." || entry.name.contains('/') || entry.name.contains('\\') {
                                            obj_problems.push(Problem {
                                                severity: ProblemSeverity::Error,
                                                code: "TREE_INVALID_ENTRY_NAME",
                                                message: format!("Tree {} has invalid entry name: {:?}", hash, entry.name),
                                                target: hash.clone(),
                                            });
                                        }
                                        if !seen_names.insert(&entry.name) {
                                            obj_problems.push(Problem {
                                                severity: ProblemSeverity::Error,
                                                code: "TREE_DUPLICATE_ENTRY",
                                                message: format!("Tree {} contains duplicate entry name: {}", hash, entry.name),
                                                target: hash.clone(),
                                            });
                                        }
                                        if !is_valid_hex_hash(&entry.object_hash) {
                                            obj_problems.push(Problem {
                                                severity: ProblemSeverity::Error,
                                                code: "INVALID_HASH_SYNTAX",
                                                message: format!("Tree {} has invalid object hash: {}", hash, entry.object_hash),
                                                target: entry.object_hash.clone(),
                                            });
                                        }
                                    }

                                    let target_type = match entry.object_type.as_str() {
                                        "blob" => ObjectType::Blob,
                                        "tree" => ObjectType::Tree,
                                        other => {
                                            obj_problems.push(Problem {
                                                severity: ProblemSeverity::Error,
                                                code: "TREE_UNKNOWN_OBJECT_TYPE",
                                                message: format!("Unknown entry type '{}' in tree {}", other, hash),
                                                target: hash.clone(),
                                            });
                                            ObjectType::Blob
                                        }
                                    };
                                    children.push((entry.object_hash.clone(), target_type));
                                }
                            }
                            Err(e) => {
                                obj_problems.push(Problem {
                                    severity: ProblemSeverity::Error,
                                    code: "CORRUPT_TREE_OBJECT",
                                    message: format!("Tree {} JSON deserialization failed: {}", hash, e),
                                    target: hash.clone(),
                                });
                            }
                        }
                    }
                    ObjectType::Commit => {
                        commits_checked.fetch_add(1, Ordering::Relaxed);
                        match serde_json::from_slice::<serde_json::Value>(bytes) {
                            Ok(val) => {
                                // Extract tree
                                if let Some(tree_hash) = val.get("tree").and_then(|t| t.as_str()) {
                                    if options.strict && !is_valid_hex_hash(tree_hash) {
                                        obj_problems.push(Problem {
                                            severity: ProblemSeverity::Error,
                                            code: "INVALID_HASH_SYNTAX",
                                            message: format!("Commit {} has invalid tree hash: {}", hash, tree_hash),
                                            target: tree_hash.to_string(),
                                        });
                                    }
                                    children.push((tree_hash.to_string(), ObjectType::Tree));
                                } else {
                                    obj_problems.push(Problem {
                                        severity: ProblemSeverity::Error,
                                        code: "COMMIT_MISSING_TREE",
                                        message: format!("Commit {} missing required 'tree' field", hash),
                                        target: hash.clone(),
                                    });
                                }

                                // Extract parents
                                if let Some(parents_array) = val.get("parents").and_then(|p| p.as_array()) {
                                    for parent in parents_array {
                                        if let Some(p_hash) = parent.as_str() {
                                            if options.strict && !is_valid_hex_hash(p_hash) {
                                                obj_problems.push(Problem {
                                                    severity: ProblemSeverity::Error,
                                                    code: "INVALID_HASH_SYNTAX",
                                                    message: format!("Commit {} parent has invalid hash syntax: {}", hash, p_hash),
                                                    target: p_hash.to_string(),
                                                });
                                            }
                                            children.push((p_hash.to_string(), ObjectType::Commit));
                                        }
                                    }
                                } else if let Some(parent_str) = val.get("parent").and_then(|p| p.as_str()) {
                                    // Legacy single parent
                                    children.push((parent_str.to_string(), ObjectType::Commit));
                                }

                                if options.strict {
                                    if val.get("message").and_then(|m| m.as_str()).is_none() {
                                        obj_problems.push(Problem {
                                            severity: ProblemSeverity::Warning,
                                            code: "COMMIT_MISSING_MESSAGE",
                                            message: format!("Commit {} has no message field", hash),
                                            target: hash.clone(),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                obj_problems.push(Problem {
                                    severity: ProblemSeverity::Error,
                                    code: "CORRUPT_COMMIT_OBJECT",
                                    message: format!("Commit {} JSON deserialization failed: {}", hash, e),
                                    target: hash.clone(),
                                });
                            }
                        }
                    }
                }
            }

            ParsedObject {
                hash: hash.clone(),
                obj_type: *obj_type,
                children,
                problems: obj_problems,
            }
        })
        .collect();

    // Aggregate verification problems and build graph structures
    let mut adj: HashMap<String, Vec<(String, ObjectType)>> = HashMap::new();
    let mut incoming_count: HashMap<String, usize> = HashMap::new();

    for p in &parsed_results {
        problems.extend(p.problems.clone());
        adj.insert(p.hash.clone(), p.children.clone());

        for (child_hash, expected_type) in &p.children {
            *incoming_count.entry(child_hash.clone()).or_insert(0) += 1;

            // Check if child exists on disk and matches expected type
            match disk_map.get(child_hash) {
                Some((actual_type, _)) => {
                    if *actual_type != *expected_type {
                        problems.push(Problem {
                            severity: ProblemSeverity::Error,
                            code: "OBJECT_TYPE_MISMATCH",
                            message: format!(
                                "Object {} referenced as {} by {}, but exists on disk as {}",
                                child_hash, expected_type, p.hash, actual_type
                            ),
                            target: child_hash.clone(),
                        });
                    }
                }
                None => {
                    problems.push(Problem {
                        severity: ProblemSeverity::Error,
                        code: "MISSING_OBJECT",
                        message: format!(
                            "Missing {} {} (referenced by {} {})",
                            expected_type, child_hash, p.obj_type, p.hash
                        ),
                        target: child_hash.clone(),
                    });
                }
            }
        }
    }

    // Check root references existence
    for (root_hash, root_desc, expected_type) in &root_refs {
        *incoming_count.entry(root_hash.clone()).or_insert(0) += 1;
        match disk_map.get(root_hash) {
            Some((actual_type, _)) => {
                if *actual_type != *expected_type {
                    problems.push(Problem {
                        severity: ProblemSeverity::Error,
                        code: "REF_OBJECT_TYPE_MISMATCH",
                        message: format!(
                            "Ref '{}' points to object {} which is a {}, expected {}",
                            root_desc, root_hash, actual_type, expected_type
                        ),
                        target: root_hash.clone(),
                    });
                }
            }
            None => {
                problems.push(Problem {
                    severity: ProblemSeverity::Error,
                    code: "BROKEN_REF",
                    message: format!(
                        "Ref '{}' points to non-existent object {}",
                        root_desc, root_hash
                    ),
                    target: root_hash.clone(),
                });
            }
        }
    }

    // 4. Reachability traversal (BFS from all valid root refs)
    let mut reachable: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = Vec::new();

    for (root_hash, _, _) in &root_refs {
        if disk_map.contains_key(root_hash) && reachable.insert(root_hash.clone()) {
            queue.push(root_hash.clone());
        }
    }

    while let Some(current) = queue.pop() {
        if let Some(children) = adj.get(&current) {
            for (child_hash, _) in children {
                if disk_map.contains_key(child_hash) && reachable.insert(child_hash.clone()) {
                    queue.push(child_hash.clone());
                }
            }
        }
    }

    // 5. Unreachable and Dangling Classification
    // An unreachable object is any disk object not in `reachable`.
    // A dangling object is an unreachable object that has NO incoming references from ANY other object or ref.
    let mut unreachable_objs = Vec::new();
    let mut dangling_objs = Vec::new();

    for (hash, obj_type, _) in &disk_objects {
        if !reachable.contains(hash) {
            unreachable_objs.push(DanglingObject {
                object_type: *obj_type,
                hash: hash.clone(),
            });

            let in_refs = incoming_count.get(hash).copied().unwrap_or(0);
            if in_refs == 0 {
                dangling_objs.push(DanglingObject {
                    object_type: *obj_type,
                    hash: hash.clone(),
                });
            }
        }
    }

    // Sort outputs deterministically by hash
    unreachable_objs.sort_by(|a, b| a.hash.cmp(&b.hash));
    dangling_objs.sort_by(|a, b| a.hash.cmp(&b.hash));

    // 6. Handle --lost-found
    let mut lost_found_written = Vec::new();
    if options.lost_found && !dangling_objs.is_empty() {
        let lost_found_dir = repo_path.join("lost-found");
        let commits_dir = lost_found_dir.join("commits");
        let other_dir = lost_found_dir.join("other");

        fs::create_dir_all(&commits_dir)?;
        fs::create_dir_all(&other_dir)?;

        for obj in &dangling_objs {
            if let Some((_, src_path)) = disk_map.get(&obj.hash) {
                let dest_dir = if obj.object_type == ObjectType::Commit {
                    &commits_dir
                } else {
                    &other_dir
                };
                let dest_path = dest_dir.join(&obj.hash);
                if !dest_path.exists() {
                    fs::copy(src_path, &dest_path)?;
                    lost_found_written.push(format!("{}: {}", obj.object_type, dest_path.display()));
                }
            }
        }
    }

    let errors_count = problems.iter().filter(|p| p.severity == ProblemSeverity::Error).count();
    let warnings_count = problems.iter().filter(|p| p.severity == ProblemSeverity::Warning).count();

    let stats = FsckStats {
        total_objects_scanned: total_scanned,
        commits_checked: commits_checked.load(Ordering::Relaxed),
        trees_checked: trees_checked.load(Ordering::Relaxed),
        blobs_checked: blobs_checked.load(Ordering::Relaxed),
        refs_checked: root_refs.len(),
        unreachable_objects_count: unreachable_objs.len(),
        dangling_objects_count: dangling_objs.len(),
        errors_count,
        warnings_count,
    };

    let is_clean = errors_count == 0 && (!options.strict || warnings_count == 0);

    Ok(FsckReport {
        is_clean,
        stats,
        problems,
        unreachable: unreachable_objs,
        dangling: dangling_objs,
        lost_found_written,
    })
}

fn is_valid_hex_hash(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Discover all root reference targets:
/// (Target Hash, Description/RefName, Expected Target Object Type)
fn discover_roots(repo_path: &Path, problems: &mut Vec<Problem>) -> Vec<(String, String, ObjectType)> {
    let mut roots = Vec::new();

    // 1. HEAD
    let head_path = repo_path.join("HEAD");
    if head_path.exists() {
        match fs::read_to_string(&head_path) {
            Ok(content) => {
                let trimmed = content.trim();
                if let Some(target_ref) = trimmed.strip_prefix("ref: ") {
                    let ref_path = repo_path.join(target_ref.trim());
                    if ref_path.exists() {
                        match fs::read_to_string(&ref_path) {
                            Ok(hash) => {
                                let h = hash.trim();
                                if !h.is_empty() {
                                    roots.push((h.to_string(), format!("HEAD -> {}", target_ref.trim()), ObjectType::Commit));
                                }
                            }
                            Err(e) => problems.push(Problem {
                                severity: ProblemSeverity::Error,
                                code: "REF_READ_ERROR",
                                message: format!("Failed to read ref {}: {}", ref_path.display(), e),
                                target: target_ref.trim().to_string(),
                            }),
                        }
                    }
                } else if !trimmed.is_empty() {
                    // Detached HEAD
                    roots.push((trimmed.to_string(), "HEAD (detached)".to_string(), ObjectType::Commit));
                }
            }
            Err(e) => problems.push(Problem {
                severity: ProblemSeverity::Error,
                code: "HEAD_READ_ERROR",
                message: format!("Failed to read HEAD: {}", e),
                target: "HEAD".to_string(),
            }),
        }
    } else {
        problems.push(Problem {
            severity: ProblemSeverity::Warning,
            code: "HEAD_MISSING",
            message: "HEAD file does not exist".to_string(),
            target: "HEAD".to_string(),
        });
    }

    // 2. All branches under refs/heads/
    let heads_dir = repo_path.join("refs").join("heads");
    if heads_dir.exists() {
        collect_ref_files(&heads_dir, "refs/heads", &mut roots, problems);
    }

    // 3. All remotes under refs/remotes/
    let remotes_dir = repo_path.join("refs").join("remotes");
    if remotes_dir.exists() {
        collect_ref_files(&remotes_dir, "refs/remotes", &mut roots, problems);
    }

    // 4. MERGE_HEAD if present
    let merge_head_path = repo_path.join("MERGE_HEAD");
    if merge_head_path.exists() {
        if let Ok(content) = fs::read_to_string(&merge_head_path) {
            let h = content.trim();
            if !h.is_empty() {
                roots.push((h.to_string(), "MERGE_HEAD".to_string(), ObjectType::Commit));
            }
        }
    }

    // 5. Index entries (blobs staged in index are roots of reachability)
    if let Ok(index) = Index::load(repo_path) {
        for (path, entry) in index.entries {
            roots.push((
                entry.blob_hash.clone(),
                format!("index:{}", path),
                ObjectType::Blob,
            ));
        }
    }

    roots
}

fn collect_ref_files(
    dir: &Path,
    prefix: &str,
    roots: &mut Vec<(String, String, ObjectType)>,
    problems: &mut Vec<Problem>,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ref_name = format!("{}/{}", prefix, file_name);

            if path.is_dir() {
                collect_ref_files(&path, &ref_name, roots, problems);
            } else if path.is_file() {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let hash = content.trim();
                        if !hash.is_empty() {
                            roots.push((hash.to_string(), ref_name, ObjectType::Commit));
                        }
                    }
                    Err(e) => {
                        problems.push(Problem {
                            severity: ProblemSeverity::Error,
                            code: "REF_READ_ERROR",
                            message: format!("Failed to read ref file {}: {}", path.display(), e),
                            target: ref_name,
                        });
                    }
                }
            }
        }
    }
}

/// Discovers all objects in .rusty/objects/{blobs,trees,commits}
fn scan_disk_objects(
    repo_path: &Path,
    problems: &mut Vec<Problem>,
) -> Vec<(String, ObjectType, PathBuf)> {
    let mut objects = Vec::new();
    let objects_dir = repo_path.join("objects");

    let subdirs = [
        ("blobs", ObjectType::Blob),
        ("trees", ObjectType::Tree),
        ("commits", ObjectType::Commit),
    ];

    for (subdir, obj_type) in subdirs {
        let dir = objects_dir.join(subdir);
        if !dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(name) => name.to_string(),
                        None => continue,
                    };

                    // Check for invalid filenames or stray files in objects dir
                    if !is_valid_hex_hash(&file_name) {
                        problems.push(Problem {
                            severity: ProblemSeverity::Warning,
                            code: "UNEXPECTED_OBJECT_FILENAME",
                            message: format!(
                                "Unexpected file in objects/{}: {}",
                                subdir, file_name
                            ),
                            target: file_name.clone(),
                        });
                    }

                    objects.push((file_name, obj_type, path));
                }
            }
        }
    }

    objects
}

pub fn print_fsck_report(report: &FsckReport, options: &FsckOptions) {
    if options.json {
        if let Ok(json_str) = serde_json::to_string_pretty(report) {
            println!("{}", json_str);
        }
        return;
    }

    // Print problems
    for problem in &report.problems {
        match problem.severity {
            ProblemSeverity::Error => {
                eprintln!("error [{}]: {} ({})", problem.code, problem.message, problem.target);
            }
            ProblemSeverity::Warning => {
                eprintln!("warning [{}]: {} ({})", problem.code, problem.message, problem.target);
            }
        }
    }

    // Print unreachable objects if requested
    if options.unreachable {
        for obj in &report.unreachable {
            println!("unreachable {} {}", obj.object_type, obj.hash);
        }
    }

    // Print dangling objects if requested
    if options.dangling {
        for obj in &report.dangling {
            println!("dangling {} {}", obj.object_type, obj.hash);
        }
    }

    // Lost and found notice
    for item in &report.lost_found_written {
        println!("wrote lost-found object {}", item);
    }

    // Summary banner
    println!("--------------------------------------------------");
    println!(
        "Checked {} objects ({} commits, {} trees, {} blobs) across {} refs.",
        report.stats.total_objects_scanned,
        report.stats.commits_checked,
        report.stats.trees_checked,
        report.stats.blobs_checked,
        report.stats.refs_checked
    );

    if report.is_clean {
        println!("Repository integrity verified: clean (0 errors, {} warnings).", report.stats.warnings_count);
    } else {
        println!(
            "Repository integrity check FAILED: {} error(s), {} warning(s).",
            report.stats.errors_count, report.stats.warnings_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().join(".rusty");

        fs::create_dir_all(repo_path.join("objects").join("blobs")).unwrap();
        fs::create_dir_all(repo_path.join("objects").join("trees")).unwrap();
        fs::create_dir_all(repo_path.join("objects").join("commits")).unwrap();
        fs::create_dir_all(repo_path.join("refs").join("heads")).unwrap();
        fs::create_dir_all(repo_path.join("refs").join("remotes").join("origin")).unwrap();

        fs::write(repo_path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(repo_path.join("index"), "").unwrap();

        (temp, repo_path)
    }

    fn write_blob(repo_path: &Path, content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash = hex::encode(hasher.finalize());
        let path = repo_path.join("objects").join("blobs").join(&hash);
        fs::write(path, content).unwrap();
        hash
    }

    fn write_tree(repo_path: &Path, entries: &[(&str, &str, &str)]) -> String {
        let tree_entries: Vec<crate::tree::TreeEntry> = entries
            .iter()
            .map(|(name, o_type, hash)| crate::tree::TreeEntry {
                name: name.to_string(),
                object_type: o_type.to_string(),
                object_hash: hash.to_string(),
            })
            .collect();
        let tree = crate::tree::Tree {
            entries: tree_entries,
        };
        let data = serde_json::to_vec(&tree).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hex::encode(hasher.finalize());
        let path = repo_path.join("objects").join("trees").join(&hash);
        fs::write(path, data).unwrap();
        hash
    }

    fn write_commit(repo_path: &Path, tree_hash: &str, parents: &[&str], msg: &str) -> String {
        let commit = serde_json::json!({
            "tree": tree_hash,
            "parents": parents,
            "message": msg
        });
        let data = serde_json::to_vec(&commit).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hex::encode(hasher.finalize());
        let path = repo_path.join("objects").join("commits").join(&hash);
        fs::write(path, data).unwrap();
        hash
    }

    #[test]
    fn test_clean_repository_fsck() {
        let (_temp, repo_path) = setup_test_repo();

        let blob_hash = write_blob(&repo_path, b"hello rusty fsck\n");
        let tree_hash = write_tree(&repo_path, &[("hello.txt", "blob", &blob_hash)]);
        let commit_hash = write_commit(&repo_path, &tree_hash, &[], "initial commit");

        // Point refs/heads/main to commit_hash
        fs::write(repo_path.join("refs").join("heads").join("main"), &commit_hash).unwrap();

        let opts = FsckOptions::default();
        let report = run_fsck(&repo_path, &opts).unwrap();

        assert!(report.is_clean);
        assert_eq!(report.stats.errors_count, 0);
        assert_eq!(report.stats.dangling_objects_count, 0);
        assert_eq!(report.stats.commits_checked, 1);
        assert_eq!(report.stats.trees_checked, 1);
        assert_eq!(report.stats.blobs_checked, 1);
    }

    #[test]
    fn test_corrupt_blob_hash_mismatch() {
        let (_temp, repo_path) = setup_test_repo();

        let blob_hash = write_blob(&repo_path, b"genuine content");
        let tree_hash = write_tree(&repo_path, &[("file.txt", "blob", &blob_hash)]);
        let commit_hash = write_commit(&repo_path, &tree_hash, &[], "c1");
        fs::write(repo_path.join("refs").join("heads").join("main"), &commit_hash).unwrap();

        // Intentionally tamper with the blob file
        let blob_path = repo_path.join("objects").join("blobs").join(&blob_hash);
        fs::write(&blob_path, b"corrupted bytes").unwrap();

        let opts = FsckOptions::default();
        let report = run_fsck(&repo_path, &opts).unwrap();

        assert!(!report.is_clean);
        assert!(report.problems.iter().any(|p| p.code == "CORRUPT_OBJECT_HASH_MISMATCH"));
    }

    #[test]
    fn test_missing_referenced_object() {
        let (_temp, repo_path) = setup_test_repo();

        let fake_blob = "1111111111111111111111111111111111111111111111111111111111111111";
        let tree_hash = write_tree(&repo_path, &[("ghost.txt", "blob", fake_blob)]);
        let commit_hash = write_commit(&repo_path, &tree_hash, &[], "missing blob commit");
        fs::write(repo_path.join("refs").join("heads").join("main"), &commit_hash).unwrap();

        let opts = FsckOptions::default();
        let report = run_fsck(&repo_path, &opts).unwrap();

        assert!(!report.is_clean);
        assert!(report.problems.iter().any(|p| p.code == "MISSING_OBJECT" && p.target == fake_blob));
    }

    #[test]
    fn test_broken_ref() {
        let (_temp, repo_path) = setup_test_repo();

        let ghost_commit = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        fs::write(repo_path.join("refs").join("heads").join("main"), ghost_commit).unwrap();

        let opts = FsckOptions::default();
        let report = run_fsck(&repo_path, &opts).unwrap();

        assert!(!report.is_clean);
        assert!(report.problems.iter().any(|p| p.code == "BROKEN_REF" && p.target == ghost_commit));
    }

    #[test]
    fn test_dangling_and_unreachable_detection() {
        let (_temp, repo_path) = setup_test_repo();

        // 1. Root commit on main
        let b1 = write_blob(&repo_path, b"tracked");
        let t1 = write_tree(&repo_path, &[("tracked.txt", "blob", &b1)]);
        let c1 = write_commit(&repo_path, &t1, &[], "tracked commit");
        fs::write(repo_path.join("refs").join("heads").join("main"), &c1).unwrap();

        // 2. Unreachable commit with its own tree and blob
        let b2 = write_blob(&repo_path, b"abandoned branch blob");
        let t2 = write_tree(&repo_path, &[("orphan.txt", "blob", &b2)]);
        let c2 = write_commit(&repo_path, &t2, &[], "abandoned commit");

        let mut opts = FsckOptions::default();
        opts.unreachable = true;
        opts.dangling = true;
        let report = run_fsck(&repo_path, &opts).unwrap();

        assert!(report.is_clean);
        // c2, t2, b2 should be unreachable (3 objects)
        assert_eq!(report.stats.unreachable_objects_count, 3);
        // But only c2 is dangling (top-level orphan without incoming edges)
        assert_eq!(report.stats.dangling_objects_count, 1);
        assert_eq!(report.dangling[0].hash, c2);
        assert_eq!(report.dangling[0].object_type, ObjectType::Commit);
    }

    #[test]
    fn test_lost_found_recovery() {
        let (_temp, repo_path) = setup_test_repo();

        let b_orphan = write_blob(&repo_path, b"standalone dangling blob");
        let opts = FsckOptions {
            dangling: true,
            lost_found: true,
            ..Default::default()
        };

        let report = run_fsck(&repo_path, &opts).unwrap();
        assert_eq!(report.stats.dangling_objects_count, 1);
        assert_eq!(report.lost_found_written.len(), 1);

        let recovered_path = repo_path.join("lost-found").join("other").join(&b_orphan);
        assert!(recovered_path.exists());
        assert_eq!(fs::read(&recovered_path).unwrap(), b"standalone dangling blob");
    }

    #[test]
    fn test_strict_mode_validation() {
        let (_temp, repo_path) = setup_test_repo();

        let b1 = write_blob(&repo_path, b"data");
        // Tree with invalid path traversing entry name
        let bad_tree = crate::tree::Tree {
            entries: vec![crate::tree::TreeEntry {
                name: "../hack.txt".to_string(),
                object_type: "blob".to_string(),
                object_hash: b1.clone(),
            }],
        };
        let data = serde_json::to_vec(&bad_tree).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let t_hash = hex::encode(hasher.finalize());
        fs::write(repo_path.join("objects").join("trees").join(&t_hash), data).unwrap();

        let c1 = write_commit(&repo_path, &t_hash, &[], "strict commit");
        fs::write(repo_path.join("refs").join("heads").join("main"), &c1).unwrap();

        let opts = FsckOptions {
            strict: true,
            ..Default::default()
        };
        let report = run_fsck(&repo_path, &opts).unwrap();

        assert!(!report.is_clean);
        assert!(report.problems.iter().any(|p| p.code == "TREE_INVALID_ENTRY_NAME"));
    }
}
