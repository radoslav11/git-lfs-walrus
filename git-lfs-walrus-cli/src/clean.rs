use std::io::Read;

use anyhow::Result;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use sha2::{Digest, Sha256};

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
    
    // Store the data in Walrus
    let blob_id = client.store_bytes(&data).await?;
    
    // For git-lfs compatibility, we output the original file content
    // The mapping between SHA256 and Walrus blob ID would need to be maintained separately
    // For now, we'll store the blob ID as a comment in the file content
    let lfs_pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\n# walrus-blob-id: {}\n",
        sha256_hex,
        data.len(),
        blob_id
    );
    
    output.write_all(lfs_pointer.as_bytes()).await?;
    
    Ok(())
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