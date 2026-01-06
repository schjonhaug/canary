# Canary

<img src="frontend/public/images/canary.svg" alt="Canary Logo" width="100" height="86">

Canary is a **Bitcoin monitoring and early warning system** built in [Rust](https://www.rust-lang.org/) using [BDK (Bitcoin Development Kit)](https://bitcoindevkit.org/) with a [Next.js](https://nextjs.org/) frontend. It provides real-time transaction intelligence, advanced pattern recognition (RBF, CPFP, consolidation), and instant multilingual notifications for Bitcoin wallet activity - designed specifically for monitoring cold storage and Bitcoin holdings you don't actively use.

## Why Use Canary?

**A canary in the cold mine** - When your bitcoins are in cold storage, you seldom check on them. Canary acts as an early warning system that alerts you the moment your coins move, giving you immediate notification of any activity on your wallets.

## Key Features

### Multi-Channel Notifications
Real-time notifications in 9 languages via multiple channels:
- **[ntfy.sh](https://ntfy.sh)** - Push notifications (self-hostable)
- **SMS** - Via Twilio integration
- **Email** - Via Resend integration

**Supported Languages:** English, Norwegian, Spanish, Portuguese, German, French, Japanese, Danish, Swedish

### Transaction Monitoring
- Sending and receiving bitcoin notifications
- Transaction confirmation alerts
- **RBF (Replace-By-Fee)** detection - fee bumping notifications
- **CPFP (Child-Pays-For-Parent)** detection - transaction acceleration notifications

### Balance Alerts
- Configurable threshold alerts (above/below/equals)
- Support for both BTC and fiat currency thresholds
- Smart crossing detection to prevent notification spam
- Auto-disable after firing with manual reactivation

### Authentication & Multi-User Support
- Optional email/password authentication with email verification
- JWT-based sessions with user isolation
- Password reset functionality

### Subscription Billing (Cloud Mode)
- Two-tier system: Personal and Team plans
- Stripe integration with native trial subscriptions
- Customer portal for subscription management

### Deployment Modes
- **Self-hosted**: Single user, no auth, ntfy notifications only
- **Cloud**: Multi-user with authentication, billing, and all notification providers

## Quick Start

### Prerequisites
- Rust toolchain
- Node.js 18+
- Docker and Docker Compose (for local Bitcoin regtest)

### Configuration
```bash
# Backend - choose your mode
cd backend
cp .env.example.self-hosted .env  # or .env.example.cloud

# Frontend
cd frontend
cp .env.example.self-hosted .env.local  # or .env.example.cloud
```

### Development
```bash
# Start local Bitcoin regtest environment
cd scripts && docker-compose up -d

# Start backend (requires Stripe CLI for cloud mode)
cd backend && cargo run

# Start frontend
cd frontend && npm run dev
```

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
- **Balance Alerts**: Threshold-based balance monitoring

#### Prerequisites
- Docker and Docker Compose installed
- Rust toolchain for building

#### Run All System Tests
```bash
# Run all system tests (sequential to avoid Docker conflicts)
cd backend
cargo test --test advanced_transactions --test mined_directly_scenarios --test two_stage_send_scenarios --test high_index_scanning --test balance_alert_scenarios -- --ignored --test-threads=1
```

#### Run Individual Test Categories
```bash
cd backend

# Advanced transactions (RBF, CPFP) - 3 tests
cargo test --test advanced_transactions -- --ignored

# High index scanning - 1 test
cargo test --test high_index_scanning -- --ignored

# Mined directly scenarios - 3 tests
cargo test --test mined_directly_scenarios -- --ignored

# Two-stage send scenarios - 3 tests
cargo test --test two_stage_send_scenarios -- --ignored

# Balance alert scenarios
cargo test --test balance_alert_scenarios -- --ignored
```

**Note**: System tests must run sequentially (`--test-threads=1`) to avoid Docker resource conflicts between parallel test environments.

## Documentation

See [CLAUDE.md](CLAUDE.md) for comprehensive documentation including:
- API endpoints
- Database schema
- Notification setup
- Stripe integration
- Architecture details

## License

See [LICENSE.md](LICENSE.md) for license information.
