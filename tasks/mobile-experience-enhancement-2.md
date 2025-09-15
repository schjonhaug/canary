
## Priority 1: Plans Modal Mobile Fix

### Problem
The wide plans modal (85vw) with comparison tables is challenging to use on small screens.

### Solution
Create mobile-specific plan comparison layout with vertical stacking

### Implementation Tasks
- [ ] Design mobile-first plan comparison layout
- [ ] Stack features vertically instead of side-by-side
- [ ] Improve pricing display hierarchy for mobile
- [ ] Add swipe between plans on mobile
- [ ] Optimize modal sizing for different screen sizes
- [ ] Ensure CTA buttons easily tappable

### Files to Modify
- `frontend/src/components/subscription/plans-modal.tsx`
- `frontend/src/components/subscription/plan-comparison.tsx`

### Acceptance Criteria
- Plan comparison readable on mobile
- All features clearly visible without horizontal scroll
- Pricing information prominent and clear
- Upgrade buttons easily accessible
