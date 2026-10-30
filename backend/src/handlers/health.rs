//! Admin database health and integrity handlers.

use super::error::{ApiError, ApiResult, OrInternal};
use crate::api::{AppServicesState, AppState};
use crate::auth::AuthUser;
use crate::extractors::AuthenticatedUser;
use crate::models::{
    CheckResult, CleanupReport, DatabaseHealthResponse, DuplicatesReport, IntegrityCheckRequest,
    IntegrityChecks, IntegrityReportResponse, OrphanedRecordsReport, PoolHealth,
};
use axum::{extract::State, http::StatusCode, response::Json};
use std::time::Duration;
use tracing::{info, warn};

const DATABASE_HEALTH_RATE_LIMIT_SCOPE: &str = "database_health";
const DATABASE_INTEGRITY_RATE_LIMIT_SCOPE: &str = "database_integrity";
const MAX_DATABASE_HEALTH_REQUESTS_PER_WINDOW: i64 = 6;
const DATABASE_HEALTH_RATE_LIMIT_WINDOW_MINUTES: i64 = 5;
const MAX_DATABASE_INTEGRITY_REQUESTS_PER_WINDOW: i64 = 2;
const DATABASE_INTEGRITY_RATE_LIMIT_WINDOW_MINUTES: i64 = 10;

fn require_admin(user: &AuthUser) -> ApiResult<()> {
    if user.is_admin {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "access_denied",
            "Admin access required",
        ))
    }
}

/// Rejects the request with 429 when the caller is over its quota.
async fn enforce_admin_rate_limit(
    app_state: &AppState,
    scope: &str,
    user_id: &str,
    max_attempts: i64,
    window_minutes: i64,
) -> ApiResult<()> {
    // Reuse the existing DB-backed limiter with admin-specific scopes. Keeping
    // it persistent is useful here because these endpoints are expensive enough
    // that a restart should not immediately reset an aggressive caller's quota.
    let fallback_window = Duration::from_secs(u64::try_from(window_minutes).unwrap_or(0) * 60);
    let check_fallback = || {
        app_state
            .admin_rate_limit_fallback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check(scope, user_id, max_attempts, fallback_window)
    };

    let decision = match app_state
        .app_services
        .metadata_db
        .check_endpoint_rate_limit(scope, user_id, max_attempts, window_minutes)
        .await
    {
        Ok(decision) => {
            // Shadow persistent decisions so a storage failure cannot reset an
            // active caller's effective quota. The persistent decision remains
            // authoritative while it is available.
            let _ = check_fallback();
            decision
        }
        Err(e) => {
            warn!(
                "Failed to check admin endpoint rate limit for scope {scope}; using in-memory fallback: {e}"
            );
            check_fallback()
        }
    };

    if decision.allowed {
        return Ok(());
    }
    let retry_after_secs = decision.retry_after_seconds.unwrap_or(window_minutes * 60);
    Err(ApiError::too_many_requests(
        "admin_endpoint_rate_limit",
        "Too many admin endpoint requests. Please try again later.",
        u64::try_from(retry_after_secs).unwrap_or(0),
    ))
}

/// Build the full database health report (shared by both endpoints)
async fn build_health_report(app_services: &AppServicesState) -> ApiResult<DatabaseHealthResponse> {
    let db = &app_services.metadata_db;

    // Pool health (synchronous, no DB query)
    let pool_report = db.check_pool_health();
    let pool_status = if pool_report.idle_connections == 0
        && pool_report.total_connections == pool_report.max_connections
    {
        "saturated"
    } else if pool_report.idle_connections == 0 && pool_report.total_connections > 0 {
        "busy"
    } else {
        "healthy"
    };

    // Schema version
    let schema_version = db.get_schema_version().await.unwrap_or_else(|e| {
        warn!("Failed to get schema version: {e}");
        "unknown".to_string()
    });

    // SQLite integrity
    let sqlite_results = db
        .check_sqlite_integrity()
        .await
        .or_internal("SQLite integrity check failed")?;
    let sqlite_ok = sqlite_results.len() == 1 && sqlite_results[0] == "ok";
    let sqlite_integrity = CheckResult {
        status: if sqlite_ok { "pass" } else { "fail" }.to_string(),
        message: if sqlite_ok {
            "SQLite integrity check passed".to_string()
        } else {
            format!("{} issue(s) found", sqlite_results.len())
        },
        details: if sqlite_ok { vec![] } else { sqlite_results },
    };

    // Foreign key check
    let fk_violations = db
        .check_foreign_keys()
        .await
        .or_internal("Foreign key check failed")?;
    let foreign_keys = CheckResult {
        status: if fk_violations.is_empty() {
            "pass"
        } else {
            "fail"
        }
        .to_string(),
        message: if fk_violations.is_empty() {
            "No foreign key violations".to_string()
        } else {
            format!("{} violation(s) found", fk_violations.len())
        },
        details: fk_violations
            .iter()
            .map(|v| match v.rowid {
                Some(rowid) => format!("table={}, rowid={}, parent={}", v.table, rowid, v.parent),
                None => format!("table={}, parent={}", v.table, v.parent),
            })
            .collect(),
    };

    // Orphaned records (run concurrently)
    let (
        orphaned_contacts_r,
        orphaned_methods_r,
        orphaned_logs_r,
        orphaned_txs_r,
        orphaned_alerts_r,
        orphaned_alert_logs_r,
    ) = tokio::try_join!(
        db.find_orphaned_contacts(),
        db.find_orphaned_notification_methods(),
        db.find_orphaned_notification_logs(),
        db.find_orphaned_transactions(),
        db.find_orphaned_balance_alerts(),
        db.find_orphaned_balance_alert_notification_logs(),
    )
    .or_internal("Orphan check failed")?;
    let orphaned_contacts = orphaned_contacts_r.len();
    let orphaned_methods = orphaned_methods_r.len();
    let orphaned_logs = orphaned_logs_r.len();
    let orphaned_txs = orphaned_txs_r.len();
    let orphaned_alerts = orphaned_alerts_r.len();
    let orphaned_alert_logs = orphaned_alert_logs_r.len();
    let total_orphans = orphaned_contacts
        + orphaned_methods
        + orphaned_logs
        + orphaned_txs
        + orphaned_alerts
        + orphaned_alert_logs;

    let orphaned_records = OrphanedRecordsReport {
        status: if total_orphans == 0 { "pass" } else { "warn" }.to_string(),
        contacts: orphaned_contacts,
        notification_methods: orphaned_methods,
        notification_logs: orphaned_logs,
        transactions: orphaned_txs,
        balance_alerts: orphaned_alerts,
        balance_alert_notification_logs: orphaned_alert_logs,
        total: total_orphans,
    };

    // Duplicates
    let dup_methods = db
        .find_duplicate_notification_methods()
        .await
        .or_internal("Duplicate notification methods check failed")?
        .len();

    let duplicates = DuplicatesReport {
        status: if dup_methods == 0 { "pass" } else { "warn" }.to_string(),
        duplicate_notification_methods: dup_methods,
        total: dup_methods,
    };

    // Overall status
    let overall = if !sqlite_ok || !fk_violations.is_empty() {
        "unhealthy"
    } else if total_orphans > 0 || dup_methods > 0 || pool_status != "healthy" {
        "degraded"
    } else {
        "healthy"
    };

    let timestamp = chrono::Utc::now().to_rfc3339();

    Ok(DatabaseHealthResponse {
        status: overall.to_string(),
        schema_version,
        pool: PoolHealth {
            total_connections: pool_report.total_connections,
            idle_connections: pool_report.idle_connections,
            max_connections: pool_report.max_connections,
            status: pool_status.to_string(),
        },
        checks: IntegrityChecks {
            sqlite_integrity,
            foreign_keys,
            orphaned_records,
            duplicates,
        },
        timestamp,
    })
}

