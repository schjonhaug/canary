# Database Integrity Checks

## Objective
Add database health monitoring and integrity checks to detect and report data consistency issues.

## Requirements

### 1. Health Check Endpoint
- Create `/api/health/database` endpoint
- Check for:
  - Orphaned contacts (referencing non-existent wallets)
  - Duplicate contacts for the same wallet
  - Foreign key enforcement status
  - Database connection pool health
  - Schema version consistency

### 2. Startup Checks
- Run integrity checks on application startup
- Log warnings for any issues found
- Optional: Auto-fix safe issues (with configuration flag)

### 3. Monitoring Integration
- Export metrics for monitoring systems
- Track:
  - Number of orphaned records
  - Number of duplicate contacts
  - Foreign key violations
  - Database connection pool stats

### 4. Admin Tools
- Admin endpoint to trigger manual integrity check
- Detailed report generation
- Safe cleanup operations with audit logging

## Implementation Notes
- Should not impact normal operation performance
- Consider read-only checks during high traffic
- Implement rate limiting for manual checks
- Add configuration for automatic remediation

## Priority
Medium - Important for production reliability but not blocking core functionality