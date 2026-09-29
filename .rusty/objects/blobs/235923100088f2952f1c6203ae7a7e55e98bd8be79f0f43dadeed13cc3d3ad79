use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

const DEFAULT_SERVER: &str = "http://localhost:3000";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Credentials {
    pub email: String,
    pub token: String,
    pub server: String,
    #[serde(default)]
    pub username: Option<String>,
}

fn credentials_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let rusty_home = home.join(".rusty");
    fs::create_dir_all(&rusty_home)?;
    Ok(rusty_home.join("credentials.json"))
}

pub fn load_credentials() -> Result<Option<Credentials>> {
    let path = credentials_path()?;

    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(&path)?;

    if data.trim().is_empty() {
        return Ok(None);
    }

    let creds: Credentials = serde_json::from_str(&data)
        .context("Corrupted credentials file. Run `rusty login` to re-authenticate.")?;

    Ok(Some(creds))
}

fn save_credentials(creds: &Credentials) -> Result<()> {
    let path = credentials_path()?;
    let data = serde_json::to_string_pretty(creds)?;
    fs::write(&path, data)?;
    Ok(())
}

fn remove_credentials() -> Result<()> {
    let path = credentials_path()?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn login(server_url_opt: Option<&str>) -> Result<()> {
    let server_url = server_url_opt.unwrap_or(DEFAULT_SERVER).trim_end_matches('/');

    println!("\n🔑 Rusty VCS Authentication");
    println!("  Connect CLI to your Git Version Control account.");
    println!("  (You can generate a Security Token / PAT in your website Profile Settings)\n");

    print!("Enter your email: ");
    io::stdout().flush()?;
    let mut email = String::new();
    io::stdin().read_line(&mut email)?;
    let email = email.trim().to_string();

    if email.is_empty() {
        anyhow::bail!("Email cannot be empty");
    }

    print!("Enter security token (PAT): ");
    io::stdout().flush()?;
    let mut token = String::new();
    io::stdin().read_line(&mut token)?;
    let token = token.trim().to_string();

    if token.is_empty() {
        anyhow::bail!("Security token cannot be empty");
    }

    println!("\nVerifying credentials with {}...", server_url);

    let client = reqwest::blocking::Client::new();
    let login_url = format!("{}/api/v1/auth/cli/login", server_url);

    let response = client
        .post(&login_url)
        .json(&serde_json::json!({
            "email": email,
            "pat": token
        }))
        .send();

    let (is_success, body_text, username) = match response {
        Ok(res) => {
            let status = res.status();
            let text = res.text().unwrap_or_default();
            if status.is_success() {
                let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                let username = parsed["data"]["user"]["username"].as_str().map(|s| s.to_string());
                (true, text, username)
            } else {
                (false, text, None)
            }
        }
        Err(err) => {
            // Try fallback URL /auth/verify-token in case running on custom server
            let alt_url = format!("{}/auth/verify-token", server_url);
            match client.post(&alt_url).json(&serde_json::json!({ "email": email, "token": token })).send() {
                Ok(res) if res.status().is_success() => (true, "OK".to_string(), None),
                _ => anyhow::bail!("Failed to connect to server at {}: {}", server_url, err),
            }
        }
    };

    if !is_success {
        anyhow::bail!("Authentication failed: {}", body_text);
    }

    let creds = Credentials {
        email: email.clone(),
        token,
        server: server_url.to_string(),
        username,
    };
    save_credentials(&creds)?;

    println!("✓ Successfully logged in as {}", email);
    if let Some(ref u) = creds.username {
        println!("  Username:    {}", u);
    }
    println!("  Server:      {}", server_url);
    println!("  Credentials: ~/.rusty/credentials.json\n");

    Ok(())
}

pub fn logout() -> Result<()> {
    let creds = load_credentials()?;
    remove_credentials()?;

    match creds {
        Some(c) => println!("Logged out. Removed credentials for {}.", c.email),
        None => println!("No credentials found. Already logged out."),
    }

    Ok(())
}

pub fn require_auth() -> Result<Credentials> {
    load_credentials()?.ok_or_else(|| {
        anyhow::anyhow!(
            "Authentication required. Run `rusty login` to authenticate before pushing."
        )
    })
}
