# Prevent Duplicate Contact Emails Within Same Wallet

## Objective
Ensure that the same email address cannot be used by multiple contacts within the same wallet, while allowing the same email to be used across different wallets.

## Problem Description
Currently, users can create multiple contacts with the same email address within a single wallet (e.g., "Andreas" and "Work Andreas" both using andreas@schjonhaug.no in the same wallet). This can cause confusion and duplicate notifications.

## Requirements

### 1. Database Constraint
- Add unique constraint on `(wallet_checksum, notification_target)` for email provider type
- This prevents the same email from being used multiple times in the same wallet
- Allow the same email across different wallets

### 2. API Validation
- Update contact creation endpoints to validate email uniqueness within wallet
- Return clear error message when duplicate email is attempted
- Handle both manual contact creation and auto-contact creation

### 3. Migration Strategy
- Create migration to add the unique constraint
- Handle existing duplicate emails gracefully:
  - Option A: Keep most recent contact, delete older duplicates
  - Option B: Append suffix to older contacts (e.g., "andreas+1@schjonhaug.no")
  - Option C: Manual cleanup with admin tools

### 4. Frontend Updates
- Update contact creation forms to show email uniqueness validation
- Display helpful error messages when duplicate email is entered
- Consider email suggestion/autocomplete from existing contacts in other wallets

## Database Schema Changes

### New Migration: `005_unique_contact_emails.sql`
```sql
-- Create unique index for email notification methods within same wallet
CREATE UNIQUE INDEX idx_unique_wallet_email 
ON contact_notification_methods (
    (SELECT wallet_checksum FROM contacts WHERE id = contact_id),
    notification_target
) 
WHERE provider_type = 'email';
```

Alternative approach using composite unique constraint:
```sql
-- Add wallet_checksum column to notification_methods table for direct constraint
-- This would require more significant schema changes
```

## API Changes

### Contact Creation Validation
```rust
// Before creating contact, check for existing email in same wallet
pub async fn validate_email_uniqueness(
    &self,
    wallet_checksum: &str,
    email: &str
) -> Result<bool> {
    // Check if email already exists for any contact in this wallet
}
```

### Error Handling
- Return specific error code for duplicate email attempts
- Include suggested alternatives in error response

## Edge Cases to Consider

1. **Auto-created contacts**: Ensure system doesn't create duplicate emails
2. **Case sensitivity**: Handle email case variations (test@example.com vs TEST@example.com)
3. **Email validation**: Ensure valid email format before uniqueness check
4. **Bulk operations**: Handle batch contact creation with duplicate detection

## Testing Requirements

1. **Unit Tests**
   - Test unique constraint enforcement
   - Test API validation logic
   - Test migration with existing duplicate data

2. **Integration Tests**
   - Test contact creation with duplicate emails
   - Test auto-contact creation behavior
   - Test cross-wallet email usage (should be allowed)

3. **Manual Testing**
   - Create contacts with same email in same wallet (should fail)
   - Create contacts with same email in different wallets (should succeed)
   - Test error messages and user experience

## Implementation Priority
Medium - Important for user experience but not critical for core functionality

## Notes
- Consider if similar constraints needed for phone numbers/SMS
- Evaluate impact on existing notification workflows
- Consider user education about email uniqueness within wallets