/// GET /api/health/database - Database health check (admin only)
pub async fn get_database_health(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_state): State<AppState>,
) -> ApiResult<Json<DatabaseHealthResponse>> {
    require_admin(&user)?;
    enforce_admin_rate_limit(
        &app_state,
        DATABASE_HEALTH_RATE_LIMIT_SCOPE,
        &user.user_id,
        MAX_DATABASE_HEALTH_REQUESTS_PER_WINDOW,
        DATABASE_HEALTH_RATE_LIMIT_WINDOW_MINUTES,
    )
    .await?;

    let report = build_health_report(&app_state.app_services).await?;
    Ok(Json(report))
}

/// POST /api/admin/database/integrity - Full integrity check with optional auto-fix (admin only)
pub async fn run_integrity_check(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_state): State<AppState>,
    request: Option<Json<IntegrityCheckRequest>>,
) -> ApiResult<Json<IntegrityReportResponse>> {
    require_admin(&user)?;
    enforce_admin_rate_limit(
        &app_state,
        DATABASE_INTEGRITY_RATE_LIMIT_SCOPE,
        &user.user_id,
        MAX_DATABASE_INTEGRITY_REQUESTS_PER_WINDOW,
        DATABASE_INTEGRITY_RATE_LIMIT_WINDOW_MINUTES,
    )
    .await?;

    let auto_fix = request.is_some_and(|r| r.auto_fix);
    let cleanup = if auto_fix {
        Some(run_cleanup(&app_state.app_services, &user.user_id).await?)
    } else {
        None
    };

    // Build health report after cleanup so it reflects the post-cleanup state
    let health = build_health_report(&app_state.app_services).await?;
    Ok(Json(IntegrityReportResponse { health, cleanup }))
}

async fn run_cleanup(app_services: &AppServicesState, user_id: &str) -> ApiResult<CleanupReport> {
    warn!("Admin user {user_id} triggered database auto-fix cleanup");

    let counts = app_services
        .metadata_db
        .run_cleanup(user_id)
        .await
        .map_err(|e| {
            warn!("Database cleanup failed: {e}");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_cleanup_failed",
                "Database cleanup failed",
            )
        })?;

    let total = counts.total_deleted();
    if total > 0 {
        info!(
            "Database cleanup complete: {} records deleted (contacts={}, methods={}, logs={}, alert_logs={}, alerts={}, transactions={})",
            total,
            counts.contacts_deleted,
            counts.methods_deleted,
            counts.logs_deleted,
            counts.alert_logs_deleted,
            counts.alerts_deleted,
            counts.transactions_deleted
        );
    } else {
        info!("Database cleanup complete: no orphaned records found");
    }

    Ok(CleanupReport {
        orphaned_contacts_deleted: counts.contacts_deleted,
        orphaned_methods_deleted: counts.methods_deleted,
        orphaned_logs_deleted: counts.logs_deleted,
        orphaned_balance_alert_notification_logs_deleted: counts.alert_logs_deleted,
        orphaned_alerts_deleted: counts.alerts_deleted,
        orphaned_transactions_deleted: counts.transactions_deleted,
        total_deleted: total,
    })
}
