use std::io::Read;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::walrus::WalrusClient;

pub async fn clean(
    client: WalrusClient,
    mut input: impl Read + Send + Sync + Unpin + 'static,
    mut output: impl AsyncWrite + AsyncWriteExt + Unpin,
) -> Result<()> {
    // Read all input data
    let mut data = Vec::new();
    input.read_to_end(&mut data)?;

    // Calculate SHA256 hash for the original file
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hasher.finalize();
    let sha256_hex = hex::encode(hash);

    // Perform a dry run to get the estimated cost
    let dry_run_output = client.store_bytes_dry_run(&data).await?;
    let json_output: Value = serde_json::from_str(&dry_run_output)?;
    let _total_cost = if let Some(array) = json_output.as_array() {
        if let Some(first_item) = array.first() {
            first_item["storageCost"].as_u64().unwrap_or(0).to_string()
        } else {
            "0".to_string()
        }
    } else {
        "0".to_string()
    };

    

    // Store the data in Walrus
    let blob_id = client.store_bytes(&data).await?;

    // Store the mapping between SHA256 and Walrus blob ID
    if let Err(e) = store_blob_mapping(&sha256_hex, &blob_id).await {
        eprintln!("Warning: Could not store blob mapping: {}", e);
    }

    // Create LFS pointer with Walrus blob ID stored in extension field
    let lfs_pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\next-0-walrus {}\n",
        sha256_hex,
        data.len(),
        blob_id
    );

    // Also store mapping with LFS pointer SHA256 (for git lookup)
    let mut pointer_hasher = Sha256::new();
    pointer_hasher.update(lfs_pointer.as_bytes());
    let pointer_hash = pointer_hasher.finalize();
    let pointer_sha256_hex = hex::encode(pointer_hash);
    
    if let Err(e) = store_blob_mapping(&pointer_sha256_hex, &blob_id).await {
        eprintln!("Warning: Could not store pointer mapping: {}", e);
    }

    output.write_all(lfs_pointer.as_bytes()).await?;

    Ok(())
}

async fn store_blob_mapping(sha256_hex: &str, blob_id: &str) -> Result<()> {
    let mapping_file = get_mapping_file_path()?;
    
    // Read existing mappings
    let mut mappings = if mapping_file.exists() {
        let content = tokio::fs::read_to_string(&mapping_file).await?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::Map::new())
    } else {
        serde_json::Map::new()
    };
    
    // Add new mapping
    mappings.insert(sha256_hex.to_string(), serde_json::Value::String(blob_id.to_string()));
    
    // Write back to file
    let content = serde_json::to_string_pretty(&mappings)?;
    if let Some(parent) = mapping_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&mapping_file, content).await?;
    
    Ok(())
}

fn get_mapping_file_path() -> Result<std::path::PathBuf> {
    // Try to find git root directory
    let output = std::process::Command::new("git")
        .args(&["rev-parse", "--git-dir"])
        .output()?;
    
    if output.status.success() {
        let git_dir = String::from_utf8(output.stdout)?.trim().to_string();
        Ok(Path::new(&git_dir).join("walrus-mapping.json"))
    } else {
        // Fallback to current directory
        Ok(std::env::current_dir()?.join(".walrus-mapping.json"))
    }
}

pub async fn get_blob_id_from_sha(sha256_hex: &str) -> Result<Option<String>> {
    let mapping_file = get_mapping_file_path()?;
    
    if !mapping_file.exists() {
        return Ok(None);
    }
    
    let content = tokio::fs::read_to_string(&mapping_file).await?;
    let mappings: serde_json::Map<String, serde_json::Value> = 
        serde_json::from_str(&content).unwrap_or_default();
    
    Ok(mappings.get(sha256_hex)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walrus::client;
    use std::io::Cursor;

    const FILE: &[u8] = b"hello world";

    #[tokio::test]
    #[ignore] // Requires Walrus to be installed and configured
    async fn clean_converts_file_into_lfs_pointer() {
        let client = client();
        let mut cursor = Cursor::new(vec![]);
        clean(client, FILE, &mut cursor).await.unwrap();

        let result = String::from_utf8(cursor.into_inner()).unwrap();
        assert!(result.contains("version https://git-lfs.github.com/spec/v1"));
        assert!(result.contains("oid sha256:"));
        assert!(result.contains("size 11"));
        assert!(result.contains("# walrus-blob-id:"));
    }
}
