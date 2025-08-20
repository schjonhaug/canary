#!/usr/bin/env node

const { addressFromExtPubKey, addressesFromExtPubKey, Purpose, initEccLib } = require('@swan-bitcoin/xpub-lib');
const { fromBase58 } = require('bip32');

// Initialize ECC library for Taproot support
try {
  const ecc = require('tiny-secp256k1');
  initEccLib(ecc);
} catch (error) {
  console.error(JSON.stringify({ error: 'Failed to initialize ECC library', details: error.message }));
  process.exit(1);
}

// Parse command line arguments
const args = process.argv.slice(2);
if (args.length < 2) {
  console.error(JSON.stringify({ 
    error: 'Usage: node xpub_converter.js <xpub> <network> [addressCount]',
    example: 'node xpub_converter.js xpub6BmG... mainnet 5'
  }));
  process.exit(1);
}

const [xpub, network, addressCountArg] = args;
const addressCount = parseInt(addressCountArg) || 5;

// Validate network
if (!['mainnet', 'testnet'].includes(network)) {
  console.error(JSON.stringify({ error: 'Network must be "mainnet" or "testnet"' }));
  process.exit(1);
}

// Validate XPUB format (basic check)
const xpubRegex = /^[xyztuv]pub[1-9A-HJ-NP-Za-km-z]{107,108}$/;
if (!xpubRegex.test(xpub)) {
  console.error(JSON.stringify({ error: 'Invalid extended public key format' }));
  process.exit(1);
}

async function deriveAddresses() {
  try {
    const results = {
      xpub: xpub,
      network: network,
      script_types: {}
    };

    // Define script types to test
    const scriptTypes = [
      { name: 'p2pkh', purpose: '44', desc_template: 'pkh({xpub}/<0;1>/*)' },
      { name: 'p2sh', purpose: '49', desc_template: 'sh(wpkh({xpub}/<0;1>/*))' },
      { name: 'p2wpkh', purpose: '84', desc_template: 'wpkh({xpub}/<0;1>/*)' },
      { name: 'p2tr', purpose: '86', desc_template: 'tr({xpub}/<0;1>/*)' }
    ];

    // Derive addresses for each script type
    for (const scriptType of scriptTypes) {
      try {
        // Derive receiving addresses (change = 0)
        const receivingAddresses = addressesFromExtPubKey({
          extPubKey: xpub,
          network: network,
          purpose: scriptType.purpose,
          change: 0, // receiving addresses
          addressCount: addressCount
        });

        // Derive change addresses (change = 1) - fewer of these
        const changeAddresses = addressesFromExtPubKey({
          extPubKey: xpub,
          network: network,
          purpose: scriptType.purpose,
          change: 1, // change addresses
          addressCount: Math.min(addressCount, 3) // Less change addresses
        });

        results.script_types[scriptType.name] = {
          descriptor_template: scriptType.desc_template,
          receiving_addresses: receivingAddresses,
          change_addresses: changeAddresses,
          all_addresses: [...receivingAddresses, ...changeAddresses]
        };

      } catch (error) {
        // Some script types might fail (e.g., Taproot on older networks)
        results.script_types[scriptType.name] = {
          error: error.message,
          descriptor_template: scriptType.desc_template,
          receiving_addresses: [],
          change_addresses: [],
          all_addresses: []
        };
      }
    }

    // Output JSON result
    console.log(JSON.stringify(results, null, 2));

  } catch (error) {
    console.error(JSON.stringify({ 
      error: 'Failed to derive addresses', 
      details: error.message,
      stack: error.stack 
    }));
    process.exit(1);
  }
}

// Run the conversion
deriveAddresses().catch(error => {
  console.error(JSON.stringify({ 
    error: 'Unexpected error', 
    details: error.message,
    stack: error.stack 
  }));
  process.exit(1);
});