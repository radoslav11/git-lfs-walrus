# git-lfs-walrus

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

1. **Build the project**:
   ```bash
   cargo build --release
   ```

2. **Manual CLI testing**:
   ```bash
   # Test storing a file
   echo "hello world" > test.txt
   ./target/release/git-lfs-walrus-cli clean test.txt < test.txt
   
   # Test basic argument parsing
   ./target/release/git-lfs-walrus-cli --help
   ```

3. **Git LFS integration test**:
   ```bash
   # Create a test repository
   mkdir test-repo && cd test-repo
   git init
   git lfs install
   
   # Configure git-lfs-walrus (make sure the path is correct)
   git config lfs.standalonetransferagent walrus
   git config lfs.customtransfer.walrus.path "/path/to/git-lfs-walrus-cli"
   git config lfs.customtransfer.walrus.args "transfer"
   git config lfs.customtransfer.walrus.concurrent true
   git config lfs.customtransfer.walrus.direction both
   git config lfs.extension.walrus.clean "git-lfs-walrus-cli clean %f"
   git config lfs.extension.walrus.smudge "git-lfs-walrus-cli smudge %f"
   git config lfs.extension.walrus.priority 0
   
   # Track large files
   git lfs track "*.bin"
   echo "large file content" > large-file.bin
   
   # Add and commit (this will use Walrus)
   git add .gitattributes large-file.bin
   git commit -m "Add large file stored in Walrus"
   
   # Test retrieval
   rm large-file.bin
   git checkout large-file.bin  # Should retrieve from Walrus
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