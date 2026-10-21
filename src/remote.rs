use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::auth::Credentials;

// ============================================================
// ADD REMOTE
// ============================================================

pub fn add_remote(repo_path: &Path, name: &str, url: &str) -> Result<()> {
    let config_path = repo_path.join("config");

    let mut config = if config_path.exists() {
        let data = fs::read_to_string(&config_path)?;

        if data.trim().is_empty() {
            json!({
                "remotes": {}
            })
        } else {
            serde_json::from_str(&data)?
        }
    } else {
        json!({
            "remotes": {}
        })
    };

    config["remotes"][name] = json!(url.trim_end_matches('/'));

    let data = serde_json::to_string_pretty(&config)?;
    fs::write(config_path, data)?;

    println!("Added remote '{}' -> {}", name, url);

    Ok(())
}

// ============================================================
// GET REMOTE URL
// ============================================================

pub fn get_remote(repo_path: &Path, name: &str) -> Result<String> {
    let config_path = repo_path.join("config");

    let data = fs::read_to_string(config_path)?;
    let config: serde_json::Value = serde_json::from_str(&data)?;

    let url = config["remotes"][name].as_str().ok_or_else(|| {
        anyhow::anyhow!(
            "Remote '{}' not found. Run `rusty remote add origin <URL>` first.",
            name
        )
    })?;

    Ok(url.to_string())
}

// ============================================================
// SEND OBJECT
// ============================================================

pub fn send_object(
    origin: &str,
    object_type: &str,
    hash: &str,
    data: &[u8],
    creds: &Credentials,
) -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let body = json!({
        "type": object_type,
        "hash": hash,
        "data": String::from_utf8_lossy(data)
    });

    let url = format!("{}/objects", origin.trim_end_matches('/'));

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .json(&body)
        .send()
        .context("Could not reach remote")?;

    if !response.status().is_success() {
        let status = response.status();
        let body_str = response.text().unwrap_or_default();

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!(
                "Authentication failed. Run `rusty login` to authenticate with your website email and PAT token.\nServer: {}",
                body_str
            );
        }

        anyhow::bail!(
            "Failed to send {} {}: ({}) {}",
            object_type,
            hash,
            status,
            body_str
        );
    }

    Ok(())
}

// ============================================================
// CHECK IF OBJECT EXISTS
// ============================================================

pub fn object_exists(origin: &str, hash: &str, creds: &Credentials) -> Result<bool> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let url = format!("{}/objects/{}/exists", origin.trim_end_matches('/'), hash);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .send()
        .context("Could not reach remote")?;

    if !response.status().is_success() {
        return Ok(false);
    }

    let data: serde_json::Value = response.json()?;

    Ok(data["exists"].as_bool().unwrap_or(false))
}

// ============================================================
// UPDATE REMOTE BRANCH REFERENCE
// ============================================================

pub fn update_ref(
    origin: &str,
    branch_ref: &str,
    commit_hash: &str,
    creds: &Credentials,
) -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let clean_branch = branch_ref.strip_prefix("refs/heads/").unwrap_or(branch_ref);

    let url = format!("{}/refs", origin.trim_end_matches('/'));

    let body = json!({
        "branch": clean_branch,
        "commitHash": commit_hash
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .json(&body)
        .send()
        .context("Could not reach remote")?;

    if !response.status().is_success() {
        let status = response.status();
        let body_str = response.text().unwrap_or_default();

        if status == reqwest::StatusCode::CONFLICT || status == reqwest::StatusCode::BAD_REQUEST {
            anyhow::bail!(
                "Updates were rejected because the remote contains work that you do not have locally. Run 'rusty pull' first.\nServer: {}",
                body_str
            );
        }

        anyhow::bail!(
            "Failed to update remote branch reference ({}): {}",
            status,
            body_str
        );
    }

    Ok(())
}

// ============================================================
// GET SINGLE REMOTE REF
// ============================================================

#[allow(dead_code)]
pub fn get_ref(origin: &str, branch: &str, creds: &Credentials) -> Result<String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let url = format!("{}/refs/{}", origin.trim_end_matches('/'), branch);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .send()
        .context("Could not reach remote")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();

        anyhow::bail!("Failed to get remote ref {} ({}): {}", branch, status, body);
    }

    let data: serde_json::Value = response.json()?;

    let hash = data["commitHash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Remote response does not contain commitHash"))?;

    Ok(hash.to_string())
}

// ============================================================
// GET ALL REMOTE BRANCH REFERENCES
// ============================================================

pub fn get_all_refs(origin: &str, creds: &Credentials) -> Result<BTreeMap<String, String>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let url = format!("{}/refs", origin.trim_end_matches('/'));

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .send()
        .context("Could not reach remote")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();

        anyhow::bail!("Could not fetch remote branches ({}): {}", status, body);
    }

    let data: serde_json::Value = response.json()?;

    let mut refs = BTreeMap::new();

    if let Some(branches) = data["branches"].as_array() {
        for branch in branches {
            let name = match branch["branch"].as_str() {
                Some(name) if !name.trim().is_empty() => name.trim(),
                _ => continue,
            };

            let hash = match branch["commitHash"].as_str() {
                Some(hash) if !hash.trim().is_empty() => hash.trim(),
                _ => continue,
            };

            refs.insert(name.to_string(), hash.to_string());
        }
    }

    Ok(refs)
}

// ============================================================
// GET OBJECT FROM REMOTE
// ============================================================

