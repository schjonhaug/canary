# Telegram Bot notifications

Canary can send every contact notification through a Telegram bot when `TELEGRAM_BOT_TOKEN` is set. Create a bot with [@BotFather](https://t.me/BotFather), paste the token into the backend environment, and add a contact destination with the numeric chat ID (including negative group IDs) or a public `@username`. Message the bot first so it can deliver to a private chat.

Telegram is available in self-hosted and cloud mode once the token is set. Restore-drill mode never registers the provider, even if the token is present.

## Delivery behavior

- Canary calls Telegram Bot API `sendMessage` with `disable_web_page_preview` enabled.
- Message text uses the same localized, content-filtered copy as other providers.
- The request timeout is 10 seconds.
- Redirects are not followed.
- Canary delivers at most four Telegram messages concurrently.
- Failed deliveries are recorded but are not retried automatically.
- Destinations must be a numeric chat ID of 1–20 digits (optional leading `-`) or a public `@username` of 5–32 characters starting with a letter.

Chat IDs may be shared across contacts, the same way ntfy topics can be shared. Telegram is omitted from the unique-target index that applies to email, SMS, and Nostr.

### Test endpoint

Authenticated administrators on self-hosted installs can test a destination without saving a contact. Cloud mode still delivers live Telegram notifications when the token is set, but the test endpoint is self-hosted only, matching ntfy/webhook/Nostr tests:

```http
POST /api/telegram/test
Content-Type: application/json

{"chat_id":"123456789"}
```

The response is `{"success":true}` when Telegram accepts the message. Delivery failures still return an HTTP `200` response with `{"success":false,"error":"..."}` so the UI can show endpoint feedback. Invalid chat IDs use HTTP `400` with `invalid_telegram_chat_id`. A missing token uses HTTP `403` with `telegram_not_configured`. Cloud-mode requests use HTTP `403` with `telegram_test_self_hosted_only`.

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
