# Contact Update Bug Investigation

## Issue Description
When attempting to enable ntfy.sh on an existing contact (Andreas) that already had SMS and email configured, the operation resulted in a 422 error and the contact was permanently deleted from the wallet.

**Wallet URL**: https://canarybitcoin.com/wallets/l6refrr3  
**Wallet Name**: "Enkeltsignatur (gammel, ikke i bruk)"

## Root Cause Analysis

### Critical Issue: Delete-Then-Create Pattern
The frontend's `ContactModal` component uses a dangerous update pattern:

1. **Current Flow** (contact-modal.tsx, lines 459-461):
   - First: Delete the existing contact
   - Then: Attempt to create a new contact with updated data
   - Problem: If creation fails, the contact is already deleted and cannot be recovered

```typescript
// For edit mode, delete first only after validation passes
if (isEditMode && editContact) {
    await api.deleteContact(walletChecksum, editContact.id)  // DELETES FIRST!
}
// ... then attempts to create new contact
await api.createContact(walletChecksum, name.trim(), language, notificationMethods)
```

### Why the 422 Error Occurred
Several potential causes:

1. **Database Constraint Violation**
   - There's a UNIQUE constraint on `(contact_id, provider_type, notification_target)` in the database
   - The auto-generated ntfy topic might have conflicted with another contact's topic

2. **Ntfy Topic Generation**
   - Topics are generated using: `{contact_name}-{language}-{wallet_checksum}`
   - Max 64 characters, sanitized to alphanumeric and hyphens
   - Contact name "Andreas" would generate: `andreas-{language}-{checksum}`

3. **Missing Update Endpoint**
   - Backend has no PUT/PATCH endpoint for contacts
   - Only supports CREATE (POST) and DELETE operations
   - Forces frontend to use delete-then-create pattern

## Impact
- **Data Loss**: Contact permanently deleted when update fails
- **No Recovery**: No rollback mechanism when creation fails
- **Poor UX**: User loses all contact configuration on failed updates

## Recommended Solutions

### Option 1: Add Proper Update Endpoint (Recommended)
- Create PUT endpoint: `/api/wallets/{checksum}/contacts/{contact_id}`
- Implement atomic update logic in backend
- Update frontend to use PUT for edits

### Option 2: Safer Frontend Pattern
- Try creating new contact first
- Only delete old contact after successful creation
- Add rollback on failure

### Option 3: Transactional Approach
- Store original contact data before any operations
- Implement automatic restore on failure
- Add proper error handling and user feedback

## Technical Details

### Database Schema
```sql
CREATE TABLE contact_notification_methods (
    id TEXT PRIMARY KEY,
    contact_id TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy', 'email')),
    notification_target TEXT NOT NULL,
    UNIQUE(contact_id, provider_type, notification_target)
);
```

### API Routes
- GET `/wallets/{checksum}/contacts` - List contacts
- POST `/wallets/{checksum}/contacts` - Create contact  
- DELETE `/wallets/{wallet_checksum}/contacts/{contact_id}` - Delete contact
- **MISSING**: PUT/PATCH for updates

## Immediate Action Items
1. Implement proper update endpoint in backend
2. Fix frontend to use atomic updates
3. Add transaction/rollback logic for data safety
4. Test with duplicate ntfy topics
5. Add logging for better debugging of 422 errors

## Files Affected
- `/opt/canary/frontend/src/components/contact-modal.tsx` - Contains buggy delete-then-create logic
- `/opt/canary/backend/src/api.rs` - Missing update endpoint
- `/opt/canary/backend/src/metadata.rs` - Needs update contact methods