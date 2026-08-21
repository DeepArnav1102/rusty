use std::collections::BTreeMap;
use std::fs;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub blob_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    pub entries: BTreeMap<String, IndexEntry>,
}

impl Index {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn load(repo_path: &Path) -> Result<Self> {
        let index_path = repo_path.join("index");

        if !index_path.exists() {
            return Ok(Self::new());
        }
        if fs::metadata(&index_path)?.len() == 0 {
            return Ok(Self::new());
        }

        let data = fs::read_to_string(&index_path)?;
        let index: Index = serde_json::from_str(&data)?;

        Ok(index)
    }

    pub fn save(&self, repo_path: &Path) -> Result<()> {
        let index_path = repo_path.join("index");

        let data = serde_json::to_string_pretty(self)?;

        fs::write(index_path, data)?;

        Ok(())
    }

    pub fn add(&mut self, path: String, blob_hash: String) {
        self.entries.insert(
            path,
            IndexEntry {
                blob_hash,
            },
        );
    }

    pub fn remove(&mut self, path: &str) {
        self.entries.remove(path);
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }
}
