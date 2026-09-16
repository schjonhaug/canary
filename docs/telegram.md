# Telegram Bot notifications

Telegram is available on **self-hosted** Canary Wallet only. Cloud stays email and SMS.

Create a bot with [@BotFather](https://t.me/BotFather). On Umbrel, StartOS, myNode, and other self-hosted installs, paste the token in **Settings → Telegram**. Message the bot first so it can deliver to a private chat, then add a destination with the numeric chat ID (including negative group IDs) or a public `@username`.

`TELEGRAM_BOT_TOKEN` remains an optional fallback for VPS and Docker Compose. A token saved in Settings wins when both are set. Clearing Settings falls back to the environment variable. Restore-drill mode never enables Telegram, even if a token is present.

## Delivery behavior

- Canary Wallet calls Telegram Bot API `sendMessage` with `disable_web_page_preview` enabled.
- Message text uses the same localized, content-filtered copy as other providers.
- The request timeout is 10 seconds.
- Redirects are not followed.
- Canary Wallet delivers at most four Telegram messages concurrently.
- Failed deliveries are recorded but are not retried automatically.
- Destinations must be a numeric chat ID of 1–20 digits (optional leading `-`) or a public `@username` of 5–32 characters starting with a letter.

Chat IDs may be shared across contacts, the same way ntfy topics can be shared. Telegram is omitted from the unique-target index that applies to email, SMS, and Nostr.

The destination picker omits Telegram until a token exists (Settings or env). Saving a token in Settings makes it available without restarting Canary Wallet.

### Settings

Authenticated administrators can read and update the bot token without restarting. The API never echoes the stored token:

```http
GET /api/telegram/settings
```

```http
PUT /api/telegram/settings
Content-Type: application/json

{"bot_token":"123456:ABC-DEF"}
```

An empty `bot_token` clears the Settings value. Responses are `{"configured":true}` or `{"configured":false}`. Cloud-mode requests use HTTP `403` with `telegram_self_hosted_only`.

### Test endpoint

Authenticated administrators on self-hosted installs can test a destination without saving a contact:

```http
POST /api/telegram/test
Content-Type: application/json

{"chat_id":"123456789"}
```

The response is `{"success":true}` when Telegram accepts the message. Delivery failures still return an HTTP `200` response with `{"success":false,"error":"..."}` so the UI can show endpoint feedback. Invalid chat IDs use HTTP `400` with `invalid_telegram_chat_id`. A missing token uses HTTP `403` with `telegram_not_configured`. Cloud-mode requests use HTTP `403` with `telegram_self_hosted_only`.

A destination-only request sends a generic connectivity payload. To confirm a saved contact's configuration, include the saved identifiers. The test still delivers to the request chat ID, which must match the saved method:

```http
POST /api/telegram/test
Content-Type: application/json

{
  "chat_id": "123456789",
  "wallet_checksum": "abcd1234",
  "contact_id": "6a7f63c0-0f41-4c8e-9565-e5185b1dc065",
  "method_id": "c3f1a2b0-9e44-4d1f-8a77-2b1c0d9e8f70"
}
```
