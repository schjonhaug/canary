#!/bin/bash

# Bitcoin Regtest Development Utilities
# 
# This script provides a complete Bitcoin regtest development environment with:
# - Docker-based Bitcoin Core + Fulcrum Electrum server
# - Alice (funded with 100 BTC) and Bob (unfunded) wallet creation
# - Backend integration for Output Descriptor Monitor
# - Advanced Bitcoin transaction testing (RBF, CPFP, mempool operations)
#
# Key Commands:
#   start           - Start infrastructure (Bitcoin Core + Fulcrum)
#   create-wallets  - Create Alice (100 BTC) and Bob (unfunded) wallets
#   add-wallets-to-backend - Integrate wallets with backend API
#
# Workflow:
#   1. ./docker-utils.sh start
#   2. ./docker-utils.sh create-wallets  (Alice gets 100 BTC automatically)
#   3. cd ../backend && BITCOIN_NETWORK=regtest cargo run
#   4. cd ../regtest-env && ./docker-utils.sh add-wallets-to-backend

# Function to run bitcoin-cli against the Docker container  
btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

# Function to run bitcoin-cli with Alice wallet against the Docker container
btc_alice() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=alice "$@"
}

# Function to run bitcoin-cli with Bob wallet against the Docker container
btc_bob() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=bob "$@"
}

# Function to run bitcoin-cli with Miner wallet against the Docker container
btc_miner() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=miner "$@"
}


# Function to run bitcoin-cli with specific wallet (generic version)
btc_wallet() {
    local wallet_name=$1
    shift
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet="$wallet_name" "$@"
}

