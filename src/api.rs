use axum::{
    extract::{Path, Query},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::documents::{get_document_info, get_document_pdf_path, DocumentInfoResponse};
use crate::docker_store::{
    control_docker_container, get_docker_logs, get_store_catalog, install_store_app,
    uninstall_store_app, ContainerActionRequest, InstallAppRequest, StoreCatalog,
    UninstallAppRequest,
};
use crate::services::{get_docker_containers, DockerContainer};
use crate::youtube::{cancel_youtube_job, clear_youtube_jobs, get_job_status, get_youtube_info, list_jobs, start_youtube_download, YoutubeDownloadRequest, YoutubeInfoRequest, YoutubeJobStatus, YoutubeVideoInfo};
use crate::trash::{delete_trash_item, empty_trash, get_trash_overview, restore_trash_item, TrashActionRequest, TrashOverview};
use crate::files::{
    copy_item, create_directory, delete_item, get_image_info, get_image_preview_path, list_directory, move_item, rename_item,
    ActionRequest, DeleteRequest, DirectoryListing, ImageInfoResponse, ListQuery, MkdirRequest, RenameRequest,
};
use crate::firewall::{get_firewall_overview, FirewallOverview};
use crate::services::{control_service, get_service_logs, get_services_overview, ServicesOverview};
use crate::storage::{create_raid, format_disk, get_raid_sync_progress, get_storage_overview, mount_volume, repair_path_permissions, trigger_disk_spindown, umount_volume, CreateRaidRequest, FormatDiskRequest, MountVolumeRequest, RaidSyncProgress, RepairPermissionsRequest, StorageOverview, UmountVolumeRequest};
use crate::system::{cancel_power, get_gpu_info, get_power_status, get_system_info, schedule_power, GpuInfo, ImmediatePowerRequest, PowerStatusResponse, SchedulePowerRequest, SystemInfo};
use crate::terminal::{autocomplete, execute_command, CompleteRequest, CompleteResponse, ExecRequest, ExecResponse};
use crate::updates::{apply_intelligent_update, check_updates, get_live_log, ApplyUpdateResult, UpdateCheckStatus};
use crate::hardware::{get_hardware_overview, HardwareOverview};
use crate::smart::{get_smart_overview, SmartOverview};
use crate::speedtest::{get_latest_speedtest, run_speedtest, SpeedtestResult};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImageViewQuery {
    pub path: String,
    pub thumb: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ImageInfoQuery {
    pub path: String,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct LiveLogsResponse {
    pub logs: String,
    pub is_updating: bool,
}

pub fn api_routes() -> Router {
    Router::new()
        .route("/system", get(handle_system))
        .route("/system/power/status", get(handle_power_status))
        .route("/system/power/immediate", post(handle_power_immediate))
        .route("/system/power/schedule", post(handle_power_schedule))
        .route("/system/power/cancel", post(handle_power_cancel))
        .route("/gpu", get(handle_gpu))
        .route("/storage", get(handle_storage))
        .route("/storage/raids/progress", get(handle_raid_progress))
        .route("/storage/disks/format", post(handle_format_disk))
        .route("/storage/raids/create", post(handle_create_raid))
        .route("/storage/mount", post(handle_mount_volume))
        .route("/storage/umount", post(handle_umount_volume))
        .route("/storage/permissions/repair", post(handle_repair_permissions))
        .route("/services", get(handle_services))
        .route("/docker/containers", get(handle_docker_containers))
        .route("/docker/containers/:name/action", post(handle_docker_container_action))
        .route("/docker/containers/:name/logs", get(handle_docker_container_logs))
        .route("/docker/store", get(handle_docker_store))
        .route("/docker/store/install", post(handle_docker_store_install))
        .route("/docker/store/uninstall", post(handle_docker_store_uninstall))
        .route("/firewall", get(handle_firewall))
        .route("/logs", get(handle_logs))
        .route("/updates/status", get(handle_updates_status))
        .route("/updates/apply", post(handle_updates_apply))
        .route("/updates/logs", get(handle_updates_logs))
        .route("/terminal/exec", post(handle_terminal_exec))
        .route("/terminal/complete", post(handle_terminal_complete))
        .route("/files/list", get(handle_files_list))
        .route("/files/mkdir", post(handle_files_mkdir))
        .route("/files/delete", post(handle_files_delete))
        .route("/files/rename", post(handle_files_rename))
        .route("/files/copy", post(handle_files_copy))
        .route("/files/move", post(handle_files_move))
        .route("/files/upload", post(handle_files_upload).layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/files/stream", get(handle_files_stream))
        .route("/files/read", get(handle_files_read))
        .route("/files/image-view", get(handle_files_image_view))
        .route("/files/image-info", get(handle_files_image_info))
        .route("/files/write", post(handle_files_write))
        .route("/files/trash", get(handle_trash_overview))
        .route("/files/trash/restore", post(handle_trash_restore))
        .route("/files/trash/delete", post(handle_trash_delete))
        .route("/files/trash/empty", post(handle_trash_empty))
        .route("/youtube/info", post(handle_youtube_info))
        .route("/youtube/download", post(handle_youtube_download))
        .route("/youtube/status/:job_id", get(handle_youtube_status))
                .route("/youtube/jobs", get(handle_youtube_jobs))
        .route("/youtube/cancel/:job_id", post(handle_youtube_cancel))
        .route("/youtube/clear", post(handle_youtube_clear))
        .route("/documents/preview", get(handle_document_preview))
        .route("/documents/:id/preview", get(handle_document_preview_by_id))
        .route("/documents/info", get(handle_document_info))
        .route("/service/:unit/:action", post(handle_service_action))
        .route("/storage/:disk/spindown", post(handle_disk_spindown))
        .route("/hardware", get(handle_hardware))
        .route("/smart", get(handle_smart))
        .route("/speedtest/latest", get(handle_speedtest_latest))
        .route("/speedtest/run", post(handle_speedtest_run))
}


async fn handle_power_status() -> Json<ApiResponse<PowerStatusResponse>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_power_status()),
        message: None,
    })
}

