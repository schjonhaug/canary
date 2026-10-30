# System Tests

System tests exercise the whole sync pipeline against a real regtest chain:
Bitcoin Core → Fulcrum (Electrum) → BDK → `WalletSyncService` → metadata
database → transaction events. Each test starts its own Docker environment,
sends real transactions with `bitcoin-cli`, syncs the wallets and asserts on
the events Canary records.

Every scenario file is a separate `[[test]]` target in
[`../Cargo.toml`](../Cargo.toml) with `required-features = ["system-tests"]`,
so a plain `cargo test` neither compiles nor runs them. Every test is also
marked `#[ignore]`, so the runner passes `--ignored`. CI lints them with
`cargo clippy --all-targets --features system-tests`.

## Prerequisites

- Docker, running
- The `docker-compose` command on `PATH` (the harness calls it directly)
- Network access on first run to pull `ghcr.io/sethforprivacy/bitcoind:latest`
  and `cculianu/fulcrum:latest`

## Running

From `backend/`:

```bash
./run-system-tests.sh              # standard set, one file at a time
./run-system-tests.sh --nocapture  # same, with full test output
```

The runner checks that Docker is available, runs each file in its list with
`cargo test --features system-tests --test <file> -- --ignored --test-threads=1`, and prints a
summary. It runs `two_stage_send_scenarios`, `advanced_transactions`,
`mined_directly_scenarios`, `transaction_timestamp_scenarios` and
`balance_alert_scenarios`. `high_index_scanning` is commented out because it
hangs during deep scanning (#633). The remaining files are not in the runner's
list; run them individually.

Run one file, one test, or list a file's tests:

```bash
cargo test --features system-tests --test self_send_scenarios -- --ignored --test-threads=1
cargo test --features system-tests --test balance_alert_scenarios test_balance_alert_below_threshold -- --ignored --nocapture
cargo test --features system-tests --test mined_directly_scenarios -- --list
```

Always pass `--test-threads=1` when running more than one test: each test
starts its own containers, and running them in parallel competes for ports and
Docker resources. Add `RUST_LOG=debug` for internal sync logging.

## Scenarios

| Target | Tests | In runner |
|---|---|---|
| `two_stage_send_scenarios` | `test_alice_partial_send_bob_two_stage`, `test_alice_full_send_bob_two_stage`, `test_multiple_partial_sends_bob_two_stage`: sends seen in the mempool, then confirmed | Yes |
| `advanced_transactions` | `test_single_rbf_sender_and_receiver_perspective`, `test_multiple_rbf_sender_and_receiver_perspective`, `test_cpfp_detection_and_tracking`: RBF replacement and CPFP detection | Yes |
| `mined_directly_scenarios` | `test_alice_partial_send_bob_mined_directly`, `test_alice_full_send_bob_mined_directly`, `test_multiple_partial_sends_mined_directly`: transactions mined before the first sync sees them | Yes |
| `transaction_timestamp_scenarios` | `test_mempool_first_transaction_timestamp`, `test_direct_mining_transaction_timestamp`: mempool time versus block time | Yes |
| `balance_alert_scenarios` | `test_balance_alert_below_threshold`, `test_balance_alert_above_threshold`, `test_balance_drain_alert_equals_zero`, `test_multiple_balance_alerts`, `test_balance_alert_deactivation` | Yes |
| `high_index_scanning` | `test_high_index_fund_detection`: funds at index 249 inside a 250-script stop gap | No (#633) |
| `utxo_consolidation_scenarios` | `test_utxo_consolidation_send_all_to_bob`: several UTXOs spent in one send | No |
| `self_send_scenarios` | `test_self_send_partial`, `test_self_send_full_amount`: a wallet paying itself is recorded as a send of roughly the fee | No |
| `wallet_recovery_scenarios` | `test_no_duplicate_events_after_restart`, `test_recovery_detects_transactions_during_downtime`: recreating the `WalletManager` as a restart | No |
| `batch_transaction_scenarios` | `test_batch_send_to_bob_and_charlie`: one transaction with several recipients | No |
| `mempool_chain_scenarios` | `test_chain_of_unconfirmed_transactions`, `test_unconfirmed_chain_with_separate_confirmations`: unconfirmed transactions spending each other's change | No |
| `address_watch_scenarios` | `test_unused_bech32_address_watch_becomes_ready_after_empty_sync`: a single-address watch with no history | No |

## Test environment

The harness is in [`common/docker_environment.rs`](common/docker_environment.rs).
For each test, `IsolatedTestEnvironment`:

1. Generates a short random test id and writes a `docker-compose.yml`,
   `bitcoin.conf` and `fulcrum.conf` into a temporary directory.
2. Starts `test-bitcoin-<id>` and `test-fulcrum-<id>` on their own bridge
   network, with free host ports chosen at random for Bitcoin RPC and
   Electrum.
3. Mines 101 blocks to a miner wallet and funds the test wallets.
4. Creates a `WalletManager` against a temporary metadata database and
   registers the wallets through `WalletCreationService`.
5. Runs `docker-compose down -v` and removes the temporary directory when the
   environment is dropped.

Test wallets use fixed descriptors:

- **Alice**: funded with 1.0 BTC.
- **Bob**: empty, used as the receiver.
- **Charlie**: only in `IsolatedTestEnvironment::new_with_charlie()` (used by
  `high_index_scanning` and `batch_transaction_scenarios`). Created with a stop
  gap of 250 and funded with 0.5 BTC at address index 249.

Tests take tens of seconds to minutes each because they start containers,
wait for Fulcrum to index and wait for propagation between steps.

## Cleaning up

If a test is killed before its environment is dropped, containers can be left
behind. Remove them with:

```bash
docker rm -f $(docker ps -aq --filter "name=test-bitcoin-" --filter "name=test-fulcrum-") 2>/dev/null || true
docker volume rm $(docker volume ls -q --filter "name=canary_test_") 2>/dev/null || true
docker network rm $(docker network ls -q --filter "name=canary_test_network_") 2>/dev/null || true
```
