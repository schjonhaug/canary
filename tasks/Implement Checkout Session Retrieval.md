# Implement Checkout Session Retrieval Using Client Service

## Location
- `backend/src/stripe_billing.rs:903`

## Current Issue
The `get_checkout_session` method returns placeholder data instead of making actual Stripe API calls:
```rust
// TODO: Implement using our client service
// For now, return a placeholder
```

## Technical Details
This method should retrieve actual checkout session details from Stripe using the existing `StripeClientService`, but currently returns hardcoded placeholder data.

## Context
The system has a `StripeClientService` that handles Stripe API communication, but this specific method (`get_checkout_session`) isn't implemented yet. The placeholder suggests this functionality was planned but not completed.

## Implementation Requirements
1. Use the existing `StripeClientService` to make GET request to Stripe API
2. Endpoint: `GET /v1/checkout/sessions/{session_id}`
3. Parse the response into the existing `CheckoutSessionDetails` struct
4. Handle error cases (session not found, API failures)
5. Remove the placeholder implementation

## Current Placeholder Returns
```rust
CheckoutSessionDetails {
    session_id: session_id.to_string(),
    customer_id: None,
    subscription_id: None,
    status: Some("pending".to_string()),
}
```

## Expected Stripe Response Fields
- `id`: session ID
- `customer`: Stripe customer ID  
- `subscription`: Stripe subscription ID (if applicable)
- `payment_status`: completion status
- `mode`: subscription vs payment

## Integration
This method may be used for:
- Confirming checkout completion
- Debugging subscription issues
- Verifying payment status

## Priority
Low-Medium - Appears to be debugging/administrative functionality, not critical for core user flows.