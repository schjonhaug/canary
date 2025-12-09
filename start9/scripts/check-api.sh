#!/bin/bash

# Health check script for Start9 - Backend API
# Output format: JSON as required by Start9

BACKEND_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/api/block-headers/current 2>/dev/null)

if [ "$BACKEND_STATUS" = "200" ] || [ "$BACKEND_STATUS" = "204" ]; then
    echo '{"result": "The Canary API is responding"}'
    exit 0
else
    echo '{"error": "Backend API not responding (status: '"$BACKEND_STATUS"')"}'
    exit 1
fi