async fn handle_power_immediate(
    Json(req): Json<ImmediatePowerRequest>,
) -> Json<ApiResponse<()>> {
    let action = req.action.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
        let _ = std::process::Command::new("systemctl")
            .arg(if action == "reboot" { "reboot" } else { "poweroff" })
            .spawn();
    });

    Json(ApiResponse {
        success: true,
        data: None,
        message: Some(format!(
            "Ordre de {} envoyé au système",
            if req.action == "reboot" { "redémarrage" } else { "mise hors tension" }
        )),
    })
}

async fn handle_power_schedule(
    Json(req): Json<SchedulePowerRequest>,
) -> Json<ApiResponse<()>> {
    match schedule_power(req) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some(msg),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}

async fn handle_power_cancel() -> Json<ApiResponse<()>> {
    match cancel_power() {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some(msg),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
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

async fn handle_updates_logs() -> Json<ApiResponse<LiveLogsResponse>> {
    let (logs, is_updating) = get_live_log();
    Json(ApiResponse {
        success: true,
        data: Some(LiveLogsResponse { logs, is_updating }),
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


async fn handle_trash_overview() -> Json<ApiResponse<TrashOverview>> {
    match get_trash_overview() {
        Ok(data) => Json(ApiResponse {
            success: true,
            data: Some(data),
            message: None,
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}

async fn handle_trash_restore(
    Json(payload): Json<TrashActionRequest>,
) -> Json<ApiResponse<()>> {
    match restore_trash_item(&payload.id) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some(msg),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}

async fn handle_trash_delete(
    Json(payload): Json<TrashActionRequest>,
) -> Json<ApiResponse<()>> {
    match delete_trash_item(&payload.id) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some(msg),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}

async fn handle_trash_empty() -> Json<ApiResponse<()>> {
    match empty_trash() {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some(msg),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}


async fn handle_youtube_info(
    Json(payload): Json<YoutubeInfoRequest>,
) -> Json<ApiResponse<YoutubeVideoInfo>> {
    match get_youtube_info(&payload.url).await {
        Ok(info) => Json(ApiResponse {
            success: true,
            data: Some(info),
            message: None,
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}

async fn handle_youtube_download(
    Json(payload): Json<YoutubeDownloadRequest>,
) -> Json<ApiResponse<String>> {
    match start_youtube_download(payload) {
        Ok(job_id) => Json(ApiResponse {
            success: true,
            data: Some(job_id.clone()),
            message: Some(format!("Téléchargement lancé (ID: {})", job_id)),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}

async fn handle_youtube_status(
    Path(job_id): Path<String>,
) -> Json<ApiResponse<YoutubeJobStatus>> {
    match get_job_status(&job_id) {
        Some(job) => Json(ApiResponse {
            success: true,
            data: Some(job),
            message: None,
        }),
        None => Json(ApiResponse {
            success: false,
            data: None,
            message: Some("Téléchargement introuvable.".into()),
        }),
    }
}

async fn handle_youtube_jobs() -> Json<ApiResponse<Vec<YoutubeJobStatus>>> {
    Json(ApiResponse {
        success: true,
        data: Some(list_jobs()),
        message: None,
    })
}

async fn handle_youtube_cancel(
    Path(job_id): Path<String>,
) -> Json<ApiResponse<()>> {
    match cancel_youtube_job(&job_id) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some("Téléchargement annulé avec succès.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_youtube_clear() -> Json<ApiResponse<()>> {
    clear_youtube_jobs();
    Json(ApiResponse {
        success: true,
        data: Some(()),
        message: Some("Historique nettoyé.".into()),
    })
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
    let permanent = req.permanent.unwrap_or(false);
    let res = tokio::task::spawn_blocking(move || {
        delete_item(&req.path, permanent)
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

async fn handle_raid_progress() -> Json<ApiResponse<Option<RaidSyncProgress>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_raid_sync_progress()),
        message: None,
    })
}

async fn handle_repair_permissions(Json(payload): Json<RepairPermissionsRequest>) -> Json<ApiResponse<String>> {
    match repair_path_permissions(&payload) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_mount_volume(Json(payload): Json<MountVolumeRequest>) -> Json<ApiResponse<String>> {
    match mount_volume(&payload) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_umount_volume(Json(payload): Json<UmountVolumeRequest>) -> Json<ApiResponse<String>> {
    match umount_volume(&payload) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_format_disk(Json(payload): Json<FormatDiskRequest>) -> Json<ApiResponse<String>> {
    match format_disk(&payload) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_create_raid(Json(payload): Json<CreateRaidRequest>) -> Json<ApiResponse<String>> {
    match create_raid(&payload) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
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

async fn handle_hardware() -> Json<ApiResponse<HardwareOverview>> {
    let hw = tokio::task::spawn_blocking(get_hardware_overview).await.unwrap_or_else(|_| get_hardware_overview());
    Json(ApiResponse {
        success: true,
        data: Some(hw),
        message: None,
    })
}

async fn handle_smart() -> Json<ApiResponse<SmartOverview>> {
    let smart = tokio::task::spawn_blocking(get_smart_overview).await.unwrap_or_else(|_| get_smart_overview());
    Json(ApiResponse {
        success: true,
        data: Some(smart),
        message: None,
    })
}

async fn handle_speedtest_latest() -> Json<ApiResponse<Option<SpeedtestResult>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_latest_speedtest()),
        message: None,
    })
}

async fn handle_speedtest_run() -> Json<ApiResponse<SpeedtestResult>> {
    let res = tokio::task::spawn_blocking(run_speedtest).await.unwrap_or_else(|_| run_speedtest());
    Json(ApiResponse {
        success: true,
        data: Some(res),
        message: None,
    })
}


#[derive(Deserialize)]
pub struct UploadQuery {
    pub dir: Option<String>,
}

#[derive(Serialize)]
pub struct UploadedFileItem {
    pub name: String,
    pub bytes: u64,
}

async fn handle_files_upload(
    Query(params): Query<UploadQuery>,
    mut multipart: axum::extract::Multipart,
) -> Json<ApiResponse<Vec<UploadedFileItem>>> {
    use std::path::{Path, PathBuf};
    use tokio::io::AsyncWriteExt;

    let target_dir = crate::files::normalize_user_path(PathBuf::from(
        params.dir.unwrap_or_else(|| "/home/chomiam".to_string())
    ));
    if !target_dir.is_dir() {
        return Json(ApiResponse {
            success: false,
            data: None,
            message: Some("Dossier de destination introuvable.".into()),
        });
    }

    let mut uploaded = Vec::new();

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let raw_name = field.file_name().unwrap_or("fichier_sans_nom").to_string();
        let safe_name = Path::new(&raw_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("fichier")
            .to_string();

        if safe_name.is_empty() || safe_name == "." || safe_name == ".." {
            continue;
        }

        let dest_path = target_dir.join(&safe_name);
        let mut file = match tokio::fs::File::create(&dest_path).await {
            Ok(f) => f,
            Err(e) => {
                return Json(ApiResponse {
                    success: false,
                    data: Some(uploaded),
                    message: Some(format!("Impossible de créer le fichier '{}' : {}", safe_name, e)),
                });
            }
        };

        let mut bytes_written = 0u64;
        while let Ok(Some(chunk)) = field.chunk().await {
            if let Err(e) = file.write_all(&chunk).await {
                return Json(ApiResponse {
                    success: false,
                    data: Some(uploaded),
                    message: Some(format!("Erreur lors de l'écriture de '{}' : {}", safe_name, e)),
                });
            }
            bytes_written += chunk.len() as u64;
        }

        let _ = file.flush().await;

        uploaded.push(UploadedFileItem {
            name: safe_name,
            bytes: bytes_written,
        });
    }

    Json(ApiResponse {
        success: true,
        data: Some(uploaded),
        message: Some("Transfert(s) terminé(s) avec succès.".into()),
    })
}

#[derive(Deserialize)]
pub struct StreamQuery {
    pub path: String,
}

async fn handle_files_stream(
    Query(params): Query<StreamQuery>,
    req: axum::extract::Request,
) -> impl axum::response::IntoResponse {
    use std::path::PathBuf;
    use tower_http::services::fs::ServeFile;
    use tower::ServiceExt;
    use axum::response::IntoResponse;

    let file_path = crate::files::normalize_user_path(PathBuf::from(&params.path));
    if !file_path.exists() || !file_path.is_file() {
        return (axum::http::StatusCode::NOT_FOUND, "Fichier multimédia introuvable.").into_response();
    }
    let service = ServeFile::new(file_path);
    match service.oneshot(req).await {
        Ok(res) => res.into_response(),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Erreur lors de la lecture du flux : {}", err),
        ).into_response(),
    }
}

async fn handle_files_read(
    Query(params): Query<crate::files::ReadFileQuery>,
) -> Json<ApiResponse<crate::files::ReadFileResponse>> {
    match crate::files::read_file_content(&params.path) {
        Ok(data) => Json(ApiResponse {
            success: true,
            data: Some(data),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_files_write(
    Json(payload): Json<crate::files::WriteFileRequest>,
) -> Json<ApiResponse<String>> {
    match crate::files::write_file_content(&payload.path, &payload.content) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}


async fn handle_files_image_view(
    Query(params): Query<ImageViewQuery>,
    req: axum::extract::Request,
) -> impl axum::response::IntoResponse {
    use tower_http::services::fs::ServeFile;
    use tower::ServiceExt;
    use axum::response::IntoResponse;

    let is_thumb = params.thumb.unwrap_or(false);
    let preview_res = tokio::task::spawn_blocking(move || {
        get_image_preview_path(&params.path, is_thumb)
    }).await;

    match preview_res {
        Ok(Ok((file_path, _mime))) => {
            let service = ServeFile::new(file_path);
            match service.oneshot(req).await {
                Ok(resp) => resp.into_response(),
                Err(err) => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Erreur lors de la lecture de l'image : {}", err),
                ).into_response(),
            }
        }
        Ok(Err(err)) => (
            axum::http::StatusCode::NOT_FOUND,
            err,
        ).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Erreur serveur : {}", e),
        ).into_response(),
    }
}

async fn handle_files_image_info(
    Query(params): Query<ImageInfoQuery>,
) -> Json<ApiResponse<ImageInfoResponse>> {
    let res = tokio::task::spawn_blocking(move || {
        get_image_info(&params.path)
    }).await;

    match res {
        Ok(Ok(info)) => Json(ApiResponse {
            success: true,
            data: Some(info),
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
            message: Some(e.to_string()),
        }),
    }
}


#[derive(Debug, Deserialize)]
pub struct DocumentPreviewQuery {
    pub path: String,
}

async fn handle_document_preview(
    Query(query): Query<DocumentPreviewQuery>,
    req: axum::extract::Request,
) -> impl axum::response::IntoResponse {
    use tower_http::services::fs::ServeFile;
    use tower::ServiceExt;
    use axum::response::IntoResponse;

    let res = get_document_pdf_path(&query.path).await;
    match res {
        Ok(pdf_path) => {
            let service = ServeFile::new(pdf_path);
            match service.oneshot(req).await {
                Ok(resp) => resp.into_response(),
                Err(err) => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Erreur lors de la lecture du PDF : {}", err),
                ).into_response(),
            }
        }
        Err(err) => (
            axum::http::StatusCode::BAD_REQUEST,
            err,
        ).into_response(),
    }
}

async fn handle_document_preview_by_id(
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> impl axum::response::IntoResponse {
    use tower_http::services::fs::ServeFile;
    use tower::ServiceExt;
    use axum::response::IntoResponse;

    let raw_path = id;
    let res = get_document_pdf_path(&raw_path).await;
    match res {
        Ok(pdf_path) => {
            let service = ServeFile::new(pdf_path);
            match service.oneshot(req).await {
                Ok(resp) => resp.into_response(),
                Err(err) => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Erreur lors de la lecture du PDF : {}", err),
                ).into_response(),
            }
        }
        Err(err) => (
            axum::http::StatusCode::BAD_REQUEST,
            err,
        ).into_response(),
    }
}

async fn handle_document_info(
    Query(query): Query<DocumentPreviewQuery>,
) -> Json<ApiResponse<DocumentInfoResponse>> {
    match get_document_info(&query.path) {
        Ok(info) => Json(ApiResponse {
            success: true,
            data: Some(info),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}


async fn handle_docker_containers() -> Json<ApiResponse<Vec<DockerContainer>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_docker_containers()),
        message: None,
    })
}

async fn handle_docker_container_action(
    Path(name): Path<String>,
    Json(payload): Json<ContainerActionRequest>,
) -> Json<ApiResponse<String>> {
    match control_docker_container(&name, &payload.action) {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_docker_container_logs(
    Path(name): Path<String>,
) -> Json<ApiResponse<String>> {
    match get_docker_logs(&name, 100) {
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

async fn handle_docker_store() -> Json<ApiResponse<StoreCatalog>> {
    let catalog = get_store_catalog();
    Json(ApiResponse {
        success: true,
        data: Some(catalog),
        message: None,
    })
}

async fn handle_docker_store_install(
    Json(payload): Json<InstallAppRequest>,
) -> Json<ApiResponse<String>> {
    match install_store_app(payload).await {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

async fn handle_docker_store_uninstall(
    Json(payload): Json<UninstallAppRequest>,
) -> Json<ApiResponse<String>> {
    match uninstall_store_app(&payload.app_id, payload.delete_data).await {
        Ok(msg) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}
