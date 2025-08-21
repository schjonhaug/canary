# Add Display Target Logic for Notification Methods

## Location
- `backend/src/metadata.rs:982`

## Current Issue
The `display_target` field is hardcoded to `None` when loading notification methods:
```rust
display_target: None, // TODO: Add display_target logic if needed
```

## Technical Details
The `NotificationMethod` struct has a `display_target` field that should provide a user-friendly display version of the notification target, but it's currently unused.

## Context
The notification system has:
- `notification_target`: The raw target (e.g., "+4712345678", "john-en-8nt3y08q", "user@example.com")  
- `display_target`: User-friendly version (e.g., "+47 123 45 678", "john (English)", "user@example.com")

## Implementation Requirements
1. Analyze the `ProviderType` and `notification_target` to determine appropriate display format
2. For SMS: Format phone numbers with country code separation
3. For ntfy: Show topic with language info extracted
4. For email: Use as-is or potentially show just the local part for privacy

## Example Logic
```rust
let display_target = match provider_type {
    ProviderType::Sms => format_phone_number(&notification_target),
    ProviderType::Ntfy => format_ntfy_topic(&notification_target),  
    ProviderType::Email => notification_target.clone(), // or format_email for privacy
};
```

## Database Impact
This affects the API response for contact listings where users see their notification methods.

## Priority
Low - UX enhancement, system works without it but display would be more user-friendly.