use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::Command;

#[derive(Debug, Serialize)]
struct StoreCommand {
    config: Option<String>,
    command: StoreRequest,
}

#[derive(Debug, Serialize)]
struct StoreRequest {
    store: StoreParams,
}

#[derive(Debug, Serialize)]
struct StoreParams {
    files: Vec<String>,
    epochs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ReadCommand {
    config: Option<String>,
    command: ReadRequest,
}

#[derive(Debug, Serialize)]
struct ReadRequest {
    read: ReadParams,
}

#[derive(Debug, Serialize)]
struct ReadParams {
    #[serde(rename = "blobId")]
    blob_id: String,
}

#[derive(Debug, Deserialize)]
struct StoreResponse {
    #[serde(rename = "newlyCreated")]
    newly_created: Option<BlobInfo>,
    #[serde(rename = "alreadyCertified")]
    already_certified: Option<BlobInfo>,
}

#[derive(Debug, Deserialize)]
struct BlobInfo {
    #[serde(rename = "blobObject")]
    blob_object: BlobObject,
    #[serde(rename = "resourceOperation")]
    resource_operation: ResourceOperation,
}

#[derive(Debug, Deserialize)]
struct BlobObject {
    id: String,
    #[serde(rename = "storedEpoch")]
    stored_epoch: u64,
    #[serde(rename = "blobId")]
    blob_id: String,
    size: u64,
    #[serde(rename = "erasureCodeType")]
    erasure_code_type: String,
    #[serde(rename = "certifiedEpoch")]
    certified_epoch: u64,
    storage: Storage,
}

#[derive(Debug, Deserialize)]
struct Storage {
    id: String,
    #[serde(rename = "startEpoch")]
    start_epoch: u64,
    #[serde(rename = "endEpoch")]
    end_epoch: u64,
    #[serde(rename = "storageSize")]
    storage_size: u64,
}

#[derive(Debug, Deserialize)]
struct ResourceOperation {
    #[serde(rename = "RegisterFromScratch")]
    register_from_scratch: Option<RegisterFromScratch>,
}

#[derive(Debug, Deserialize)]
struct RegisterFromScratch {
    #[serde(rename = "encoded_length")]
    encoded_length: u64,
    epochs: u64,
}

pub struct WalrusClient {
    config_path: Option<String>,
}

impl WalrusClient {
    pub fn new() -> Self {
        Self { config_path: None }
    }

    pub fn with_config(config_path: String) -> Self {
        Self {
            config_path: Some(config_path),
        }
    }

    pub async fn store_file(&self, file_path: &Path) -> Result<String> {
        let store_cmd = StoreCommand {
            config: self.config_path.clone(),
            command: StoreRequest {
                store: StoreParams {
                    files: vec![file_path.to_string_lossy().to_string()],
                    epochs: Some(100), // Default to 100 epochs
                },
            },
        };

        let json_input = serde_json::to_string(&store_cmd)?;
        
        let mut child = Command::new("walrus")
            .args(&["json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(json_input.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let output = child.wait_with_output().await?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Walrus store command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let response_text = String::from_utf8(output.stdout)?;
        let response: StoreResponse = serde_json::from_str(&response_text)?;
        
        // Extract blob ID from either newly created or already certified
        let blob_id = if let Some(newly_created) = response.newly_created {
            newly_created.blob_object.blob_id
        } else if let Some(already_certified) = response.already_certified {
            already_certified.blob_object.blob_id
        } else {
            return Err(anyhow::anyhow!("No blob ID found in response"));
        };

        Ok(blob_id)
    }

    pub async fn read_blob(&self, blob_id: &str, output_path: &Path) -> Result<()> {
        let read_cmd = ReadCommand {
            config: self.config_path.clone(),
            command: ReadRequest {
                read: ReadParams {
                    blob_id: blob_id.to_string(),
                },
            },
        };

        let json_input = serde_json::to_string(&read_cmd)?;
        
        let mut child = Command::new("walrus")
            .args(&["json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(json_input.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let output = child.wait_with_output().await?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Walrus read command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Write the blob content to the output file
        tokio::fs::write(output_path, &output.stdout).await?;
        
        Ok(())
    }

    pub async fn store_bytes(&self, data: &[u8]) -> Result<String> {
        // Create a temporary file to store the data
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().join("temp_blob");
        tokio::fs::write(&temp_path, data).await?;
        
        self.store_file(&temp_path).await
    }

    pub async fn read_blob_to_writer(&self, blob_id: &str, mut writer: impl AsyncWrite + Unpin) -> Result<()> {
        let read_cmd = ReadCommand {
            config: self.config_path.clone(),
            command: ReadRequest {
                read: ReadParams {
                    blob_id: blob_id.to_string(),
                },
            },
        };

        let json_input = serde_json::to_string(&read_cmd)?;
        
        let mut child = Command::new("walrus")
            .args(&["json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(json_input.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let output = child.wait_with_output().await?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Walrus read command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        writer.write_all(&output.stdout).await?;
        
        Ok(())
    }
}

impl Default for WalrusClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn sha256_to_blob_id(sha256_str: &str) -> Result<String> {
    // For git-lfs compatibility, we use the SHA256 hash as the blob ID
    // Since Walrus generates its own blob IDs, we'll need to maintain a mapping
    // This is a simplified approach - in practice, you might want to use
    // the actual Walrus blob ID and maintain a separate mapping
    Ok(sha256_str.to_string())
}

pub fn client() -> WalrusClient {
    WalrusClient::default()
}