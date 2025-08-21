# Implement User Notifications for Trial Ending

## Location
- `backend/src/stripe_billing.rs:629`

## Current Issue
The system receives Stripe webhook for `customer.subscription.trial_will_end` (fired 3 days before trial ends) but doesn't notify the user:
```rust
// TODO: Send notification to user about trial ending in 3 days
```

## Technical Details
This webhook event is already being processed and logged, but no user notification is sent. This is a critical touchpoint in the subscription lifecycle.

## Context
Users on 30-day Team trials should be proactively notified that their trial is ending so they can:
1. Subscribe to continue service
2. Understand what happens when trial expires
3. Not be surprised by service changes

## Implementation Requirements
1. Extract user information from the Stripe subscription webhook data
2. Find the user record by `stripe_customer_id`
3. Choose notification method:
   - Email notification (if user has verified email)  
   - In-app notification flag
   - Both

## Email Content Requirements
- Subject: "Your Canary trial ends in 3 days"
- Clear explanation of what happens when trial ends
- Call-to-action button to subscribe
- Information about current usage (wallets, contacts)

## Database Changes
May need to add an `email_notifications` table or use the existing notification system if email provider is configured.

## Integration Points
- Use existing email provider (Resend) if available
- Use user's email from the `users` table
- Link to subscription upgrade page

## Priority  
Medium-High - Important for user retention and preventing churn at trial expiration.