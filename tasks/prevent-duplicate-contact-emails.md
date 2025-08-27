# Prevent Duplicate Contact Emails and Phone Numbers Within Same Wallet

## Objective
Ensure that the same email address or phone number cannot be used by multiple contacts within the same wallet, while allowing the same email/phone across different wallets.

## Problem Description
Currently, users can create multiple contacts with the same email address or phone number within a single wallet (e.g., "Andreas" and "Work Andreas" both using andreas@schjonhaug.no or +4712345678 in the same wallet). This can cause confusion and duplicate notifications.

## Requirements

### 1. Database Constraint
- Add unique constraint on `(wallet_checksum, provider_type, notification_target)` for email and SMS provider types
- This prevents the same email or phone number from being used multiple times in the same wallet
- Allow the same email/phone across different wallets
- ntfy topics are excluded from this constraint as they are auto-generated and unique

### 2. API Validation
- Update contact creation endpoints to validate email and phone number uniqueness within wallet
- Return clear error message when duplicate email/phone is attempted
- Handle both manual contact creation and auto-contact creation
- Perform case-insensitive validation for emails
- Normalize phone numbers before validation (E.164 format)

### 3. Migration Strategy
- Create migration 004 to add `wallet_checksum` column to `contact_notification_methods` table
- Add unique constraint for email and SMS notifications within wallet
- Handle existing duplicate emails/phones gracefully:
  - Option A: Keep most recent contact, delete older duplicates
  - Option B: Append suffix to older contacts (e.g., "andreas+1@schjonhaug.no", "+4712345678-2")
  - Option C: Manual cleanup with admin tools

### 4. Frontend Updates
- Update contact creation forms to show email and phone number uniqueness validation
- Display helpful error messages when duplicate email/phone is entered
- Consider email/phone suggestion/autocomplete from existing contacts in other wallets
- Show clear validation messages for both email and SMS contact methods

## Database Schema Changes

### Analysis Results
The originally suggested approach using function-based unique indexes with subqueries is **not supported by SQLite**. SQLite does not allow expressions or subqueries in unique constraints/indexes.

### Recommended Migration: `004_prevent_duplicate_notification_targets.sql`
```sql
-- Add wallet_checksum column to contact_notification_methods table for direct constraint
ALTER TABLE contact_notification_methods ADD COLUMN wallet_checksum TEXT;

-- Update existing records with wallet_checksum from contacts table
UPDATE contact_notification_methods 
SET wallet_checksum = (
    SELECT wallet_checksum 
    FROM contacts 
    WHERE contacts.id = contact_notification_methods.contact_id
);

-- Create unique constraint for email and SMS notifications within same wallet
-- Note: ntfy is excluded as topics are auto-generated and guaranteed unique
CREATE UNIQUE INDEX idx_unique_wallet_notification_target 
ON contact_notification_methods (wallet_checksum, provider_type, notification_target) 
WHERE provider_type IN ('email', 'sms');

-- Add NOT NULL constraint to wallet_checksum (separate statement for SQLite compatibility)
-- This will prevent any future insertions without wallet_checksum
CREATE TABLE contact_notification_methods_new (
    id TEXT PRIMARY KEY,
    contact_id TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy', 'email')),
    notification_target TEXT NOT NULL,
    wallet_checksum TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE CASCADE,
    UNIQUE(contact_id, provider_type, notification_target)
);

-- Copy data
INSERT INTO contact_notification_methods_new 
SELECT id, contact_id, provider_type, notification_target, wallet_checksum, created_at 
FROM contact_notification_methods;

-- Replace table
DROP TABLE contact_notification_methods;
ALTER TABLE contact_notification_methods_new RENAME TO contact_notification_methods;

-- Recreate indexes
CREATE INDEX idx_contact_notification_methods_contact_id ON contact_notification_methods (contact_id);
CREATE INDEX idx_contact_notification_methods_provider_type ON contact_notification_methods (provider_type);
CREATE UNIQUE INDEX idx_unique_wallet_notification_target 
ON contact_notification_methods (wallet_checksum, provider_type, notification_target) 
WHERE provider_type IN ('email', 'sms');
```