pub fn get_object(origin: &str, hash: &str, creds: &Credentials) -> Result<serde_json::Value> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let url = format!("{}/objects/{}", origin.trim_end_matches('/'), hash);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .send()
        .context("Could not reach remote")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();

        anyhow::bail!("Failed to fetch object {} ({}): {}", hash, status, body);
    }

    let data: serde_json::Value = response.json()?;

    Ok(data)
}

// ============================================================
// BATCH OBJECTS: CHECK EXISTENCE
// ============================================================

#[derive(serde::Deserialize)]
pub struct BatchCheckResult {
    pub existing: Vec<String>,
    pub missing: Vec<String>,
}

pub fn check_objects_batch(
    origin: &str,
    hashes: &[String],
    creds: &Credentials,
) -> Result<BatchCheckResult> {
    if hashes.is_empty() {
        return Ok(BatchCheckResult {
            existing: Vec::new(),
            missing: Vec::new(),
        });
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let url = format!("{}/objects/batch-check", origin.trim_end_matches('/'));

    let body = json!({
        "hashes": hashes
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .json(&body)
        .send();

    let response = match response {
        Ok(r) => r,
        Err(_) => {
            // Fallback for older servers without batch-check:
            // Query one-by-one
            let mut existing = Vec::new();
            let mut missing = Vec::new();
            for h in hashes {
                if object_exists(origin, h, creds).unwrap_or(false) {
                    existing.push(h.clone());
                } else {
                    missing.push(h.clone());
                }
            }
            return Ok(BatchCheckResult { existing, missing });
        }
    };

    if !response.status().is_success() {
        // Fallback gracefully to individual checks
        let mut existing = Vec::new();
        let mut missing = Vec::new();
        for h in hashes {
            if object_exists(origin, h, creds).unwrap_or(false) {
                existing.push(h.clone());
            } else {
                missing.push(h.clone());
            }
        }
        return Ok(BatchCheckResult { existing, missing });
    }

    let result: serde_json::Value = response.json()?;
    let existing = result["existing"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let missing = result["missing"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(BatchCheckResult { existing, missing })
}

// ============================================================
// BATCH OBJECTS: SEND / STORE OBJECTS
// ============================================================

pub struct BatchObjectPayload {
    pub object_type: String,
    pub hash: String,
    pub data: String,
}

pub fn send_objects_batch(
    origin: &str,
    objects: &[BatchObjectPayload],
    creds: &Credentials,
) -> Result<()> {
    if objects.is_empty() {
        return Ok(());
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let url = format!("{}/objects/batch-store", origin.trim_end_matches('/'));

    let json_objects: Vec<serde_json::Value> = objects
        .iter()
        .map(|obj| {
            json!({
                "type": obj.object_type,
                "hash": obj.hash,
                "data": obj.data,
            })
        })
        .collect();

    let body = json!({
        "objects": json_objects
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .json(&body)
        .send();

    let response = match response {
        Ok(r) => r,
        Err(_) => {
            // Fallback: send one by one
            for obj in objects {
                send_object(origin, &obj.object_type, &obj.hash, obj.data.as_bytes(), creds)?;
            }
            return Ok(());
        }
    };

    if !response.status().is_success() {
        // Fallback to one-by-one upload
        for obj in objects {
            send_object(origin, &obj.object_type, &obj.hash, obj.data.as_bytes(), creds)?;
        }
        return Ok(());
    }

    Ok(())
}

// ============================================================
// BATCH OBJECTS: GET / DOWNLOAD OBJECTS
// ============================================================

#[derive(serde::Deserialize)]
pub struct FetchedObject {
    pub hash: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub object_type: String,
    pub data: String,
}

pub fn get_objects_batch(
    origin: &str,
    hashes: &[String],
    creds: &Credentials,
) -> Result<Vec<FetchedObject>> {
    if hashes.is_empty() {
        return Ok(Vec::new());
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let url = format!("{}/objects/batch-get", origin.trim_end_matches('/'));

    let body = json!({
        "hashes": hashes
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .json(&body)
        .send();

    let response = match response {
        Ok(r) => r,
        Err(_) => {
            // Fallback: fetch one by one
            let mut list = Vec::new();
            for h in hashes {
                if let Ok(obj_val) = get_object(origin, h, creds) {
                    if let (Some(hash), Some(obj_type), Some(data)) = (
                        obj_val["hash"].as_str(),
                        obj_val["type"].as_str(),
                        obj_val["data"].as_str(),
                    ) {
                        list.push(FetchedObject {
                            hash: hash.to_string(),
                            object_type: obj_type.to_string(),
                            data: data.to_string(),
                        });
                    }
                }
            }
            return Ok(list);
        }
    };

    if !response.status().is_success() {
        // Fallback to one-by-one
        let mut list = Vec::new();
        for h in hashes {
            if let Ok(obj_val) = get_object(origin, h, creds) {
                if let (Some(hash), Some(obj_type), Some(data)) = (
                    obj_val["hash"].as_str(),
                    obj_val["type"].as_str(),
                    obj_val["data"].as_str(),
                ) {
                    list.push(FetchedObject {
                        hash: hash.to_string(),
                        object_type: obj_type.to_string(),
                        data: data.to_string(),
                    });
                }
            }
        }
        return Ok(list);
    }

    let result: serde_json::Value = response.json()?;
    let mut list = Vec::new();

    if let Some(arr) = result["objects"].as_array() {
        for item in arr {
            if let (Some(hash), Some(obj_type), Some(data)) = (
                item["hash"].as_str(),
                item["type"].as_str(),
                item["data"].as_str(),
            ) {
                list.push(FetchedObject {
                    hash: hash.to_string(),
                    object_type: obj_type.to_string(),
                    data: data.to_string(),
                });
            }
        }
    }

    Ok(list)
}
