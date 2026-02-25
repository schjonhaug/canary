# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview
This is the frontend application for Canary, a Bitcoin wallet management service. The frontend is built with Next.js 16 and React 19, serving as the user interface for wallet management, contact notifications, and subscription billing. It communicates with a Rust-based backend API service.

## Development Commands

### Frontend Development
```bash
# Development server (runs on port 3001)
pnpm dev

# Build for production
pnpm build

# Start production server
pnpm start

# Linting
pnpm lint

# Testing
pnpm test             # Run all tests once
pnpm test:watch       # Run tests in watch mode
```

## Architecture Overview

### Tech Stack
- **Next.js 16** with App Router (React 19)
- **TypeScript 5.9** for type safety
- **Tailwind CSS 4** for styling
- **Radix UI** components with shadcn/ui
- **Jest 30** with Testing Library for unit tests

### Key Architectural Patterns

#### Context-Based State Management
- **AuthContext** (`src/contexts/auth-context.tsx`): Manages user authentication, JWT tokens, billing status, and subscription state
- **WalletsContext** (`src/contexts/wallets-context.tsx`): Provides wallet data sharing across wallet-related components

#### API Communication
- **Centralized API client** (`src/lib/api.ts`): Singleton class handling all backend communication with automatic auth token management
- **Type-safe requests**: Full TypeScript interfaces for all API responses
- **Automatic token handling**: JWT tokens automatically included in requests when authenticated
- **Machine-readable error codes**: Backend returns `error_code` in error responses, mapped to translated strings via next-intl

#### Operating Modes
- **Self-hosted Mode**: When `NEXT_PUBLIC_CANARY_MODE=self-hosted`, single hardcoded admin user with no authentication, billing, or subscription limits
- **Cloud Mode**: When `NEXT_PUBLIC_CANARY_MODE=cloud` (default), complete email/password authentication with JWT, billing integration, and subscription management

#### Subscription Management
- **Tiered billing system**: Personal ($9/month) vs Team ($29/month) tiers with different limits
- **Proactive limit enforcement**: Checks limits before showing forms, not after submission
- **Billing context integration**: Real-time subscription status and billing information
- **Stripe integration**: Complete checkout, customer portal, and subscription lifecycle management

### Component Architecture

#### Shared Pricing Logic
- **pricing-data.ts**: Single source of truth for subscription features and tier definitions
- **Reusable components**: `PlanComparison` and `PlansModal` for consistent pricing display across the app

#### Modal System
- **Consistent modal patterns**: All create/edit operations use dialog-based modals
- **Context-aware upgrades**: Smart upgrade prompts that adapt based on current subscription limits

#### Testing Strategy
- **Comprehensive test coverage**: Tests for subscription limits, modal interactions, contact management, and auth flows
- **Real component testing**: Uses actual component rendering with mocked API calls
- **Edge case coverage**: Boundary conditions, error states, and user flow validation

### Type System

#### Core Types (`src/types/index.ts`)
- **Wallet**: Complete wallet metadata including balance, contacts, activity, and sync_status ('pending'|'ready')
- **Contact**: User contacts with multi-provider notification methods (SMS, email, ntfy)
- **NotificationMethod**: Flexible notification system supporting multiple providers per contact
- **TransactionEvent**: Transaction history with RBF/CPFP analysis and delivery status
- **BillingStatus**: Complete subscription and billing information

#### API Response Types
- **Type-safe API contracts**: All API responses have corresponding TypeScript interfaces
- **Normalized notification methods**: Database schema supports multiple notification providers per contact

### File Structure Patterns
```
src/
├── app/                    # Next.js App Router pages
│   ├── sign-in/           # Sign-in page
│   ├── sign-up/           # Sign-up and success pages
│   ├── forgot-password/   # Password reset pages
│   ├── reset-password/    # Password reset with token
│   ├── verify-email/      # Email verification
│   ├── settings/          # User settings and subscription management
│   └── wallets/           # Wallet management pages
├── components/            # Reusable UI components
│   ├── __tests__/         # Component test files
│   └── ui/                # Base shadcn/ui components
├── contexts/              # React context providers
├── hooks/                 # Custom React hooks
├── lib/                   # Shared utilities and API client
└── types/                 # TypeScript type definitions
```

