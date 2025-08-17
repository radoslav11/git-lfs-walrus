use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncWrite, AsyncWriteExt};
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
    #[serde(rename = "blobStoreResult")]
    blob_store_result: BlobStoreResult,
    // path: String,
}

#[derive(Debug, Deserialize)]
struct BlobStoreResult {
    #[serde(rename = "newlyCreated")]
    newly_created: Option<BlobResult>,
    #[serde(rename = "alreadyCertified")]
    already_certified: Option<BlobResult>,
}

#[derive(Debug, Deserialize)]
struct BlobResult {
    #[serde(rename = "blobObject")]
    blob_object: Option<BlobObject>,
    #[serde(rename = "blobId")]
    blob_id: Option<String>,
    // event: EventInfo,
    // #[serde(rename = "endEpoch")]
    // end_epoch: u64,
}

#[derive(Debug, Deserialize)]
struct EventInfo {
    // #[serde(rename = "txDigest")]
    // tx_digest: String,
    // #[serde(rename = "eventSeq")]
    // event_seq: String,
}

#[derive(Debug, Deserialize)]
struct BlobInfo {
    // #[serde(rename = "blobObject")]
    // blob_object: BlobObject,
    // #[serde(rename = "resourceOperation")]
    // resource_operation: ResourceOperation,
}

#[derive(Debug, Deserialize)]
struct BlobObject {
    #[serde(rename = "blobId")]
    blob_id: String,
    // id: String,
    // #[serde(rename = "storedEpoch")]
    // stored_epoch: u64,
    // size: u64,
    // #[serde(rename = "erasureCodeType")]
    // erasure_code_type: String,
    // #[serde(rename = "certifiedEpoch")]
    // certified_epoch: u64,
    // storage: Storage,
}

#[derive(Debug, Deserialize)]
struct Storage {
    // id: String,
    // #[serde(rename = "startEpoch")]
    // start_epoch: u64,
    // #[serde(rename = "endEpoch")]
    // end_epoch: u64,
    // #[serde(rename = "storageSize")]
    // storage_size: u64,
}

#[derive(Debug, Deserialize)]
struct ResourceOperation {
    // #[serde(rename = "RegisterFromScratch")]
    // register_from_scratch: Option<RegisterFromScratch>,
}

#[derive(Debug, Deserialize)]
struct RegisterFromScratch {
    // #[serde(rename = "encoded_length")]
    // encoded_length: u64,
    // epochs: u64,
}

pub struct WalrusClient {
    config_path: Option<String>,
    walrus_path: Option<PathBuf>,
    default_epochs: u64,
}

