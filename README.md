# git-lfs-walrus

<div align="center">
  <img src="logo.png" alt="git-lfs-walrus logo" width="200"/>
</div>

[Git LFS](https://git-lfs.github.com/) replaces large files with text pointers in Git repositories while storing the actual files on remote servers, keeping repos lightweight. Traditional Git LFS relies on centralized storage (GitHub LFS, etc.) with vendor lock-in and single points of failure. git-lfs-walrus provides a decentralized alternative using [Walrus](https://docs.wal.app/) distributed storage with erasure coding for redundancy, censorship resistance, and true data ownership.

*Based on the excellent [git-lfs-ipfs](https://github.com/sameer/git-lfs-ipfs) project by Sameer Puri.*

## Prerequisites

- [Walrus CLI](https://docs.wal.app/) installed and configured
- Git LFS installed (`git lfs install`)

## Installation

### Building

```bash
git clone <this-repository>
cd git-lfs-walrus/git-lfs-walrus-cli
cargo build --release
```

### Configuration

Set environment variables for easier configuration (adjust paths as needed):

```bash
# Set these in your shell profile (~/.bashrc, ~/.zshrc, etc.)
export WALRUS_CLI_PATH="/usr/local/bin/walrus"  # Or wherever walrus is installed
export GIT_LFS_WALRUS_CLI="${THIS_REPO}/target/release/git-lfs-walrus-cli"
export GIT_LFS_WALRUS_WRAPPER="${THIS_REPO}/clean_wrapper.sh" 
```

Add the custom transfer and extensions for Walrus to your `~/.gitconfig`:

```
[lfs]
	standalonetransferagent = walrus
[lfs "customtransfer.walrus"]
	path = git-lfs-walrus-cli
	args = --walrus-path ${WALRUS_CLI_PATH} transfer
	concurrent = true
	direction = both
[lfs "extension.walrus"]
    clean = ${GIT_LFS_WALRUS_WRAPPER} ${GIT_LFS_WALRUS_CLI} --walrus-path ${WALRUS_CLI_PATH} clean %f
    smudge = ${GIT_LFS_WALRUS_CLI} --walrus-path ${WALRUS_CLI_PATH} smudge %f
    priority = 0
```

## Usage

Use git LFS normally - all subsequent files added to LFS will be stored in Walrus.

```bash
git lfs track "*.bin"
git add large-file.bin
git commit -m "Add large file"
```

### Configuration Options

Set the default number of epochs for Walrus storage:

```bash
git config lfs.walrus.defaultepochs 25  # Defaults to 50 if not set
```

## How it works

- **Clean**: Stores files in Walrus and creates LFS pointer files with Walrus blob IDs
- **Smudge**: Retrieves original files from Walrus using blob IDs from LFS pointers  
- **Transfer**: Handles upload/download operations for LFS custom transfers

Files are stored using Walrus's decentralized blob storage with erasure coding for reliability.

## Testing

### Prerequisites for Testing

1. **Install Walrus CLI**: Follow the [Walrus documentation](https://docs.wal.app/) to install and configure the Walrus client
2. **Configure Walrus**: Ensure you have a valid Walrus configuration file and can run basic commands:
   ```bash
   walrus --help
   walrus list-blobs  # Should work without errors
   ```

### Integration Testing

An interactive test script is provided to demonstrate the functionality of the extension:

```bash
./integration_test.sh
``` 

#### Full Manual Setup

If you want to create your own test repository:

```bash
# Create a test repository
mkdir my-test-repo && cd my-test-repo
git init
git lfs install

# Configure git-lfs-walrus (using relative paths from parent directory)
git config lfs.standalonetransferagent walrus
git config lfs.customtransfer.walrus.path "../target/release/git-lfs-walrus-cli"
git config lfs.customtransfer.walrus.args "--walrus-path walrus transfer"
git config lfs.customtransfer.walrus.concurrent true
git config lfs.customtransfer.walrus.direction both
git config lfs.extension.walrus.clean "../clean_wrapper.sh ../target/release/git-lfs-walrus-cli --walrus-path walrus clean %f"
git config lfs.extension.walrus.smudge "../target/release/git-lfs-walrus-cli --walrus-path walrus smudge %f"
git config lfs.extension.walrus.priority 0

# Configure default epochs (optional)
git config lfs.walrus.defaultepochs 25

# Track large files
echo "*.bin filter=lfs diff=lfs merge=lfs -text" > .gitattributes
echo "large file content" > large-file.bin

# Add and commit (this will use Walrus)
git add .gitattributes large-file.bin
git commit -m "Add large file stored in Walrus"

# Check the blob
BLOB_ID=$(git show HEAD:large-file.bin | grep "ext-0-walrus" | cut -d' ' -f2 | cut -d':' -f2)
echo "Walrus Blob ID: $BLOB_ID"
   
# Check blob status in Walrus (when blob ID format is supported)
walrus blob-status --blob-id $BLOB_ID
```

### Additional Commands

Get the actual Walrus blob ID for a file:

```bash
git-lfs-walrus-cli walrus-blob-id file.txt          # Shows file SHA256 and Walrus blob ID
```

Check if your LFS files stored in Walrus have expired:

```bash
git-lfs-walrus-cli walrus-check                      # Check all LFS files
git-lfs-walrus-cli walrus-check file1.bin file2.bin  # Check specific files
```

Refresh expired files in Walrus:

```bash
git-lfs-walrus-cli walrus-refresh                   # Refresh all expired files
git-lfs-walrus-cli walrus-refresh file1.bin         # Refresh specific files
```

### Unit Tests

Run the unit tests (note that integration tests are ignored by default since they require Walrus):

```bash
cd git-lfs-walrus-cli
cargo test
````
