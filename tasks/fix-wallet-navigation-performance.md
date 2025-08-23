# Fix Wallet Navigation Performance

## Problem Summary
Navigation between wallets is extremely slow, with UI hanging for up to a minute before responding. The wallet detail page shows "Loading wallet..." for extended periods before displaying content.

## Root Causes Identified

### 1. Large Transaction History
- Wallet `l6refrr3` has 26 historical transactions
- Wallet `wgws4n7n` has 15 historical transactions  
- The `/wallets/{checksum}/detail` endpoint fetches up to 100 events by default with a complex JOIN query

### 2. Excessive Address Revelation
- Wallet `l6refrr3`: 501 external + 501 internal addresses (1,002 total)
- Custom stop gap of 500 addresses triggers massive address scanning
- Each address requires Electrum server queries

### 3. Next.js API Proxy Overhead
- All API calls go through `/api/[...slug]/route.ts` proxy
- Adds extra hop: Browser → Next.js → Backend
- Text conversion for request body adds latency

### 4. No Response Caching
- API proxy doesn't set cache headers
- Every navigation triggers fresh database queries
- No ETags or conditional requests

### 5. Synchronous Database Queries
- SQLite connection pool blocks during complex queries
- JOIN operations on large transaction_events table
- No database indexes on frequently queried columns

## Fix Plan

### Immediate Optimizations

1. **Add Database Indexes**
   - Index on `transaction_events.wallet_checksum` for faster filtering
   - Index on `transaction_events.transaction_time` for sorting
   - Composite index on `(wallet_checksum, transaction_time)` for optimal query performance

2. **Optimize API Proxy**
   - Add response caching with appropriate cache headers
   - Stream responses instead of buffering entire body
   - Add timeout handling to prevent hanging requests

3. **Limit Default Event Fetching**
   - Reduce default limit from 100 to 25 events
   - Add pagination for loading more events
   - Only fetch events when explicitly needed

4. **Fix Frontend Loading States**
   - Add proper loading skeleton while wallet is in 'pending' state
   - Don't wait for all data before showing initial UI
   - Show cached data immediately while fetching updates

5. **Optimize Wallet List Query**
   - Remove subquery for last_activity calculation
   - Cache wallet list data more aggressively
   - Add database index on wallets.user_id

### Implementation Steps

1. Create database migration to add indexes
2. Update API proxy to stream responses and add caching
3. Modify wallet detail endpoint to fetch fewer events by default
4. Update frontend to show skeleton UI for pending wallets
5. Add response caching headers to backend endpoints
6. Test performance with large wallets to verify improvements

## Affected Files

### Backend
- `/backend/src/metadata.rs` - Database queries and indexes
- `/backend/src/wallet.rs` - Wallet detail endpoint logic
- `/backend/src/api.rs` - API response headers
- `/backend/migrations/` - New migration for indexes

### Frontend  
- `/frontend/src/app/api/[...slug]/route.ts` - API proxy optimization
- `/frontend/src/app/wallets/[checksum]/page.tsx` - Loading states
- `/frontend/src/hooks/useWalletDetail.ts` - Data fetching logic

## Expected Improvements
- Wallet navigation: From ~60s to <2s
- Initial page load: From "Loading wallet..." hang to immediate skeleton UI
- Database queries: 10-100x faster with proper indexes
- API response time: 30-50% reduction with streaming and caching