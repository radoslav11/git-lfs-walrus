use anyhow::Result;
use futures::StreamExt;
use git_lfs_spec::transfer::custom::Event;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::io::{stdin, stdout, BufReader};

use crate::{clean::clean, smudge::smudge, walrus::WalrusClient};

mod clean;
mod smudge;
mod transfer;
mod walrus;

#[derive(Debug, StructOpt)]
#[structopt(author, about)]
struct GitLfsWalrus {
    #[structopt(subcommand)]
    command: Command,

    #[structopt(long, env = "WALRUS_CLI_PATH")]
    walrus_path: Option<PathBuf>,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// git-lfs smudge filter extension for Walrus
    ///
    /// https://github.com/git-lfs/git-lfs/blob/main/docs/extensions.md#smudge
    Smudge {
        /// Name of the file
        _filename: PathBuf,
    },
    /// git-lfs clean filter extension for Walrus
    ///
    /// <https://github.com/git-lfs/git-lfs/blob/main/docs/extensions.md#clean>
    Clean {
        /// Name of the file
        _filename: PathBuf,
    },
    /// git-lfs custom transfer for Walrus
    ///
    /// <https://github.com/git-lfs/git-lfs/blob/main/docs/custom-transfers.md>
    Transfer,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = GitLfsWalrus::from_args();
    let client = if let Some(path) = args.walrus_path {
        WalrusClient::with_path(path)
    } else {
        crate::walrus::client()
    };

    match args.command {
        Command::Smudge { .. } => smudge(client, stdin(), stdout()).await,
        Command::Clean { .. } => clean(client, std::io::stdin(), stdout()).await,
        Command::Transfer => {
            let buffered_stdin = BufReader::new(stdin());
            let input_event_stream = transfer::read_events(buffered_stdin);
            let download_folder = std::env::current_dir()?;
            let output_event_stream =
                transfer::transfer(client, input_event_stream, download_folder);
            futures_util::pin_mut!(output_event_stream);
            while let Some(output_event) = output_event_stream.next().await.transpose()? {
                if Event::AcknowledgeInit == output_event {
                    println!("{{ }}");
                } else {
                    println!("{}", serde_json::to_string(&output_event)?);
                }
            }
            Ok(())
        }
    }
}
