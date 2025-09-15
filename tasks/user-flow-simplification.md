# User Flow Simplification Tasks

## Overview
Critical improvements to reduce cognitive load and friction in key user workflows. These changes will improve user onboarding, reduce abandonment rates, and create a more intuitive experience.

## Priority 1: Simplified Contact Creation

### Problem
Current contact creation has complex multi-provider verification flows that create confusion and high abandonment rates. Users must navigate through SMS/email OTP verification which creates multiple friction points.

### Solution
Implement single-step contact creation for common cases with progressive disclosure for advanced options

### Implementation Tasks
- [ ] Create simplified contact creation flow for common use cases
- [ ] Implement progressive disclosure pattern for advanced notification options
- [ ] Add auto-detection logic for contact types (email/SMS/ntfy)
- [ ] Design clearer verification status indicators
- [ ] Add "Skip verification for now" option with later completion
- [ ] Implement smart defaults based on user context

### Files to Modify
- `frontend/src/components/contacts/contact-modal.tsx`
- Add: `frontend/src/components/contacts/simple-contact-form.tsx`
- Add: `frontend/src/components/contacts/verification-status.tsx`
- `frontend/src/hooks/useContactCreation.ts` (new)

### User Flow Improvements
**Before:** Contact Form → Provider Selection → Verification → Completion (4 steps)
**After:** Contact Form → Auto-detection → Optional Advanced → Completion (2-3 steps)

### Acceptance Criteria
- Contact creation completable in 2 steps for 80% of use cases
- Clear indication of verification requirements upfront
- Option to complete verification later without blocking contact creation
- Auto-detection works accurately for common contact formats

## Priority 1: Proactive Limit Management

### Problem
Users hit subscription limits unexpectedly, with upgrade modals appearing reactively after they've already been blocked. This creates frustration and poor user experience.

### Solution
Implement proactive limit awareness with gentle upgrade nudges before limits are reached

### Implementation Tasks
- [ ] Add usage indicators throughout the interface
- [ ] Create limit proximity warnings (at 80% capacity)
- [ ] Implement contextual upgrade suggestions
- [ ] Add usage dashboard in settings
- [ ] Create gentle upgrade nudges in key workflows
- [ ] Design seamless upgrade path without workflow disruption

### Files to Modify
- `frontend/src/components/ui/usage-indicator.tsx` (new)
- `frontend/src/components/subscription/upgrade-nudge.tsx` (new)
- `frontend/src/hooks/useLimits.ts` (new)
- `frontend/src/components/wallets/wallet-list.tsx`
- `frontend/src/components/contacts/contact-list.tsx`

### UI Elements to Add
- Usage bars showing "3/5 wallets" in wallet section
- Subtle badges indicating plan limits
- Contextual tooltips explaining limits
- Progressive disclosure of upgrade benefits

### Acceptance Criteria
- Users see their usage status before hitting limits
- Upgrade path clearly presented without being pushy
- Workflow continues smoothly after upgrade
- Usage indicators update in real-time

## Priority 1: Status Communication Clarity

### Problem
Wallet states (pending, inactive, syncing, ready) aren't clearly differentiated, creating uncertainty about wallet availability and sync status.

### Solution
Redesign wallet status indicators with better visual hierarchy and contextual help

### Implementation Tasks
- [ ] Create comprehensive status indicator component
- [ ] Add contextual tooltips explaining each status
- [ ] Implement progress indicators for sync operations
- [ ] Design clear visual hierarchy for status information
- [ ] Add estimated time remaining for pending operations
- [ ] Create consistent status patterns across all components

### Files to Modify
- `frontend/src/components/ui/status-indicator.tsx` (new)
- `frontend/src/components/wallets/wallet-card.tsx`
- `frontend/src/components/wallets/wallet-detail.tsx`
- Add: `frontend/src/lib/status-definitions.ts`

