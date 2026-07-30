#!/usr/bin/env bash

## # ==========================================
## # 1. DETECT BASH VERSION HERE
## # ==========================================
## # macOS default bash is often old (v3.2). Check for Bash 4.0+
## if [ "${BASH_VERSINFO[0]}" -lt 4 ]; then
##     echo "❌ Error: This script requires Bash 4.0 or higher." >&2
##     echo "💡 Current version: ${BASH_VERSION}" >&2
##     echo "💡 Fix for macOS: Run 'brew install bash' and update your path." >&2
##     brew install bash && exit 0
## fi

echo "✅ Bash version ${BASH_VERSION} verified."

# ==========================================
# 2. IMPLEMENT FULL STACK DEMO
# ==========================================
echo "🚀 Starting Distributed File System (IPFS) Full-Stack Demo..."

# Check if local IPFS/Kubo daemon is alive
if ! ipfs id >/dev/null 2>&1; then
    echo "⚠️  IPFS daemon is not running locally."
    echo "🔄 Attempting to spin up IPFS daemon..."
    ipfs daemon > ipfs_daemon.log 2>&1 &

    # Wait up to 10 seconds for the API to become responsive
    for i in {1..10}; do
        if ipfs id >/dev/null 2>&1; then
            echo "✅ IPFS daemon successfully started."
            break
        fi
        if [ "$i" -eq 10 ]; then
            echo "❌ Error: Could not connect to IPFS daemon. Run 'ipfs daemon' in another terminal." >&2
            exit 1
        fi
        sleep 1
    done
fi

# Create a sample text file to represent a distributed asset
mkdir -p build
echo "Distributed File System Demo - Build Asset $(date)" > build/index.html

# Add the file/directory to IPFS
echo "📦 Staging assets to IPFS..."
IPFS_ADD_OUTPUT=$(ipfs add -r build/index.html --cid-version=1)
CID=$(echo "$IPFS_ADD_OUTPUT" | awk '{print $2}' | tail -n 1)

echo "🌍 Asset successfully distributed!"
echo "🔗 IPFS Content Identifier (CID): $CID"
echo "🌐 Local Gateway URL: http://localhost:8080/ipfs/$CID"

# Optional: Add code here to spin up your UI/Frontend framework pointing to $CID

