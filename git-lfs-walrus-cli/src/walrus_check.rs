use anyhow::Result;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use serde::{Deserialize};

use crate::walrus::WalrusClient;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BlobStatusResponse {
    #[serde(rename = "blobObject")]
    blob_object: Option<BlobObjectStatus>,
    status: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
struct StorageStatus {
    id: String,
    #[serde(rename = "startEpoch")]
    start_epoch: u64,
    #[serde(rename = "endEpoch")]
    end_epoch: u64,
    #[serde(rename = "storageSize")]
    storage_size: u64,
}

pub async fn walrus_check(client: WalrusClient, files: Vec<PathBuf>) -> Result<()> {
    if files.is_empty() {
        println!("Checking all LFS files for expiration...");
        check_all_lfs_files(&client).await
    } else {
        println!("Checking {} files for expiration...", files.len());
        check_specific_files(&client, files).await
    }
}

async fn check_all_lfs_files(client: &WalrusClient) -> Result<()> {
    // Get all LFS files in the repository
    let lfs_files = get_lfs_files().await?;
    
    if lfs_files.is_empty() {
        println!("No LFS files found in repository.");
        return Ok(());
    }

    println!("Found {} LFS files to check:", lfs_files.len());
    
    let mut expired_count = 0;
    let mut valid_count = 0;
    let mut error_count = 0;

    for file_path in lfs_files {
        match check_lfs_file(client, &file_path).await {
            Ok(status) => {
                if status.contains("expired") || status.contains("invalid") {
                    expired_count += 1;
                    println!("❌ {} - {}", file_path.display(), status);
                } else {
                    valid_count += 1;
                    println!("✅ {} - {}", file_path.display(), status);
                }
            }
            Err(e) => {
                error_count += 1;
                println!("⚠️  {} - Error: {}", file_path.display(), e);
            }
        }
    }

    println!("\nSummary:");
    println!("  Valid: {}", valid_count);
    println!("  Expired/Invalid: {}", expired_count);
    println!("  Errors: {}", error_count);

    Ok(())
}

async fn check_specific_files(client: &WalrusClient, files: Vec<PathBuf>) -> Result<()> {
    let mut expired_count = 0;
    let mut valid_count = 0;
    let mut error_count = 0;

    for file_path in files {
        match check_lfs_file(client, &file_path).await {
            Ok(status) => {
                if status.contains("expired") || status.contains("invalid") {
                    expired_count += 1;
                    println!("❌ {} - {}", file_path.display(), status);
                } else {
                    valid_count += 1;
                    println!("✅ {} - {}", file_path.display(), status);
                }
            }
            Err(e) => {
                error_count += 1;
                println!("⚠️  {} - Error: {}", file_path.display(), e);
            }
        }
    }

    println!("\nSummary:");
    println!("  Valid: {}", valid_count);
    println!("  Expired/Invalid: {}", expired_count);
    println!("  Errors: {}", error_count);

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

async fn check_lfs_file(client: &WalrusClient, file_path: &PathBuf) -> Result<String> {
    // Try to get blob ID from mapping file first
    if let Some(blob_id) = get_blob_id_from_mapping(file_path).await? {
        return check_blob_status(client, &blob_id).await;
    }

    // Fallback: try to extract from LFS pointer directly
    if file_path.exists() {
        let content = tokio::fs::read_to_string(file_path).await?;
        if let Ok(blob_id) = extract_walrus_blob_id(&content) {
            return check_blob_status(client, &blob_id).await;
        }
    }

    Ok("No Walrus blob ID found (file may not be stored in Walrus)".to_string())
}

async fn get_blob_id_from_mapping(file_path: &PathBuf) -> Result<Option<String>> {
    // Extract SHA256 from git LFS pointer
    let output = std::process::Command::new("git")
        .args(&["show", &format!("HEAD:{}", file_path.display())])
        .output()?;

    if output.status.success() {
        let content = String::from_utf8(output.stdout)?;
        
        // Parse the LFS pointer to get SHA256
        for line in content.lines() {
            if line.starts_with("oid sha256:") {
                let sha256 = line.strip_prefix("oid sha256:").unwrap();
                // Look up in mapping file
                return crate::clean::get_blob_id_from_sha(sha256).await;
            }
        }
    }

    Ok(None)
}

async fn check_blob_status(client: &WalrusClient, blob_id: &str) -> Result<String> {
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
            return Ok("Blob not found in Walrus".to_string());
        }
        return Err(anyhow::anyhow!(
            "Walrus blob-status command failed: {}",
            error_msg
        ));
    }

    let response_text = String::from_utf8(output.stdout)?;
    let status_response: BlobStatusResponse = serde_json::from_str(&response_text)?;

    Ok(format_blob_status(&status_response))
}

fn format_blob_status(status: &BlobStatusResponse) -> String {
    match &status.blob_object {
        Some(blob_obj) => {
            // Check if blob is expired (simplified check - you might want to get current epoch)
            let storage_end = blob_obj.storage.end_epoch;
            format!(
                "Status: {} | Size: {} bytes | Storage until epoch: {}",
                status.status,
                blob_obj.size,
                storage_end
            )
        }
        None => format!("Status: {}", status.status),
    }
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