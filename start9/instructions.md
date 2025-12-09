# Canary - Bitcoin Wallet Monitoring

Canary is a self-hosted Bitcoin wallet monitoring service that helps you keep track of your Bitcoin wallets without exposing your private keys.

## Getting Started

### 1. Configure Electrum Server

By default, Canary uses your local **Electrs** service for blockchain data. This is the recommended option for privacy.

If you don't have Electrs installed:
1. Go to the Marketplace and install **Electrs**
2. Wait for Electrs to sync with the blockchain (this may take several hours on first run)

Alternatively, you can configure an external Electrum server in Settings, but this may expose your wallet addresses to third parties.

### 2. Access the Web Interface

1. Click on **Interfaces** in the service details
2. Choose either:
   - **Tor Address** - Access via Tor Browser (more private, but slower)
   - **LAN Address** - Access from your local network (faster, requires HTTPS certificate acceptance)

### 3. Add Your First Wallet

1. In the Canary web interface, click **Add Wallet**
2. Enter a name for your wallet (e.g., "Cold Storage", "Hardware Wallet")
3. Paste your **extended public key (xpub/ypub/zpub)** or **output descriptor**
4. Click **Create**

Your wallet will begin syncing. Depending on the wallet's transaction history, this may take a few minutes.

## Features

### Transaction Monitoring
- View all incoming and outgoing transactions
- See transaction confirmations in real-time
- Detect RBF (Replace-By-Fee) and CPFP (Child-Pays-For-Parent) transactions

### Push Notifications
Canary supports push notifications via **ntfy.sh**:

1. Install the ntfy app on your phone ([Android](https://play.google.com/store/apps/details?id=io.heckel.ntfy) / [iOS](https://apps.apple.com/app/ntfy/id1625396347))
2. In Canary, add a contact with an ntfy topic
3. Subscribe to the same topic in the ntfy app
4. Receive instant notifications for all wallet transactions

### Balance Alerts
Set up alerts to notify you when your wallet balance crosses a threshold:
- **Above** - Alert when balance exceeds a certain amount
- **Below** - Alert when balance drops below a certain amount
- **Equals** - Alert when balance reaches an exact amount

### Multi-Language Support
Canary supports notifications in:
- English
- Norwegian (Norsk)

## Security Notes

- **Watch-Only**: Canary only uses your public keys. It cannot spend your Bitcoin.
- **No Cloud**: All data stays on your StartOS server.
- **Privacy**: Using local Electrs keeps your wallet addresses private.

## Troubleshooting

### Wallet Not Syncing
1. Check that Electrs is running and fully synced
2. Verify your descriptor/xpub is valid
3. Check the Canary logs for errors

### Notifications Not Working
1. Verify your ntfy topic is correct
2. Make sure you're subscribed to the same topic in the ntfy app
3. Check your phone's notification settings

### High Address Index
If your wallet uses high address indexes (e.g., index 200+), Canary will automatically detect this through deep scanning. The initial sync may take longer.

## Support

For issues and feature requests, please visit:
https://github.com/schjonhaug/canary/issues
