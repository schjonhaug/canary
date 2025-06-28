#!/bin/bash

# Comprehensive Test Runner for Mempool Features
# Usage: ./run-all-tests.sh [wallet_address]

set -e

WALLET_ADDRESS=${1:-}

echo "🧪 Output Descriptor Monitor - Mempool Features Test Suite"
echo "========================================================"

if [ -z "$WALLET_ADDRESS" ]; then
    echo "⚠️  No wallet address provided. You'll need to:"
    echo "   1. Start your application"
    echo "   2. Add a test wallet"
    echo "   3. Get an address from that wallet"
    echo "   4. Run: $0 <wallet_address>"
    echo ""
    echo "Example: $0 bcrt1qtest123456789abcdef"
    exit 1
fi

# Function to run bitcoin-cli with wallet
btc_wallet() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=default "$@"
}

# Function to pause between tests
pause_test() {
    echo ""
    echo "⏸️  Pausing for 5 seconds to observe changes..."
    echo "   Check your application for updates!"
    sleep 5
    echo ""
}

echo "🚀 Starting comprehensive test suite with address: $WALLET_ADDRESS"
echo ""

# Test 1: Basic mempool transaction
echo "📍 TEST 1: Basic Mempool Transaction"
echo "-----------------------------------"
./docker-utils.sh alice-fund $WALLET_ADDRESS 0.001
pause_test

# Test 2: RBF Testing
echo "📍 TEST 2: RBF (Replace-By-Fee)"
echo "-------------------------------"
echo "Creating low-fee transaction for RBF testing..."
FIRST_TXID=$(btc_wallet sendtoaddress $WALLET_ADDRESS 0.002 "" "" false true 0.00001 "unset")
echo "First transaction: $FIRST_TXID"
sleep 2

echo "Attempting RBF replacement with bumpfee..."
# Use new bumpfee shortcut
./docker-utils.sh alice-bumpfee $FIRST_TXID 15 || echo "RBF failed (transaction may be confirmed)"
pause_test

# Test 3: CPFP Testing
echo "📍 TEST 3: CPFP (Child-Pays-For-Parent)"
echo "---------------------------------------"
echo "Creating low-fee parent transaction..."
PARENT_TXID=$(btc_wallet sendtoaddress $WALLET_ADDRESS 0.003 "" "" false true 0.00001 "unset")
echo "Parent transaction: $PARENT_TXID"
sleep 2

echo "Creating CPFP child transaction..."
CHILD_ADDRESS=$(btc_wallet getnewaddress)
./test-cpfp.sh $PARENT_TXID $CHILD_ADDRESS 0.001
pause_test

# Test 4: Mempool Purge Testing
echo "📍 TEST 4: Mempool Purge (Node Restart)"
echo "---------------------------------------"
echo "Creating transaction to be purged..."
./docker-utils.sh alice-fund $WALLET_ADDRESS 0.001
sleep 2

echo "Testing mempool purge via restart..."
./test-mempool-purge.sh restart
pause_test

# Test 5: Blockchain Reorganization
echo "📍 TEST 5: Blockchain Reorganization"
echo "------------------------------------"
echo "Creating transaction for reorg testing..."
./docker-utils.sh alice-fund $WALLET_ADDRESS 0.004
sleep 2

echo "Mining blocks to confirm transaction..."
./docker-utils.sh mine 3
sleep 2

echo "Testing blockchain reorganization..."
./test-reorg.sh 2
pause_test

# Test 6: Confirmation Testing
echo "📍 TEST 6: Transaction Confirmation"
echo "-----------------------------------"
echo "Creating final test transaction..."
./docker-utils.sh alice-fund $WALLET_ADDRESS 0.005
sleep 2

echo "Confirming transaction..."
./docker-utils.sh mine 1
pause_test

# Final Status
echo "📍 FINAL STATUS"
echo "==============="
echo "All tests completed! Your application should now have examples of:"
echo ""
echo "✅ Basic mempool transactions"
echo "✅ RBF (Replace-By-Fee) relationships"
echo "✅ CPFP (Child-Pays-For-Parent) chains"
echo "✅ Mempool purge scenarios"
echo "✅ Blockchain reorganizations"
echo "✅ Transaction confirmations"
echo ""
echo "🔍 Check your application and database for:"
echo "   - Transaction state changes"
echo "   - RBF and CPFP relationships"
echo "   - Proper balance calculations"
echo "   - Real-time SSE updates"
echo ""

# Show final blockchain and mempool state
echo "📊 Final blockchain state:"
docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 getblockchaininfo | jq '.blocks, .bestblockhash'

echo ""
echo "📊 Final mempool state:"
docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 getmempoolinfo

echo ""
echo "🎉 Test suite completed successfully!"
echo "Monitor your backend logs and application for all the changes!"