## API Changes

### Contact Creation Validation
```rust
// Before creating contact, check for existing notification target in same wallet
pub async fn validate_notification_target_uniqueness(
    &self,
    wallet_checksum: &str,
    provider_type: &str,
    notification_target: &str
) -> Result<bool> {
    // Check if email/phone already exists for any contact in this wallet
    // Perform case-insensitive comparison for emails
    // Normalize phone numbers to E.164 format before comparison
}

pub async fn get_duplicate_notification_targets(
    &self,
    wallet_checksum: &str,
    notification_methods: &[(ProviderType, String)]
) -> Result<Vec<String>> {
    // Return list of notification targets that already exist in wallet
    // Used for batch validation and better error messages
}
```

### Error Handling
- Return specific error code for duplicate email/phone attempts
- Include provider type (email/SMS) in error message  
- List all duplicate targets when multiple conflicts exist
- Provide helpful suggestions for resolving conflicts

## Edge Cases to Consider

1. **Auto-created contacts**: Ensure system doesn't create duplicate emails when auto-creating user contacts
2. **Case sensitivity**: Handle email case variations (test@example.com vs TEST@example.com) 
3. **Phone number normalization**: Handle various phone number formats (+47 123 45 678 vs +4712345678)
4. **Email validation**: Ensure valid email format before uniqueness check
5. **Phone validation**: Ensure valid E.164 phone number format before uniqueness check
6. **Bulk operations**: Handle batch contact creation with duplicate detection
7. **ntfy topics**: Exclude auto-generated ntfy topics from duplicate validation
8. **Mixed provider types**: Allow same string for different providers (email vs phone vs ntfy)
9. **International phone numbers**: Handle country codes and formatting correctly

## Testing Requirements

1. **Unit Tests**
   - Test unique constraint enforcement for both emails and phone numbers
   - Test API validation logic with case variations and number formats
   - Test migration with existing duplicate data
   - Test normalization functions for emails (case) and phones (E.164)

2. **Integration Tests**
   - Test contact creation with duplicate emails within same wallet (should fail)
   - Test contact creation with duplicate phone numbers within same wallet (should fail)
   - Test auto-contact creation behavior with duplicate prevention
   - Test cross-wallet email/phone usage (should be allowed)
   - Test ntfy topic creation (should always work - not subject to duplicate rules)
   - Test mixed provider types (same string as email and phone should work)

3. **Manual Testing**
   - Create contacts with same email in same wallet (should fail)
   - Create contacts with same phone in same wallet (should fail)
   - Create contacts with same email/phone in different wallets (should succeed)
   - Test error messages and user experience for both email and SMS duplicates
   - Test case variations: test@example.com vs TEST@example.com (should fail)
   - Test phone format variations: +4712345678 vs +47 123 45 678 (should fail)

## Implementation Priority
Medium - Important for user experience but not critical for core functionality

## Implementation Notes

### Technical Findings
- **SQLite Limitation**: Function-based unique indexes with subqueries are not supported
- **Schema Change Required**: Must add `wallet_checksum` column to `contact_notification_methods` table
- **Normalization Required**: Emails need case-insensitive comparison, phones need E.164 normalization
- **Provider-Specific Rules**: ntfy topics are excluded from duplicate validation as they're auto-generated

### Implementation Approach
- Database constraint prevents duplicates at the storage level
- API validation provides user-friendly error messages before database constraint violation
- Migration handles existing data gracefully
- Both emails and phone numbers are subject to uniqueness within wallet scope

### Impact Assessment  
- **Existing Workflows**: Minimal impact as duplicate notifications are generally undesirable
- **User Experience**: Improved by preventing confusion from duplicate notifications
- **Performance**: Negligible impact due to indexed constraint
- **Data Integrity**: Significantly improved by preventing logical duplicates

### Current Status
- Analysis complete
- Schema changes designed  
- API validation patterns identified
- Ready for implementation