### Status Design System
- **Pending**: Spinner with "Setting up..." text and progress bar
- **Syncing**: Animated icon with "Syncing transactions..." and percentage
- **Ready**: Green checkmark with "Up to date" text
- **Error**: Warning icon with clear error message and action button
- **Inactive**: Gray state with "Paused" and resume option

### Acceptance Criteria
- Users understand wallet status at a glance
- Progress indicators show meaningful information
- Error states provide clear next steps
- Status updates reflect real backend state

## Priority 2: Onboarding Experience Enhancement

### Problem
Technical descriptor explanations may intimidate novice Bitcoin users during wallet creation, leading to abandonment.

### Solution
Provide progressive disclosure with simple "Quick Start" option alongside technical details

### Implementation Tasks
- [ ] Create simplified wallet creation flow for beginners
- [ ] Add progressive disclosure for technical details
- [ ] Implement guided tour for first-time users
- [ ] Add contextual help throughout onboarding
- [ ] Create wallet templates for common use cases
- [ ] Add success celebrations and next steps

### Files to Modify
- `frontend/src/components/wallets/create-wallet-modal.tsx`
- Add: `frontend/src/components/onboarding/quick-start.tsx`
- Add: `frontend/src/components/onboarding/guided-tour.tsx`
- Add: `frontend/src/components/onboarding/wallet-templates.tsx`

### Onboarding Flow Options
**Beginner Path:** Quick Setup → Generate Wallet → Success → Add Contacts
**Advanced Path:** Custom Descriptor → Technical Options → Manual Setup

### Acceptance Criteria
- New users can create wallet without technical knowledge
- Advanced users still have access to full customization
- Guided tour explains key concepts progressively
- Success rate for wallet creation increases

## Priority 2: Error Handling Consistency

### Problem
Multiple error states across forms aren't consistently handled, leading to poor user experience when things go wrong.

### Solution
Implement unified error handling patterns with clear recovery paths

### Implementation Tasks
- [ ] Create standard error component library
- [ ] Implement consistent error messaging patterns
- [ ] Add clear recovery actions for all error states
- [ ] Design offline/connectivity error handling
- [ ] Add form validation with helpful error messages
- [ ] Implement retry mechanisms with exponential backoff

### Files to Modify
- Add: `frontend/src/components/ui/error-boundary.tsx`
- Add: `frontend/src/components/ui/error-message.tsx`
- Add: `frontend/src/lib/error-handling.ts`
- Update all form components with consistent error patterns

### Error Categories
- **Network Errors**: Clear retry options with offline indicators
- **Validation Errors**: Inline messages with fix suggestions
- **Server Errors**: Helpful explanations with support contact
- **Permission Errors**: Clear upgrade paths or contact options

### Acceptance Criteria
- All error states have consistent visual treatment
- Users understand what went wrong and how to fix it
- Recovery actions work reliably
- Error messages are helpful, not technical

## Testing Requirements

### User Flow Testing
- [ ] A/B test simplified vs. current contact creation flow
- [ ] Measure completion rates for wallet creation
- [ ] Test upgrade conversion rates with proactive nudges
- [ ] Monitor abandonment points in key flows

### Usability Testing
- [ ] Test with non-technical Bitcoin users
- [ ] Validate error message comprehension
- [ ] Test status indicator understanding
- [ ] Verify progressive disclosure effectiveness

### Performance Testing
- [ ] Measure time to complete key tasks
- [ ] Test perceived performance improvements
- [ ] Validate real-time status updates
- [ ] Check impact on bundle size

## Success Metrics
- Reduced abandonment rates in key flows
- Improved task completion times
- Decreased support requests about confusion
- Higher user satisfaction scores
- Improved conversion rates for upgrades

## Implementation Priority
**Phase 2** - Critical for user retention and conversion, implement after mobile fixes in Phase 1.

## Dependencies
- Requires backend API for usage data
- Coordinated with mobile improvements from Phase 1
- May need new backend endpoints for proactive notifications