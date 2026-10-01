use anyhow::Result;
use reqwest::blocking::Client;
use serde_json::json;
use std::fs;
use std::path::Path;

use crate::auth::Credentials;

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

pub fn send_object(
    origin: &str,
    object_type: &str,
    hash: &str,
    data: &[u8],
    creds: &Credentials,
) -> Result<()> {
    let client = Client::new();

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
        .send()?;

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

pub fn object_exists(origin: &str, hash: &str, creds: &Credentials) -> Result<bool> {
    let client = Client::new();

    let url = format!("{}/objects/{}/exists", origin.trim_end_matches('/'), hash);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .send()?;

    if !response.status().is_success() {
        return Ok(false);
    }

    let data: serde_json::Value = response.json()?;

    Ok(data["exists"].as_bool().unwrap_or(false))
}

pub fn update_ref(
    origin: &str,
    branch_ref: &str,
    commit_hash: &str,
    creds: &Credentials,
) -> Result<()> {
    let client = Client::new();

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
        .send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body_str = response.text().unwrap_or_default();
        anyhow::bail!(
            "Failed to update remote branch reference ({}): {}",
            status,
            body_str
        );
    }

    Ok(())
}

pub fn get_ref(origin: &str, branch: &str, creds: &Credentials) -> Result<String> {
    let client = Client::new();

    let url = format!("{}/refs/{}", origin.trim_end_matches('/'), branch);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .send()?;

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

pub fn get_object(origin: &str, hash: &str, creds: &Credentials) -> Result<serde_json::Value> {
    let client = Client::new();

    let url = format!("{}/objects/{}", origin.trim_end_matches('/'), hash);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-Rusty-Email", &creds.email)
        .header("X-Rusty-Token", &creds.token)
        .send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();

        anyhow::bail!("Failed to fetch object {} ({}): {}", hash, status, body);
    }

    let data: serde_json::Value = response.json()?;

    Ok(data)
}
