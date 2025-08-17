use anyhow::Result;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::collections::HashMap;

use crate::walrus::WalrusClient;

pub async fn smudge(
    client: WalrusClient,
    mut input: impl AsyncRead + Unpin,
    mut output: impl AsyncWrite + Unpin,
) -> Result<()> {
    // Read the LFS pointer content
    let mut pointer_content = String::new();
    input.read_to_string(&mut pointer_content).await?;
    
    // Parse the LFS pointer to extract metadata
    let _metadata = parse_lfs_pointer(&pointer_content)?;
    
    // Extract the Walrus blob ID from the comment
    let blob_id = extract_walrus_blob_id(&pointer_content)?;
    
    // Retrieve the original file content from Walrus
    client.read_blob_to_writer(&blob_id, &mut output).await?;
    
    Ok(())
}

fn parse_lfs_pointer(content: &str) -> Result<HashMap<String, String>> {
    let mut metadata = HashMap::new();
    
    for line in content.lines() {
        if line.starts_with('#') {
            continue; // Skip comments
        }
        
        if let Some((key, value)) = line.split_once(' ') {
            metadata.insert(key.to_string(), value.to_string());
        }
    }
    
    Ok(metadata)
}

fn extract_walrus_blob_id(content: &str) -> Result<String> {
    for line in content.lines() {
        if line.starts_with("# walrus-blob-id:") {
            if let Some((_, blob_id)) = line.split_once(": ") {
                return Ok(blob_id.trim().to_string());
            }
        }
    }
    
    Err(anyhow::anyhow!("No Walrus blob ID found in LFS pointer"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walrus::client;
    use std::io::Cursor;

    const LFS_POINTER: &str = r#"version https://git-lfs.github.com/spec/v1
oid sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
size 11
# walrus-blob-id: test-blob-id-123
"#;

    #[test]
    fn parse_lfs_pointer_extracts_metadata() {
        let metadata = parse_lfs_pointer(LFS_POINTER).unwrap();
        assert_eq!(metadata.get("version"), Some(&"https://git-lfs.github.com/spec/v1".to_string()));
        assert_eq!(metadata.get("oid"), Some(&"sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9".to_string()));
        assert_eq!(metadata.get("size"), Some(&"11".to_string()));
    }

    #[test]
    fn extract_walrus_blob_id_finds_id() {
        let blob_id = extract_walrus_blob_id(LFS_POINTER).unwrap();
        assert_eq!(blob_id, "test-blob-id-123");
    }

    #[tokio::test]
    #[ignore] // Requires Walrus to be installed and configured
    async fn smudge_converts_lfs_pointer_to_file_contents() {
        let client = client();
        let mut cursor = Cursor::new(vec![]);
        smudge(client, LFS_POINTER.as_bytes(), &mut cursor).await.unwrap();
        
        // This test would need a valid blob ID that exists in Walrus
        // For now, we just verify the parsing works
    }
}