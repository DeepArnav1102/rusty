use anyhow::Result;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::commit::get_commit;
use crate::ignore::RustyIgnore;
use crate::index::Index;
use crate::merge::{self, MergeConflict, MergeState};
use crate::objects;
use crate::repository::get_head_commit;
use crate::tree::load_tree_files;

#[derive(Debug, Clone, Default)]
pub struct WorkingTreeStatus {
    pub staged_new: Vec<String>,
    pub staged_modified: Vec<String>,
    pub staged_deleted: Vec<String>,
    pub unstaged_modified: Vec<String>,
    pub unstaged_deleted: Vec<String>,
    pub untracked: Vec<String>,
    pub conflicts: Vec<MergeConflict>,
    pub is_merging: bool,
    pub merge_head: Option<String>,
}

impl WorkingTreeStatus {
    pub fn is_clean(&self) -> bool {
        self.staged_count() == 0
            && self.unstaged_count() == 0
            && self.untracked.is_empty()
            && self.conflicts.is_empty()
            && !self.is_merging
    }

    pub fn staged_count(&self) -> usize {
        self.staged_new.len() + self.staged_modified.len() + self.staged_deleted.len()
    }

    pub fn unstaged_count(&self) -> usize {
        self.unstaged_modified.len() + self.unstaged_deleted.len()
    }

    pub fn untracked_count(&self) -> usize {
        self.untracked.len()
    }

