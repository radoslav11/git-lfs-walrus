use anyhow::{Context, Result};
use futures::{Stream, StreamExt};
use std::{io::Write, path::Path};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use git_lfs_spec::transfer::custom::{self, Complete, Error, Event, Operation, Progress};
use crate::walrus::WalrusClient;

pub fn read_events(input: impl AsyncBufRead + Unpin) -> impl Stream<Item = Result<Event>> {
    async_stream::stream! {
        let mut lines = input.lines();
        while let Some(line) = lines.next_line().await? {
            let parsed = serde_json::from_str(&line).context("could not parse JSON");
            yield parsed
        }
    }
}

const INTERNAL_SERVER_ERROR: i32 = 500;

pub fn transfer(
    client: WalrusClient,
    input_event_stream: impl Stream<Item = Result<Event>>,
    download_folder: impl AsRef<Path>,
) -> impl Stream<Item = Result<Event>> {
    let mut init_opt = None;
    async_stream::stream! {
        futures_util::pin_mut!(input_event_stream);
        while let Some(event) = input_event_stream.next().await.transpose()? {
            match (init_opt.as_ref(), event) {
                (None, Event::Init(init)) => {
                    init_opt = Some(init);
                    yield Ok(Event::AcknowledgeInit)
                }
                (None, event) => {
                    yield Err(anyhow::anyhow!("Unexpected event: {:?}", event))
                }
                (Some(_), Event::Init(init)) => {
                    yield Err(anyhow::anyhow!("Unexpected init event: {:?}", init))
                }

                (Some(_), Event::Terminate) => {
                    break
                }
                (Some(init), event) => {
                    match (event, &init.operation) {
                        (Event::Download(download), Operation::Download) => {
                            // For download, we need to extract the Walrus blob ID from the OID
                            // In practice, this would require a mapping between SHA256 and Walrus blob IDs
                            // For now, we'll assume the OID contains the blob ID or we have a way to resolve it
                            let blob_id = &download.object.oid; // Simplified - in reality needs mapping
                            
                            match download_blob(&client, blob_id, &download_folder).await {
                                Ok((output_path, bytes_downloaded)) => {
                                    yield Ok(Event::Progress(
                                        Progress {
                                            oid: download.object.oid.clone(),
                                            bytes_so_far: bytes_downloaded,
                                            bytes_since_last: bytes_downloaded,
                                        }
                                        .into()
                                    ));
                                    
                                    yield Ok(Event::Complete(
                                        Complete {
                                            oid: download.object.oid.clone(),
                                            result: Some(custom::Result::Path(output_path)),
                                        }
                                        .into(),
                                    ));
                                }
                                Err(err) => {
                                    yield Ok(Event::Complete(
                                        Complete {
                                            oid: download.object.oid.clone(),
                                            result: Some(custom::Result::Error(Error {
                                                code: INTERNAL_SERVER_ERROR,
                                                message: err.to_string(),
                                            })),
                                        }
                                        .into(),
                                    ))
                                }
                            }
                        }
                        // Upload transfer - store file in Walrus
                        (Event::Upload(upload), Operation::Upload) => {
                            match upload_blob(&client, &upload.path).await {
                                Ok(_blob_id) => {
                                    yield Ok(Event::Complete(
                                        Complete {
                                            oid: upload.object.oid.clone(),
                                            result: None,
                                        }
                                        .into(),
                                    ))
                                }
                                Err(err) => {
                                    yield Ok(Event::Complete(
                                        Complete {
                                            oid: upload.object.oid.clone(),
                                            result: Some(custom::Result::Error(Error {
                                                code: INTERNAL_SERVER_ERROR,
                                                message: err.to_string(),
                                            })),
                                        }
                                        .into(),
                                    ))
                                }
                            }
                        }
                        (event, _) => {
                            yield Err(anyhow::anyhow!("Unexpected event: {:?}", event))
                        }
                    };
                }
            }
        }
    }
}

async fn download_blob(
    client: &WalrusClient,
    blob_id: &str,
    download_folder: impl AsRef<Path>,
) -> Result<(std::path::PathBuf, u64)> {
    let output_path = download_folder.as_ref().join(blob_id);
    
    // Download the blob from Walrus
    client.read_blob(blob_id, &output_path).await?;
    
    // Get the file size for progress reporting
    let metadata = tokio::fs::metadata(&output_path).await?;
    let bytes_downloaded = metadata.len();
    
    Ok((output_path, bytes_downloaded))
}

async fn upload_blob(
    client: &WalrusClient,
    file_path: &std::path::Path,
) -> Result<String> {
    // Store the file in Walrus
    let blob_id = client.store_file(file_path).await?;
    Ok(blob_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walrus::client;
    use git_lfs_spec::{
        transfer::custom::{Download, Event, Init, Result, Upload},
        Object,
    };
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    const FILE: &[u8] = b"hello world";
    const OID: &str = "test-blob-id-123";
    const SIZE: u64 = FILE.len() as u64;

    #[tokio::test]
    async fn read_events_parses_event_successfully() {
        let init = Event::Init(Init {
            operation: Operation::Download,
            remote: "origin".to_string(),
            concurrent: true,
            concurrenttransfers: Some(3),
        });
        let input: &[u8] = br#"{"event":"init","operation":"download","remote":"origin","concurrent":true,"concurrenttransfers":3}"#;
        let stream = read_events(input);
        futures::pin_mut!(stream);
        let mut events = vec![];
        while let Some(output) = stream.next().await {
            events.push(output.unwrap());
        }
        assert_eq!(events, &[init]);
    }

    #[tokio::test]
    #[ignore] // Requires Walrus to be installed and configured
    async fn transfer_handles_upload_events() {
        let temp_dir = tempdir().unwrap();
        let temp_file = temp_dir.path().join(OID);
        tokio::fs::write(&temp_file, FILE).await.unwrap();

        let client = client();
        let input_events = [
            Event::Init(Init {
                operation: Operation::Upload,
                remote: "origin".to_string(),
                concurrent: true,
                concurrenttransfers: Some(3),
            }),
            Event::Upload(
                Upload {
                    object: Object {
                        oid: OID.to_string(),
                        size: SIZE,
                    },
                    path: temp_file.clone(),
                }
                .into(),
            ),
            Event::Terminate,
        ];
        
        let output_stream = transfer(
            client,
            futures::stream::iter(input_events.iter().cloned().map(anyhow::Result::Ok)),
            temp_dir.path(),
        );
        
        futures_util::pin_mut!(output_stream);
        
        // Collect events and verify structure
        let mut events = vec![];
        while let Some(event) = output_stream.next().await {
            events.push(event.unwrap());
        }
        
        assert!(matches!(events[0], Event::AcknowledgeInit));
        assert!(matches!(events[1], Event::Complete(_)));
    }
}