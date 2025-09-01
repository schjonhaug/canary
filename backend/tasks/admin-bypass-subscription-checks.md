# Task: Admin Users Should Bypass All Subscription Checks

## Problem
Currently, admin users are still subject to subscription status checks when syncing wallets. This causes issues in:
1. System tests - where we want to test Bitcoin functionality without Stripe dependencies
2. Production - where admins should have unrestricted access regardless of subscription status

## Current Behavior
The `get_wallets_for_tier_sync()` method in `metadata.rs` (line 1521-1546) checks:
- User must have `subscription_status` of 'active', 'trialing' (with valid trial_ends_at), or 'canceled' (with valid subscription_ends_at)
- These checks apply to ALL users, including admins

## Required Changes

### 1. Update `get_wallets_for_tier_sync()` in `src/metadata.rs`
Modify the SQL query to include an OR condition for admin users:

```sql
WHERE w.is_active = 1 AND w.status = 'ready' 
  AND u.subscription_tier = ?1
  AND (
    -- Admin users bypass all subscription checks
    u.is_admin = 1
    OR
    -- Regular users need valid subscription
    (
      -- Active subscriptions
      u.subscription_status = 'active'
      OR 
      -- Trial users within trial period  
      (u.subscription_status = 'trialing' AND datetime(u.trial_ends_at) > datetime('now'))
      OR
      -- Cancelled users still within their paid period
      (u.subscription_status = 'canceled' AND u.subscription_ends_at IS NOT NULL AND datetime(u.subscription_ends_at) > datetime('now'))
    )
  )
```

### 2. Update Test Setup in `system_tests/common/docker_environment.rs`
Change line 89-94 to create an admin test user:

```rust
// Create test user as admin to bypass subscription checks
let test_user_id = metadata_db.create_user(
    "test@example.com",
    "hashedpassword", 
    Some("Test User"),
    true  // email_verified - keep as true
).await?;

// Make the user an admin (this might need a separate method)
metadata_db.update_user_admin_status(&test_user_id, true).await?;
```

### 3. Add `update_user_admin_status()` method if it doesn't exist
In `src/metadata.rs`, add:

```rust
pub async fn update_user_admin_status(&self, user_id: &str, is_admin: bool) -> Result<()> {
    let pool = self.pool.clone();
    let user_id = user_id.to_string();
    
    spawn_blocking(move || -> Result<()> {
        let conn = pool.get()?;
        conn.execute(
            "UPDATE users SET is_admin = ?1 WHERE id = ?2",
            params![is_admin, user_id],
        )?;
        Ok(())
    })
    .await?
}
```

## Benefits
1. **System Tests**: Tests can focus on Bitcoin functionality without Stripe dependencies
2. **Development**: Easier local development without needing Stripe webhooks
3. **Production**: Admin users have proper unrestricted access as expected
4. **Maintenance**: Cleaner separation between billing logic and core functionality

## Testing
After implementation:
1. Run `cargo test test_alice_sent_bob_direct_confirmed -- --test-threads=1 --ignored`
2. Verify admin users can sync wallets regardless of subscription status
3. Verify non-admin users still require valid subscription status

## Notes
- This change maintains backward compatibility
- Admin status should be carefully controlled in production
- Consider adding logging when admin bypass is used for audit trail