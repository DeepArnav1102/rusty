use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

use crate::commit::{Commit, get_commit};
use crate::index::Index;
use crate::objects;
use crate::repository::get_head_commit;
use crate::tree::load_tree_files;

// ============================================================================
// MERGE STATE TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MergeConflictKind {
    BothModified,
    ModifyDelete,
    DeleteModify,
    AddAdd,
}

impl std::fmt::Display for MergeConflictKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeConflictKind::BothModified => write!(f, "both_modified"),
            MergeConflictKind::ModifyDelete => write!(f, "modify_delete"),
            MergeConflictKind::DeleteModify => write!(f, "delete_modify"),
            MergeConflictKind::AddAdd => write!(f, "add_add"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConflict {
    pub path: String,
    pub kind: MergeConflictKind,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergeState {
    pub merge_head: String,
    pub conflicts: Vec<MergeConflict>,
}

impl MergeState {
    pub fn load(repo_path: &Path) -> Result<Option<Self>> {
        let state_path = repo_path.join("MERGE_STATE");

        if !state_path.exists() {
            return Ok(None);
        }

        let data = fs::read_to_string(&state_path)?;
        let state: MergeState =
            serde_json::from_str(&data).context("Invalid MERGE_STATE file")?;

        Ok(Some(state))
    }

    pub fn save(&self, repo_path: &Path) -> Result<()> {
        let state_path = repo_path.join("MERGE_STATE");
        let data = serde_json::to_string_pretty(self)?;

        fs::write(&state_path, data)?;

        Ok(())
    }

    pub fn remove(repo_path: &Path) -> Result<()> {
        let state_path = repo_path.join("MERGE_STATE");

        if state_path.exists() {
            fs::remove_file(&state_path)?;
        }

        Ok(())
    }

    pub fn has_unresolved_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    pub fn resolve_path(&mut self, path: &str) {
        self.conflicts.retain(|c| c.path != path);
    }
}

// ============================================================================
// MERGE PLAN
// ============================================================================

struct MergePlan {
    files: BTreeMap<String, String>,
    conflicts: Vec<MergeConflict>,
}

// ============================================================================
// PUBLIC API: is_merge_in_progress
// ============================================================================

pub fn is_merge_in_progress(repo_path: &Path) -> bool {
    repo_path.join("MERGE_HEAD").exists()
}

pub fn read_merge_head(repo_path: &Path) -> Result<Option<String>> {
    let merge_head_path = repo_path.join("MERGE_HEAD");

    if !merge_head_path.exists() {
        return Ok(None);
    }

    let hash = fs::read_to_string(&merge_head_path)?
        .trim()
        .to_string();

    if hash.is_empty() {
        Ok(None)
    } else {
        Ok(Some(hash))
    }
}

// ============================================================================
// PUBLIC API: merge
// ============================================================================

pub fn merge(repo_path: &Path, target: &str) -> Result<()> {
    // ------------------------------------------------------------
    // 0. Block if another merge is in progress
    // ------------------------------------------------------------

    if is_merge_in_progress(repo_path) {
        anyhow::bail!(
            "Merge in progress. Commit the merge or abort it first \
             (rusty merge --abort)."
        );
    }

    // ------------------------------------------------------------
    // 1. Make sure HEAD is attached
    // ------------------------------------------------------------

    let head_path = repo_path.join("HEAD");
    let head_content = fs::read_to_string(&head_path)?;
    let head_content = head_content.trim();

    let branch_ref = head_content
        .strip_prefix("ref: ")
        .context("Cannot merge while HEAD is detached")?
        .trim();

    let current_branch_path = repo_path.join(branch_ref);

    if !current_branch_path.exists() {
        anyhow::bail!("Current branch has no commits");
    }

    let current_hash = fs::read_to_string(&current_branch_path)?
        .trim()
        .to_string();

    if current_hash.is_empty() {
        anyhow::bail!("Current branch has no commits");
    }

    // ------------------------------------------------------------
    // 2. Resolve merge target
    // ------------------------------------------------------------

    let target_hash = resolve_target(repo_path, target)?;

    // ------------------------------------------------------------
    // 3. Same commit
    // ------------------------------------------------------------

    if target_hash == current_hash {
        println!("Already up to date.");
        return Ok(());
    }

    println!(
        "Merging {} into {}...",
        target,
        branch_name_from_ref(branch_ref)
    );

    // ------------------------------------------------------------
    // 4. Working tree must be clean
    // ------------------------------------------------------------

    ensure_clean_working_tree(repo_path, &current_hash)?;

    // ------------------------------------------------------------
    // 5. Load commits
    // ------------------------------------------------------------

    let current_commit = get_commit(repo_path, &current_hash)?;
    let target_commit = get_commit(repo_path, &target_hash)?;

    // ------------------------------------------------------------
    // 6. Fast-forward checks
    // ------------------------------------------------------------

    if is_ancestor(repo_path, &target_hash, &current_hash)? {
        println!("Already up to date.");
        return Ok(());
    }

    if is_ancestor(repo_path, &current_hash, &target_hash)? {
        println!("Fast-forwarding...");

        let target_files = load_tree_files(repo_path, &target_commit.tree)?;

        check_untracked_overwrites(repo_path, &target_files)?;

        apply_tree(repo_path, &target_files)?;

        let mut index = Index::new();

        for (path, blob_hash) in &target_files {
            index.add(path.clone(), blob_hash.clone());
        }

        index.save(repo_path)?;

        fs::write(&current_branch_path, &target_hash)?;

        println!("Fast-forwarded to {}", short_hash(&target_hash));

        return Ok(());
    }

    // ------------------------------------------------------------
    // 7. Find common ancestor
    // ------------------------------------------------------------

    let base_hash = find_common_ancestor(repo_path, &current_hash, &target_hash)?
        .context("No common ancestor found")?;

    let base_commit = get_commit(repo_path, &base_hash)?;

    // ------------------------------------------------------------
    // 8. Load all three trees
    // ------------------------------------------------------------

    let base_files = load_tree_files(repo_path, &base_commit.tree)?;
    let ours_files = load_tree_files(repo_path, &current_commit.tree)?;
    let theirs_files = load_tree_files(repo_path, &target_commit.tree)?;

    // ------------------------------------------------------------
    // 9. Build merge plan
    // ------------------------------------------------------------

    let mut plan =
        build_merge_plan(repo_path, &base_files, &ours_files, &theirs_files)?;

    // ------------------------------------------------------------
    // 10. Prevent untracked files from being overwritten
    // ------------------------------------------------------------

    let mut all_paths_to_write: BTreeMap<String, String> = plan.files.clone();

    for conflict in &plan.conflicts {
        if let Some(ours_hash) = ours_files.get(&conflict.path) {
            all_paths_to_write.insert(
                conflict.path.clone(),
                ours_hash.clone(),
            );
        }
    }

    check_untracked_overwrites(repo_path, &all_paths_to_write)?;

    // ------------------------------------------------------------
    // 11. Write MERGE_HEAD
    // ------------------------------------------------------------

    let merge_head_path = repo_path.join("MERGE_HEAD");
    fs::write(&merge_head_path, &target_hash)?;

    // ------------------------------------------------------------
    // 12. Apply clean results
    // ------------------------------------------------------------

    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let conflict_paths: HashSet<String> = plan
        .conflicts
        .iter()
        .map(|c| c.path.clone())
        .collect();

    // Remove files deleted by merge.
    for path in ours_files.keys() {
        if !plan.files.contains_key(path)
            && !conflict_paths.contains(path)
        {
            let full_path = repo_root.join(path);

            if full_path.exists() {
                fs::remove_file(&full_path)?;
            }
        }
    }

    // Write clean merged files.
    for (path, blob_hash) in &plan.files {
        let full_path = repo_root.join(path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let blob_path = repo_path
            .join("objects")
            .join("blobs")
            .join(blob_hash);

        if !blob_path.exists() {
            anyhow::bail!("Blob object not found: {}", blob_hash);
        }

        let data = fs::read(&blob_path)?;
        fs::write(&full_path, data)?;
    }

    // ------------------------------------------------------------
    // 13. Write conflict files
    // ------------------------------------------------------------

    for conflict in &plan.conflicts {
        let path = &conflict.path;
        let full_path = repo_root.join(path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        match conflict.kind {
            MergeConflictKind::ModifyDelete => {
                if let Some(ours_hash) = ours_files.get(path) {
                    let blob_path = repo_path
                        .join("objects")
                        .join("blobs")
                        .join(ours_hash);

                    if blob_path.exists() {
                        let data = fs::read(&blob_path)?;
                        fs::write(&full_path, data)?;
                    }
                }

                println!("CONFLICT (modify/delete): {}", path);
            }

            MergeConflictKind::DeleteModify => {
                if let Some(theirs_hash) = theirs_files.get(path) {
                    let blob_path = repo_path
                        .join("objects")
                        .join("blobs")
                        .join(theirs_hash);

                    if blob_path.exists() {
                        let data = fs::read(&blob_path)?;
                        fs::write(&full_path, data)?;
                    }
                }

                println!("CONFLICT (delete/modify): {}", path);
            }

            MergeConflictKind::BothModified
            | MergeConflictKind::AddAdd => {
                let ours_hash = ours_files.get(path);
                let theirs_hash = theirs_files.get(path);

                let ours_bytes = if let Some(h) = ours_hash {
                    let bp = repo_path
                        .join("objects")
                        .join("blobs")
                        .join(h);

                    if bp.exists() {
                        fs::read(&bp)?
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                let theirs_bytes = if let Some(h) = theirs_hash {
                    let bp = repo_path
                        .join("objects")
                        .join("blobs")
                        .join(h);

                    if bp.exists() {
                        fs::read(&bp)?
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                if is_binary(&ours_bytes) || is_binary(&theirs_bytes) {
                    fs::write(&full_path, &ours_bytes)?;

                    println!("CONFLICT (binary): {}", path);
                } else {
                    let ours_str =
                        String::from_utf8_lossy(&ours_bytes);

                    let theirs_str =
                        String::from_utf8_lossy(&theirs_bytes);

                    let conflict_content = format!(
                        "<<<<<<< ours\n{}\n=======\n{}\n>>>>>>> theirs\n",
                        ours_str,
                        theirs_str
                    );

                    fs::write(
                        &full_path,
                        conflict_content.as_bytes(),
                    )?;

                    println!("CONFLICT (content): {}", path);
                }
            }
        }
    }

    // ------------------------------------------------------------
    // 14. Rebuild index
    // ------------------------------------------------------------

    let mut index = Index::new();

    for (path, blob_hash) in &plan.files {
        index.add(path.clone(), blob_hash.clone());
    }

    for conflict in &plan.conflicts {
        if let Some(ours_hash) = ours_files.get(&conflict.path) {
            index.add(
                conflict.path.clone(),
                ours_hash.clone(),
            );
        }
    }

    index.save(repo_path)?;

    // ------------------------------------------------------------
    // 15. Write MERGE_STATE
    // ------------------------------------------------------------

    let state = MergeState {
        merge_head: target_hash.clone(),
        conflicts: plan.conflicts.clone(),
    };

    state.save(repo_path)?;

    // ------------------------------------------------------------
    // 16. Report
    // ------------------------------------------------------------

    println!();

    if plan.conflicts.is_empty() {
        println!("Merge successful (no conflicts).");
        println!("Run 'rusty commit' to create the merge commit.");
    } else {
        plan.conflicts.sort_by(|a, b| a.path.cmp(&b.path));

        println!(
            "Automatic merge failed; fix conflicts and then commit."
        );

        println!(
            "{} conflict(s) detected. Resolve them, then run \
             'rusty add' followed by 'rusty commit'.",
            plan.conflicts.len()
        );
    }

    Ok(())
}

// ============================================================================
// PUBLIC API: merge_abort
// ============================================================================

pub fn merge_abort(repo_path: &Path) -> Result<()> {
    if !is_merge_in_progress(repo_path) {
        println!("No merge in progress.");
        return Ok(());
    }

    let current_hash = get_head_commit(repo_path)?
        .context("Cannot abort: HEAD has no commits")?;

    let current_commit = get_commit(repo_path, &current_hash)?;

    let head_files =
        load_tree_files(repo_path, &current_commit.tree)?;

    restore_working_tree(repo_path, &head_files)?;

    let mut index = Index::new();

    for (path, blob_hash) in &head_files {
        index.add(path.clone(), blob_hash.clone());
    }

    index.save(repo_path)?;

    let merge_head_path = repo_path.join("MERGE_HEAD");

    if merge_head_path.exists() {
        fs::remove_file(&merge_head_path)?;
    }

    MergeState::remove(repo_path)?;

    println!("Merge aborted. Working tree restored to HEAD.");

    Ok(())
}

// ============================================================================
// BUILD MERGE PLAN
// ============================================================================

fn build_merge_plan(
    repo_path: &Path,
    base: &BTreeMap<String, String>,
    ours: &BTreeMap<String, String>,
    theirs: &BTreeMap<String, String>,
) -> Result<MergePlan> {
    let mut all_paths = HashSet::new();

    all_paths.extend(base.keys().cloned());
    all_paths.extend(ours.keys().cloned());
    all_paths.extend(theirs.keys().cloned());

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    let mut conflicts: Vec<MergeConflict> = Vec::new();

    for path in &all_paths {
        let base_hash = base.get(path);
        let ours_hash = ours.get(path);
        let theirs_hash = theirs.get(path);

        match (base_hash, ours_hash, theirs_hash) {
            (_, o, t) if o == t => {
                if let Some(h) = ours_hash {
                    files.insert(path.clone(), h.clone());
                }
            }

            (b, o, t) if o == b => {
                if let Some(h) = t {
                    files.insert(path.clone(), h.clone());
                }
            }

            (b, o, t) if t == b => {
                if let Some(h) = o {
                    files.insert(path.clone(), h.clone());
                }
            }

            (b, o, t) => {
                let kind =
                    classify_conflict(b, o, t, repo_path, path)?;

                conflicts.push(MergeConflict {
                    path: path.clone(),
                    kind,
                });
            }
        }
    }

    Ok(MergePlan { files, conflicts })
}

fn classify_conflict(
    base: Option<&String>,
    ours: Option<&String>,
    theirs: Option<&String>,
    _repo_path: &Path,
    _path: &str,
) -> Result<MergeConflictKind> {
    match (base.is_some(), ours.is_some(), theirs.is_some()) {
        (_, true, true) if base.is_some() => {
            Ok(MergeConflictKind::BothModified)
        }

        (false, true, true) => {
            Ok(MergeConflictKind::AddAdd)
        }

        (true, true, false) => {
            Ok(MergeConflictKind::ModifyDelete)
        }

        (true, false, true) => {
            Ok(MergeConflictKind::DeleteModify)
        }

        _ => Ok(MergeConflictKind::BothModified),
    }
}

// ============================================================================
// RESTORE WORKING TREE
// ============================================================================

fn restore_working_tree(
    repo_path: &Path,
    files: &BTreeMap<String, String>,
) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let current_index = Index::load(repo_path)?;

    for path in current_index.entries.keys() {
        if !files.contains_key(path) {
            let file_path = repo_root.join(path);

            if file_path.exists() {
                fs::remove_file(&file_path)?;
            }
        }
    }

    for (path, blob_hash) in files {
        let file_path = repo_root.join(path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let blob_path = repo_path
            .join("objects")
            .join("blobs")
            .join(blob_hash);

        if !blob_path.exists() {
            anyhow::bail!("Missing blob object {}", blob_hash);
        }

        let data = fs::read(&blob_path)?;
        fs::write(&file_path, data)?;
    }

    Ok(())
}

// ============================================================================
// TARGET RESOLUTION
// ============================================================================

fn resolve_target(repo_path: &Path, target: &str) -> Result<String> {
    // Direct commit hash.
    let commit_path = repo_path
        .join("objects")
        .join("commits")
        .join(target);

    if commit_path.exists() {
        return Ok(target.to_string());
    }

    // Remote-tracking branch: origin/main
    if let Some(rest) = target.strip_prefix("origin/") {
        let remote_ref = repo_path
            .join("refs")
            .join("remotes")
            .join("origin")
            .join(rest);

        if remote_ref.exists() {
            let hash = fs::read_to_string(remote_ref)?
                .trim()
                .to_string();

            if !hash.is_empty() {
                return Ok(hash);
            }
        }

        anyhow::bail!("Remote branch not found: {}", target);
    }

    // Local branch.
    let branch_ref = repo_path
        .join("refs")
        .join("heads")
        .join(target);

    if branch_ref.exists() {
        let hash = fs::read_to_string(&branch_ref)?
            .trim()
            .to_string();

        if !hash.is_empty() {
            return Ok(hash);
        }

        anyhow::bail!("Branch '{}' has no commits", target);
    }

    // Full ref.
    let full_ref = repo_path.join(target);

    if full_ref.exists() {
        let hash = fs::read_to_string(full_ref)?
            .trim()
            .to_string();

        if !hash.is_empty() {
            return Ok(hash);
        }
    }

    anyhow::bail!("Merge target '{}' not found", target);
}

// ============================================================================
// COMMON ANCESTOR
// ============================================================================

fn find_common_ancestor(
    repo_path: &Path,
    ours: &str,
    theirs: &str,
) -> Result<Option<String>> {
    let ours_ancestors = collect_ancestors(repo_path, ours)?;
    let theirs_ancestors = collect_ancestors(repo_path, theirs)?;

    for hash in ours_ancestors {
        if theirs_ancestors.contains(&hash) {
            return Ok(Some(hash));
        }
    }

    Ok(None)
}

fn collect_ancestors(
    repo_path: &Path,
    start: &str,
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back(start.to_string());

    while let Some(hash) = queue.pop_front() {
        if !visited.insert(hash.clone()) {
            continue;
        }

        result.push(hash.clone());

        let commit = get_commit(repo_path, &hash)?;

        for parent in commit.parents {
            queue.push_back(parent);
        }
    }

    Ok(result)
}

// ============================================================================
// ANCESTOR CHECK
// ============================================================================

fn is_ancestor(
    repo_path: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool> {
    if ancestor == descendant {
        return Ok(true);
    }

    let ancestors =
        collect_ancestors(repo_path, descendant)?;

    Ok(ancestors.iter().any(|hash| hash == ancestor))
}

// ============================================================================
// CLEAN WORKING TREE CHECK
// ============================================================================

fn ensure_clean_working_tree(
    repo_path: &Path,
    current_hash: &str,
) -> Result<()> {
    let index = Index::load(repo_path)?;

    let current_commit =
        get_commit(repo_path, current_hash)?;

    let head_files =
        load_tree_files(repo_path, &current_commit.tree)?;

    // Check staged changes.
    if index.entries.len() != head_files.len() {
        anyhow::bail!(
            "Cannot merge: you have staged changes. Commit them first."
        );
    }

    for (path, entry) in &index.entries {
        if head_files.get(path) != Some(&entry.blob_hash) {
            anyhow::bail!(
                "Cannot merge: you have staged changes. Commit them first."
            );
        }
    }

    // Check working tree changes.
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    for (path, expected_hash) in &head_files {
        let full_path = repo_root.join(path);

        if !full_path.exists() {
            anyhow::bail!(
                "Cannot merge: '{}' was deleted locally. \
                 Commit or restore it first.",
                path
            );
        }

        if !full_path.is_file() {
            anyhow::bail!(
                "Cannot merge: '{}' is not a regular file.",
                path
            );
        }

        let actual_hash =
            objects::hash_file(&full_path)?;

        if &actual_hash != expected_hash {
            anyhow::bail!(
                "Cannot merge: '{}' has uncommitted changes. \
                 Commit or restore it first.",
                path
            );
        }
    }

    Ok(())
}

// ============================================================================
// UNTRACKED OVERWRITE CHECK
// ============================================================================

fn check_untracked_overwrites(
    repo_path: &Path,
    target_files: &BTreeMap<String, String>,
) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let index = Index::load(repo_path)?;

    let head_files = match get_head_commit(repo_path)? {
        Some(current_hash) => {
            let commit =
                get_commit(repo_path, &current_hash)?;

            load_tree_files(repo_path, &commit.tree)?
        }

        None => BTreeMap::new(),
    };

    for path in target_files.keys() {
        if head_files.contains_key(path) {
            continue;
        }

        if index.contains(path) {
            continue;
        }

        let full_path = repo_root.join(path);

        if full_path.exists() {
            anyhow::bail!(
                "Cannot merge: untracked file '{}' would be overwritten.",
                path
            );
        }
    }

    Ok(())
}

// ============================================================================
// APPLY TREE
// ============================================================================

fn apply_tree(
    repo_path: &Path,
    target_files: &BTreeMap<String, String>,
) -> Result<()> {
    let repo_root = repo_path
        .parent()
        .context("Invalid repository path")?
        .canonicalize()?;

    let current_files = match get_head_commit(repo_path)? {
        Some(current_hash) => {
            let commit =
                get_commit(repo_path, &current_hash)?;

            load_tree_files(repo_path, &commit.tree)?
        }

        None => BTreeMap::new(),
    };

    // Remove files that no longer exist in target.
    for path in current_files.keys() {
        if !target_files.contains_key(path) {
            let full_path = repo_root.join(path);

            if full_path.exists() {
                fs::remove_file(&full_path)?;
            }
        }
    }

    // Write target files.
    for (path, blob_hash) in target_files {
        let full_path = repo_root.join(path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let blob_path = repo_path
            .join("objects")
            .join("blobs")
            .join(blob_hash);

        if !blob_path.exists() {
            anyhow::bail!(
                "Blob object not found: {}",
                blob_hash
            );
        }

        let data = fs::read(&blob_path)?;
        fs::write(&full_path, data)?;
    }

    Ok(())
}

// ============================================================================
// BINARY DETECTION
// ============================================================================

fn is_binary(data: &[u8]) -> bool {
    let sample = &data[..data.len().min(8000)];

    sample.contains(&0u8)
}

// ============================================================================
// HELPERS
// ============================================================================

fn branch_name_from_ref(branch_ref: &str) -> &str {
    branch_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(branch_ref)
}

fn short_hash(hash: &str) -> &str {
    &hash[..8.min(hash.len())]
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{Commit, create_commit};
    use crate::index::Index;
    use crate::objects::hash_objects;
    use sha2::{Digest, Sha256};
    use std::fs;
    use tempfile::TempDir;

    // ------------------------------------------------------------------
    // Helper: set up a fresh repo in a temp dir.
    // ------------------------------------------------------------------

    fn setup_repo() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();

        let repo_path = dir.path().join(".rusty");

        fs::create_dir(&repo_path).unwrap();

        fs::create_dir(repo_path.join("objects")).unwrap();

        fs::create_dir(
            repo_path.join("objects").join("blobs")
        )
        .unwrap();

        fs::create_dir(
            repo_path.join("objects").join("trees")
        )
        .unwrap();

        fs::create_dir(
            repo_path.join("objects").join("commits")
        )
        .unwrap();

        fs::create_dir(repo_path.join("refs")).unwrap();

        fs::create_dir(
            repo_path.join("refs").join("heads")
        )
        .unwrap();

        fs::create_dir(
            repo_path.join("refs").join("remotes")
        )
        .unwrap();

        fs::write(
            repo_path.join("HEAD"),
            "ref: refs/heads/main\n",
        )
        .unwrap();

        fs::write(repo_path.join("index"), "").unwrap();

        (dir, repo_path)
    }

    // ------------------------------------------------------------------
    // Helper: write a file + stage + commit.
    // ------------------------------------------------------------------

    fn write_and_commit(
        dir: &TempDir,
        repo_path: &std::path::PathBuf,
        files: &[(&str, &[u8])],
        msg: &str,
    ) -> String {
        for (rel, content) in files {
            let full = dir.path().join(rel);

            if let Some(p) = full.parent() {
                fs::create_dir_all(p).unwrap();
            }

            fs::write(&full, content).unwrap();

            hash_objects(&full, repo_path).unwrap();
        }

        let mut index =
            Index::load(repo_path).unwrap();

        for (rel, _) in files {
            let full = dir.path().join(rel);

            let hash =
                hash_objects(&full, repo_path).unwrap();

            index.add(
                rel.replace('\\', "/"),
                hash,
            );
        }

        index.save(repo_path).unwrap();

        create_commit(
            repo_path,
            msg.to_string(),
        )
        .unwrap()
    }

    // ------------------------------------------------------------------
    // Helper: write a raw commit object.
    // ------------------------------------------------------------------

    fn raw_commit(
        repo_path: &std::path::PathBuf,
        tree: &str,
        parents: Vec<String>,
        message: &str,
    ) -> String {
        let c = Commit {
            tree: tree.to_string(),
            parents,
            message: message.to_string(),
        };

        let data =
            serde_json::to_vec(&c).unwrap();

        let mut h = Sha256::new();

        h.update(&data);

        let hash =
            hex::encode(h.finalize());

        let path = repo_path
            .join("objects")
            .join("commits")
            .join(&hash);

        fs::write(&path, &data).unwrap();

        hash
    }

    // ------------------------------------------------------------------
    // Basic helper tests
    // ------------------------------------------------------------------

    #[test]
    fn test_merge_state_round_trip() {
        let (_dir, repo_path) = setup_repo();

        let state = MergeState {
            merge_head: "abc123".to_string(),
            conflicts: vec![MergeConflict {
                path: "file.txt".to_string(),
                kind: MergeConflictKind::BothModified,
            }],
        };

        state.save(&repo_path).unwrap();

        let loaded =
            MergeState::load(&repo_path)
                .unwrap()
                .unwrap();

        assert_eq!(loaded.merge_head, "abc123");
        assert_eq!(loaded.conflicts.len(), 1);
        assert_eq!(
            loaded.conflicts[0].path,
            "file.txt"
        );
    }

    #[test]
    fn test_merge_state_resolve_path() {
        let mut state = MergeState {
            merge_head: "abc123".to_string(),
            conflicts: vec![
                MergeConflict {
                    path: "a.txt".to_string(),
                    kind: MergeConflictKind::BothModified,
                },
                MergeConflict {
                    path: "b.txt".to_string(),
                    kind: MergeConflictKind::AddAdd,
                },
            ],
        };

        state.resolve_path("a.txt");

        assert_eq!(state.conflicts.len(), 1);
        assert_eq!(
            state.conflicts[0].path,
            "b.txt"
        );
    }

    #[test]
    fn test_merge_in_progress() {
        let (_dir, repo_path) = setup_repo();

        assert!(!is_merge_in_progress(&repo_path));

        fs::write(
            repo_path.join("MERGE_HEAD"),
            "abcdef",
        )
        .unwrap();

        assert!(is_merge_in_progress(&repo_path));
    }

    #[test]
    fn test_short_hash() {
        assert_eq!(
            short_hash("1234567890abcdef"),
            "12345678"
        );

        assert_eq!(
            short_hash("1234"),
            "1234"
        );
    }

    #[test]
    fn test_branch_name_from_ref() {
        assert_eq!(
            branch_name_from_ref("refs/heads/main"),
            "main"
        );

        assert_eq!(
            branch_name_from_ref("main"),
            "main"
        );
    }

    // ------------------------------------------------------------------
    // Conflict classification tests
    // ------------------------------------------------------------------

    #[test]
    fn test_classify_both_modified() {
        let base = Some("base".to_string());
        let ours = Some("ours".to_string());
        let theirs = Some("theirs".to_string());

        let result = classify_conflict(
            base.as_ref(),
            ours.as_ref(),
            theirs.as_ref(),
            Path::new(".rusty"),
            "file.txt",
        )
        .unwrap();

        assert_eq!(
            result,
            MergeConflictKind::BothModified
        );
    }

    #[test]
    fn test_classify_modify_delete() {
        let base = Some("base".to_string());
        let ours = Some("ours".to_string());
        let theirs: Option<String> = None;

        let result = classify_conflict(
            base.as_ref(),
            ours.as_ref(),
            theirs.as_ref(),
            Path::new(".rusty"),
            "file.txt",
        )
        .unwrap();

        assert_eq!(
            result,
            MergeConflictKind::ModifyDelete
        );
    }

    #[test]
    fn test_classify_delete_modify() {
        let base = Some("base".to_string());
        let ours: Option<String> = None;
        let theirs = Some("theirs".to_string());

        let result = classify_conflict(
            base.as_ref(),
            ours.as_ref(),
            theirs.as_ref(),
            Path::new(".rusty"),
            "file.txt",
        )
        .unwrap();

        assert_eq!(
            result,
            MergeConflictKind::DeleteModify
        );
    }

    #[test]
    fn test_classify_add_add() {
        let base: Option<String> = None;
        let ours = Some("ours".to_string());
        let theirs = Some("theirs".to_string());

        let result = classify_conflict(
            base.as_ref(),
            ours.as_ref(),
            theirs.as_ref(),
            Path::new(".rusty"),
            "file.txt",
        )
        .unwrap();

        assert_eq!(
            result,
            MergeConflictKind::AddAdd
        );
    }

    // ------------------------------------------------------------------
    // Merge plan tests
    // ------------------------------------------------------------------

    #[test]
    fn test_merge_plan_clean_theirs_change() {
        let mut base = BTreeMap::new();
        let mut ours = BTreeMap::new();
        let mut theirs = BTreeMap::new();

        base.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        ours.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        theirs.insert(
            "file.txt".to_string(),
            "theirs".to_string(),
        );

        let plan = build_merge_plan(
            Path::new(".rusty"),
            &base,
            &ours,
            &theirs,
        )
        .unwrap();

        assert_eq!(
            plan.files.get("file.txt"),
            Some(&"theirs".to_string())
        );

        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn test_merge_plan_clean_ours_change() {
        let mut base = BTreeMap::new();
        let mut ours = BTreeMap::new();
        let mut theirs = BTreeMap::new();

        base.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        ours.insert(
            "file.txt".to_string(),
            "ours".to_string(),
        );

        theirs.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        let plan = build_merge_plan(
            Path::new(".rusty"),
            &base,
            &ours,
            &theirs,
        )
        .unwrap();

        assert_eq!(
            plan.files.get("file.txt"),
            Some(&"ours".to_string())
        );

        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn test_merge_plan_both_modified() {
        let mut base = BTreeMap::new();
        let mut ours = BTreeMap::new();
        let mut theirs = BTreeMap::new();

        base.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        ours.insert(
            "file.txt".to_string(),
            "ours".to_string(),
        );

        theirs.insert(
            "file.txt".to_string(),
            "theirs".to_string(),
        );

        let plan = build_merge_plan(
            Path::new(".rusty"),
            &base,
            &ours,
            &theirs,
        )
        .unwrap();

        assert!(plan.files.is_empty());
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(
            plan.conflicts[0].kind,
            MergeConflictKind::BothModified
        );
    }

    #[test]
    fn test_merge_plan_same_change_is_clean() {
        let mut base = BTreeMap::new();
        let mut ours = BTreeMap::new();
        let mut theirs = BTreeMap::new();

        base.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        ours.insert(
            "file.txt".to_string(),
            "same".to_string(),
        );

        theirs.insert(
            "file.txt".to_string(),
            "same".to_string(),
        );

        let plan = build_merge_plan(
            Path::new(".rusty"),
            &base,
            &ours,
            &theirs,
        )
        .unwrap();

        assert_eq!(
            plan.files.get("file.txt"),
            Some(&"same".to_string())
        );

        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn test_merge_plan_modify_delete() {
        let mut base = BTreeMap::new();
        let mut ours = BTreeMap::new();
        let theirs = BTreeMap::new();

        base.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        ours.insert(
            "file.txt".to_string(),
            "ours".to_string(),
        );

        let plan = build_merge_plan(
            Path::new(".rusty"),
            &base,
            &ours,
            &theirs,
        )
        .unwrap();

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(
            plan.conflicts[0].kind,
            MergeConflictKind::ModifyDelete
        );
    }

    #[test]
    fn test_merge_plan_delete_modify() {
        let mut base = BTreeMap::new();
        let ours = BTreeMap::new();
        let mut theirs = BTreeMap::new();

        base.insert(
            "file.txt".to_string(),
            "base".to_string(),
        );

        theirs.insert(
            "file.txt".to_string(),
            "theirs".to_string(),
        );

        let plan = build_merge_plan(
            Path::new(".rusty"),
            &base,
            &ours,
            &theirs,
        )
        .unwrap();

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(
            plan.conflicts[0].kind,
            MergeConflictKind::DeleteModify
        );
    }

    #[test]
    fn test_merge_plan_add_add() {
        let base = BTreeMap::new();
        let mut ours = BTreeMap::new();
        let mut theirs = BTreeMap::new();

        ours.insert(
            "new.txt".to_string(),
            "ours".to_string(),
        );

        theirs.insert(
            "new.txt".to_string(),
            "theirs".to_string(),
        );

        let plan = build_merge_plan(
            Path::new(".rusty"),
            &base,
            &ours,
            &theirs,
        )
        .unwrap();

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(
            plan.conflicts[0].kind,
            MergeConflictKind::AddAdd
        );
    }

    // ------------------------------------------------------------------
    // Binary detection tests
    // ------------------------------------------------------------------

    #[test]
    fn test_binary_detection() {
        assert!(!is_binary(b"hello world"));

        assert!(is_binary(&[
            0x01, 0x02, 0x00, 0x04
        ]));
    }

    // ------------------------------------------------------------------
    // Ancestor tests
    // ------------------------------------------------------------------

    #[test]
    fn test_collect_ancestors_empty_repo_fails() {
        let (_dir, repo_path) = setup_repo();

        let result =
            collect_ancestors(
                &repo_path,
                "does-not-exist",
            );

        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // MERGE_HEAD tests
    // ------------------------------------------------------------------

    #[test]
    fn test_read_merge_head() {
        let (_dir, repo_path) = setup_repo();

        assert_eq!(
            read_merge_head(&repo_path)
                .unwrap(),
            None
        );

        fs::write(
            repo_path.join("MERGE_HEAD"),
            "abcdef123456\n",
        )
        .unwrap();

        assert_eq!(
            read_merge_head(&repo_path)
                .unwrap(),
            Some("abcdef123456".to_string())
        );
    }

    // ------------------------------------------------------------------
    // State removal test
    // ------------------------------------------------------------------

    #[test]
    fn test_merge_state_remove() {
        let (_dir, repo_path) = setup_repo();

        let state = MergeState {
            merge_head: "abc".to_string(),
            conflicts: Vec::new(),
        };

        state.save(&repo_path).unwrap();

        assert!(
            repo_path.join("MERGE_STATE").exists()
        );

        MergeState::remove(&repo_path).unwrap();

        assert!(
            !repo_path.join("MERGE_STATE").exists()
        );
    }
}
