# Canary

<img src="frontend/public/images/canary.svg" alt="Canary Logo" width="100" height="86">

Canary is a **Bitcoin monitoring and early warning system** built in [Rust](https://www.rust-lang.org/) using [BDK (Bitcoin Development Kit)](https://bitcoindevkit.org/) with a [Next.js](https://nextjs.org/) frontend. It provides real-time transaction intelligence, advanced pattern recognition (RBF, CPFP, consolidation), and instant multilingual notifications via [ntfy.sh](https://ntfy.sh) for Bitcoin wallet activity - designed specifically for monitoring cold storage and Bitcoin holdings you don't actively use.

## Why Use Canary?

**A canary in the cold mine** - When your bitcoins are in cold storage, you seldom check on them. Canary acts as an early warning system that alerts you the moment your coins move, giving you immediate notification of any activity on your wallets.

**Real-time notifications in Norwegian and English for all Bitcoin transactions via ntfy.sh:**
- 📤 Sending bitcoins
- ✅ Transaction sent and confirmed  
- 📥 Receiving bitcoins
- ✅ Transaction received and confirmed
- 📤 **RBF (Replace-By-Fee)** detection - fee bumping notifications
- 🚀 **CPFP (Child-Pays-For-Parent)** detection - transaction acceleration notifications

## Development & Testing

### Displaying Git Version in Footer

The frontend displays the git version and commit hash in the footer. To generate this build info locally:

```bash
# Generate build info (creates src/lib/build-info.json)
cd frontend && node scripts/generate-build-info.js

# Then start the dev server
npm run dev
```

The footer will display in format: `v0.13.0 • 5e66fe3` (tag and commit) or just the commit hash if no tag exists.

**Note**: The build info is automatically generated during production builds via the webpack configuration. For local development, you need to run the script manually.

### Running System Tests

The project includes comprehensive system tests that use Docker to create isolated Bitcoin regtest environments. These tests cover:

- **Advanced Transactions**: RBF (Replace-By-Fee) and CPFP (Child-Pays-For-Parent) scenarios
- **High Index Scanning**: Deep wallet address discovery for funds at high indexes  
- **Direct Mining**: Transactions that get mined directly without mempool delays
- **Two-Stage Scenarios**: Complex transaction flows with multiple confirmations

#### Prerequisites
- Docker and Docker Compose installed
- Rust toolchain for building

#### Run All System Tests
```bash
# Run all 10 system tests (sequential to avoid Docker conflicts)
cargo test --test advanced_transactions --test mined_directly_scenarios --test two_stage_send_scenarios --test high_index_scanning -- --ignored --test-threads=1
```

#### Run Individual Test Categories
```bash
# Advanced transactions (RBF, CPFP) - 3 tests
cargo test --test advanced_transactions -- --ignored

# High index scanning - 1 test
cargo test --test high_index_scanning -- --ignored

# Mined directly scenarios - 3 tests
cargo test --test mined_directly_scenarios -- --ignored

# Two-stage send scenarios - 3 tests
cargo test --test two_stage_send_scenarios -- --ignored
```

**Note**: System tests must run sequentially (`--test-threads=1`) to avoid Docker resource conflicts between parallel test environments.