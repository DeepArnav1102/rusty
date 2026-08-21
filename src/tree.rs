use anyhow::Result;
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
        let parts: Vec<&str> = path.split('/').collect();

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
