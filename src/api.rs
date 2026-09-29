use axum::{
    extract::{Path, Query},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::files::{
    copy_item, create_directory, delete_item, list_directory, move_item, rename_item,
    ActionRequest, DeleteRequest, DirectoryListing, ListQuery, MkdirRequest, RenameRequest,
};
use crate::firewall::{get_firewall_overview, FirewallOverview};
use crate::services::{control_service, get_service_logs, get_services_overview, ServicesOverview};
use crate::storage::{get_storage_overview, trigger_disk_spindown, StorageOverview};
use crate::system::{get_gpu_info, get_system_info, GpuInfo, SystemInfo};
use crate::terminal::{autocomplete, execute_command, CompleteRequest, CompleteResponse, ExecRequest, ExecResponse};
use crate::updates::{apply_intelligent_update, check_updates, ApplyUpdateResult, UpdateCheckStatus};

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

#[derive(Debug, Deserialize)]
pub struct UpdateCheckQuery {
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyUpdateQuery {
    pub force_packages: Option<bool>,
}

pub fn api_routes() -> Router {
    Router::new()
        .route("/system", get(handle_system))
        .route("/gpu", get(handle_gpu))
        .route("/storage", get(handle_storage))
        .route("/services", get(handle_services))
        .route("/firewall", get(handle_firewall))
        .route("/logs", get(handle_logs))
        .route("/updates/status", get(handle_updates_status))
        .route("/updates/apply", post(handle_updates_apply))
        .route("/terminal/exec", post(handle_terminal_exec))
        .route("/terminal/complete", post(handle_terminal_complete))
        .route("/files/list", get(handle_files_list))
        .route("/files/mkdir", post(handle_files_mkdir))
        .route("/files/delete", post(handle_files_delete))
        .route("/files/rename", post(handle_files_rename))
        .route("/files/copy", post(handle_files_copy))
        .route("/files/move", post(handle_files_move))
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

async fn handle_updates_status(Query(params): Query<UpdateCheckQuery>) -> Json<ApiResponse<UpdateCheckStatus>> {
    let force = params.force.unwrap_or(false);
    let status = tokio::task::spawn_blocking(move || {
        check_updates(force)
    }).await.unwrap_or_else(|_| check_updates(false));

    Json(ApiResponse {
        success: true,
        data: Some(status),
        message: None,
    })
}

async fn handle_updates_apply(Query(params): Query<ApplyUpdateQuery>) -> Json<ApiResponse<ApplyUpdateResult>> {
    let force_pkgs = params.force_packages.unwrap_or(false);
    let result = tokio::task::spawn_blocking(move || {
        apply_intelligent_update(force_pkgs)
    }).await.unwrap_or_else(|e| ApplyUpdateResult {
        success: false,
        steps_executed: vec![],
        output_log: format!("Erreur interne du serveur lors de la tâche : {}", e),
        error: Some(e.to_string()),
    });

    Json(ApiResponse {
        success: result.success,
        data: Some(result),
        message: None,
    })
}

async fn handle_terminal_exec(Json(req): Json<ExecRequest>) -> Json<ApiResponse<ExecResponse>> {
    let res = tokio::task::spawn_blocking(move || {
        execute_command(req)
    }).await.unwrap_or_else(|e| ExecResponse {
        success: false,
        stdout: String::new(),
        stderr: format!("Erreur serveur interne : {}", e),
        exit_code: -1,
        cwd: "/home/chomiam".to_string(),
        duration_ms: 0,
    });

    Json(ApiResponse {
        success: res.success,
        data: Some(res),
        message: None,
    })
}

async fn handle_terminal_complete(Json(req): Json<CompleteRequest>) -> Json<ApiResponse<CompleteResponse>> {
    let res = tokio::task::spawn_blocking(move || {
        autocomplete(req)
    }).await.unwrap_or_else(|_| CompleteResponse {
        suggestions: vec![],
    });

    Json(ApiResponse {
        success: true,
        data: Some(res),
        message: None,
    })
}

// --------------------------------------------------------------------------
// GESTIONNAIRE DE FICHIERS (FILE MANAGER)
// --------------------------------------------------------------------------
async fn handle_files_list(Query(params): Query<ListQuery>) -> Json<ApiResponse<DirectoryListing>> {
    let res = tokio::task::spawn_blocking(move || {
        list_directory(params.path.as_deref())
    }).await;

    match res {
        Ok(Ok(listing)) => Json(ApiResponse {
            success: true,
            data: Some(listing),
            message: None,
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur interne : {}", e)),
        }),
    }
}

async fn handle_files_mkdir(Json(req): Json<MkdirRequest>) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || {
        create_directory(&req.path, &req.name)
    }).await;

    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e.to_string()),
        }),
    }
}

async fn handle_files_delete(Json(req): Json<DeleteRequest>) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || {
        delete_item(&req.path)
    }).await;

    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e.to_string()),
        }),
    }
}

async fn handle_files_rename(Json(req): Json<RenameRequest>) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || {
        rename_item(&req.path, &req.new_name)
    }).await;

    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e.to_string()),
        }),
    }
}

async fn handle_files_copy(Json(req): Json<ActionRequest>) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || {
        copy_item(&req.src_path, &req.dest_dir)
    }).await;

    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e.to_string()),
        }),
    }
}

async fn handle_files_move(Json(req): Json<ActionRequest>) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || {
        move_item(&req.src_path, &req.dest_dir)
    }).await;

    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e.to_string()),
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
