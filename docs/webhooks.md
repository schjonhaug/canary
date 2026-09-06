# JSON webhook notifications

Canary's self-hosted mode can send every contact notification to an HTTP or HTTPS endpoint. Add a contact, enable **JSON Webhook**, enter the endpoint URL, and use the inline **Test** button before saving.

Webhook contacts are not available on canarybitcoin.com. The create, update, test, and provider-discovery APIs enforce that boundary on the server.

## Delivery behavior

- Canary sends an HTTP `POST` with `Content-Type: application/json`.
- Any `2xx` response is successful.
- The request timeout is 10 seconds.
- Redirects are not followed.
- Canary delivers at most four webhook requests concurrently.
- Failed deliveries are recorded but are not retried automatically.
- URLs must be absolute HTTP or HTTPS URLs, no longer than 2,048 characters, with a host and without user information (`user:password@host`) or a fragment (`#section`). Localhost and private-network destinations are allowed on self-hosted installs because the operator controls the host and network; cloud mode rejects webhooks entirely. Link-local, CGNAT, and other special-use ranges remain blocked.

The complete URL, including its path and query string, is stored so Canary can deliver to it and show it to the authenticated administrator while editing. Treat path segments and query parameters as secrets. Collapsed contact summaries, notification snapshots, and application logs show only the URL origin, such as `https://hooks.example.com:8443`.

Version 1 does not add bearer-token fields, custom headers or templates, HMAC signatures, a delivery queue, or retries. If a receiver requires those features, put a small adapter or reverse proxy in front of it.

### Test endpoint

Authenticated self-hosted administrators can test an endpoint without saving a contact:

```http
POST /api/webhook/test
Content-Type: application/json

{"url":"https://hooks.example.com/canary?token=secret"}
```

The response is `{"success":true}` when the receiver returns any `2xx` status. Delivery failures still return an HTTP `200` response with `{"success":false,"error":"..."}` so the UI can show endpoint feedback. Invalid URLs use HTTP `400`; cloud-mode requests use HTTP `403` with the `webhook_self_hosted_only` error code.

## Payload contract

Every payload has the same top-level shape. `schema_version` is `1`; consumers should ignore unknown fields so the contract can grow compatibly.

```json
{
  "schema_version": 1,
  "event": "receiving",
  "title": "Receiving Bitcoin - Cold Storage",
  "message": "💸 Receiving: 0.00125 BTC to Cold Storage (unconfirmed)",
  "sent_at": "2026-08-11T12:34:56.789Z",
  "wallet": {
    "checksum": "abcd1234",
    "name": "Cold Storage",
    "balance_sats": 250000
  },
  "contact": {
    "id": "6a7f63c0-0f41-4c8e-9565-e5185b1dc065",
    "name": "Home automation"
  },
  "transaction": {
    "txid": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "direction": "receive",
    "amount_sats": 125000,
    "fee_sats": null,
    "block_height": null,
    "first_seen_at": 1786451696,
    "confirmed_at": null,
    "status": "pending",
    "parent_txid": null,
    "replaced_by_txid": null,
    "replaced_at": null
  },
  "balance_alert": null
}
```

`sent_at` is RFC 3339. Bitcoin transaction and alert timestamps retain Canary's Unix-second representation.

### Events

| `event` | Meaning | Detail object |
|---|---|---|
| `sending` | New outgoing transaction | `transaction` |
| `sent` | Outgoing transaction confirmed | `transaction` |
| `receiving` | New incoming transaction | `transaction` |
| `received` | Incoming transaction confirmed | `transaction` |
| `rbf` | Transaction replaced by fee | `transaction` |
| `cpfp` | Child-pays-for-parent transaction | `transaction` |
| `balance_alert` | Configured balance threshold crossed | `balance_alert` |
| `test` | Inline endpoint test | neither |

Titles and messages use the Canary administrator's preferred notification language.

For `balance_alert`, `transaction` is `null` and the detail object is:

```json
{
  "id": "alert-notification-id",
  "alert_id": "configured-alert-id",
  "alert_type": "above",
  "threshold_sats": 100000000,
  "current_balance_sats": 150000000,
  "threshold_currency": "USD",
  "threshold_fiat_amount": 50000.0,
  "exchange_rate_snapshot": 60000.0,
  "current_fiat_amount": 90000.0
}
```

Fiat fields are `null` for BTC-denominated alerts. For a test event, `wallet`, `contact`, `transaction`, and `balance_alert` are all `null`.

## Container reachability

The Canary backend sends the request, so the URL must be reachable from the backend container or process—not merely from the browser. Private LAN IPs, unique-local IPv6, and loopback are accepted after DNS resolution. Inside Docker, `localhost` points to the Canary container itself. Use the receiver's Compose service name when both services share a Docker network, a platform-provided internal hostname on Umbrel/StartOS, a LAN address such as a Home Assistant host, or `host.docker.internal` where the Docker host provides it. Test the URL from the contact form after choosing the address.
