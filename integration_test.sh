#!/bin/bash

# Exit on error
set -e

# Find the walrus CLI
WALRUS_PATH=$(which walrus)
if [ -z "$WALRUS_PATH" ]; then
    echo "walrus CLI not found. Please install it and make sure it's in your PATH."
    exit 1
fi

# Check if walrus is configured
if ! $WALRUS_PATH list-blobs >/dev/null 2>&1; then
    echo "walrus CLI is not configured correctly. Please run \"walrus setup\" and try again."
    exit 1
fi

# Enable verbose logging
export GIT_TRACE=1
export GIT_CURL_VERBOSE=1

# Build the project
echo "Building the project..."
./target/release/git-lfs-walrus-cli --help >/dev/null || cargo build --release

# Create a test repository
echo "Creating a test repository..."
rm -rf test-repo
mkdir test-repo && cd test-repo

# Initialize git and git-lfs
echo "Initializing git and git-lfs..."
git init
git lfs install

# Configure git-lfs-walrus
WALRUS_CLI_PATH=$(realpath ../target/release/git-lfs-walrus-cli)
CLEAN_WRAPPER_PATH=$(realpath ../clean_wrapper.sh)
echo "Configuring git-lfs-walrus with path: $WALRUS_CLI_PATH"
git config lfs.standalonetransferagent walrus
git config lfs.customtransfer.walrus.path "$WALRUS_CLI_PATH"
_args="--walrus-path $WALRUS_PATH transfer"
git config lfs.customtransfer.walrus.args "$_args"
git config lfs.customtransfer.walrus.concurrent true
git config lfs.customtransfer.walrus.direction both
_clean_args="--walrus-path $WALRUS_PATH clean %f"
git config lfs.extension.walrus.clean "$CLEAN_WRAPPER_PATH $WALRUS_CLI_PATH $_clean_args"
_smudge_args="--walrus-path $WALRUS_PATH smudge %f"
git config lfs.extension.walrus.smudge "$WALRUS_CLI_PATH $_smudge_args"
git config lfs.extension.walrus.priority 0

# Track a file type
echo "Tracking .txt files with git-lfs..."
git lfs track "*.txt"

# Create a large file
echo "Creating a large file..."
echo "This is a large file that should be stored in Walrus." > large_file.txt

# Add and commit the file
echo "Adding and committing the file..."
git add .gitattributes large_file.txt
git commit -m "Add large file stored in Walrus"

# Test retrieval
echo "Testing retrieval from Walrus..."
rm large_file.txt
git checkout large_file.txt

# Verify the file content
echo "Verifying the file content..."
if [ "$(cat large_file.txt)" = "This is a large file that should be stored in Walrus." ]; then
    echo "File content is correct."
else
    echo "File content is incorrect!"
    exit 1
fi

# Clean up
echo "Cleaning up..."
cd ..
# rm -rf test-repo

echo "Integration test completed successfully!"