impl WalrusClient {
    pub fn new() -> Self {
        Self {
            config_path: None,
            walrus_path: None,
            default_epochs: Self::get_default_epochs(),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self {
            config_path: None,
            walrus_path: Some(path),
            default_epochs: Self::get_default_epochs(),
        }
    }

    fn get_default_epochs() -> u64 {
        // Try to get from git config, fall back to 50
        std::process::Command::new("git")
            .args(&["config", "--get", "lfs.walrus.defaultepochs"])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                } else {
                    None
                }
            })
            .unwrap_or(50)
    }

    // pub fn with_config(config_path: String) -> Self {
    //     Self {
    //         config_path: Some(config_path),
    //     }
    // }

    pub async fn store_file(&self, file_path: &Path) -> Result<String> {
        let store_cmd = StoreCommand {
            config: self.config_path.clone(),
            command: StoreRequest {
                store: StoreParams {
                    files: vec![file_path.to_string_lossy().to_string()],
                    epochs: Some(self.default_epochs),
                },
            },
        };

        let json_input = serde_json::to_string(&store_cmd)?;

        let mut child = Command::new(
            self.walrus_path
                .as_deref()
                .unwrap_or_else(|| "walrus".as_ref()),
        )
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
        let responses: Vec<StoreResponse> = serde_json::from_str(&response_text)?;

        if responses.is_empty() {
            return Err(anyhow::anyhow!("No response from Walrus store command"));
        }

        let response = &responses[0];

        // Extract blob ID from either newly created or already certified
        let blob_id = if let Some(newly_created) = &response.blob_store_result.newly_created {
            extract_blob_id_from_result(newly_created)?
        } else if let Some(already_certified) = &response.blob_store_result.already_certified {
            extract_blob_id_from_result(already_certified)?
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

        let mut child = Command::new(
            self.walrus_path
                .as_deref()
                .unwrap_or_else(|| "walrus".as_ref()),
        )
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

        // Parse the JSON response and decode the base64 blob
        let response_text = String::from_utf8(output.stdout)?;
        let blob_response: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(blob_base64) = blob_response.get("blob").and_then(|v| v.as_str()) {
            let blob_data = base64::engine::general_purpose::STANDARD.decode(blob_base64)?;
            tokio::fs::write(output_path, &blob_data).await?;
        } else {
            return Err(anyhow::anyhow!("No blob data found in Walrus response"));
        }

        Ok(())
    }

    pub async fn store_bytes(&self, data: &[u8]) -> Result<String> {
        // Create a temporary file to store the data
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().join("temp_blob");
        tokio::fs::write(&temp_path, data).await?;

        self.store_file(&temp_path).await
    }

    pub async fn store_bytes_dry_run(&self, data: &[u8]) -> Result<String> {
        // Create a temporary file to store the data
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().join("temp_blob");
        tokio::fs::write(&temp_path, data).await?;

        let mut cmd = Command::new(
            self.walrus_path
                .as_deref()
                .unwrap_or_else(|| "walrus".as_ref()),
        );
        cmd.args(&["store", "--dry-run", "--json", "--epochs", &self.default_epochs.to_string(), &temp_path.to_string_lossy()]);

        let output = cmd.output().await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Walrus store dry-run command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(String::from_utf8(output.stdout)?)
    }

    pub async fn read_blob_to_writer(
        &self,
        blob_id: &str,
        mut writer: impl AsyncWrite + Unpin,
    ) -> Result<()> {
        let read_cmd = ReadCommand {
            config: self.config_path.clone(),
            command: ReadRequest {
                read: ReadParams {
                    blob_id: blob_id.to_string(),
                },
            },
        };

        let json_input = serde_json::to_string(&read_cmd)?;

        let mut child = Command::new(
            self.walrus_path
                .as_deref()
                .unwrap_or_else(|| "walrus".as_ref()),
        )
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

        // Parse the JSON response and decode the base64 blob
        let response_text = String::from_utf8(output.stdout)?;
        let blob_response: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(blob_base64) = blob_response.get("blob").and_then(|v| v.as_str()) {
            let blob_data = base64::engine::general_purpose::STANDARD.decode(blob_base64)?;
            writer.write_all(&blob_data).await?;
        } else {
            return Err(anyhow::anyhow!("No blob data found in Walrus response"));
        }

        Ok(())
    }
}

impl Default for WalrusClient {
    fn default() -> Self {
        Self::new()
    }
}

// pub fn sha256_to_blob_id(sha256_str: &str) -> Result<String> {
//     // For git-lfs compatibility, we use the SHA256 hash as the blob ID
//     // Since Walrus generates its own blob IDs, we'll need to maintain a mapping
//     // This is a simplified approach - in practice, you might want to use
//     // the actual Walrus blob ID and maintain a separate mapping
//     Ok(sha256_str.to_string())
// }

fn extract_blob_id_from_result(result: &BlobResult) -> anyhow::Result<String> {
    // Try new format first (with blobObject)
    if let Some(blob_object) = &result.blob_object {
        return Ok(blob_object.blob_id.clone());
    }
    
    // Fall back to old format (direct blobId)
    if let Some(blob_id) = &result.blob_id {
        return Ok(blob_id.clone());
    }
    
    Err(anyhow::anyhow!("No blob ID found in result"))
}

impl WalrusClient {
    pub fn walrus_path(&self) -> Option<&PathBuf> {
        self.walrus_path.as_ref()
    }
}

pub fn client() -> WalrusClient {
    WalrusClient::default()
}
