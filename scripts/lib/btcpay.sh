cmd_btcpay_setup() {
    local btcpay="http://localhost:14142"
    local i user_response user_http_code user_body apikey_response apikey_http_code apikey_body api_key store_response store_http_code store_body store_id wallet_response wallet_http_code wallet_body offering_response offering_id plan_response plan_id backend_env

    require_tools jq sed
    echo "Waiting for BTCPay Server to be ready..."
    for i in $(seq 1 60); do
        if curl -sf "$btcpay/api/v1/health" > /dev/null 2>&1; then
            echo "✅ BTCPay Server is ready"
            break
        fi
        if [ "$i" -eq 60 ]; then
            echo "❌ BTCPay Server did not become ready in time"
            echo "   Check logs with: docker-compose logs btcpayserver"
            exit 1
        fi
        sleep 2
    done

    sleep 3
    echo "Creating admin user..."
    user_response=$(curl -s -w "\n%{http_code}" -X POST "$btcpay/api/v1/users" \
        -H "Content-Type: application/json" \
        -d '{"email":"admin@test.com","password":"password123","isAdministrator":true}')
    user_http_code=$(echo "$user_response" | tail -1)
    user_body=$(echo "$user_response" | sed '$d')
    if [ "$user_http_code" -ge 200 ] && [ "$user_http_code" -lt 300 ]; then
        echo "✅ Admin user created"
    elif [ "$user_http_code" -eq 422 ]; then
        echo "✅ Admin user already exists, continuing..."
    else
        echo "❌ Failed to create admin user (HTTP $user_http_code)"
        echo "   Response: $user_body"
        exit 1
    fi

    echo "Creating API key..."
    apikey_response=$(curl -s -w "\n%{http_code}" -X POST "$btcpay/api/v1/api-keys" \
        -H "Content-Type: application/json" \
        -u "admin@test.com:password123" \
        -d '{"permissions":["unrestricted"]}')
    apikey_http_code=$(echo "$apikey_response" | tail -1)
    apikey_body=$(echo "$apikey_response" | sed '$d')
    if [ "$apikey_http_code" -lt 200 ] || [ "$apikey_http_code" -ge 300 ]; then
        echo "❌ Failed to create API key (HTTP $apikey_http_code)"
        echo "   Response: $apikey_body"
        exit 1
    fi
    api_key=$(echo "$apikey_body" | jq -r '.apiKey')
    echo "✅ API key created"

    echo "Creating store..."
    store_response=$(curl -s -w "\n%{http_code}" -X POST "$btcpay/api/v1/stores" \
        -H "Content-Type: application/json" \
        -H "Authorization: token $api_key" \
        -d '{"name":"Canary Wallet Dev Store"}')
    store_http_code=$(echo "$store_response" | tail -1)
    store_body=$(echo "$store_response" | sed '$d')
    if [ "$store_http_code" -lt 200 ] || [ "$store_http_code" -ge 300 ]; then
        echo "❌ Failed to create store (HTTP $store_http_code)"
        echo "   Response: $store_body"
        exit 1
    fi
    store_id=$(echo "$store_body" | jq -r '.id')
    echo "✅ Store created: $store_id"

    echo "Generating store wallet..."
    wallet_response=$(curl -s -w "\n%{http_code}" -X POST "$btcpay/api/v1/stores/$store_id/payment-methods/BTC-CHAIN/wallet/generate" \
        -H "Content-Type: application/json" \
        -H "Authorization: token $api_key" \
        -d '{"savePrivateKeys":true,"scriptPubKeyType":"Segwit"}')
    wallet_http_code=$(echo "$wallet_response" | tail -1)
    wallet_body=$(echo "$wallet_response" | sed '$d')
    if [ "$wallet_http_code" -lt 200 ] || [ "$wallet_http_code" -ge 300 ]; then
        echo "❌ Failed to generate store wallet (HTTP $wallet_http_code)"
        echo "   Response: $wallet_body"
        exit 1
    fi
    echo "✅ Store wallet generated"

    echo "Creating subscription offering..."
    offering_response=$(curl -sf -X POST "$btcpay/api/v1/stores/$store_id/offerings" \
        -H "Content-Type: application/json" \
        -H "Authorization: token $api_key" \
        -d '{"appName":"Canary Wallet Donations"}')
    if [ $? -ne 0 ]; then
        echo "⚠️  Failed to create offering (subscription API may not be available in this BTCPay version)"
        echo "   One-time donations will still work. Recurring donations require manual BTCPay setup."
        offering_id=""
        plan_id=""
    else
        offering_id=$(echo "$offering_response" | jq -r '.id')
        echo "✅ Offering created: $offering_id"
        echo "Creating subscription plan..."
        plan_response=$(curl -sf -X POST "$btcpay/api/v1/stores/$store_id/offerings/$offering_id/plans" \
            -H "Content-Type: application/json" \
            -H "Authorization: token $api_key" \
            -d '{"name":"Monthly Supporter","currency":"USD","price":"5","recurringType":"Monthly"}')
        if [ $? -ne 0 ]; then
            echo "⚠️  Failed to create plan"
            plan_id=""
        else
            plan_id=$(echo "$plan_response" | jq -r '.id')
            echo "✅ Plan created: $plan_id"
        fi
    fi

    # Development-only credentials for the local BTCPay container.
    backend_env="../backend/.env"
    if [ -f "$backend_env" ]; then
        sed_in_place '/^BTCPAY_/d' "$backend_env"
        sed_in_place '/^# BTCPay Server (auto-configured/d' "$backend_env"
        echo "" >> "$backend_env"
        echo "# BTCPay Server (auto-configured by dev.sh btcpay-setup)" >> "$backend_env"
        echo "BTCPAY_URL=http://localhost:14142" >> "$backend_env"
        echo "BTCPAY_API_KEY=$api_key" >> "$backend_env"
        echo "BTCPAY_STORE_ID=$store_id" >> "$backend_env"
        if [ -n "$offering_id" ]; then
            echo "BTCPAY_OFFERING_ID=$offering_id" >> "$backend_env"
        fi
        if [ -n "$plan_id" ]; then
            echo "BTCPAY_PLAN_ID=$plan_id" >> "$backend_env"
        fi
        echo "✅ BTCPay config written to $backend_env"
    else
        echo "⚠️  $backend_env not found — printing env vars instead:"
        echo ""
        echo "BTCPAY_URL=http://localhost:14142"
        echo "BTCPAY_API_KEY=$api_key"
        echo "BTCPAY_STORE_ID=$store_id"
        if [ -n "$offering_id" ]; then
            echo "BTCPAY_OFFERING_ID=$offering_id"
        fi
        if [ -n "$plan_id" ]; then
            echo "BTCPAY_PLAN_ID=$plan_id"
        fi
    fi

    echo ""
    echo "BTCPay admin UI: http://localhost:14142 (admin@test.com / password123)"
}
