# Fix Resend Rate Limiting with Batch Emails

## Problem
When we send too many emails for notifications in succession we get an error from Resend:

```
empt(s)); changes=true, new_transactions=1, confirmations=0, conflicts_marked=0
backend-1  | ❌ Failed to send email to andreas@schjonhaug.no: Resend API error: Too many requests. Limit is Some(2) per Some(1) seconds.
backend-1  | 🔔 Notified 4 contacts for Multisig: ✅ Sent 0.08231752 BTC (3×email, 2×twilio)
```

**Current behavior**: Sending individual emails sequentially hits Resend's 2 req/sec rate limit when notifying 3+ contacts.

## Scope

**What this fixes:**
- ✅ Transaction notifications (pending/confirmed) - sent via notification worker
- ✅ Balance alert notifications (above/below/equals) - sent via notification worker
- Both use `email_provider.rs` → notification manager → broadcast channel

**What this does NOT affect:**
- ❌ Contact verification emails (OTP codes for adding contacts)
- ❌ Auth verification emails (email address verification)
- ❌ Password reset emails
- These use `email_service.rs` and are sent individually (won't hit rate limits)

## Solution: Use Resend Batch Email API

### Why This Fixes It
- **Current**: 1 email = 1 API request (hits 2/sec limit with 3+ contacts)
- **Batch API**: Up to 100 emails = 1 API request (easily handles all contacts in single call)

### Resend Batch API Details
- **Endpoint**: `POST /emails/batch`
- **Limit**: Up to 100 emails per request
- **Rate counting**: 1 batch = 1 request (not 100 separate requests)
- **Validation modes**:
  - `strict` (default): All emails must be valid or entire batch fails
  - `permissive`: Processes valid emails, returns errors for invalid ones

### Implementation Plan

**File to modify**: `backend/src/email_provider.rs`

1. **Update `send_notifications` method**:
   - Collect all email recipients for this notification event
   - Build batch payload as array of email objects
   - Make single `POST /emails/batch` request
   - Parse batch response array to get individual email IDs

2. **Add batch send function**:
   ```rust
   async fn send_batch_emails(
       &self,
       emails: Vec<EmailRequest>
   ) -> Result<Vec<(String, Result<String>)>>
   ```

3. **Request format change**:
   ```json
   // OLD: Individual send (POST /emails)
   {
     "from": "notifications@canarybitcoin.com",
     "to": ["user@example.com"],
     "subject": "Transaction notification",
     "html": "<p>...</p>"
   }

   // NEW: Batch send (POST /emails/batch)
   [
     {
       "from": "notifications@canarybitcoin.com",
       "to": ["user1@example.com"],
       "subject": "Transaction notification",
       "html": "<p>...</p>"
     },
     {
       "from": "notifications@canarybitcoin.com",
       "to": ["user2@example.com"],
       "subject": "Transaction notification",
       "html": "<p>...</p>"
     }
   ]
   ```

4. **Response format**:
   ```json
   {
     "data": [
       { "id": "ae2014de-c168-4c61-8267-70d2662a1ce1" },
       { "id": "faccb7a5-8a28-4e9a-ac64-8da1cc3bc1cb" }
     ],
     "errors": [  // Only in permissive mode
       {
         "index": 2,
         "message": "The `to` field is missing."
       }
     ]
   }
   ```

5. **Handle batch response**:
   - Success: Extract email IDs from `data` array
   - Map IDs back to contacts for notification logging
   - Handle partial failures if using permissive mode

### Benefits
- **Eliminates rate limit**: 100 emails/request far exceeds any realistic contact count
- **Reduced latency**: Single network round-trip vs multiple sequential requests
- **Simpler code**: No queue, throttling, or retry logic needed
- **Better UX**: All notifications sent simultaneously, not delayed

### Implementation Steps
1. Read current `email_provider.rs` implementation
2. Update API endpoint from `/emails` to `/emails/batch`
3. Change request body from object to array of objects
4. Update response parsing to handle batch format
5. Map batch response IDs back to individual contacts
6. Test with 5+ contacts to verify no rate limit errors

### Testing
- Create wallet with 5+ email contacts
- Send transaction to trigger notifications
- Verify all emails sent in single batch request
- Confirm no 429 rate limit errors in logs
- Check notification logs show correct delivery status
