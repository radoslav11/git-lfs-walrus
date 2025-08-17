use anyhow::Result;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use serde::{Deserialize, Serialize};

use crate::walrus::WalrusClient;

#[derive(Debug, Deserialize)]
struct BlobStatusResponse {
    #[serde(rename = "blobObject")]
    blob_object: Option<BlobObjectStatus>,
    status: String,
}

#[derive(Debug, Deserialize)]
struct BlobObjectStatus {
    id: String,
    #[serde(rename = "storedEpoch")]
    stored_epoch: u64,
    #[serde(rename = "blobId")]
    blob_id: String,
    size: u64,
    #[serde(rename = "certifiedEpoch")]
    certified_epoch: u64,
    storage: StorageStatus,
}

#[derive(Debug, Deserialize)]
struct StorageStatus {
    id: String,
    #[serde(rename = "startEpoch")]
    start_epoch: u64,
    #[serde(rename = "endEpoch")]
    end_epoch: u64,
    #[serde(rename = "storageSize")]
    storage_size: u64,
}

pub async fn walrus_refresh(client: WalrusClient, files: Vec<PathBuf>) -> Result<()> {
    if files.is_empty() {
        println!("Refreshing all expired LFS files...");
        refresh_all_expired_files(&client).await
    } else {
        println!("Refreshing {} files...", files.len());
        refresh_specific_files(&client, files).await
    }
}

async fn refresh_all_expired_files(client: &WalrusClient) -> Result<()> {
    // Get all LFS files in the repository
    let lfs_files = get_lfs_files().await?;
    
    if lfs_files.is_empty() {
        println!("No LFS files found in repository.");
        return Ok(());
    }

    println!("Found {} LFS files to check for expiration:", lfs_files.len());
    
    let mut refreshed_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for file_path in lfs_files {
        match check_and_refresh_file(client, &file_path).await {
            Ok(RefreshResult::Refreshed) => {
                refreshed_count += 1;
                println!("🔄 {} - Refreshed", file_path.display());
            }
            Ok(RefreshResult::NotNeeded) => {
                skipped_count += 1;
                println!("✅ {} - No refresh needed", file_path.display());
            }
            Err(e) => {
                error_count += 1;
                println!("⚠️  {} - Error: {}", file_path.display(), e);
            }
        }
    }

    println!("\nSummary:");
    println!("  Refreshed: {}", refreshed_count);
    println!("  Skipped (valid): {}", skipped_count);
    println!("  Errors: {}", error_count);

    Ok(())
}

async fn refresh_specific_files(client: &WalrusClient, files: Vec<PathBuf>) -> Result<()> {
    let mut refreshed_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for file_path in files {
        match refresh_file(client, &file_path).await {
            Ok(RefreshResult::Refreshed) => {
                refreshed_count += 1;
                println!("🔄 {} - Refreshed", file_path.display());
            }
            Ok(RefreshResult::NotNeeded) => {
                skipped_count += 1;
                println!("✅ {} - No refresh needed", file_path.display());
            }
            Err(e) => {
                error_count += 1;
                println!("⚠️  {} - Error: {}", file_path.display(), e);
            }
        }
    }

    println!("\nSummary:");
    println!("  Refreshed: {}", refreshed_count);
    println!("  Skipped (valid): {}", skipped_count);
    println!("  Errors: {}", error_count);

    Ok(())
}

#[derive(Debug)]
enum RefreshResult {
    Refreshed,
    NotNeeded,
}

async fn check_and_refresh_file(client: &WalrusClient, file_path: &PathBuf) -> Result<RefreshResult> {
    // Check if file exists
    if !file_path.exists() {
        return Err(anyhow::anyhow!("File does not exist locally"));
    }

    // Read the LFS pointer to get the blob ID
    let content = tokio::fs::read_to_string(file_path).await?;
    let blob_id = extract_walrus_blob_id(&content)?;

    // Check blob status in Walrus
    let needs_refresh = check_blob_needs_refresh(client, &blob_id).await?;
    
    if needs_refresh {
        refresh_blob(client, file_path, &blob_id).await?;
        Ok(RefreshResult::Refreshed)
    } else {
        Ok(RefreshResult::NotNeeded)
    }
}

async fn refresh_file(client: &WalrusClient, file_path: &PathBuf) -> Result<RefreshResult> {
    // Check if file exists
    if !file_path.exists() {
        return Err(anyhow::anyhow!("File does not exist locally"));
    }

    // Read the LFS pointer to get the blob ID
    let content = tokio::fs::read_to_string(file_path).await?;
    let blob_id = extract_walrus_blob_id(&content)?;

    // Always refresh the specific file
    refresh_blob(client, file_path, &blob_id).await?;
    Ok(RefreshResult::Refreshed)
}

async fn check_blob_needs_refresh(client: &WalrusClient, blob_id: &str) -> Result<bool> {
    let mut cmd = Command::new(
        client.walrus_path()
            .map(|p| p.as_os_str())
            .unwrap_or_else(|| "walrus".as_ref()),
    );
    
    cmd.args(&["blob-status", "--json", "--blob-id", blob_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().await?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        if error_msg.contains("not found") || error_msg.contains("does not exist") {
            return Ok(true); // Blob not found, needs refresh
        }
        return Err(anyhow::anyhow!(
            "Walrus blob-status command failed: {}",
            error_msg
        ));
    }

    let response_text = String::from_utf8(output.stdout)?;
    let status_response: BlobStatusResponse = serde_json::from_str(&response_text)?;

    // Check if blob is expired or invalid
    let needs_refresh = status_response.status.contains("expired") 
        || status_response.status.contains("invalid")
        || status_response.blob_object.is_none();

    Ok(needs_refresh)
}

async fn refresh_blob(client: &WalrusClient, file_path: &PathBuf, _old_blob_id: &str) -> Result<()> {
    // Read the original file content from the working directory
    // This assumes the file has been checked out from LFS
    let file_content = tokio::fs::read(file_path).await?;
    
    // Store the file content in Walrus again to get a new blob ID
    let new_blob_id = client.store_bytes(&file_content).await?;
    
    // Update the LFS pointer with the new blob ID
    update_lfs_pointer(file_path, &new_blob_id, file_content.len()).await?;
    
    Ok(())
}

async fn update_lfs_pointer(file_path: &PathBuf, new_blob_id: &str, file_size: usize) -> Result<()> {
    use sha2::{Digest, Sha256};
    
    // Read the current file to calculate its SHA256
    let file_content = tokio::fs::read(file_path).await?;
    
    // Calculate SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(&file_content);
    let hash = hasher.finalize();
    let sha256_hex = hex::encode(hash);

    // Create new LFS pointer with new Walrus blob ID
    let lfs_pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\next-0-walrus {}\n",
        sha256_hex,
        file_size,
        new_blob_id
    );

    // Write the new LFS pointer back to the file
    tokio::fs::write(file_path, lfs_pointer.as_bytes()).await?;
    
    Ok(())
}

async fn get_lfs_files() -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(&["lfs", "ls-files", "--name-only"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to list LFS files: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let files_output = String::from_utf8(output.stdout)?;
    let files: Vec<PathBuf> = files_output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| PathBuf::from(line.trim()))
        .collect();

    Ok(files)
}

fn extract_walrus_blob_id(content: &str) -> Result<String> {
    for line in content.lines() {
        if line.starts_with("ext-0-walrus ") {
            if let Some((_, blob_id)) = line.split_once(' ') {
                return Ok(blob_id.trim().to_string());
            }
        }
    }
    Err(anyhow::anyhow!("No Walrus blob ID found in LFS pointer"))
}