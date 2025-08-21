# Make Stripe Checkout URLs Configurable

## Location
- `backend/src/api.rs:3019-3020`

## Current Issue
Stripe checkout session success and cancel URLs are hardcoded to localhost:3001:
```rust
let success_url = "http://localhost:3001/settings/subscription?success=true"; // TODO: Make configurable
let cancel_url = "http://localhost:3001/settings/subscription?cancelled=true"; // TODO: Make configurable
```

## Technical Details
These URLs are used in the `create_checkout_session` endpoint when users upgrade their subscription. Currently hardcoded for development environment.

## Implementation Requirements
1. Add new environment variables:
   - `FRONTEND_URL` or `CANARY_FRONTEND_URL` 
   - Should default to `http://localhost:3001` for development
   - Should support production URLs like `https://app.canarybitcoin.com`

2. Update both `.env.example.foss` and `.env.example.saas` with the new variable

3. Modify the checkout session creation to use the configurable URL:
   ```rust
   let frontend_url = env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3001".to_string());
   let success_url = format!("{}/settings/subscription?success=true", frontend_url);
   let cancel_url = format!("{}/settings/subscription?cancelled=true", frontend_url);
   ```

## Context
This is part of the Stripe billing integration where users are redirected back to the frontend after completing or canceling the checkout process. Essential for production deployment.

## Priority
Medium - Required for production deployment but doesn't affect development.