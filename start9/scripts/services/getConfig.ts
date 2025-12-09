// Configuration structure for Canary
// These options display as changeable UI elements in the "Config" section

import { compat, types as T } from "../deps.ts";

export const getConfig: T.ExpectedExports.getConfig = compat.getConfig({
  "tor-address": {
    "name": "Tor Address",
    "description": "The Tor address of the web interface",
    "type": "pointer",
    "subtype": "package",
    "package-id": "canary",
    "target": "tor-address",
    "interface": "main",
  },
  "lan-address": {
    "name": "LAN Address",
    "description": "The LAN address of the web interface",
    "type": "pointer",
    "subtype": "package",
    "package-id": "canary",
    "target": "lan-address",
    "interface": "main",
  },
  "network": {
    "type": "enum",
    "name": "Bitcoin Network",
    "description": "The Bitcoin network to monitor wallets on",
    "values": ["mainnet", "testnet", "regtest"],
    "value-names": {
      "mainnet": "Mainnet (Production)",
      "testnet": "Testnet (Testing)",
      "regtest": "Regtest (Local Development)",
    },
    "default": "mainnet",
  },
  "electrum-source": {
    "type": "enum",
    "name": "Electrum Server",
    "description": "Where to get blockchain data from",
    "warning": "Using an external Electrum server may expose your wallet addresses to third parties.",
    "values": ["local", "external"],
    "value-names": {
      "local": "Local Electrs (Recommended)",
      "external": "External Server",
    },
    "default": "local",
  },
  "external-electrum-url": {
    "type": "string",
    "name": "External Electrum URL",
    "description": "URL of external Electrum server (only used if 'External Server' is selected above)",
    "nullable": true,
    "default": "ssl://electrum.blockstream.info:50002",
    "pattern": "^(tcp|ssl)://[^\\s]+:\\d+$",
    "pattern-description": "Must be a valid Electrum URL (tcp:// or ssl:// followed by host:port)",
  },
  "admin-notification-topic": {
    "type": "string",
    "name": "Admin Notification Topic",
    "description": "ntfy.sh topic for admin notifications (optional). Subscribe at https://ntfy.sh/your-topic",
    "nullable": true,
    "default": null,
  },
});
