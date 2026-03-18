use crate::api::AppServicesState;
use crate::extractors::AuthenticatedUser;
use crate::models::{
    CheckResult, CleanupReport, DatabaseHealthResponse, DuplicatesReport, ErrorResponse,
    IntegrityChecks, IntegrityCheckRequest, IntegrityReportResponse, OrphanedRecordsReport,
    PoolHealth,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use tracing::{info, warn};

/// Build the full database health report (shared by both endpoints)
async fn build_health_report(app_services: &AppServicesState) -> Result<DatabaseHealthResponse, Response> {
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
        warn!("Failed to get schema version: {}", e);
        "unknown".to_string()
    });

    // SQLite integrity
    let sqlite_results = db.check_sqlite_integrity().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("SQLite integrity check failed: {}", e))),
        )
            .into_response()
    })?;
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
    let fk_violations = db.check_foreign_keys().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("Foreign key check failed: {}", e))),
        )
            .into_response()
    })?;
    let foreign_keys = CheckResult {
        status: if fk_violations.is_empty() { "pass" } else { "fail" }.to_string(),
        message: if fk_violations.is_empty() {
            "No foreign key violations".to_string()
        } else {
            format!("{} violation(s) found", fk_violations.len())
        },
        details: fk_violations
            .iter()
            .map(|v| format!("table={}, rowid={}, parent={}", v.table, v.rowid, v.parent))
            .collect(),
    };

    // Orphaned records (run concurrently)
    let (orphaned_contacts_r, orphaned_methods_r, orphaned_logs_r, orphaned_txs_r, orphaned_alerts_r, orphaned_alert_logs_r) =
        tokio::try_join!(
            db.find_orphaned_contacts(),
            db.find_orphaned_notification_methods(),
            db.find_orphaned_notification_logs(),
            db.find_orphaned_transactions(),
            db.find_orphaned_balance_alerts(),
            db.find_orphaned_balance_alert_notification_logs(),
        ).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Orphan check failed: {}", e))),
            )
                .into_response()
        })?;
    let orphaned_contacts = orphaned_contacts_r.len();
    let orphaned_methods = orphaned_methods_r.len();
    let orphaned_logs = orphaned_logs_r.len();
    let orphaned_txs = orphaned_txs_r.len();
    let orphaned_alerts = orphaned_alerts_r.len();
    let orphaned_alert_logs = orphaned_alert_logs_r.len();
    let total_orphans = orphaned_contacts + orphaned_methods + orphaned_logs + orphaned_txs + orphaned_alerts + orphaned_alert_logs;

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
    let dup_methods = db.find_duplicate_notification_methods().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("Duplicate notification methods check failed: {}", e))),
        )
            .into_response()
    })?.len();

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
    State(app_services): State<AppServicesState>,
) -> Response {
    if !user.is_admin {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::coded("access_denied", "Admin access required")),
        )
            .into_response();
    }

    match build_health_report(&app_services).await {
        Ok(report) => Json(report).into_response(),
        Err(err_response) => err_response,
    }
}

/// POST /api/admin/database/integrity - Full integrity check with optional auto-fix (admin only)
pub async fn run_integrity_check(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    request: Option<Json<IntegrityCheckRequest>>,
) -> Response {
    if !user.is_admin {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::coded("access_denied", "Admin access required")),
        )
            .into_response();
    }

    let auto_fix = request.map(|r| r.auto_fix).unwrap_or(false);

    let cleanup = if auto_fix {
        warn!("Admin user {} triggered database auto-fix cleanup", user.user_id);
        let db = &app_services.metadata_db;

        match db.run_cleanup().await {
            Ok(counts) => {
                let total = counts.contacts_deleted
                    + counts.methods_deleted
                    + counts.logs_deleted
                    + counts.alert_logs_deleted
                    + counts.alerts_deleted
                    + counts.transactions_deleted;
                if total > 0 {
                    info!(
                        "Database cleanup complete: {} records deleted (contacts={}, methods={}, logs={}, alert_logs={}, alerts={}, transactions={})",
                        total, counts.contacts_deleted, counts.methods_deleted, counts.logs_deleted,
                        counts.alert_logs_deleted, counts.alerts_deleted, counts.transactions_deleted
                    );
                }

                Some(CleanupReport {
                    orphaned_contacts_deleted: counts.contacts_deleted,
                    orphaned_methods_deleted: counts.methods_deleted,
                    orphaned_logs_deleted: counts.logs_deleted,
                    orphaned_balance_alert_notification_logs_deleted: counts.alert_logs_deleted,
                    orphaned_alerts_deleted: counts.alerts_deleted,
                    orphaned_transactions_deleted: counts.transactions_deleted,
                    total_deleted: total,
                })
            }
            Err(e) => {
                warn!("Database cleanup failed: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!("Database cleanup failed: {}", e))),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    // Build health report after cleanup so it reflects the post-cleanup state
    let health = match build_health_report(&app_services).await {
        Ok(report) => report,
        Err(err_response) => return err_response,
    };

    Json(IntegrityReportResponse { health, cleanup }).into_response()
}
