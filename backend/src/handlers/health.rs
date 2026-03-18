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
    let pool_status = if pool_report.idle_connections > 0 {
        "healthy"
    } else if pool_report.total_connections < pool_report.max_connections {
        "degraded"
    } else {
        "exhausted"
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

    // Orphaned records
    let orphaned_contacts = db.find_orphaned_contacts().await.map(|r| r.len()).unwrap_or(0);
    let orphaned_methods = db.find_orphaned_notification_methods().await.map(|r| r.len()).unwrap_or(0);
    let orphaned_logs = db.find_orphaned_notification_logs().await.map(|r| r.len()).unwrap_or(0);
    let orphaned_txs = db.find_orphaned_transactions().await.map(|r| r.len()).unwrap_or(0);
    let orphaned_alerts = db.find_orphaned_balance_alerts().await.map(|r| r.len()).unwrap_or(0);
    let total_orphans = orphaned_contacts + orphaned_methods + orphaned_logs + orphaned_txs + orphaned_alerts;

    let orphaned_records = OrphanedRecordsReport {
        status: if total_orphans == 0 { "pass" } else { "warn" }.to_string(),
        contacts: orphaned_contacts,
        notification_methods: orphaned_methods,
        notification_logs: orphaned_logs,
        transactions: orphaned_txs,
        balance_alerts: orphaned_alerts,
        total: total_orphans,
    };

    // Duplicates
    let dup_contacts = db.find_duplicate_contacts().await.map(|r| r.len()).unwrap_or(0);
    let dup_methods = db.find_duplicate_notification_methods().await.map(|r| r.len()).unwrap_or(0);
    let total_dups = dup_contacts + dup_methods;

    let duplicates = DuplicatesReport {
        status: if total_dups == 0 { "pass" } else { "warn" }.to_string(),
        duplicate_contacts: dup_contacts,
        duplicate_notification_methods: dup_methods,
        total: total_dups,
    };

    // Overall status
    let overall = if !sqlite_ok || !fk_violations.is_empty() {
        "unhealthy"
    } else if total_orphans > 0 || total_dups > 0 || pool_status != "healthy" {
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
    Json(request): Json<IntegrityCheckRequest>,
) -> Response {
    if !user.is_admin {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::coded("access_denied", "Admin access required")),
        )
            .into_response();
    }

    let health = match build_health_report(&app_services).await {
        Ok(report) => report,
        Err(err_response) => return err_response,
    };

    let cleanup = if request.auto_fix {
        info!("Running database auto-fix cleanup...");
        let db = &app_services.metadata_db;

        let contacts_deleted = db.cleanup_orphaned_contacts().await.unwrap_or_else(|e| {
            warn!("Failed to clean orphaned contacts: {}", e);
            0
        });
        let methods_deleted = db.cleanup_orphaned_notification_methods().await.unwrap_or_else(|e| {
            warn!("Failed to clean orphaned notification methods: {}", e);
            0
        });
        let logs_deleted = db.cleanup_orphaned_notification_logs().await.unwrap_or_else(|e| {
            warn!("Failed to clean orphaned notification logs: {}", e);
            0
        });
        let txs_deleted = db.cleanup_orphaned_transactions().await.unwrap_or_else(|e| {
            warn!("Failed to clean orphaned transactions: {}", e);
            0
        });
        let alerts_deleted = db.cleanup_orphaned_balance_alerts().await.unwrap_or_else(|e| {
            warn!("Failed to clean orphaned balance alerts: {}", e);
            0
        });

        let total = contacts_deleted + methods_deleted + logs_deleted + txs_deleted + alerts_deleted;
        if total > 0 {
            info!(
                "Database cleanup complete: {} records deleted (contacts={}, methods={}, logs={}, transactions={}, alerts={})",
                total, contacts_deleted, methods_deleted, logs_deleted, txs_deleted, alerts_deleted
            );
        }

        Some(CleanupReport {
            orphaned_contacts_deleted: contacts_deleted,
            orphaned_methods_deleted: methods_deleted,
            orphaned_logs_deleted: logs_deleted,
            orphaned_transactions_deleted: txs_deleted,
            orphaned_alerts_deleted: alerts_deleted,
            total_deleted: total,
        })
    } else {
        None
    };

    Json(IntegrityReportResponse { health, cleanup }).into_response()
}
