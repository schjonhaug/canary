#!/bin/bash

# Health check script for Start9 - Web Interface
# Output format: JSON as required by Start9

FRONTEND_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3001 2>/dev/null)

if [ "$FRONTEND_STATUS" = "200" ]; then
    echo '{"result": "The Canary web interface is ready"}'
    exit 0
else
    echo '{"error": "Web interface not responding (status: '"$FRONTEND_STATUS"')"}'
    exit 1
fi
