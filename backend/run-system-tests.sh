#!/bin/bash

# System Tests Runner
# Runs all Docker-based integration tests sequentially

set -e  # Exit on first error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse arguments
NOCAPTURE=""
if [[ "$1" == "--nocapture" ]]; then
    NOCAPTURE="--nocapture"
    echo -e "${YELLOW}Running with --nocapture (verbose output)${NC}"
    echo ""
fi

# Check if Docker is running
echo -e "${BLUE}Checking Docker availability...${NC}"
if ! docker ps > /dev/null 2>&1; then
    echo -e "${RED}❌ Docker is not running${NC}"
    echo "Please start Docker and try again"
    exit 1
fi
echo -e "${GREEN}✅ Docker is running${NC}"
echo ""

# Define all system tests
TESTS=(
    "two_stage_send_scenarios"
    "high_index_scanning"
    "advanced_transactions"
    "mined_directly_scenarios"
    "transaction_timestamp_scenarios"
    "balance_alert_scenarios"
)

# Track results
PASSED=0
FAILED=0
FAILED_TESTS=()

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Running ${#TESTS[@]} System Tests${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Run each test
for test in "${TESTS[@]}"; do
    echo -e "${YELLOW}▶ Running: ${test}${NC}"
    echo "----------------------------------------"

    if cargo test --test "$test" -- --ignored --test-threads=1 $NOCAPTURE; then
        echo -e "${GREEN}✅ PASSED: ${test}${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}❌ FAILED: ${test}${NC}"
        FAILED=$((FAILED + 1))
        FAILED_TESTS+=("$test")
    fi

    echo ""
done

# Print summary
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "Total:  ${#TESTS[@]} tests"
echo -e "${GREEN}Passed: ${PASSED}${NC}"
echo -e "${RED}Failed: ${FAILED}${NC}"

if [ $FAILED -gt 0 ]; then
    echo ""
    echo -e "${RED}Failed tests:${NC}"
    for test in "${FAILED_TESTS[@]}"; do
        echo -e "  ${RED}• ${test}${NC}"
    done
    echo ""
    echo -e "${YELLOW}💡 Tip: Run with --nocapture for detailed output:${NC}"
    echo -e "   ./run-system-tests.sh --nocapture"
    exit 1
else
    echo ""
    echo -e "${GREEN}🎉 All system tests passed!${NC}"
    exit 0
fi
