use anyhow::Result;
use std::path::PathBuf;

use crate::walrus::WalrusClient;
use crate::clean::get_blob_id_from_sha;

pub async fn walrus_blob_id(_client: WalrusClient, file: PathBuf) -> Result<()> {
    // Get the SHA256 from the LFS pointer
    let sha256 = extract_sha256_from_lfs_pointer(&file).await?;
    
    // Look up the actual Walrus blob ID
    match get_blob_id_from_sha(&sha256).await? {
        Some(blob_id) => {
            println!("File: {}", file.display());
            println!("SHA256: {}", sha256);
            println!("Walrus Blob ID: {}", blob_id);
        }
        None => {
            println!("No Walrus blob ID found for file: {}", file.display());
            println!("SHA256: {}", sha256);
            println!("This file may not have been processed by git-lfs-walrus");
        }
    }
    
    Ok(())
}

async fn extract_sha256_from_lfs_pointer(file: &PathBuf) -> Result<String> {
    // First try to get it from the git object
    let output = std::process::Command::new("git")
        .args(&["show", &format!("HEAD:{}", file.display())])
        .output()?;
    
    if output.status.success() {
        let content = String::from_utf8(output.stdout)?;
        
        // Parse the LFS pointer
        for line in content.lines() {
            if line.starts_with("oid sha256:") {
                return Ok(line.strip_prefix("oid sha256:").unwrap().to_string());
            }
        }
    }
    
    // Fallback: calculate SHA256 of the current file
    let file_content = tokio::fs::read(&file).await?;
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(&file_content);
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}