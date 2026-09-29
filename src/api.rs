use axum::{
    extract::{Path, Query},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::firewall::{get_firewall_overview, FirewallOverview};
use crate::services::{control_service, get_service_logs, get_services_overview, ServicesOverview};
use crate::storage::{get_storage_overview, trigger_disk_spindown, StorageOverview};
use crate::system::{get_gpu_info, get_system_info, GpuInfo, SystemInfo};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub unit: Option<String>,
    pub lines: Option<usize>,
}

pub fn api_routes() -> Router {
    Router::new()
        .route("/system", get(handle_system))
        .route("/gpu", get(handle_gpu))
        .route("/storage", get(handle_storage))
        .route("/services", get(handle_services))
        .route("/firewall", get(handle_firewall))
        .route("/logs", get(handle_logs))
        .route("/service/:unit/:action", post(handle_service_action))
        .route("/storage/:disk/spindown", post(handle_disk_spindown))
}

async fn handle_system() -> Json<ApiResponse<SystemInfo>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_system_info()),
        message: None,
    })
}

async fn handle_gpu() -> Json<ApiResponse<GpuInfo>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_gpu_info()),
        message: None,
    })
}

async fn handle_storage() -> Json<ApiResponse<StorageOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_storage_overview()),
        message: None,
    })
}

async fn handle_services() -> Json<ApiResponse<ServicesOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_services_overview()),
        message: None,
    })
}

async fn handle_firewall() -> Json<ApiResponse<FirewallOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_firewall_overview()),
        message: None,
    })
}

async fn handle_logs(Query(params): Query<LogsQuery>) -> Json<ApiResponse<String>> {
    let unit = params.unit.unwrap_or_else(|| "sshd".to_string());
    let lines = params.lines.unwrap_or(50);

    match get_service_logs(&unit, lines) {
        Ok(logs) => Json(ApiResponse {
            success: true,
            data: Some(logs),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_service_action(Path((unit, action)): Path<(String, String)>) -> Json<ApiResponse<String>> {
    match control_service(&unit, &action) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_disk_spindown(Path(disk): Path<String>) -> Json<ApiResponse<String>> {
    match trigger_disk_spindown(&disk) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}