# --- CPFP logic as a function for both Alice and Bob ---
cpfp_for_wallet() {
    WALLET="$1"
    PARENT_TXID="$2"
    if [ -z "$PARENT_TXID" ]; then
        echo "Usage: $0 $WALLET cpfp <parent_txid>"
        exit 1
    fi
    btc_wallet() {
        local wallet_name=$1
        shift
        docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet="$wallet_name" "$@"
    }
    # Check if wallet exists
    if ! btc_wallet "$WALLET" getwalletinfo >/dev/null 2>&1; then
        echo "❌ $WALLET wallet not found. Run '$0 create-wallets' first"
        exit 1
    fi
    # Check wallet has sufficient confirmed funds for fees
    WALLET_BALANCE=$(btc_wallet "$WALLET" getbalance)
    echo "💰 $WALLET wallet balance: $WALLET_BALANCE BTC (confirmed)"
    if [ "$(echo "$WALLET_BALANCE < 0.001" | bc -l)" -eq 1 ]; then
        echo "❌ $WALLET needs confirmed funds for CPFP fees. Current balance: $WALLET_BALANCE BTC"
        echo "💡 Fund $WALLET first with: $0 $([[ "$WALLET" == "alice" ]] && echo "bob" || echo "alice") send 0.01 && $0 mine 1"
        exit 1
    fi
    # $WALLET creates high-fee child transaction (CPFP)
    echo "👶 Creating CPFP child transaction ($WALLET spends unconfirmed output)..."
    # Verify parent transaction is in wallet and unconfirmed
    PARENT_IN_WALLET=$(btc_wallet "$WALLET" gettransaction $PARENT_TXID 2>/dev/null || echo "not found")
    if [ "$PARENT_IN_WALLET" = "not found" ]; then
        echo "❌ Parent transaction not found in $WALLET wallet"
        exit 1
    fi
    PARENT_CONFIRMATIONS=$(echo "$PARENT_IN_WALLET" | jq -r '.confirmations')
    PARENT_AMOUNT=$(echo "$PARENT_IN_WALLET" | jq -r '.amount')
    if [ "$PARENT_CONFIRMATIONS" -gt 0 ]; then
        echo "❌ Parent transaction is already confirmed ($PARENT_CONFIRMATIONS confirmations)"
        echo "💡 CPFP only works on unconfirmed transactions"
        exit 1
    fi
    echo "   ✅ Parent transaction found in $WALLET wallet (unconfirmed)"
    echo "   💰 Parent amount: $PARENT_AMOUNT BTC"
    # Get the raw parent transaction to find outputs that belong to wallet
    PARENT_RAW=$(btc getrawtransaction $PARENT_TXID true)
    # Find which output(s) in the parent transaction belong to wallet
    WALLET_OUTPUTS=()
    TOTAL_WALLET_AMOUNT=0
    OUTPUT_COUNT=$(echo "$PARENT_RAW" | jq '.vout | length')
    for ((i=0; i<OUTPUT_COUNT; i++)); do
        OUTPUT_ADDRESS=$(echo "$PARENT_RAW" | jq -r ".vout[$i].scriptPubKey.address")
        OUTPUT_VALUE=$(echo "$PARENT_RAW" | jq -r ".vout[$i].value")
        # Check if this address belongs to wallet
        if btc_wallet "$WALLET" getaddressinfo "$OUTPUT_ADDRESS" 2>/dev/null | jq -r '.ismine' | grep -q "true"; then
            WALLET_OUTPUTS+=("$i:$OUTPUT_VALUE")
            TOTAL_WALLET_AMOUNT=$(echo "scale=8; $TOTAL_WALLET_AMOUNT + $OUTPUT_VALUE" | bc -l)
            echo "   📍 Found $WALLET's output $i: $OUTPUT_VALUE BTC at $OUTPUT_ADDRESS"
        fi
    done
    if [ ${#WALLET_OUTPUTS[@]} -eq 0 ]; then
        echo "❌ No outputs in parent transaction belong to $WALLET's wallet"
        exit 1
    fi
    # Calculate child amount (leave high fee for CPFP acceleration)
    CHILD_AMOUNT_RAW=$(echo "scale=8; $TOTAL_WALLET_AMOUNT - 0.005" | bc -l)  # 0.005 BTC fee
    CHILD_AMOUNT=$(echo "$CHILD_AMOUNT_RAW" | sed 's/^\.*$/0./')
    if [ "$(echo "$CHILD_AMOUNT < 0.0001" | bc -l)" -eq 1 ]; then
        echo "❌ Child amount too small: $CHILD_AMOUNT BTC (need at least 0.0001 BTC after fees)"
        exit 1
    fi
    # Get change address for the child transaction
    CHANGE_ADDRESS=$(btc_wallet "$WALLET" getnewaddress)
    echo "   🔍 Creating CPFP child spending $TOTAL_WALLET_AMOUNT BTC → $CHILD_AMOUNT BTC (0.005 BTC fee)"
    echo "   🎯 Target: $CHANGE_ADDRESS"
    # Create raw transaction inputs from wallet's outputs in the parent transaction
    INPUTS="["
    for i in "${!WALLET_OUTPUTS[@]}"; do
        OUTPUT_INDEX=$(echo "${WALLET_OUTPUTS[$i]}" | cut -d':' -f1)
        if [ $i -gt 0 ]; then
            INPUTS+="," 
        fi
        INPUTS+="{\"txid\":\"$PARENT_TXID\",\"vout\":$OUTPUT_INDEX}"
    done
    INPUTS+="]"
    # Create raw transaction output
    OUTPUTS="{\"$CHANGE_ADDRESS\":$CHILD_AMOUNT}"
    # Create the raw transaction that specifically spends from the unconfirmed parent
    echo "   🔧 Creating raw transaction..."
    RAW_TX=$(btc_wallet "$WALLET" createrawtransaction "$INPUTS" "$OUTPUTS")
    if [ -z "$RAW_TX" ]; then
        echo "❌ Failed to create raw transaction"
        exit 1
    fi
    # Sign the raw transaction
    echo "   ✍️  Signing transaction..."
    SIGNED_TX=$(btc_wallet "$WALLET" signrawtransactionwithwallet "$RAW_TX")
    SIGNED_HEX=$(echo "$SIGNED_TX" | jq -r '.hex')
    SIGN_COMPLETE=$(echo "$SIGNED_TX" | jq -r '.complete')
    if [ "$SIGN_COMPLETE" != "true" ]; then
        echo "❌ Failed to sign transaction"
        echo "Signing result: $SIGNED_TX"
        exit 1
    fi
    # Broadcast the child transaction
    echo "   📡 Broadcasting CPFP child transaction..."
    CHILD_TXID=$(btc sendrawtransaction "$SIGNED_HEX")
    if [ -z "$CHILD_TXID" ]; then
        echo "❌ Failed to create child transaction"
        echo "   $WALLET balance (confirmed): $(btc_wallet "$WALLET" getbalance)"
        echo "   $WALLET balance (unconfirmed): $(btc_wallet "$WALLET" getbalance "*" 0)"
        exit 1
    fi
    echo "   ✅ Child transaction created: $CHILD_TXID"
    echo "   💰 Amount: $CHILD_AMOUNT BTC (high fee: 0.005 BTC)"
    echo "   🎯 Target: $CHANGE_ADDRESS ($WALLET change address)"
    echo ""
    echo "🔗 CPFP Relationship Created:"
    echo "   👨 Parent: $PARENT_TXID ($WALLET → $WALLET, stuck due to low fee)"
    echo "   👶 Child:  $CHILD_TXID ($WALLET → $WALLET, high fee accelerates parent)"
    echo ""
    echo "📊 Current mempool status:"
    MEMPOOL_SIZE=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
    echo "   Transactions in mempool: $MEMPOOL_SIZE"
    echo ""
    echo "🔍 Transaction Details:"
    echo "Parent transaction ($WALLET wallet view):"
    btc_wallet "$WALLET" gettransaction $PARENT_TXID | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
    echo ""
    echo "Child transaction ($WALLET wallet view):"
    btc_wallet "$WALLET" gettransaction $CHILD_TXID | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
    echo ""
    echo "🎉 CPFP test scenario complete!"
    echo ""
    echo "📱 Check your application to see:"
    echo "   - Both transactions appear in mempool"
    echo "   - $WALLET's balance shows pending amounts"
    echo "   - CPFP relationship should be detected"
    echo ""
    echo "⛏️  Mine blocks to confirm both transactions:"
    echo "   $0 mine 1"
    echo ""
    echo "🔍 Both transactions should confirm together due to CPFP!"
}
# --- End CPFP logic function ---

# --- New multi-word command parsing for wallet actions ---
if [[ "$1" == "alice" || "$1" == "bob" ]]; then
    WALLET="$1"
    SUBCMD="$2"
    shift 2
    case "$SUBCMD" in
        send)
            AMOUNT="$1"
            if [ -z "$AMOUNT" ]; then
                echo "Usage: $0 $WALLET send <amount>"
                exit 1
            fi
            btc loadwallet "$WALLET" 2>/dev/null || true
            if [ "$WALLET" == "alice" ]; then
                btc loadwallet "bob" 2>/dev/null || true
                TARGET_ADDRESS=$(btc_bob getnewaddress)
                echo "🎯 Sending $AMOUNT BTC from Alice to Bob address: $TARGET_ADDRESS"
                TXID=$(btc_alice sendtoaddress "$TARGET_ADDRESS" "$AMOUNT")
            else
                btc loadwallet "alice" 2>/dev/null || true
                TARGET_ADDRESS=$(btc_alice getnewaddress)
                echo "🎯 Sending $AMOUNT BTC from Bob to Alice address: $TARGET_ADDRESS"
                TXID=$(btc_bob sendtoaddress "$TARGET_ADDRESS" "$AMOUNT")
            fi
            echo "✅ Transaction sent: $TXID"
            echo "💡 Use '$0 mine' to confirm transaction"
            exit 0
            ;;
        rbf)
            TXID="$1"
            FEE_RATE=${2:-10}
            if [ -z "$TXID" ]; then
                echo "Usage: $0 $WALLET rbf <txid> [fee_rate_sat_per_byte]"
                exit 1
            fi
            echo "🔄 Bumping fee for transaction $TXID to $FEE_RATE sat/byte..."
            btc loadwallet "$WALLET" 2>/dev/null || true
            if [ "$WALLET" == "alice" ]; then
                RESULT=$(btc_alice bumpfee "$TXID" "{\"fee_rate\": $FEE_RATE}" 2>&1)
            else
                RESULT=$(btc_bob bumpfee "$TXID" "{\"fee_rate\": $FEE_RATE}" 2>&1)
            fi
            if echo "$RESULT" | jq -e '.txid' > /dev/null 2>&1; then
                NEW_TXID=$(echo "$RESULT" | jq -r '.txid')
                OLD_FEE=$(echo "$RESULT" | jq -r '.origfee')
                NEW_FEE=$(echo "$RESULT" | jq -r '.fee')
                echo "✅ RBF replacement successful!"
                echo "   Original TXID: $TXID"
                echo "   New TXID: $NEW_TXID"
                echo "   Original fee: $OLD_FEE BTC"
                echo "   New fee: $NEW_FEE BTC"
                echo "💡 Use '$0 mine' to confirm when ready"
            else
                echo "❌ RBF failed: $RESULT"
                echo "💡 Common reasons:"
                echo "   - Transaction already confirmed"
                echo "   - Transaction was not RBF-enabled"
                echo "   - Fee rate not higher than original"
            fi
            exit 0
            ;;
        cpfp)
            PARENT_TXID="$1"
            cpfp_for_wallet "$WALLET" "$PARENT_TXID"
            exit 0
            ;;
        *)
            echo "Unknown subcommand for $WALLET: $SUBCMD"
            exit 1
            ;;
    esac