## Testing Guidelines

### Test File Patterns
- **Component tests**: `component-name.test.tsx` files in `src/components/__tests__/`
- **Test files**: `contact-modal.test.tsx`, `contact-limit-enforcement.test.tsx`, `plans-modal-basic.test.tsx`, `inline-wallet-name-edit.test.tsx`, `wallet-contacts-list.test.tsx`
- **Test coverage areas**: Subscription limits, modal interactions, authentication flows, contact management, inline editing, wallet contacts
- **Mocking strategy**: Mock API calls, use real component rendering for integration-like tests

### Running Specific Tests
```bash
# Run specific test file
pnpm test contact-modal.test.tsx

# Run tests matching pattern
pnpm test --testNamePattern="subscription limits"

# Run tests in watch mode for specific file
pnpm test --watch contact-modal.test.tsx
```

## Environment Configuration

### Quick Start
Choose your deployment mode and copy the appropriate configuration:

- **self-hosted mode**: `cp .env.example.self-hosted .env.local`
- **cloud mode**: `cp .env.example.cloud .env.local`

### Configuration Files
- `.env.example.self-hosted` - Self-hosted single-user frontend configuration
- `.env.example.cloud` - Hosted service frontend configuration with Stripe billing

### Environment Variables
```bash
# Operating mode (required)
NEXT_PUBLIC_CANARY_MODE=cloud   # or 'self-hosted' for self-hosted mode

# Backend API URL (required)
NEXT_PUBLIC_API_URL=http://localhost:3000
```

### Operating Mode Details
- **Cloud Mode** (`NEXT_PUBLIC_CANARY_MODE=cloud`):
  - Full multi-user authentication with email/password
  - Stripe subscription billing and tier-based limits
  - Multiple notification providers (SMS, email, ntfy)
  - Development: Pre-configured test users (delivered+admin@resend.dev, delivered+alice@resend.dev, delivered+bob@resend.dev) with password `password123`
  - Production: Email verification required for new accounts
- **Self-hosted Mode** (`NEXT_PUBLIC_CANARY_MODE=self-hosted`):
  - Single hardcoded admin user (no authentication)
  - No subscription billing or limits
  - Only ntfy notifications (self-hostable)
  - Users provide their own Bitcoin/Electrum nodes

## Key Integration Points

### Backend API Endpoints
The frontend communicates with these main API categories:
- **Wallet Management**: CRUD operations for Bitcoin wallets
- **Contact Management**: Multi-provider notification setup (SMS, email, ntfy)
- **Authentication**: Registration, login, email verification, password reset
- **Billing**: Stripe integration for subscription management and customer portal
- **Transaction Events**: Real-time transaction analysis with RBF/CPFP detection

### Subscription System Integration
- **Tier-based limits**: Dynamic form availability based on subscription tier
- **Proactive enforcement**: Upgrade modals shown before hitting limits
- **Real-time billing status**: Subscription state updates from Stripe webhooks
- **Trial management**: 30-day Team tier trials for new users

## Development Workflow

### Working with Subscription Features
1. **Check billing context**: Always verify subscription limits before showing forms
2. **Use upgrade modals**: Implement context-aware upgrade prompts for limit enforcement
3. **Test both tiers**: Verify functionality works correctly for both Personal and Team tiers
4. **Mock billing in tests**: Use test users with different subscription states

### Adding New Components
1. **Follow naming conventions**: Use kebab-case for component files
2. **Include tests**: Add test file in `__tests__/` directory
3. **Use TypeScript**: Ensure all props and state are properly typed
4. **Follow accessibility**: Use Radix UI components for accessibility compliance

### API Integration Patterns
1. **Use the api client**: Always use the singleton `api` instance from `src/lib/api.ts`
2. **Handle auth automatically**: JWT tokens are managed automatically by the API client
3. **Type responses**: Use existing types or add new ones to `src/types/index.ts`
4. **Error handling**: Follow existing error handling patterns in components
5. **Wallet creation**: Use object parameters: `createWallet({name, descriptor, isFreshWallet?, scriptType?})`
6. **Test API calls**: Ensure test mocks match actual API signatures (use object parameters, not positional)