#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p $PWD/distributed-fs-demo
DEMO_DIR="$PWD/distributed-fs-demo"
echo $DEMO_DIR
LOG_DIR="$DEMO_DIR/logs"
DEMO_INPUT="$DEMO_DIR/sample"
PIDS=()
TAIL_PIDS=()
LAST_LOG_FILE=""

cleanup() {
    local pid
    for pid in "${PIDS[@]:-}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done

    for pid in "${TAIL_PIDS[@]:-}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done

    #if [[ "${KEEP_DEMO_ARTIFACTS:-0}" != "1" ]]; then
    #    rm -rf "$DEMO_DIR"
    #else
    #    echo "Demo artifacts kept at: $DEMO_DIR"
    #fi
}

trap cleanup EXIT

wait_for_log() {
    local log_file="$1"
    local needle="$2"
    local timeout="${3:-90}"
    local elapsed=0

    while [[ "$elapsed" -lt "$timeout" ]]; do
        if grep -q "$needle" "$log_file" 2>/dev/null; then
            return 0
        fi

        for pid in "${PIDS[@]:-}"; do
            if ! kill -0 "$pid" 2>/dev/null; then
                echo "A demo process exited early."
                echo "--- $log_file ---"
                cat "$log_file" 2>/dev/null || true
                exit 1
            fi
        done

        sleep 1
        elapsed=$((elapsed + 1))
    done

    echo "Timed out waiting for: $needle"
    echo "--- $log_file ---"
    cat "$log_file" 2>/dev/null || true
    exit 1
}

start_node() {
    local name="$1"
    shift
    local log_file="$LOG_DIR/$name.log"
    mkdir -p "$LOG_DIR"
    : > "$log_file"

    (
        tail -n 0 -F "$log_file" | while IFS= read -r line; do
            printf '[%s] %s\n' "$name" "$line"
        done
    ) &
    TAIL_PIDS+=("$!")

    (
        cd "$ROOT_DIR"
        "$@"
    ) >"$log_file" 2>&1 &

    PIDS+=("$!")
    LAST_LOG_FILE="$log_file"
}

echo "Building workspace..."
(
    cd "$ROOT_DIR"
    cargo build --quiet
)

mkdir -p "$DEMO_INPUT/nested"
cat > "$DEMO_INPUT/README.txt" <<'EOF'
Distributed FS demo payload
EOF
cat > "$DEMO_INPUT/nested/data.txt" <<'EOF'
hello from the decentralized demo
EOF

echo "Starting demo nodes..."
start_node api cargo run --quiet --bin gnostr-p2p -- --role api --addr 127.0.0.1
API_LOG="$LAST_LOG_FILE"
start_node storage-a cargo run --quiet --bin gnostr-p2p -- --role storage --addr 127.0.0.1
STORAGE_A_LOG="$LAST_LOG_FILE"
start_node storage-b cargo run --quiet --bin gnostr-p2p -- --role storage --addr 127.0.0.1
STORAGE_B_LOG="$LAST_LOG_FILE"

wait_for_log "$API_LOG" "Listening on"
wait_for_log "$STORAGE_A_LOG" "Listening on"
wait_for_log "$STORAGE_B_LOG" "Listening on"

echo "Allowing peer discovery to stabilize..."
sleep "${DEMO_SETTLE_SECONDS:-10}"

echo "Uploading sample content..."
UPLOAD_OUTPUT="$(
    cd "$ROOT_DIR"
    DEMO_EXIT_AFTER_UPLOAD=1 cargo run --quiet --bin client -- --upload "$DEMO_INPUT"
)"
echo "$UPLOAD_OUTPUT"

LOCATION="$(printf '%s\n' "$UPLOAD_OUTPUT" | sed -n 's/^UPLOAD_OK location=\([^ ]*\) signature=.*$/\1/p' | tail -n 1)"
SIGNATURE="$(printf '%s\n' "$UPLOAD_OUTPUT" | sed -n 's/^UPLOAD_OK location=[^ ]* signature=\(.*\)$/\1/p' | tail -n 1)"

if [[ -z "$LOCATION" || -z "$SIGNATURE" ]]; then
    echo "Failed to parse upload output." >&2
    exit 1
fi

echo "Downloading sample content..."
(
    cd "$ROOT_DIR"
    cargo run --quiet --bin client -- --download "$LOCATION" "$SIGNATURE"
)

echo "Demo completed successfully."
