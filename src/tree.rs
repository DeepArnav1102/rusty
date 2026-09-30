use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::index::Index;

#[derive(Debug)]
enum Node {
    File { hash: String },

    Directory { children: BTreeMap<String, Node> },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub object_hash: String,
    pub object_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tree {
    pub entries: Vec<TreeEntry>,
}

fn insert_into_tree(node: &mut Node, path_parts: &[&str], blob_hash: String) {
    if path_parts.is_empty() {
        return;
    }

    match node {
        Node::Directory { children } => {
            let name = path_parts[0];

            if path_parts.len() == 1 {
                children.insert(name.to_string(), Node::File { hash: blob_hash });
                return;
            }

            let child = children
                .entry(name.to_string())
                .or_insert_with(|| Node::Directory {
                    children: BTreeMap::new(),
                });

            if let Node::File { .. } = child {
                *child = Node::Directory {
                    children: BTreeMap::new(),
                };
            }

            insert_into_tree(child, &path_parts[1..], blob_hash);
        }

        Node::File { .. } => {}
    }
}

fn build_tree(index: &Index) -> Node {
    let mut root = Node::Directory {
        children: BTreeMap::new(),
    };

    for (path, entry) in &index.entries {
        let normalized = path.replace('\\', "/");
        let parts: Vec<&str> = normalized
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();

        insert_into_tree(&mut root, &parts, entry.blob_hash.clone());
    }

    root
}

fn write_node(node: &Node, repo_path: &Path) -> Result<String> {
    match node {
        Node::File { hash } => Ok(hash.clone()),

        Node::Directory { children } => {
            let mut entries = Vec::new();

            for (name, child) in children {
                let object_hash = write_node(child, repo_path)?;

                let object_type = match child {
                    Node::File { .. } => "blob",
                    Node::Directory { .. } => "tree",
                };

                entries.push(TreeEntry {
                    name: name.clone(),
                    object_hash,
                    object_type: object_type.to_string(),
                });
            }

            let tree = Tree { entries };

            let data = serde_json::to_vec(&tree)?;

            let mut hasher = Sha256::new();
            hasher.update(&data);

            let hash = hex::encode(hasher.finalize());

            let tree_path = repo_path.join("objects").join("trees").join(&hash);

            if !tree_path.exists() {
                fs::write(&tree_path, &data)?;
            }

            Ok(hash)
        }
    }
}

pub fn write_tree(repo_path: &Path) -> Result<String> {
    let index = Index::load(repo_path)?;

    let root = build_tree(&index);

    let root_hash = write_node(&root, repo_path)?;

    Ok(root_hash)
}

pub fn load_tree_files(repo_path: &Path, tree_hash: &str) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    collect_tree_files(repo_path, tree_hash, "", &mut files)?;
    Ok(files)
}

fn collect_tree_files(
    repo_path: &Path,
    tree_hash: &str,
    prefix: &str,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    let tree_path = repo_path.join("objects").join("trees").join(tree_hash);
    if !tree_path.exists() {
        anyhow::bail!("Tree object not found: {}", tree_hash);
    }

    let data = fs::read(&tree_path)?;
    let tree: Tree = serde_json::from_slice(&data)
        .with_context(|| format!("Invalid tree object {}", tree_hash))?;

    for entry in tree.entries {
        let relative_path = if prefix.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", prefix, entry.name)
        };

        match entry.object_type.as_str() {
            "blob" => {
                files.insert(relative_path, entry.object_hash);
            }
            "tree" => {
                collect_tree_files(repo_path, &entry.object_hash, &relative_path, files)?;
            }
            other => {
                anyhow::bail!(
                    "Unknown tree entry type '{}' for '{}'",
                    other,
                    relative_path
                );
            }
        }
    }

    Ok(())
}
