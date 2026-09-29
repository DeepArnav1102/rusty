use std::fs;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn hash_file (file_path: &Path) -> Result<String>{

    let content = fs::read(file_path)?;

    let mut hasher = Sha256::new();
    hasher.update(&content);

    let hash256 = hasher.finalize();
    
    Ok(hex::encode(hash256))
}

pub fn hash_objects (file_path : &Path ,repo_path : &Path ) -> Result<String> {

    let content = fs::read(file_path)?;

    let mut hasher = Sha256::new();
    hasher.update(&content);

    let hash256 = hasher.finalize();
    let hash = hex::encode(hash256);

    let blob_path = repo_path.join("objects").join("blobs").join(&hash);
    
    if !blob_path.exists() {
        fs::write(&blob_path, &content)?;
    }

    Ok(hash)

}