    pub fn total_changes_count(&self) -> usize {
        self.staged_count() + self.unstaged_count() + self.untracked_count()
    }
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub tree: String,
    pub message: String,
    pub parents: Vec<String>,
    pub is_merge: bool,
    pub relative_time: String,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RepoDetails {
    pub repo_path: Option<PathBuf>,
    pub work_dir: Option<PathBuf>,
    pub is_initialized: bool,
    pub current_branch: String,
    pub is_detached: bool,
    pub remote_origin: Option<String>,
    pub total_commits: usize,
    pub last_commit: Option<CommitInfo>,
    pub unpushed_status: String,
    #[allow(dead_code)]
    pub merge_state: Option<MergeState>,
    pub working_tree: WorkingTreeStatus,
    pub recent_commits: Vec<CommitInfo>,
    pub branches: Vec<BranchInfo>,
}

impl Default for RepoDetails {
    fn default() -> Self {
        Self {
            repo_path: None,
            work_dir: None,
            is_initialized: false,
            current_branch: "none".to_string(),
            is_detached: false,
            remote_origin: None,
            total_commits: 0,
            last_commit: None,
            unpushed_status: "No commits".to_string(),
            merge_state: None,
            working_tree: WorkingTreeStatus::default(),
            recent_commits: Vec::new(),
            branches: Vec::new(),
        }
    }
}

pub fn find_repo_path() -> Result<PathBuf> {
    let mut current_dir = std::env::current_dir()?;

    loop {
        let repo_path = current_dir.join(".rusty");
        if repo_path.is_dir() {
            return Ok(repo_path);
        }
        if !current_dir.pop() {
            break;
        }
    }

    anyhow::bail!("Not a rusty repository");
}

pub fn load_repo_details() -> RepoDetails {
    let repo_path = match find_repo_path() {
        Ok(p) => p,
        Err(_) => return RepoDetails::default(),
    };

    let work_dir = repo_path.parent().map(|p| p.to_path_buf());
    let is_initialized = true;

    // Detect HEAD branch & detached status
    let head_path = repo_path.join("HEAD");
    let mut current_branch = "main".to_string();
    let mut is_detached = false;
    if head_path.exists() {
        if let Ok(content) = fs::read_to_string(&head_path) {
            let content = content.trim();
            if let Some(b) = content.strip_prefix("ref: refs/heads/") {
                current_branch = b.trim().to_string();
            } else if let Some(b) = content.strip_prefix("ref: ") {
                current_branch = b.trim().to_string();
            } else if !content.is_empty() {
                is_detached = true;
                current_branch = format!("detached:{}", &content[..8.min(content.len())]);
            }
        }
    }

    // Remote origin
    let remote_origin = crate::remote::get_remote(&repo_path, "origin").ok();

    // Merge state
    let is_merging = merge::is_merge_in_progress(&repo_path);
    let merge_head = merge::read_merge_head(&repo_path).unwrap_or(None);
    let merge_state = MergeState::load(&repo_path).unwrap_or(None);

    // Working tree status
    let working_tree =
        compute_working_tree_status(&repo_path, is_merging, merge_head, &merge_state);

    // Commits
    let (all_commits, total_commits) = load_commits(&repo_path);
    let last_commit = all_commits.first().cloned();

    // Branches
    let branches = load_branches(&repo_path, &current_branch);

    // Unpushed count
    let unpushed_status = compute_unpushed_status(&repo_path, &current_branch, &all_commits);

    RepoDetails {
        repo_path: Some(repo_path),
        work_dir,
        is_initialized,
        current_branch,
        is_detached,
        remote_origin,
        total_commits,
        last_commit,
        unpushed_status,
        merge_state,
        working_tree,
        recent_commits: all_commits,
        branches,
    }
}

fn compute_working_tree_status(
    repo_path: &Path,
    is_merging: bool,
    merge_head: Option<String>,
    merge_state: &Option<MergeState>,
) -> WorkingTreeStatus {
    let mut status = WorkingTreeStatus {
        is_merging,
        merge_head,
        ..Default::default()
    };

    if let Some(state) = merge_state {
        status.conflicts = state.conflicts.clone();
    }

    let repo_root = match repo_path.parent().and_then(|p| p.canonicalize().ok()) {
        Some(p) => p,
        None => return status,
    };

    let index = match Index::load(repo_path) {
        Ok(idx) => idx,
        Err(_) => Index::new(),
    };

    let head_files: BTreeMap<String, String> = match get_head_commit(repo_path) {
        Ok(Some(commit_hash)) => match get_commit(repo_path, &commit_hash) {
            Ok(commit) => load_tree_files(repo_path, &commit.tree).unwrap_or_default(),
            Err(_) => BTreeMap::new(),
        },
        _ => BTreeMap::new(),
    };

    let ignore = RustyIgnore::load(&repo_root);

    let mut disk_files: BTreeMap<String, String> = BTreeMap::new();
    let mut untracked_files: Vec<String> = Vec::new();

    for entry in WalkDir::new(&repo_root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = match canonical.strip_prefix(&repo_root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        if rel_path
            .split('/')
            .any(|c| c == ".rusty" || c == ".git" || c == "target")
        {
            continue;
        }

        let is_tracked = index.contains(&rel_path) || head_files.contains_key(&rel_path);

        if is_tracked {
            if let Ok(hash) = objects::hash_file(&canonical) {
                disk_files.insert(rel_path, hash);
            }
        } else if !ignore.is_ignored(&rel_path, false) {
            untracked_files.push(rel_path);
        }
    }

    // Staged changes: HEAD vs Index
    for (path, entry) in &index.entries {
        match head_files.get(path) {
            Some(head_hash) => {
                if &entry.blob_hash != head_hash {
                    status.staged_modified.push(path.clone());
                }
            }
            None => {
                status.staged_new.push(path.clone());
            }
        }
    }

    for path in head_files.keys() {
        if !index.contains(path) {
            status.staged_deleted.push(path.clone());
        }
    }

    // Unstaged changes: Index vs Working Tree
    for (path, entry) in &index.entries {
        match disk_files.get(path) {
            Some(disk_hash) => {
                if disk_hash != &entry.blob_hash {
                    status.unstaged_modified.push(path.clone());
                }
            }
            None => {
                status.unstaged_deleted.push(path.clone());
            }
        }
    }

    status.untracked = untracked_files;

    status
}

fn load_commits(repo_path: &Path) -> (Vec<CommitInfo>, usize) {
    let head_path = repo_path.join("HEAD");
    if !head_path.exists() {
        return (Vec::new(), 0);
    }

    let head = match fs::read_to_string(&head_path) {
        Ok(h) => h.trim().to_string(),
        Err(_) => return (Vec::new(), 0),
    };

    let start_hash = if let Some(branch) = head.strip_prefix("ref: ") {
        let branch_path = repo_path.join(branch.trim());
        if !branch_path.exists() {
            return (Vec::new(), 0);
        }
        match fs::read_to_string(branch_path) {
            Ok(h) => h.trim().to_string(),
            Err(_) => return (Vec::new(), 0),
        }
    } else {
        head
    };

    if start_hash.is_empty() {
        return (Vec::new(), 0);
    }

    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut commits = Vec::new();

    queue.push_back(start_hash);

    let mut idx = 0;
    while let Some(current_hash) = queue.pop_front() {
        if !visited.insert(current_hash.clone()) {
            continue;
        }

        let commit = match get_commit(repo_path, &current_hash) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for parent in &commit.parents {
            queue.push_back(parent.clone());
        }

        let short_hash = if current_hash.len() >= 7 {
            current_hash[..7].to_string()
        } else {
            current_hash.clone()
        };

        let is_merge = commit.parents.len() > 1;

        let relative_time = if idx == 0 {
            "recent".to_string()
        } else if idx == 1 {
            "earlier today".to_string()
        } else {
            format!("{} commits ago", idx)
        };
        idx += 1;

        commits.push(CommitInfo {
            hash: current_hash,
            short_hash,
            tree: commit.tree,
            message: commit.message,
            parents: commit.parents,
            is_merge,
            relative_time,
        });

        if commits.len() >= 100 {
            break;
        }
    }

    let total = commits.len();
    (commits, total)
}

fn compute_unpushed_status(
    repo_path: &Path,
    current_branch: &str,
    commits: &[CommitInfo],
) -> String {
    let remote_ref_path = repo_path
        .join("refs")
        .join("remotes")
        .join("origin")
        .join(current_branch);

    if !remote_ref_path.exists() {
        if commits.is_empty() {
            return "No commits".to_string();
        }
        return format!("{} commits (unpushed)", commits.len());
    }

    let remote_hash = match fs::read_to_string(&remote_ref_path) {
        Ok(h) => h.trim().to_string(),
        Err(_) => return "Unknown".to_string(),
    };

    if let Some(pos) = commits.iter().position(|c| c.hash == remote_hash) {
        if pos == 0 {
            "Up to date".to_string()
        } else {
            format!("{} commits ahead", pos)
        }
    } else {
        format!("{} commits", commits.len())
    }
}

fn load_branches(repo_path: &Path, current_branch: &str) -> Vec<BranchInfo> {
    let mut branches = Vec::new();
    let heads_path = repo_path.join("refs").join("heads");

    if heads_path.exists() {
        let mut local_names = Vec::new();
        let _ = collect_refs(&heads_path, &heads_path, &mut local_names);
        local_names.sort();

        for name in local_names {
            let branch_file = heads_path.join(&name);
            let commit_hash = fs::read_to_string(branch_file)
                .ok()
                .map(|s| s.trim().to_string());
            branches.push(BranchInfo {
                name: name.clone(),
                is_current: name == current_branch,
                is_remote: false,
                commit_hash,
            });
        }
    }

    let remotes_path = repo_path.join("refs").join("remotes").join("origin");
    if remotes_path.exists() {
        let mut remote_names = Vec::new();
        let _ = collect_refs(&remotes_path, &remotes_path, &mut remote_names);
        remote_names.sort();

        for name in remote_names {
            let branch_file = remotes_path.join(&name);
            let commit_hash = fs::read_to_string(branch_file)
                .ok()
                .map(|s| s.trim().to_string());
            branches.push(BranchInfo {
                name: format!("origin/{}", name),
                is_current: false,
                is_remote: true,
                commit_hash,
            });
        }
    }

    branches
}

fn collect_refs(root: &Path, current: &Path, names: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_refs(root, &path, names)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                names.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}
