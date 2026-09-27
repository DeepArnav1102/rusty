use anyhow::Result;
use std::fs;
use std::path::PathBuf;

pub fn init() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let rusty_dir = current_dir.join(".rusty");

    if rusty_dir.exists() {
        anyhow::bail!("Repository already initialized");
    }

    fs::create_dir(&rusty_dir)?;

    fs::create_dir(rusty_dir.join("objects"))?;
    fs::create_dir(rusty_dir.join("objects").join("blobs"))?;
    fs::create_dir(rusty_dir.join("objects").join("trees"))?;
    fs::create_dir(rusty_dir.join("objects").join("commits"))?;

    fs::create_dir(rusty_dir.join("refs"))?;
    fs::create_dir(rusty_dir.join("refs").join("heads"))?;
    fs::create_dir(rusty_dir.join("refs").join("remotes"))?;

    fs::write(rusty_dir.join("HEAD"), "ref: refs/heads/main\n")?;

    fs::write(rusty_dir.join("index"), "")?;

    fs::write(rusty_dir.join("config"), "{\n    \"remotes\": {}\n}\n")?;

    println!(
        "Initialized empty Rusty repository in {}",
        PathBuf::from(".rusty").display()
    );

    Ok(())
}