fi
# --- End new multi-word command parsing ---

case "$1" in
    "start")
        echo "Starting Bitcoin regtest environment with Docker..."
        docker-compose up -d
        
        # Wait for Bitcoin Core to be ready
        echo "Waiting for Bitcoin Core to start..."
        timeout=60
        while [ $timeout -gt 0 ]; do
            if docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 getblockchaininfo > /dev/null 2>&1; then
                echo "✅ Bitcoin Core is ready"
                break
            fi
            sleep 1
            timeout=$((timeout-1))
        done
        
        if [ $timeout -eq 0 ]; then
            echo "❌ Bitcoin Core failed to start within 60 seconds"
            echo "Checking logs..."
            docker logs bitcoind-regtest | tail -10
            exit 1
        fi
        
        # Wait for Fulcrum to be ready
        echo "Waiting for Fulcrum Electrum server to start..."
        timeout=120
        while [ $timeout -gt 0 ]; do
            if nc -z localhost 50001 2>/dev/null; then
                echo "✅ Fulcrum Electrum server is ready"
                break
            fi
            sleep 2
            timeout=$((timeout-2))
        done
        
        if [ $timeout -le 0 ]; then
            echo "⚠️  Fulcrum may still be starting (can take a few minutes on first run)"
            echo "Check logs with: ./docker-utils.sh logs fulcrum"
        fi
        
        echo ""
        echo "🚀 Bitcoin regtest environment is running!"
        echo "Bitcoin RPC: localhost:18443"
        echo "Fulcrum Electrum server: localhost:50001"
        echo "Set BITCOIN_NETWORK=regtest in your environment"
        echo ""
        echo "💡 Next: $0 create-wallets (creates Alice with 100 BTC)"
        ;;
    
    "create-wallets")
        echo "🏦 Setting up Alice, Bob and Miner wallets..."
        
        # Check if Bitcoin Core is running
        if ! btc getblockchaininfo > /dev/null 2>&1; then
            echo "❌ Bitcoin Core is not running. Run '$0 start' first."
            exit 1
        fi
        
        # Create Alice wallet (deterministic)
        echo "📋 Creating Alice wallet..."
        btc unloadwallet "alice" 2>/dev/null || true
        
        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="alice" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Alice wallet exists, loading..."
            btc loadwallet "alice" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Alice blank wallet created"
            
            # Import deterministic descriptors for Alice (regtest vprv keys)
            btc_alice importdescriptors '[
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj",
                "timestamp": "now",
                "active": true,
                "internal": false,
                "range": [0, 999]
              },
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2",
                "timestamp": "now",
                "active": true,
                "internal": true,
                "range": [0, 999]
              }
            ]' >/dev/null 2>&1
            echo "   ✅ Alice wallet seeded with deterministic descriptors"
        else
            echo "   ❌ Failed to create Alice wallet: $CREATE_RESULT"
            exit 1
        fi
        
        # Get Alice descriptor and convert to multipath format
        # This creates a descriptor compatible with the backend API requirement
        ALICE_DESCRIPTORS=$(btc_alice listdescriptors)
        ALICE_RECEIVE_DESC=$(echo "$ALICE_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        ALICE_MULTIPATH_RAW=$(echo "$ALICE_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        ALICE_CHECKSUM_INFO=$(btc getdescriptorinfo "$ALICE_MULTIPATH_RAW")
        ALICE_CHECKSUM=$(echo "$ALICE_CHECKSUM_INFO" | jq -r '.checksum')
        ALICE_DESCRIPTOR="$ALICE_MULTIPATH_RAW#$ALICE_CHECKSUM"
        
        # Create Bob wallet (deterministic)
        echo "📋 Creating Bob wallet..."
        btc unloadwallet "bob" 2>/dev/null || true
        
        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="bob" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Bob wallet exists, loading..."
            btc loadwallet "bob" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Bob blank wallet created"
            
            # Import deterministic descriptors for Bob (regtest vprv keys)
            btc_bob importdescriptors '[
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/0/*)#y872gtkp",
                "timestamp": "now",
                "active": true,
                "internal": false,
                "range": [0, 999]
              },
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/1/*)#4nmt47xe",
                "timestamp": "now",
                "active": true,
                "internal": true,
                "range": [0, 999]
              }
            ]' >/dev/null 2>&1
            echo "   ✅ Bob wallet seeded with deterministic descriptors"
        else
            echo "   ❌ Failed to create Bob wallet: $CREATE_RESULT"
            exit 1
        fi
        
        # Get Bob descriptor and address
        BOB_DESCRIPTORS=$(btc_wallet bob listdescriptors)
        BOB_RECEIVE_DESC=$(echo "$BOB_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        BOB_MULTIPATH_RAW=$(echo "$BOB_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        BOB_CHECKSUM_INFO=$(btc getdescriptorinfo "$BOB_MULTIPATH_RAW")
        BOB_CHECKSUM=$(echo "$BOB_CHECKSUM_INFO" | jq -r '.checksum')
        BOB_DESCRIPTOR="$BOB_MULTIPATH_RAW#$BOB_CHECKSUM"
        
        # Create Miner wallet
        echo "📋 Creating Miner wallet..."
        btc unloadwallet "miner" 2>/dev/null || true
        
        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="miner" disable_private_keys=false blank=false passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Miner wallet exists, loading..."
            btc loadwallet "miner" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Miner wallet created"
        else
            echo "   ❌ Failed to create Miner wallet: $CREATE_RESULT"
            exit 1
        fi
        
        # Get Miner descriptor and address (for background operations)
        MINER_DESCRIPTORS=$(btc_wallet miner listdescriptors)
        MINER_RECEIVE_DESC=$(echo "$MINER_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        MINER_MULTIPATH_RAW=$(echo "$MINER_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        MINER_CHECKSUM_INFO=$(btc getdescriptorinfo "$MINER_MULTIPATH_RAW")
        MINER_CHECKSUM=$(echo "$MINER_CHECKSUM_INFO" | jq -r '.checksum')
        MINER_DESCRIPTOR="$MINER_MULTIPATH_RAW#$MINER_CHECKSUM"
        # Get a fresh address for mining (only used for background operations)
        MINER_ADDRESS=$(btc_wallet miner getnewaddress)
        
        # Fund Alice wallet with simplified strategy
        echo "💰 Funding Alice wallet..."
        BLOCK_COUNT=$(btc getblockcount 2>/dev/null || echo "0")
        
        if [ "$BLOCK_COUNT" -lt 104 ]; then
            echo "   ⛏️  Mining blocks and transferring funds to Alice..."
            # Mine 103 blocks to Miner (150 BTC total)
            btc generatetoaddress 103 "$MINER_ADDRESS" >/dev/null 2>&1
            
            # Send 100 BTC from Miner to Alice (get fresh address for funding)
            ALICE_FUNDING_ADDRESS=$(btc_wallet alice getnewaddress)
            TXID=$(btc_miner sendtoaddress "$ALICE_FUNDING_ADDRESS" 100)
            
            # Mine 1 block to confirm Alice's transaction
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ Alice funded with 100 BTC (spendable)"
        else
            echo "   ✅ Alice already funded"
        fi
        
        # Show final balances
        ALICE_BALANCE=$(btc_wallet alice getbalance)
        echo "   💰 Alice balance: $ALICE_BALANCE BTC"
        
        echo ""
        echo "🎉 Alice and Bob wallets setup complete!"
        echo ""
        echo "📱 Add these descriptors to your wallet app to follow along:"
        echo "   👩 Alice Wallet (funded - 100 BTC):  $ALICE_DESCRIPTOR"
        echo "   👨 Bob Wallet (unfunded):            $BOB_DESCRIPTOR"
        echo ""
        echo "💡 Wallets are ready - addresses will be derived automatically by your backend"
        echo ""
        echo "💡 Next: $0 add-wallets-to-backend (requires backend running)"
        ;;
        
    "stop")
        echo "Stopping Bitcoin regtest environment..."
        docker-compose down
        echo "✅ Environment stopped"
        ;;
        
    "restart")
        echo "Restarting Bitcoin regtest environment..."
        docker-compose restart
        ;;
        
    "reset")
        echo "⚠️  This will stop containers and delete all blockchain data AND database data!"
        read -p "Are you sure? (y/N): " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            echo "Resetting environment..."
            
            # Remove wallets from backend if it's running
            echo "Removing wallets from backend..."
            if curl -s --connect-timeout 2 http://localhost:3001/wallets > /dev/null 2>&1; then
                ./docker-utils.sh remove-wallets-from-backend
            else
                echo "⚠️  Backend not running, skipping wallet removal"
            fi
            
            # Try to unload wallets before reset (if Bitcoin is running)
            if btc getblockchaininfo > /dev/null 2>&1; then
                echo "Unloading test wallets..."
                btc unloadwallet "alice" 2>/dev/null || true
                btc unloadwallet "bob" 2>/dev/null || true
                btc unloadwallet "miner" 2>/dev/null || true
            fi
            
            # Stop containers and remove all volumes (includes wallet data)
            docker-compose down -v
            
            # Clean up BDK wallet files to prevent stale cache
            echo "Cleaning up BDK wallet cache..."
            if [ -d "../backend/wallets" ]; then
                rm -rf ../backend/wallets/*.db
                echo "✅ BDK wallet cache cleared"
            else
                echo "⚠️  BDK wallets directory not found (this is normal for first run)"
            fi
            
            # Wipe database
            echo "Wiping database..."
            ./docker-utils.sh wipe-database
            
            echo "✅ Environment reset complete (all wallets, blockchain data, and database wiped)"
        else
            echo "Reset cancelled"
        fi
        ;;
        
    "logs")
        SERVICE=${2:-""}
        if [ -n "$SERVICE" ]; then
            docker-compose logs -f "$SERVICE"
        else
            docker-compose logs -f
        fi
        ;;
        
    "mine")
        BLOCKS=${2:-1}
        echo "Mining $BLOCKS block(s) to Miner wallet..."
        btc loadwallet "miner" 2>/dev/null || true
        ADDRESS=$(btc_miner getnewaddress)
        btc generatetoaddress "$BLOCKS" "$ADDRESS"
        echo "✅ Mined $BLOCKS block(s) to Miner wallet"
        ;;
    
    "invalidate-tip")
        echo "🔄 Invalidating tip block (simulating blockchain reorganization)..."
        
        # Get current tip block hash
        TIP_HASH=$(btc getbestblockhash)
        TIP_HEIGHT=$(btc getblockcount)
        
        echo "   Current tip: $TIP_HASH (height: $TIP_HEIGHT)"
        
        # Invalidate the tip block
        btc invalidateblock "$TIP_HASH"
        
        # Show new state
        NEW_TIP_HASH=$(btc getbestblockhash)
        NEW_TIP_HEIGHT=$(btc getblockcount)
        
        echo "   ✅ Block invalidated!"
        echo "   New tip: $NEW_TIP_HASH (height: $NEW_TIP_HEIGHT)"
        echo ""
        echo "📊 Effect:"
        echo "   - Blockchain reorganized (tip block removed)"
        echo "   - Transactions from invalidated block moved back to mempool"
        echo "   - Balance changes from that block are reverted"
        echo ""
        echo "💡 To restore the block:"
        echo "   $0 reconsider-block $TIP_HASH"
        echo ""
        echo "💡 To mine a new competing block:"
        echo "   $0 mine 1"
        ;;
    
    "reconsider-block")
        BLOCK_HASH="$2"
        if [ -z "$BLOCK_HASH" ]; then
            echo "Usage: $0 reconsider-block <block_hash>"
            echo "Example: $0 reconsider-block 1a2b3c4d5e6f..."
            exit 1
        fi
        
        echo "🔄 Reconsidering block $BLOCK_HASH..."
        
        # Get current state
        OLD_TIP_HEIGHT=$(btc getblockcount)
        
        # Reconsider the block
        btc reconsiderblock "$BLOCK_HASH"
        
        # Show new state
        NEW_TIP_HASH=$(btc getbestblockhash)
        NEW_TIP_HEIGHT=$(btc getblockcount)
        
        echo "   ✅ Block reconsidered!"
        echo "   New tip: $NEW_TIP_HASH (height: $NEW_TIP_HEIGHT)"
        echo ""
        if [ "$NEW_TIP_HEIGHT" -gt "$OLD_TIP_HEIGHT" ]; then
            echo "📊 Effect:"
            echo "   - Block was reactivated and became the new tip"
            echo "   - Blockchain height increased from $OLD_TIP_HEIGHT to $NEW_TIP_HEIGHT"
            echo "   - Transactions in the block are now confirmed again"
        else
            echo "📊 Effect:"
            echo "   - Block was reconsidered but did not become the new tip"
            echo "   - Another chain may have more work"
        fi
        ;;
    
    "alice-balance")
        btc loadwallet "alice" 2>/dev/null || true
        BALANCE=$(btc_alice getbalance)
        echo "Alice wallet balance: $BALANCE BTC"
        ;;
    
    "alice-address")
        btc loadwallet "alice" 2>/dev/null || true
        ADDRESS=$(btc_alice getnewaddress)
        echo "New Alice address: $ADDRESS"
        ;;
    
    "bob-balance")
        btc loadwallet "bob" 2>/dev/null || true
        BALANCE=$(btc_bob getbalance)
        echo "Bob wallet balance: $BALANCE BTC"
        ;;
    
    "bob-address")
        btc loadwallet "bob" 2>/dev/null || true
        ADDRESS=$(btc_bob getnewaddress)
        echo "New Bob address: $ADDRESS"
        ;;
    
    "alice-fund")
        if [ -z "$2" ]; then
            echo "Usage: $0 alice-fund <address> [amount=1.0]"
            exit 1
        fi
        AMOUNT=${3:-1.0}
        echo "Funding address $2 with $AMOUNT BTC from Alice..."
        btc loadwallet "alice" 2>/dev/null || true
        TXID=$(btc_alice sendtoaddress "$2" "$AMOUNT")
        echo "Transaction: $TXID"
        echo "💡 Use '$0 mine' to confirm transaction"
        ;;
    
    
    "mempool-status")
        echo "=== Mempool Status ==="
        if btc getblockchaininfo > /dev/null 2>&1; then
            btc loadwallet "alice" 2>/dev/null || true
            MEMPOOL_SIZE=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
            MEMPOOL_BYTES=$(btc getmempoolinfo | grep '"bytes"' | cut -d':' -f2 | tr -d ' ,')
            echo "Mempool transactions: $MEMPOOL_SIZE"
            echo "Mempool size: $MEMPOOL_BYTES bytes"
            if [ "$MEMPOOL_SIZE" -gt 0 ]; then
                echo "Pending transactions:"
                btc getrawmempool | grep -E '"[a-f0-9]{64}"' | head -5
                if [ "$MEMPOOL_SIZE" -gt 5 ]; then
                    echo "... and $((MEMPOOL_SIZE - 5)) more"
                fi
            else
                echo "No pending transactions"
            fi
        else
            echo "Bitcoin Core: ❌ Not running"
        fi
        ;;
    
    "cpfp-test")
        PARENT_TXID="$2"
        if [ -z "$PARENT_TXID" ]; then
            echo "Usage: $0 cpfp-test <parent_txid>"
            echo "Example: $0 cpfp-test abc123def456"
            echo ""
            echo "This creates a Child-Pays-For-Parent transaction where Bob spends"
            echo "an unconfirmed transaction output with a high fee to accelerate confirmation."
            exit 1
        fi
        
        echo "🧪 Running CPFP test scenario..."
        echo "   Parent TXID: $PARENT_TXID"
        echo "   Bob creates CPFP child transaction (high fee - accelerates parent)"
        echo ""
        
        # Check if wallets exist
        if ! btc_bob getwalletinfo >/dev/null 2>&1; then
            echo "❌ Bob wallet not found. Run '$0 create-wallets' first"
            exit 1
        fi
        
        # Check Bob has sufficient confirmed funds for fees
        BOB_BALANCE=$(btc_bob getbalance)
        echo "💰 Bob wallet balance: $BOB_BALANCE BTC (confirmed)"
        
        if [ "$(echo "$BOB_BALANCE < 0.001" | bc -l)" -eq 1 ]; then
            echo "❌ Bob needs confirmed funds for CPFP fees. Current balance: $BOB_BALANCE BTC"
            echo "💡 Fund Bob first with: $0 alice-send 0.01 && $0 mine 1"
            exit 1
        fi
        
        # Bob creates high-fee child transaction (CPFP)
        echo "👶 Creating CPFP child transaction (Bob spends unconfirmed output)..."
        
        # Verify parent transaction is in Bob's wallet and unconfirmed
        PARENT_IN_BOB_WALLET=$(btc_bob gettransaction $PARENT_TXID 2>/dev/null || echo "not found")
        if [ "$PARENT_IN_BOB_WALLET" = "not found" ]; then
            echo "❌ Parent transaction not found in Bob wallet"
            exit 1
        fi
        
        PARENT_CONFIRMATIONS=$(echo "$PARENT_IN_BOB_WALLET" | jq -r '.confirmations')
        PARENT_AMOUNT=$(echo "$PARENT_IN_BOB_WALLET" | jq -r '.amount')
        
        if [ "$PARENT_CONFIRMATIONS" -gt 0 ]; then
            echo "❌ Parent transaction is already confirmed ($PARENT_CONFIRMATIONS confirmations)"
            echo "💡 CPFP only works on unconfirmed transactions"
            exit 1
        fi
        
        echo "   ✅ Parent transaction found in Bob wallet (unconfirmed)"
        echo "   💰 Parent amount: $PARENT_AMOUNT BTC"
        
        # Get the raw parent transaction to find outputs that belong to Bob
        PARENT_RAW=$(btc getrawtransaction $PARENT_TXID true)
        
        # Find which output(s) in the parent transaction belong to Bob
        BOB_OUTPUTS=()
        TOTAL_BOB_AMOUNT=0
        
        # Check each output to see if it belongs to Bob
        OUTPUT_COUNT=$(echo "$PARENT_RAW" | jq '.vout | length')
        for ((i=0; i<OUTPUT_COUNT; i++)); do
            OUTPUT_ADDRESS=$(echo "$PARENT_RAW" | jq -r ".vout[$i].scriptPubKey.address")
            OUTPUT_VALUE=$(echo "$PARENT_RAW" | jq -r ".vout[$i].value")
            
            # Check if this address belongs to Bob's wallet
            if btc_bob getaddressinfo "$OUTPUT_ADDRESS" 2>/dev/null | jq -r '.ismine' | grep -q "true"; then
                BOB_OUTPUTS+=("$i:$OUTPUT_VALUE")
                TOTAL_BOB_AMOUNT=$(echo "scale=8; $TOTAL_BOB_AMOUNT + $OUTPUT_VALUE" | bc -l)
                echo "   📍 Found Bob's output $i: $OUTPUT_VALUE BTC at $OUTPUT_ADDRESS"
            fi
        done
        
        if [ ${#BOB_OUTPUTS[@]} -eq 0 ]; then
            echo "❌ No outputs in parent transaction belong to Bob's wallet"
            exit 1
        fi
        
        # Calculate child amount (leave high fee for CPFP acceleration)
        CHILD_AMOUNT_RAW=$(echo "scale=8; $TOTAL_BOB_AMOUNT - 0.005" | bc -l)  # 0.005 BTC fee
        CHILD_AMOUNT=$(echo "$CHILD_AMOUNT_RAW" | sed 's/^\.*/0./')
        
        # Check if child amount is too small
        if [ "$(echo "$CHILD_AMOUNT < 0.0001" | bc -l)" -eq 1 ]; then
            echo "❌ Child amount too small: $CHILD_AMOUNT BTC (need at least 0.0001 BTC after fees)"
            exit 1
        fi
        
        # Get Bob's change address for the child transaction
        BOB_CHANGE_ADDRESS=$(btc_bob getnewaddress)
        echo "   🔍 Creating CPFP child spending $TOTAL_BOB_AMOUNT BTC → $CHILD_AMOUNT BTC (0.005 BTC fee)"
        echo "   🎯 Target: $BOB_CHANGE_ADDRESS"
        
        # Create raw transaction inputs from Bob's outputs in the parent transaction
        INPUTS="["
        for i in "${!BOB_OUTPUTS[@]}"; do
            OUTPUT_INDEX=$(echo "${BOB_OUTPUTS[$i]}" | cut -d':' -f1)
            if [ $i -gt 0 ]; then
                INPUTS+=","
            fi
            INPUTS+="{\"txid\":\"$PARENT_TXID\",\"vout\":$OUTPUT_INDEX}"
        done
        INPUTS+="]"
        
        # Create raw transaction output
        OUTPUTS="{\"$BOB_CHANGE_ADDRESS\":$CHILD_AMOUNT}"
        
        # Create the raw transaction that specifically spends from the unconfirmed parent
        echo "   🔧 Creating raw transaction..."
        RAW_TX=$(btc_bob createrawtransaction "$INPUTS" "$OUTPUTS")
        
        if [ -z "$RAW_TX" ]; then
            echo "❌ Failed to create raw transaction"
            exit 1
        fi
        
        # Sign the raw transaction
        echo "   ✍️  Signing transaction..."
        SIGNED_TX=$(btc_bob signrawtransactionwithwallet "$RAW_TX")
        SIGNED_HEX=$(echo "$SIGNED_TX" | jq -r '.hex')
        SIGN_COMPLETE=$(echo "$SIGNED_TX" | jq -r '.complete')
        
        if [ "$SIGN_COMPLETE" != "true" ]; then
            echo "❌ Failed to sign transaction"
            echo "Signing result: $SIGNED_TX"
            exit 1
        fi
        
        # Broadcast the child transaction
        echo "   📡 Broadcasting CPFP child transaction..."
        CHILD_TXID=$(btc sendrawtransaction "$SIGNED_HEX")
        
        if [ -z "$CHILD_TXID" ]; then
            echo "❌ Failed to create child transaction"
            echo "   Bob balance (confirmed): $(btc_bob getbalance)"
            echo "   Bob balance (unconfirmed): $(btc_bob getbalance "*" 0)"
            exit 1
        fi
        
        echo "   ✅ Child transaction created: $CHILD_TXID"
        echo "   💰 Amount: $CHILD_AMOUNT BTC (high fee: 0.002 BTC)"
        echo "   🎯 Target: $BOB_CHANGE_ADDRESS (Bob change address)"
        echo ""
        
        # Show CPFP relationship
        echo "🔗 CPFP Relationship Created:"
        
        # Determine which wallet owns the parent transaction to show correct relationship
        PARENT_IN_ALICE_WALLET=$(btc_alice gettransaction $PARENT_TXID 2>/dev/null || echo "not found")
        if [ "$PARENT_IN_ALICE_WALLET" != "not found" ]; then
            echo "   👨 Parent: $PARENT_TXID (Alice → Bob, stuck due to low fee)"
            PARENT_OWNER="alice"
        else
            echo "   👨 Parent: $PARENT_TXID (Bob → Bob, stuck due to low fee)"
            PARENT_OWNER="bob"
        fi
        echo "   👶 Child:  $CHILD_TXID (Bob → Bob, high fee accelerates parent)"
        echo ""
        
        # Show mempool status
        echo "📊 Current mempool status:"
        MEMPOOL_SIZE=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
        echo "   Transactions in mempool: $MEMPOOL_SIZE"
        echo ""
        
        # Show transaction details
        echo "🔍 Transaction Details:"
        if [ "$PARENT_OWNER" = "alice" ]; then
            echo "Parent transaction (Alice wallet view):"
            btc_alice gettransaction $PARENT_TXID | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
        else
            echo "Parent transaction (Bob wallet view):"
            btc_bob gettransaction $PARENT_TXID | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
        fi
        echo ""
        echo "Child transaction (Bob wallet view):"
        btc_bob gettransaction $CHILD_TXID | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
        echo ""
        
        echo "🎉 CPFP test scenario complete!"
        echo ""
        echo "📱 Check your application to see:"
        echo "   - Both transactions appear in mempool"
        echo "   - Bob's balance shows pending amounts"
        echo "   - CPFP relationship should be detected"
        echo ""
        echo "⛏️  Mine blocks to confirm both transactions:"
        echo "   $0 mine 1"
        echo ""
        echo "🔍 Both transactions should confirm together due to CPFP!"
        ;;
    
    
    "status")
        echo "=== Bitcoin regtest Status ==="
        if btc getblockchaininfo > /dev/null 2>&1; then
            echo "Bitcoin Core: ✅ Running"
            echo "Block count: $(btc getblockcount)"
            btc loadwallet "alice" 2>/dev/null || true
            btc loadwallet "bob" 2>/dev/null || true
            btc loadwallet "miner" 2>/dev/null || true
            echo "Alice wallet balance: $(btc_alice getbalance) BTC (funded for testing)"
            echo "Bob wallet balance: $(btc_bob getbalance) BTC (unfunded)"
            echo "Miner wallet balance: $(btc_miner getbalance) BTC (background infrastructure)"
            echo "Network: $(btc getblockchaininfo | grep '"chain"' | cut -d'"' -f4)"
            
            # Add mempool info to status
            MEMPOOL_SIZE=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
            echo "Mempool transactions: $MEMPOOL_SIZE"
        else
            echo "Bitcoin Core: ❌ Not running"
        fi
        
        echo ""
        echo "=== Fulcrum Status ==="
        if nc -z localhost 50001 2>/dev/null; then
            echo "Fulcrum Electrum server: ✅ Running on port 50001"
        else
            echo "Fulcrum Electrum server: ❌ Not ready (may still be starting)"
        fi
        
        echo ""
        echo "=== Docker Containers ==="
        docker-compose ps
        ;;
    
    "add-wallets-to-backend")
        BACKEND_URL=${2:-"http://localhost:3001"}
        echo "Adding Alice and Bob wallets to backend at $BACKEND_URL..."
        
        # Check if backend is running
        echo "🔍 Checking if backend is running..."
        if ! curl -s --connect-timeout 5 --max-time 10 "$BACKEND_URL/wallets" > /dev/null 2>&1; then
            echo "❌ Backend is not running at $BACKEND_URL"
            echo ""
            echo "💡 To start the backend:"
            echo "   1. Open a new terminal"
            echo "   2. cd ../backend"
            echo "   3. BITCOIN_NETWORK=regtest cargo run"
            echo ""
            echo "   Then run this command again."
            exit 1
        fi
        echo "✅ Backend is running"
        
        # Get the descriptors from the Bitcoin wallets
        echo "📋 Getting wallet descriptors..."
        btc loadwallet "alice" 2>/dev/null || true
        btc loadwallet "bob" 2>/dev/null || true
        
        # Get Alice descriptor and convert to multipath format
        # 1. Get raw descriptor from Bitcoin Core wallet  
        # 2. Convert /0/* to /<0;1>/* for multipath support
        # 3. Remove old checksum (new one calculated by backend)
        # Note: Skip getdescriptorinfo as it reverts multipath format to single-path
        ALICE_DESCRIPTORS=$(btc_wallet alice listdescriptors)
        ALICE_RECEIVE_DESC=$(echo "$ALICE_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        ALICE_MULTIPATH_RAW=$(echo "$ALICE_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        ALICE_CHECKSUM_INFO=$(btc getdescriptorinfo "$ALICE_MULTIPATH_RAW")
        ALICE_CHECKSUM=$(echo "$ALICE_CHECKSUM_INFO" | jq -r '.checksum')
        ALICE_DESCRIPTOR="$ALICE_MULTIPATH_RAW#$ALICE_CHECKSUM"
        
        # Get Bob descriptor and convert to multipath format (same process as Alice)
        BOB_DESCRIPTORS=$(btc_wallet bob listdescriptors)
        BOB_RECEIVE_DESC=$(echo "$BOB_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        BOB_MULTIPATH_RAW=$(echo "$BOB_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        BOB_CHECKSUM_INFO=$(btc getdescriptorinfo "$BOB_MULTIPATH_RAW")
        BOB_CHECKSUM=$(echo "$BOB_CHECKSUM_INFO" | jq -r '.checksum')
        BOB_DESCRIPTOR="$BOB_MULTIPATH_RAW#$BOB_CHECKSUM"
        
        echo "👩 Alice descriptor: $ALICE_DESCRIPTOR"
        echo "👨 Bob descriptor: $BOB_DESCRIPTOR"
        
        # Add Alice wallet to backend
        echo "📤 Adding Alice wallet to backend..."
        ALICE_RESPONSE=$(curl -s -X POST "$BACKEND_URL/wallets" \
            -H "Content-Type: application/json" \
            -d "{\"name\":\"Alice (Regtest)\",\"output_descriptor\":\"$ALICE_DESCRIPTOR\",\"gap_limit\":5}")
        
        if echo "$ALICE_RESPONSE" | jq -e '.id' > /dev/null 2>&1; then
            ALICE_ID=$(echo "$ALICE_RESPONSE" | jq -r '.id')
            echo "✅ Alice wallet added with ID: $ALICE_ID"
            ALICE_SUCCESS=true
        else
            echo "❌ Failed to add Alice wallet"
            if echo "$ALICE_RESPONSE" | jq -e '.error' > /dev/null 2>&1; then
                ERROR_MSG=$(echo "$ALICE_RESPONSE" | jq -r '.error')
                echo "   Error: $ERROR_MSG"
            else
                echo "   Response: $ALICE_RESPONSE"
            fi
            ALICE_SUCCESS=false
        fi
        
        # Add Bob wallet to backend
        echo "📤 Adding Bob wallet to backend..."
        BOB_RESPONSE=$(curl -s -X POST "$BACKEND_URL/wallets" \
            -H "Content-Type: application/json" \
            -d "{\"name\":\"Bob (Regtest)\",\"output_descriptor\":\"$BOB_DESCRIPTOR\",\"gap_limit\":5}")
        
        if echo "$BOB_RESPONSE" | jq -e '.id' > /dev/null 2>&1; then
            BOB_ID=$(echo "$BOB_RESPONSE" | jq -r '.id')
            echo "✅ Bob wallet added with ID: $BOB_ID"
            BOB_SUCCESS=true
        else
            echo "❌ Failed to add Bob wallet"
            if echo "$BOB_RESPONSE" | jq -e '.error' > /dev/null 2>&1; then
                ERROR_MSG=$(echo "$BOB_RESPONSE" | jq -r '.error')
                echo "   Error: $ERROR_MSG"
            else
                echo "   Response: $BOB_RESPONSE"
            fi
            BOB_SUCCESS=false
        fi
        
        echo ""
        if [ "$ALICE_SUCCESS" = true ] && [ "$BOB_SUCCESS" = true ]; then
            echo "🎉 Both Alice and Bob wallets have been added to the backend!"
            echo "Check your frontend at http://localhost:3000 to see them."
        elif [ "$ALICE_SUCCESS" = true ] || [ "$BOB_SUCCESS" = true ]; then
            echo "⚠️  Some wallets were added successfully, but there were errors."
            echo "Check your frontend at http://localhost:3000 to see what was added."
        else
            echo "❌ Failed to add wallets to the backend."
            echo "Please check the backend logs and try again."
        fi
        ;;
    
    "remove-wallets-from-backend")
        BACKEND_URL=${2:-"http://localhost:3001"}
        echo "Removing regtest wallets from backend at $BACKEND_URL..."
        
        # Get all wallets from backend
        WALLETS_RESPONSE=$(curl -s "$BACKEND_URL/wallets")
        
        if echo "$WALLETS_RESPONSE" | jq -e '.wallets' > /dev/null 2>&1; then
            # Find and delete Alice, Bob and Miner wallets
            echo "$WALLETS_RESPONSE" | jq -r '.wallets[] | select(.name | test("Alice.*Regtest|Bob.*Regtest|Miner.*Regtest")) | .id' | while read -r wallet_id; do
                if [ -n "$wallet_id" ]; then
                    echo "🗑️  Deleting wallet $wallet_id..."
                    DELETE_RESPONSE=$(curl -s -X DELETE "$BACKEND_URL/wallets/$wallet_id")
                    if echo "$DELETE_RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
                        echo "✅ Wallet $wallet_id deleted successfully"
                    else
                        echo "❌ Failed to delete wallet $wallet_id: $DELETE_RESPONSE"
                    fi
                fi
            done
            echo "🎉 Regtest wallets removed from backend!"
        else
            echo "❌ Failed to get wallets from backend: $WALLETS_RESPONSE"
        fi
        ;;
        
    "wipe-database")
        echo "🗑️  Wiping database..."
        
        # Get database name from environment or use default
        DATABASE_URL=${DATABASE_URL:-"postgresql://localhost/output_descriptor_monitor"}
        DB_NAME=$(echo "$DATABASE_URL" | sed -n 's|.*postgresql://[^/]*/\([^?]*\).*|\1|p')
        if [ -z "$DB_NAME" ]; then
            DB_NAME="output_descriptor_monitor"
        fi
        
        echo "Database: $DB_NAME"
        echo "Dropping all tables..."
        
        # Drop all tables in the correct order (respecting foreign key constraints)
        if psql -d "$DB_NAME" -c "
            DROP TABLE IF EXISTS notifications CASCADE;
            DROP TABLE IF EXISTS address_utxos CASCADE;
            DROP TABLE IF EXISTS transaction_relationships CASCADE; 
            DROP TABLE IF EXISTS transactions CASCADE;
            DROP TABLE IF EXISTS addresses CASCADE;
            DROP TABLE IF EXISTS wallets CASCADE;
            DROP TABLE IF EXISTS _sqlx_migrations CASCADE;
        " 2>/dev/null; then
            echo "✅ Database tables dropped successfully"
            echo "💡 The database will be recreated with fresh tables when the backend starts"
        else
            echo "❌ Could not connect to database"
            echo "   Make sure PostgreSQL is running and the database '$DB_NAME' exists"
            echo "   You may need to wipe the database manually with:"
            echo "   psql -d $DB_NAME -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;'"
        fi
        ;;
        
    *)
        echo "Bitcoin regtest Docker development utilities"
        echo ""
        echo "Usage: $0 <command> [args...]"
        echo ""
        echo "Environment Commands:"
        echo "  start               Start Bitcoin + Electrum containers"
        echo "  create-wallets      Create Alice and Bob wallets (run after start)"
        echo "  stop                Stop all containers"
        echo "  restart             Restart all containers"  
        echo "  reset               Stop containers and delete all data (includes database)"
        echo "  wipe-database       Drop all database tables (standalone command)"
        echo "  logs [service]      Show logs (bitcoin/electrum or all)"
        echo "  status              Show environment status"
        echo ""
        echo "Alice Commands (funded wallet - 100 BTC):"
        echo "  alice balance             Show Alice wallet balance"
        echo "  alice address             Generate new Alice address"
        echo "  alice send <amt>          Send Bitcoin from Alice to Bob (RBF-enabled)"
        echo "  alice fund <addr> [amt]   Fund address from Alice (default: 1.0)"
        echo "  alice rbf <txid> [rate]   Replace transaction with higher fee (default: 10 sat/byte)"
        echo "  alice cpfp <txid>         Create CPFP child transaction for Alice's unconfirmed output"
        echo ""
        echo "Bob Commands (unfunded wallet):"
        echo "  bob balance               Show Bob wallet balance"
        echo "  bob address               Generate new Bob address"
        echo "  bob send <amt>            Send Bitcoin from Bob to Alice (RBF-enabled)"
        echo "  bob rbf <txid> [rate]     Replace transaction with higher fee (default: 10 sat/byte)"
        echo "  bob cpfp <txid>           Create CPFP child transaction for Bob's unconfirmed output"
        echo ""
        echo "Mining Commands:"
        echo "  mine [blocks]           Mine blocks to Miner wallet (default: 1)"
        echo "  invalidate-tip          Invalidate tip block (simulate blockchain reorg)"
        echo "  reconsider-block <hash> Reconsider invalidated block"
        echo ""
        echo "Mempool Commands:"
        echo "  mempool-status          Show mempool transaction count and details"
        echo "  cpfp-test <txid>        Create CPFP child transaction for given parent txid"
        echo ""
        echo "Backend Integration:"
        echo "  add-wallets-to-backend [url]    Add Alice/Bob wallets to backend (default: http://localhost:3001)"
        echo "  remove-wallets-from-backend [url] Remove regtest wallets from backend"
        echo ""
        echo "Examples:"
        echo "  $0 start                        # Start the environment"
        echo "  $0 create-wallets               # Create Alice/Bob wallets (Alice gets 100 BTC)"
        echo "  $0 add-wallets-to-backend       # Add Alice/Bob to your backend"
        echo "  $0 mine 6                       # Mine 6 blocks"
        echo "  $0 alice send 0.5               # Send 0.5 BTC from Alice to Bob (RBF-enabled)"
        echo "  $0 alice rbf <txid> 15          # Replace transaction with 15 sat/byte fee (Alice)"
        echo "  $0 bob send 0.01                # Send 0.01 BTC from Bob to Alice"
        echo "  $0 bob rbf <txid> 20            # Replace transaction with 20 sat/byte fee (Bob)"
        echo "  $0 alice cpfp <txid>            # Alice creates CPFP child for parent transaction"
        echo "  $0 bob cpfp <txid>              # Bob creates CPFP child for parent transaction"
        echo "  $0 mine 1                       # Mine 1 block (confirms pending transactions)"
        echo "  $0 mempool-status               # Check mempool"
        echo "  $0 invalidate-tip               # Invalidate tip block (test blockchain reorg)"
        echo "  $0 reset                        # Reset everything (includes backend cleanup)"
        ;;
esac