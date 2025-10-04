# Stripe Trial Extension Webhook Support

## Status: To Do

## Problem

When a trial period is manually extended in the Stripe dashboard, the change is not reflected in Canary's local database. This causes wallets from users with extended trials to be excluded from sync operations, as the sync query filters by trial expiration date.

### Current Behavior
1. Admin extends trial in Stripe dashboard (e.g., kontakt@bpinorge.no)
2. Stripe sends `customer.subscription.updated` webhook
3. Canary's webhook handler receives the event but **does not update the database**
4. User's `trial_ends_at` remains at the old expired date
5. Wallet sync queries exclude the user's wallets (trial appears expired)
6. Wallets stop syncing

### Root Cause
The webhook handler in `backend/src/stripe_billing.rs:839-960` only processes `customer.subscription.updated` events when:
- Trial **status** changes from `trialing` to `active`/`past_due`/etc.
- Subscription **tier** changes (items field changes)
- Subscription is **cancelled** (`cancel_at_period_end` = true)

When a trial is extended:
- The `trial_end` timestamp changes
- But the `status` remains `"trialing"`
- None of the existing conditions match
- `should_update` remains `false`
- Database is not updated

## Solution

Add trial extension detection to the webhook handler by checking for changes in the `trial_end` field within `previous_attributes`.

### Code Changes

**File:** `backend/src/stripe_billing.rs`

**Location:** After line 910 (after the tier change check)

**Add:**
```rust
// Check for trial extension (trial_end changed while still trialing)
if let Some(previous_attrs) = subscription.get("previous_attributes") {
    if let (Some(prev_trial_end), Some(current_trial_end)) = (
        previous_attrs.get("trial_end").and_then(|t| t.as_i64()),
        subscription.get("trial_end").and_then(|t| t.as_i64())
    ) {
        if prev_trial_end != current_trial_end && current_status == Some("trialing") {
            should_update = true;
            let new_date = chrono::DateTime::from_timestamp(current_trial_end, 0)
                .map(|dt| dt.format("%B %d, %Y").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            reason = format!("Trial extended to {}", new_date);
        }
    }
}
```

**Update the `SubscriptionUpdate` struct creation (line 941-955):**
```rust
// Extract trial_end for trialing subscriptions
let trial_ends_at = if current_status == Some("trialing") {
    subscription.get("trial_end").and_then(|t| t.as_i64()).map(|ts| {
        chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default()
    })
} else {
    None
};

let update = SubscriptionUpdate {
    user_id: self.extract_user_id_from_customer(customer_id),
    subscription_tier: current_tier,
    subscription_status: final_status,
    stripe_subscription_id: Some(subscription_id.to_string()),
    subscription_started_at: Some(chrono::Utc::now().to_rfc3339()),
    subscription_ends_at,
    trial_ends_at, // Include trial_ends_at instead of hardcoded None
};
```

## Testing

1. **Manual test:**
   - Create a test user with an active trial
   - Note the current `trial_ends_at` date in the database
   - Extend the trial in Stripe dashboard
   - Verify webhook is received and processed
   - Confirm `trial_ends_at` is updated in the database

2. **Edge cases to verify:**
   - Trial extension while status is still `trialing` ✓
   - Trial shortening (also a change in `trial_end`)
   - Trial removed entirely (`trial_end` becomes null)
   - Multiple rapid trial extensions

## Webhook Event Reference

Example `customer.subscription.updated` payload when trial is extended:

```json
{
  "type": "customer.subscription.updated",
  "data": {
    "object": {
      "id": "sub_xxx",
      "customer": "cus_xxx",
      "status": "trialing",
      "trial_end": 1730678400,  // New extended date
      "items": { ... },
      "previous_attributes": {
        "trial_end": 1728086400  // Old date
      }
    }
  }
}
```

## Workaround (Temporary)

If trial extensions are needed before this is implemented, manually update the database:

```sql
UPDATE users
SET trial_ends_at = datetime('now', '+30 days')
WHERE email = 'user@example.com';
```

## Related Files
- `backend/src/stripe_billing.rs` - Webhook handler
- `backend/src/metadata.rs` - Database queries (line 1692 filters by trial_ends_at)
- `backend/src/subscription.rs` - SubscriptionUpdate struct definition
