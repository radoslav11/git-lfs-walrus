# git-lfs-walrus

<div align="center">
  <img src="logo.png" alt="git-lfs-walrus logo" width="200"/>
</div>

A [git-lfs](https://git-lfs.github.com/) custom transfer & extension that stores large files with Walrus decentralized storage.

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

Add the custom transfer and extensions for Walrus to your `~/.gitconfig`:

```
[lfs]
	standalonetransferagent = walrus
[lfs "customtransfer.walrus"]
	path = git-lfs-walrus-cli
	args = transfer
	concurrent = true
	direction = both
[lfs "extension.walrus"]
    clean = git-lfs-walrus-cli clean %f
    smudge = git-lfs-walrus-cli smudge %f
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

### Additional Commands

Check if your LFS files stored in Walrus have expired:

```bash
git-lfs-walrus-cli walrus-check                    # Check all LFS files
git-lfs-walrus-cli walrus-check file1.bin file2.bin  # Check specific files
```

Refresh expired files in Walrus:

```bash
git-lfs-walrus-cli walrus-refresh                   # Refresh all expired files
git-lfs-walrus-cli walrus-refresh file1.bin         # Refresh specific files
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

### Unit Tests

Run the unit tests (note that integration tests are ignored by default since they require Walrus):

```bash
cd git-lfs-walrus-cli
cargo test
```

To run tests that require Walrus to be installed and configured:

```bash
cargo test -- --ignored
```

### Integration Testing

An interactive test script is provided to demonstrate the functionality of the extension:

```bash
./integration_test.sh
```

#### Quick Integration Test

1. **Build the project**:
   ```bash
   cargo build --release
   ```

2. **Manual CLI testing**:
   ```bash
   # Test basic argument parsing
   ./target/release/git-lfs-walrus-cli --help
   
   # Test new commands
   ./target/release/git-lfs-walrus-cli walrus-check
   ./target/release/git-lfs-walrus-cli walrus-refresh
   ```

3. **Git LFS integration test** (using the provided test-repo):
   ```bash
   cd test-repo
   
   # Configure default epochs (optional)
   git config lfs.walrus.defaultepochs 25
   
   # Add files (triggers Walrus storage)
   git add .gitattributes large_file.txt
   
   # Check status
   git status
   
   # Test commands
   ../target/release/git-lfs-walrus-cli walrus-check
   ```

#### Full Manual Setup

If you want to create your own test repository:

```bash
# Create a test repository
mkdir my-test-repo && cd my-test-repo
git init
git lfs install

# Configure git-lfs-walrus (adjust paths as needed)
git config lfs.standalonetransferagent walrus
git config lfs.customtransfer.walrus.path "/path/to/git-lfs-walrus-cli"
git config lfs.customtransfer.walrus.args "transfer"
git config lfs.customtransfer.walrus.concurrent true
git config lfs.customtransfer.walrus.direction both
git config lfs.extension.walrus.clean "/path/to/clean_wrapper.sh /path/to/git-lfs-walrus-cli --walrus-path /path/to/walrus clean %f"
git config lfs.extension.walrus.smudge "/path/to/git-lfs-walrus-cli --walrus-path /path/to/walrus smudge %f"
git config lfs.extension.walrus.priority 0

# Track large files
echo "*.bin filter=lfs diff=lfs merge=lfs -text" > .gitattributes
echo "large file content" > large-file.bin

# Add and commit (this will use Walrus)
git add .gitattributes large-file.bin
git commit -m "Add large file stored in Walrus"
```

### Troubleshooting Tests

- **Walrus CLI not found**: Ensure `walrus` is in your PATH
- **Configuration errors**: Check your Walrus config file is valid
- **Network issues**: Walrus requires network access to storage nodes
- **Permission errors**: Ensure you have write access to the working directory

### Demo Repository

You can create a demo repository to test the integration:

```bash
git clone <this-repository>
cd git-lfs-walrus
cargo build --release

# Follow the integration testing steps above
```