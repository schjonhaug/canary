# Canary for StartOS

This directory contains all the files needed to package Canary as a StartOS service.

## Prerequisites

To build the package, you need:

- Docker with buildx support
- [yq](https://github.com/mikefarah/yq) - YAML processor
- [Deno](https://deno.land/) - JavaScript/TypeScript runtime
- [start-sdk](https://github.com/Start9Labs/start-os) - StartOS SDK

### Installing Dependencies

```bash
# Install yq
snap install yq

# Install Deno
curl -fsSL https://deno.land/x/install/install.sh | sh

# Install start-sdk (from start-os repo)
git clone -b latest --recursive https://github.com/Start9Labs/start-os.git
cd start-os/core
./install-sdk.sh
start-sdk init
```

## Building

```bash
cd start9

# Build for all architectures (x86_64 and aarch64)
make

# Build for specific architecture only
make x86   # x86_64 only
make arm   # aarch64 only

# Verify the package
make verify
```

The resulting `canary.s9pk` file can be sideloaded into StartOS.

## Installing on StartOS

### Option 1: Via CLI

```bash
# Configure your StartOS server in ~/.embassy/config.yaml
# host: http://your-server.local

make install
```

### Option 2: Sideload via UI

1. Open StartOS web interface
2. Go to System > Sideload Service
3. Upload the `canary.s9pk` file

## Structure

```
start9/
├── Dockerfile          # Combined backend+frontend Docker image
├── Makefile            # Build automation
├── manifest.yaml       # StartOS service manifest
├── instructions.md     # User documentation
├── icon.png            # Service icon
├── LICENSE             # License file
├── scripts/
│   ├── deps.ts         # Deno dependencies
│   ├── embassy.ts      # Embassy script exports
│   ├── bundle.ts       # Deno bundler script
│   ├── docker_entrypoint.sh  # Container startup
│   ├── check-api.sh    # Backend health check
│   ├── check-web.sh    # Frontend health check
│   └── services/
│       ├── getConfig.ts    # Config UI definition
│       ├── setConfig.ts    # Config setter
│       ├── properties.ts   # Service properties
│       └── migrations.ts   # Version migrations
└── README.md           # This file
```

## Configuration Options

The service exposes these configuration options in the StartOS UI:

- **Bitcoin Network**: mainnet, testnet, or regtest
- **Electrum Server**: Local Electrs (recommended) or external server
- **External Electrum URL**: Custom Electrum server URL
- **Admin Notification Topic**: ntfy.sh topic for admin alerts

## Dependencies

Canary has an optional dependency on **Electrs** for local blockchain data access. When configured to use the local Electrs server, StartOS will ensure Electrs is installed and running.

## Testing

After installation, verify:

1. The service starts without errors (check logs)
2. Both health checks pass (API and Web Interface)
3. The web UI is accessible via Tor or LAN
4. Wallet syncing works with your Electrum server

## Submission to Start9 Registry

To submit this package to the Start9 Community Registry:

1. Ensure the package builds successfully on both architectures
2. Test thoroughly on a StartOS device
3. Follow the submission process at https://docs.start9.com/developer-docs/submission
