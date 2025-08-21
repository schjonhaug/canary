# Set Subscription End Date from Stripe Data

## Location  
- `backend/src/api.rs:3405`

## Current Issue
When processing Stripe webhooks for subscription updates, the `subscription_ends_at` field is not being set from Stripe's subscription data:
```rust
// TODO: Also need to set subscription_ends_at based on Stripe subscription end date
```

## Technical Details
This occurs in the webhook processing logic when handling subscription updates. The system updates subscription status but doesn't capture when the subscription actually ends.

## Context
This is in the `handle_subscription_update` logic where subscriptions are being cancelled but the system needs to know the exact end date to:
1. Maintain access until the subscription period ends
2. Show users when their access expires
3. Properly handle the transition from active to expired

## Implementation Requirements
1. Extract the subscription end date from Stripe webhook data
2. Parse the timestamp (likely Unix timestamp from Stripe)
3. Convert to DateTime format for SQLite storage
4. Update the `update_user_subscription_status` call to also set `subscription_ends_at`

## Stripe Data Structure
Need to examine the Stripe subscription object structure to find the correct field (likely `current_period_end` or `ended_at`).

## Impact
Currently users might lose access immediately upon cancellation rather than maintaining access until their paid period ends, which is incorrect billing behavior.

## Priority
High - This affects billing accuracy and user experience with subscription cancellations.