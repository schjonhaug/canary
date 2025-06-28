#!/bin/bash

# Helper script to extract TXID from mempool
# Usage: ./get-mempool-txid.sh [index]
# index: which transaction to get (default: 0 for first)

INDEX=${1:-0}

# Function to run bitcoin-cli
btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

# Get mempool transactions as array
MEMPOOL_TXIDS=$(btc getrawmempool)

# Check if mempool is empty
if [ "$MEMPOOL_TXIDS" = "[]" ]; then
    echo "Error: Mempool is empty" >&2
    exit 1
fi

# Extract the specified transaction (default to first)
TXID=$(echo "$MEMPOOL_TXIDS" | jq -r ".[$INDEX] // empty")

if [ -z "$TXID" ] || [ "$TXID" = "null" ]; then
    echo "Error: No transaction found at index $INDEX" >&2
    echo "Available transactions:" >&2
    echo "$MEMPOOL_TXIDS" | jq -r '.[]' | nl -v0 >&2
    exit 1
fi

echo "$TXID"