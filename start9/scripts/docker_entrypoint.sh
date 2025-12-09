#!/bin/bash
set -e

echo "Starting Canary Bitcoin Wallet Manager for StartOS..."

# Configuration from environment (set by Start9)
CANARY_DATA_DIR="${CANARY_DATA_DIR:-/app/data}"
CANARY_NETWORK="${CANARY_NETWORK:-mainnet}"
CANARY_BIND_ADDRESS="${CANARY_BIND_ADDRESS:-0.0.0.0:3000}"

# Determine Electrum URL
# Check if we should use local Electrs (Start9 dependency)
if [ -f /mnt/electrs/start9/stats.yaml ]; then
    # Get Electrs connection info from Start9
    ELECTRS_HOST="electrs.embassy"
    ELECTRS_PORT="50001"
    CANARY_ELECTRUM_URL="${CANARY_ELECTRUM_URL:-tcp://${ELECTRS_HOST}:${ELECTRS_PORT}}"
    echo "Using local Electrs server"
else
    # Fall back to external Electrum server
    CANARY_ELECTRUM_URL="${CANARY_ELECTRUM_URL:-ssl://electrum.blockstream.info:50002}"
    echo "Using external Electrum server"
fi

# Ensure data directory exists with proper structure
mkdir -p "${CANARY_DATA_DIR}/mainnet"
mkdir -p "${CANARY_DATA_DIR}/testnet"
mkdir -p "${CANARY_DATA_DIR}/regtest"

# Export environment for backend
export CANARY_MODE=self-hosted
export CANARY_DATA_DIR
export CANARY_NETWORK
export CANARY_ELECTRUM_URL
export CANARY_BIND_ADDRESS

# Export environment for frontend
export NEXT_PUBLIC_CANARY_MODE=self-hosted
export NEXT_PUBLIC_API_URL="http://localhost:3000"
export NODE_ENV=production
export PORT=3001
export HOSTNAME=0.0.0.0

echo "========================================"
echo "Canary Configuration:"
echo "  Mode: self-hosted (StartOS)"
echo "  Network: ${CANARY_NETWORK}"
echo "  Electrum: ${CANARY_ELECTRUM_URL}"
echo "  Backend: ${CANARY_BIND_ADDRESS}"
echo "  Data directory: ${CANARY_DATA_DIR}"
echo "========================================"

# Start backend in background
echo "Starting backend API server..."
cd /app/backend
./canary &
BACKEND_PID=$!

# Wait for backend to be ready
echo "Waiting for backend to be ready..."
for i in {1..60}; do
    if curl -s http://localhost:3000/api/block-headers/current > /dev/null 2>&1; then
        echo "Backend is ready!"
        break
    fi
    if [ $i -eq 60 ]; then
        echo "Warning: Backend not responding after 60 seconds, continuing anyway..."
    fi
    sleep 1
done

# Start frontend
echo "Starting frontend server..."
cd /app/frontend
node server.js &
FRONTEND_PID=$!

echo "========================================"
echo "Canary is running!"
echo "  Backend API: http://localhost:3000"
echo "  Frontend UI: http://localhost:3001"
echo "========================================"

# Handle shutdown gracefully
shutdown() {
    echo "Shutting down Canary..."
    kill $FRONTEND_PID 2>/dev/null || true
    kill $BACKEND_PID 2>/dev/null || true
    exit 0
}

trap shutdown SIGTERM SIGINT

# Wait for any process to exit
wait -n $BACKEND_PID $FRONTEND_PID
EXIT_CODE=$?

echo "A process exited with code $EXIT_CODE, shutting down..."
shutdown
