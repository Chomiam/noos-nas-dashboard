//! ============================================================================
//! 🌐 API REST & GESTIONNAIRE DE ROUTES — STEvE_OS NAS DASHBOARD
//! ============================================================================
//!
//! Ce module constitue la passerelle centrale (API Gateway) du dashboard.
//! Il déclare et relie l'ensemble des routes HTTP servies par le framework Axum
//! aux contrôleurs métier sous-jacents (stockage, conteneurs, réseau, etc.).
//!
//! ### 1. Contrat d'Échange Unifié (`ApiResponse<T>`)
//! Toutes les réponses de l'API respectent une enveloppe JSON standardisée :
//! - `success: bool` : `true` si l'opération a réussi, `false` en cas d'erreur.
//! - `data: Option<T>` : La charge utile typée en cas de succès.
//! - `message: Option<String>` : Message explicatif ou diagnostic d'erreur lisible.
//!
//! ### 2. Extracteurs Axum
//! - `Json<T>` : Décodage automatique et validation du corps de requête (POST/PUT).
//! - `Query<T>` : Extraction typée des paramètres de chaîne de requête (`?cle=valeur`).
//! - `Path<T>` : Capture des segments d'URL dynamiques (`/resource/:id`).
//!
//! ============================================================================

use axum::{
    extract::{Path, Query},
    http::HeaderMap,
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};

macro_rules! require_admin_or_err {
    ($headers:expr) => {
        match crate::auth::get_session_from_headers(&$headers).await {
            Some(s) if s.is_admin => s,
            _ => {
                return Json(ApiResponse {
                    success: false,
                    data: None,
                    message: Some("Accès refusé. Privilèges administrateur requis.".to_string()),
                });
            }
        }
    };
}

// ----------------------------------------------------------------------------
// Imports des Contrôleurs & Modules Métier
// ----------------------------------------------------------------------------
use crate::users::{
    handle_groups_create, handle_groups_delete, handle_groups_list, handle_groups_update_members,
    handle_users_audit, handle_users_change_password, handle_users_create, handle_users_delete,
    handle_users_get, handle_users_list, handle_users_revoke_session, handle_users_sessions,
    handle_users_set_samba_password, handle_users_toggle_lock, handle_users_update,
};
use crate::vms::{
    control_vm, create_vm, detect_gpus, get_iso_job_store, get_vm_vnc_port, handle_vm_vnc_ws,
    list_isos, list_vms, start_iso_download, CreateVmRequest, GpuDeviceInfo, IsoDownloadJob,
    IsoDownloadRequest, IsoInfo, VirtualMachine, VmActionRequest,
};
use crate::wireguard::{
    create_client, delete_client, get_wireguard_server_info, load_wireguard_clients,
    CreateClientRequest, WireguardClient, WireguardServerInfo,
};
use crate::documents::{get_document_info, get_document_pdf_path, DocumentInfoResponse};
use crate::dns::{get_dns_overview, update_dns, DnsOverview, UpdateDnsRequest};
use crate::docker_store::{
    control_docker_container, delete_docker_image, get_docker_logs, get_store_catalog,
    get_all_store_deployments, get_store_deployment_status, install_store_app,
    list_docker_images, prune_docker_images, uninstall_store_app, remove_docker_container,
    ContainerActionRequest, DockerImagesOverview, DockerStoreDeployStatus, InstallAppRequest,
    StoreCatalog, UninstallAppRequest,
};
use crate::services::{get_docker_containers, DockerContainer};
use crate::youtube::{
    cancel_youtube_job, clear_youtube_jobs, get_job_status, get_youtube_info, list_jobs,
    start_youtube_download, YoutubeDownloadRequest, YoutubeInfoRequest, YoutubeJobStatus, YoutubeVideoInfo,
};
use crate::trash::{
    delete_trash_item, empty_trash, get_trash_overview, restore_trash_item,
    TrashActionRequest, TrashOverview,
};
use crate::files::{
    copy_item, create_directory, delete_item, get_image_info, get_image_preview_path,
    list_directory, move_item, rename_item, ActionRequest, DeleteRequest, DirectoryListing,
    ImageInfoResponse, ListQuery, MkdirRequest, RenameRequest,
};
use crate::firewall::{
    create_custom_rule, delete_custom_rule, get_firewall_overview, toggle_firewall, unban_ip,
    update_custom_rule, CreatePortRuleRequest, CustomPortRule, FirewallOverview,
    ToggleFirewallRequest, UpdatePortRuleRequest,
};
use crate::sftp::{
    create_sftp_share, delete_sftp_share, disconnect_sftp_session,
    get_sftp_allowed_users, get_sftp_overview, reload_sshd_service,
    save_sftp_global_config, update_sftp_share, CreateSftpShareRequest,
    DisconnectSftpSessionRequest, SftpGlobalConfig, SftpOverview, SftpShare,
    SftpUserAccess, UpdateSftpGlobalRequest,
};
use crate::samba::{
    create_samba_share, delete_samba_share, disconnect_samba_session,
    get_samba_diagnostics, get_samba_overview, reload_samba_service,
    save_samba_global_config, update_samba_share, CreateShareRequest,
    DisconnectSessionRequest, SambaDiagResult, SambaGlobalConfig, SambaOverview,
    SambaShare, UpdateGlobalConfigRequest, UpdateShareRequest,
};
use crate::network::{
    get_live_traffic, get_network_overview, get_traffic_history, LiveTrafficOverview,
    NetworkOverview, TrafficHistoryOverview,
};
use crate::services::{control_service, get_service_logs_filtered, get_services_overview, ServicesOverview};
use crate::storage::{
    create_partition, delete_partition, dismiss_storage_job, eject_removable, format_disk,
    get_active_storage_job, get_raid_sync_progress, get_storage_overview, mount_volume,
    repair_path_permissions, start_create_raid_job, start_destroy_raid_job, trigger_disk_spindown,
    umount_volume, CreatePartitionRequest, CreateRaidRequest, DeletePartitionRequest,
    DestroyRaidRequest, EjectRemovableRequest, FormatDiskRequest, MountVolumeRequest,
    RaidSyncProgress, RepairPermissionsRequest, StorageJob, StorageOverview, UmountVolumeRequest,
};
use crate::generations;
use crate::system::{
    cancel_power, get_gpu_info, get_power_status, get_system_info, schedule_power, GpuInfo,
    ImmediatePowerRequest, PowerStatusResponse, SchedulePowerRequest, SystemInfo,
};
use crate::terminal::{
    autocomplete, drop_sudo_cache, execute_command, sudo_status, validate_sudo_password,
    CompleteRequest, CompleteResponse, ExecRequest, ExecResponse, SudoAuthRequest, SudoStatusResponse,
};
use crate::updates::{
    apply_intelligent_update, check_updates, dismiss_update_progress, get_live_log,
    get_update_progress, start_detached_update, ApplyUpdateResult, UpdateCheckStatus,
    UpdateProgressState,
};
use crate::hardware::{get_hardware_overview, HardwareOverview};
use crate::smart::{get_smart_overview, SmartOverview};
use crate::speedtest::{get_latest_speedtest, run_speedtest, SpeedtestResult};

// ============================================================================
// MODÈLES DE REQUÊTES ET RÉPONSES GÉNÉRIQUES
// ============================================================================

/// Enveloppe générique de réponse JSON de l'API REST.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Succès ou échec de la requête.
    pub success: bool,
    /// Charge utile optionnelle en cas de succès.
    pub data: Option<T>,
    /// Message informatif ou descriptif de l'erreur en cas d'échec.
    pub message: Option<String>,
}

/// Paramètres de requête pour l'affichage ou le téléchargement d'une image/vignette.
#[derive(Debug, Deserialize)]
pub struct ImageViewQuery {
    /// Chemin absolu du fichier image sur le serveur.
    pub path: String,
    /// Si true, génère et retourne une miniature allégée (vignette).
    pub thumb: Option<bool>,
    /// Token de session éventuel pour validation inline.
    #[allow(dead_code)]
    pub token: Option<String>,
}

/// Paramètres de requête pour obtenir les métadonnées photographiques EXIF d'une image.
#[derive(Debug, Deserialize)]
pub struct ImageInfoQuery {
    pub path: String,
}

/// Paramètres de consultation des logs systemd / journalctl.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Nom de l'unité systemd (ex: "samba-smbd.service", "sshd.service", "_SYSTEM_", "_KERNEL_", "_BOOT_").
    pub unit: Option<String>,
    /// Nombre maximal de lignes à récupérer.
    pub lines: Option<usize>,
    /// Niveau de sévérité journalctl ("err", "warning", "info", etc.).
    pub priority: Option<String>,
    /// Filtre de recherche textuelle journalctl (-g).
    pub grep: Option<String>,
    /// Filtrer sur le démarrage actuel (-b).
    pub boot: Option<bool>,
}

/// Paramètres de vérification de mises à jour système.
#[derive(Debug, Deserialize)]
pub struct UpdateCheckQuery {
    /// Si true, force l'actualisation depuis le dépôt GitHub distant sans utiliser le cache.
    pub force: Option<bool>,
}

/// Paramètres d'application d'une mise à jour logicielle.
#[derive(Debug, Deserialize)]
pub struct ApplyUpdateQuery {
    /// Si true, force la recompilation ou le téléchargement forcé des paquets.
    pub force_packages: Option<bool>,
}

/// Réponse retournant le flux de logs en temps réel lors d'une mise à jour.
#[derive(Debug, Serialize, Deserialize)]
pub struct LiveLogsResponse {
    /// Contenu brut du journal de mise à jour.
    pub logs: String,
    /// Indique si le processus de mise à jour est toujours en cours d'exécution.
    pub is_updating: bool,
}

// ============================================================================
// DÉCLARATION DU ROUTEUR CENTRAL AXUM
// ============================================================================

/// Construit et configure l'ensemble des routes HTTP de l'API REST du dashboard.
pub fn api_routes() -> Router {
    Router::new()
        // --------------------------------------------------------------------
        // 1. GESTION DES UTILISATEURS, GROUPES & SESSIONS PAM
        // --------------------------------------------------------------------
        .route("/users", get(handle_users_list).post(handle_users_create))
        .route("/users/audit", get(handle_users_audit))
        .route("/users/sessions", get(handle_users_sessions))
        .route("/users/sessions/:token/revoke", post(handle_users_revoke_session))
        .route("/users/:username", get(handle_users_get).put(handle_users_update).delete(handle_users_delete))
        .route("/users/:username/password", post(handle_users_change_password))
        .route("/users/:username/samba-password", post(handle_users_set_samba_password))
        .route("/users/:username/toggle-lock", post(handle_users_toggle_lock))
        .route("/groups", get(handle_groups_list).post(handle_groups_create))
        .route("/groups/:group", delete(handle_groups_delete))
        .route("/groups/:group/members", post(handle_groups_update_members))

        // --------------------------------------------------------------------
        // 2. SYSTÈME, ALIMENTATION & ÉNERGIE
        // --------------------------------------------------------------------
        .route("/system", get(handle_system))
        .route("/system/power/status", get(handle_power_status))
        .route("/system/power/immediate", post(handle_power_immediate))
        .route("/system/power/schedule", post(handle_power_schedule))
        .route("/system/power/cancel", post(handle_power_cancel))

        // --------------------------------------------------------------------
        // 3. MATÉRIEL, CAPTEURS & GPU
        // --------------------------------------------------------------------
        .route("/gpu", get(handle_gpu))
        .route("/hardware", get(handle_hardware))
        .route("/smart", get(handle_smart))

        // --------------------------------------------------------------------
        // 4. STOCKAGE, RAIDS, DISQUES & MONTAGES
        // --------------------------------------------------------------------
        .route("/storage", get(handle_storage))
        .route("/storage/raids/progress", get(handle_raid_progress))
        .route("/storage/disks/format", post(handle_format_disk))
        .route("/storage/jobs/active", get(handle_get_active_storage_job))
        .route("/storage/jobs/dismiss", post(handle_dismiss_storage_job))
        .route("/storage/raids/create", post(handle_create_raid))
        .route("/storage/raids/destroy", post(handle_destroy_raid))
        .route("/storage/mount", post(handle_mount_volume))
        .route("/storage/umount", post(handle_umount_volume))
        .route("/storage/partition/create", post(handle_create_partition))
        .route("/storage/partition/delete", post(handle_delete_partition))
        .route("/storage/removable/eject", post(handle_eject_removable))
        .route("/storage/permissions/repair", post(handle_repair_permissions))
        .route("/storage/:disk/spindown", post(handle_disk_spindown))

        // --------------------------------------------------------------------
        // 5. SERVICES SYSTÈME & CONTENEURS DOCKER
        // --------------------------------------------------------------------
        .route("/services", get(handle_services))
        .route("/service/:unit/:action", post(handle_service_action))
        .route("/docker/containers", get(handle_docker_containers))
        .route("/docker/containers/:name", delete(handle_delete_docker_container))
        .route("/docker/containers/:name/action", post(handle_docker_container_action))
        .route("/docker/containers/:name/logs", get(handle_docker_container_logs))
        .route("/docker/store", get(handle_docker_store))
        .route("/docker/store/deployments", get(handle_docker_store_deployments))
        .route("/docker/store/deployments/:app_id", get(handle_docker_store_deployment_status))
        .route("/docker/store/install", post(handle_docker_store_install))
        .route("/docker/store/uninstall", post(handle_docker_store_uninstall))
        .route("/docker/images", get(handle_docker_images))
        .route("/docker/images/prune", post(handle_docker_images_prune))
        .route("/docker/images/:id", delete(handle_delete_docker_image))

        // --------------------------------------------------------------------
        // 6. SERVEURS DE JEUX & GESTIONNAIRE D'EGGS (PTERODACTYL)
        // --------------------------------------------------------------------
        .route("/games/servers", get(handle_games_servers))
        .route("/games/catalog", get(handle_games_catalog))
        .route("/games/catalog/sync", post(handle_games_catalog_sync))
        .route("/games/create", post(handle_games_create))
        .route("/games/:id/action", post(handle_games_action))
        .route("/games/:id/delete", post(handle_games_delete))
        .route("/games/:id/logs", get(handle_games_logs))
        .route("/games/:id/deploy-status", get(handle_games_deploy_status))
        .route("/games/:id/command", post(handle_games_command))
        .route("/games/eggs/import", post(handle_games_import_egg))
        .route("/games/eggs/:id", delete(handle_games_delete_egg))
        .route("/games/minecraft/loaders", get(handle_minecraft_loaders))
        .route("/games/minecraft/versions", get(handle_minecraft_versions))
        .route("/games/minecraft/resolve", post(handle_minecraft_resolve))


        // --------------------------------------------------------------------
        // 7. RÉSEAU, TRAFIC EN TEMPS RÉEL & RÉSOLVEUR DNS
        // --------------------------------------------------------------------
        .route("/network", get(handle_network))
        .route("/network/traffic/live", get(handle_network_traffic_live))
        .route("/network/traffic/history", get(handle_network_traffic_history))
        .route("/network/dns", get(handle_get_dns).post(handle_update_dns))

        // --------------------------------------------------------------------
        // 8. PARTAGES RÉSEAU SAMBA (SMB) & SFTP (SSH)
        // --------------------------------------------------------------------
        .route("/samba", get(handle_samba_overview))
        .route("/samba/shares", post(handle_samba_create_share))
        .route("/samba/shares/:id", put(handle_samba_update_share).delete(handle_samba_delete_share))
        .route("/samba/global", post(handle_samba_update_global))
        .route("/samba/reload", post(handle_samba_reload))
        .route("/samba/diagnostics", get(handle_samba_diagnostics))
        .route("/samba/sessions/disconnect", post(handle_samba_disconnect_session))
        .route("/sftp", get(handle_sftp_overview))
        .route("/sftp/users", get(handle_sftp_users))
        .route("/sftp/shares", post(handle_sftp_create_share))
        .route("/sftp/shares/:id", put(handle_sftp_update_share).delete(handle_sftp_delete_share))
        .route("/sftp/global", post(handle_sftp_update_global))
        .route("/sftp/reload", post(handle_sftp_reload))
        .route("/sftp/sessions/disconnect", post(handle_sftp_disconnect_session))

        // --------------------------------------------------------------------
        // 9. PARE-FEU (NFTABLES) & SÉCURITÉ
        // --------------------------------------------------------------------
        .route("/firewall", get(handle_firewall))
        .route("/firewall/toggle", post(handle_firewall_toggle))
        .route("/firewall/rules", post(handle_firewall_create_rule))
        .route("/firewall/rules/:id", put(handle_firewall_update_rule).delete(handle_firewall_delete_rule))
        .route("/firewall/unban", post(handle_firewall_unban))
        .route("/logs", get(handle_logs))
        .route("/logs/export", get(handle_logs_export))

        // --------------------------------------------------------------------
        // 10. VPN SÉCURISÉ WIREGUARD
        // --------------------------------------------------------------------
        .route("/wireguard/server", get(handle_wireguard_server))
        .route("/wireguard/clients", get(handle_wireguard_clients).post(handle_create_wireguard_client))
        .route("/wireguard/clients/:id", delete(handle_delete_wireguard_client))

        // --------------------------------------------------------------------
        // 11. MACHINES VIRTUELLES (KVM / QEMU / CONSOLE VNC)
        // --------------------------------------------------------------------
        .route("/vms", get(handle_vms_list).post(handle_vms_create))
        .route("/vms/:name/action", post(handle_vms_action))
        .route("/vms/:name/vnc-port", get(handle_vms_vnc_port))
        .route("/vms/:name/vnc", get(handle_vm_vnc_ws))
        .route("/vms/isos", get(handle_vms_isos))
        .route("/vms/isos/download", post(handle_vms_iso_download))
        .route("/vms/isos/downloads", get(handle_vms_iso_downloads))
        .route("/vms/isos/upload", post(handle_vms_iso_upload).layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/vms/gpus", get(handle_vms_gpus))

        // --------------------------------------------------------------------
        // 12. GESTIONNAIRE DE FICHIERS, APERÇUS & ARCHIVES
        // --------------------------------------------------------------------
        .route("/files/list", get(handle_files_list))
        .route("/files/mkdir", post(handle_files_mkdir))
        .route("/files/delete", post(handle_files_delete))
        .route("/files/rename", post(handle_files_rename))
        .route("/files/copy", post(handle_files_copy))
        .route("/files/move", post(handle_files_move))
        .route("/files/upload", post(handle_files_upload).layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/files/stream", get(handle_files_stream))
        .route("/files/download", get(handle_files_stream))
        .route("/files/read", get(handle_files_read))
        .route("/files/write", post(handle_files_write))
        .route("/files/image-view", get(handle_files_image_view))
        .route("/files/image-info", get(handle_files_image_info))
        .route("/files/compress", post(handle_files_compress))
        .route("/files/extract", post(handle_files_extract))
        .route("/files/archive-info", post(handle_files_archive_info))
        .route("/files/storage-mounts", get(handle_storage_mounts_list))
        .route("/files/pinned-mounts", get(handle_pinned_mounts_list).post(handle_pinned_mounts_add).delete(handle_pinned_mounts_remove))
        .route("/files/pinned-mounts/reorder", post(handle_pinned_mounts_reorder))
        .route("/files/network/discover", get(handle_network_discover))
        .route("/files/remote-mounts", get(handle_remote_mounts_list).post(handle_remote_mounts_create))
        .route("/files/remote-mounts/:id", delete(handle_remote_mounts_delete))
        .route("/files/remote-mounts/:id/mount", post(handle_remote_mounts_mount))
        .route("/files/remote-mounts/:id/unmount", post(handle_remote_mounts_unmount))

        // --------------------------------------------------------------------
        // 13. CORBEILLE SYSTÈME
        // --------------------------------------------------------------------
        .route("/files/trash", get(handle_trash_overview))
        .route("/files/trash/restore", post(handle_trash_restore))
        .route("/files/trash/delete", post(handle_trash_delete))
        .route("/files/trash/empty", post(handle_trash_empty))

        // --------------------------------------------------------------------
        // 14. MULTIMÉDIA & TÉLÉCHARGEMENT YOUTUBE
        // --------------------------------------------------------------------
        .route("/youtube/info", post(handle_youtube_info))
        .route("/youtube/download", post(handle_youtube_download))
        .route("/youtube/status/:job_id", get(handle_youtube_status))
        .route("/youtube/jobs", get(handle_youtube_jobs))
        .route("/youtube/cancel/:job_id", post(handle_youtube_cancel))
        .route("/youtube/clear", post(handle_youtube_clear))

        // --------------------------------------------------------------------
        // 15. PRÉVISUALISATION DE DOCUMENTS (PDF / BUREAUTIQUE)
        // --------------------------------------------------------------------
        .route("/documents/preview", get(handle_document_preview))
        .route("/documents/:id/preview", get(handle_document_preview_by_id))
        .route("/documents/info", get(handle_document_info))

        // --------------------------------------------------------------------
        // 16. TERMINAL WEB INTÉGRÉ
        // --------------------------------------------------------------------
        .route("/terminal/exec", post(handle_terminal_exec))
        .route("/terminal/complete", post(handle_terminal_complete))
        .route("/terminal/sudo-status", get(handle_terminal_sudo_status))
        .route("/terminal/sudo-auth", post(handle_terminal_sudo_auth))
        .route("/terminal/sudo-drop", post(handle_terminal_sudo_drop))

        // --------------------------------------------------------------------
        // 17. MISES À JOUR SYSTÈME & GÉNÉRATIONS NIXOS
        // --------------------------------------------------------------------
        .route("/updates/status", get(handle_updates_status))
        .route("/updates/start", post(handle_updates_start))
        .route("/updates/progress", get(handle_updates_progress))
        .route("/updates/dismiss", post(handle_updates_dismiss))
        .route("/updates/apply", post(handle_updates_apply))
        .route("/updates/logs", get(handle_updates_logs))
        .route("/generations/list", get(handle_generations_list))
        .route("/generations/boot", post(handle_generations_boot))
        .route("/generations/cleanup", post(handle_generations_cleanup))

        // --------------------------------------------------------------------
        // 18. TESTS DE DÉBIT & DIAGNOSTICS RÉSEAU
        // --------------------------------------------------------------------
        .route("/speedtest/latest", get(handle_speedtest_latest))
        .route("/speedtest/run", post(handle_speedtest_run))
}

// ============================================================================
// CONTRÔLEURS : SYSTÈME, ÉNERGIE, MATÉRIEL & SERVICES
// ============================================================================

/// Retourne l'état de l'alimentation (extinctions ou redémarrages programmés).
async fn handle_power_status() -> Json<ApiResponse<PowerStatusResponse>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_power_status()),
        message: None,
    })
}

/// Déclenche un redémarrage ou une extinction immédiate du NAS après un léger délai de grâce.
async fn handle_power_immediate(
    headers: HeaderMap,
    Json(req): Json<ImmediatePowerRequest>,
) -> Json<ApiResponse<()>> {
    require_admin_or_err!(headers);
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

/// Programme une extinction ou un redémarrage différé via le démon de puissance.
async fn handle_power_schedule(
    headers: HeaderMap,
    Json(req): Json<SchedulePowerRequest>,
) -> Json<ApiResponse<()>> {
    require_admin_or_err!(headers);
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

/// Annule toute programmation d'extinction ou de redémarrage en attente.
async fn handle_power_cancel(headers: HeaderMap) -> Json<ApiResponse<()>> {
    require_admin_or_err!(headers);
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

/// Collecte les informations globales du système (CPU, RAM, Uptime, OS, Kernel).
async fn handle_system() -> Json<ApiResponse<SystemInfo>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_system_info()),
        message: None,
    })
}

/// Interroge les accélérateurs graphiques dédiés ou intégrés (Intel QuickSync, AMD, Nvidia).
async fn handle_gpu() -> Json<ApiResponse<GpuInfo>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_gpu_info()),
        message: None,
    })
}

/// Fournit la vue globale des disques durs, pools ZFS/Btrfs, RAIDs mdadm et points de montage.
async fn handle_storage() -> Json<ApiResponse<StorageOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_storage_overview()),
        message: None,
    })
}

/// Récupère l'état d'exécution et les statistiques des services systemd surveillés.
async fn handle_services() -> Json<ApiResponse<ServicesOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_services_overview()),
        message: None,
    })
}

/// Exécute une action de contrôle (start, stop, restart, enable, disable) sur une unité systemd.
async fn handle_service_action(
    headers: HeaderMap,
    Path((unit, action)): Path<(String, String)>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
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

/// Retourne l'état des tables nftables et de la politique de sécurité firewall.
async fn handle_firewall() -> Json<ApiResponse<FirewallOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_firewall_overview()),
        message: None,
    })
}

/// Récupère les dernières lignes du journal systemd pour une unité de service donnée, avec filtres avancés.
async fn handle_logs(Query(params): Query<LogsQuery>) -> Json<ApiResponse<String>> {
    let unit = params.unit.unwrap_or_else(|| "_SYSTEM_".to_string());
    let lines = params.lines.unwrap_or(50);
    let priority = params.priority.as_deref();
    let grep = params.grep.as_deref();
    let boot = params.boot;

    match get_service_logs_filtered(&unit, lines, priority, grep, boot) {
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

/// Télécharge un export brut des logs système ou de service au format texte.
async fn handle_logs_export(Query(params): Query<LogsQuery>) -> axum::response::Response {
    let unit = params.unit.unwrap_or_else(|| "_SYSTEM_".to_string());
    let lines = params.lines.unwrap_or(500);
    let priority = params.priority.as_deref();
    let grep = params.grep.as_deref();
    let boot = params.boot;

    let content = match get_service_logs_filtered(&unit, lines, priority, grep, boot) {
        Ok(logs) => logs,
        Err(e) => format!("Erreur lors de l'export des logs : {}", e),
    };

    let safe_unit = unit.replace(['/', '\\', ' '], "_").to_lowercase();
    let filename = format!("noos-logs-{}.log", safe_unit);

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "text/plain; charset=utf-8".parse().unwrap(),
    );
    if let Ok(disposition) = format!("attachment; filename=\"{}\"", filename).parse() {
        headers.insert(axum::http::header::CONTENT_DISPOSITION, disposition);
    }

    (headers, content).into_response()
}

// ============================================================================
// CONTRÔLEURS : MISES À JOUR LOGICIELLES & FLUX DE LOGS
// ============================================================================

/// Interroge les dépôts distants pour vérifier la présence d'une nouvelle version de STEvE_OS.
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

#[derive(Deserialize)]
struct StartUpdateQuery {
    force_packages: Option<bool>,
}

/// Lance le processus de mise à jour en tâche de fond détachée.
async fn handle_updates_start(
    headers: HeaderMap,
    Query(params): Query<StartUpdateQuery>,
) -> Json<ApiResponse<bool>> {
    require_admin_or_err!(headers);
    let force_pkgs = params.force_packages.unwrap_or(false);
    match start_detached_update(force_pkgs) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(true),
            message: Some("Mise à jour démarrée en arrière-plan".to_string()),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: Some(false),
            message: Some(e),
        }),
    }
}

/// Retourne l'état d'avancement étape par étape de la mise à jour en cours.
async fn handle_updates_progress() -> Json<ApiResponse<UpdateProgressState>> {
    let progress = get_update_progress();
    Json(ApiResponse {
        success: true,
        data: Some(progress),
        message: None,
    })
}

/// Réinitialise l'état de progression après consultation par l'utilisateur.
async fn handle_updates_dismiss() -> Json<ApiResponse<bool>> {
    dismiss_update_progress();
    Json(ApiResponse {
        success: true,
        data: Some(true),
        message: None,
    })
}

/// Applique la mise à jour de manière synchrone bloquante (mode direct).
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

/// Diffuse les logs bruts produits en direct par le processus de mise à jour NixOS / Git.
async fn handle_updates_logs() -> Json<ApiResponse<LiveLogsResponse>> {
    let (logs, is_updating) = get_live_log();
    Json(ApiResponse {
        success: true,
        data: Some(LiveLogsResponse { logs, is_updating }),
        message: None,
    })
}

// ============================================================================
// CONTRÔLEURS : TERMINAL WEB INTÉGRÉ
// ============================================================================

/// Exécute une commande shell de manière sécurisée et chronométrée dans le répertoire utilisateur.
async fn handle_terminal_exec(
    headers: HeaderMap,
    Json(req): Json<ExecRequest>,
) -> Json<ApiResponse<ExecResponse>> {
    require_admin_or_err!(headers);
    let res = tokio::task::spawn_blocking(move || {
        execute_command(req)
    }).await.unwrap_or_else(|e| ExecResponse {
        success: false,
        stdout: String::new(),
        stderr: format!("Erreur serveur interne : {}", e),
        exit_code: -1,
        cwd: crate::updates::get_user_home(&crate::updates::target_user()).to_string_lossy().to_string(),
        duration_ms: 0,
        needs_password: false,
    });

    Json(ApiResponse {
        success: res.success,
        data: Some(res),
        message: None,
    })
}

/// Fournit des suggestions d'auto-complétion de commandes et de chemins pour le terminal web.
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

/// Vérifie si l'utilisateur actif dispose de privilèges sudo actifs et identifie l'utilisateur cible.
async fn handle_terminal_sudo_status() -> Json<ApiResponse<SudoStatusResponse>> {
    let status = tokio::task::spawn_blocking(sudo_status).await.unwrap_or_else(|_| SudoStatusResponse {
        cached: false,
        user: crate::updates::target_user(),
    });

    Json(ApiResponse {
        success: true,
        data: Some(status),
        message: None,
    })
}

/// Authentifie l'utilisateur pour l'élévation sudo en validant le mot de passe via PAM.
async fn handle_terminal_sudo_auth(Json(req): Json<SudoAuthRequest>) -> Json<ApiResponse<bool>> {
    let res = tokio::task::spawn_blocking(move || {
        validate_sudo_password(&req.password)
    }).await;

    match res {
        Ok(Ok(())) => Json(ApiResponse {
            success: true,
            data: Some(true),
            message: Some("Privilèges administrateur (sudo) activés pour la session.".to_string()),
        }),
        Ok(Err(e)) => Json(ApiResponse {
            success: false,
            data: Some(false),
            message: Some(e),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur d'exécution : {}", e)),
        }),
    }
}

/// Révoque immédiatement le ticket de cache sudo pour l'utilisateur.
async fn handle_terminal_sudo_drop() -> Json<ApiResponse<bool>> {
    let _ = tokio::task::spawn_blocking(drop_sudo_cache).await;
    Json(ApiResponse {
        success: true,
        data: Some(true),
        message: Some("Privilèges administrateur (sudo) désactivés.".to_string()),
    })
}

// ============================================================================
// CONTRÔLEURS : GESTIONNAIRE DE FICHIERS (EXPLORATION & ARBORESCENCE)
// ============================================================================

/// Liste le contenu d'un répertoire (fichiers, dossiers, tailles, dates et permissions).
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

// ============================================================================
// CONTRÔLEURS : GESTION DE LA CORBEILLE (TRASH)
// ============================================================================

/// Récupère la liste de tous les éléments actuellement présents dans la corbeille.
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

/// Restaure un élément de la corbeille à son emplacement d'origine.
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

/// Supprime définitivement un élément spécifique stocké dans la corbeille.
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

/// Vide l'intégralité du contenu de la corbeille et libère l'espace disque associé.
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

// ============================================================================
// CONTRÔLEURS : TÉLÉCHARGEMENT MULTIMÉDIA (YOUTUBE / YT-DLP)
// ============================================================================

/// Analyse une URL YouTube pour extraire le titre, la vignette, la durée et les résolutions.
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

/// Lance le téléchargement asynchrone d'une vidéo ou d'une piste audio YouTube en arrière-plan.
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

/// Retourne la progression en temps réel d'une tâche de téléchargement YouTube active.
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

/// Liste l'ensemble des téléchargements YouTube en cours, terminés ou en échec.
async fn handle_youtube_jobs() -> Json<ApiResponse<Vec<YoutubeJobStatus>>> {
    Json(ApiResponse {
        success: true,
        data: Some(list_jobs()),
        message: None,
    })
}

/// Annule et interrompt un processus de téléchargement YouTube actif.
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

/// Purge la liste des tâches de téléchargement terminées ou annulées.
async fn handle_youtube_clear() -> Json<ApiResponse<()>> {
    clear_youtube_jobs();
    Json(ApiResponse {
        success: true,
        data: Some(()),
        message: Some("Historique nettoyé.".into()),
    })
}

// ============================================================================
// CONTRÔLEURS : OPÉRATIONS SUR FICHIERS (CRUD & DÉPLACEMENTS)
// ============================================================================

/// Crée un nouveau dossier dans l'arborescence.
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

/// Supprime un fichier ou un dossier (déplacement vers la corbeille ou suppression définitive).
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

/// Renomme un élément du système de fichiers sans changer son dossier parent.
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

/// Copie un fichier ou dossier vers un nouveau répertoire de destination.
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

/// Déplace un fichier ou dossier vers un autre répertoire du système de fichiers.
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

// ============================================================================
// CONTRÔLEURS : GESTION DU STOCKAGE, RAIDS, DISQUES & MONTAGES
// ============================================================================

/// Retourne l'état de synchronisation ou de reconstruction (resync / recovery) du RAID actif.
async fn handle_raid_progress() -> Json<ApiResponse<Option<RaidSyncProgress>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_raid_sync_progress()),
        message: None,
    })
}

/// Répare les droits et permissions d'accès POSIX (chown/chmod) sur un répertoire ou point de montage.
async fn handle_repair_permissions(
    headers: HeaderMap,
    Json(payload): Json<RepairPermissionsRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
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

/// Crée une nouvelle partition sur un disque physique.
async fn handle_create_partition(
    headers: HeaderMap,
    Json(payload): Json<CreatePartitionRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
    match create_partition(&payload) {
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

/// Supprime une partition existante d'un disque de stockage.
async fn handle_delete_partition(
    headers: HeaderMap,
    Json(payload): Json<DeletePartitionRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
    match delete_partition(&payload) {
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

/// Éjecte un périphérique amovible (disque USB, lecteur de carte).
async fn handle_eject_removable(Json(payload): Json<EjectRemovableRequest>) -> Json<ApiResponse<String>> {
    match eject_removable(&payload) {
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

/// Monte une partition ou volume de stockage vers un point de montage local.
async fn handle_mount_volume(
    headers: HeaderMap,
    Json(payload): Json<MountVolumeRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
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

/// Démonte un volume de stockage préalablement monté.
async fn handle_umount_volume(
    headers: HeaderMap,
    Json(payload): Json<UmountVolumeRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
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

/// Formate un disque ou une partition avec le système de fichiers choisi (ext4, btrfs, exfat, ntfs).
async fn handle_format_disk(
    headers: HeaderMap,
    Json(payload): Json<FormatDiskRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
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

/// Retourne l'état d'une tâche de stockage asynchrone en cours (formatage, construction RAID).
async fn handle_get_active_storage_job() -> Json<ApiResponse<Option<StorageJob>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_active_storage_job()),
        message: None,
    })
}

/// Acquitte et efface une notification de tâche de stockage terminée.
async fn handle_dismiss_storage_job() -> Json<ApiResponse<String>> {
    dismiss_storage_job();
    Json(ApiResponse {
        success: true,
        data: Some("Tâche de stockage acquittée.".to_string()),
        message: None,
    })
}

/// Déclenche la création asynchrone d'une grappe RAID logicielle via `mdadm`.
async fn handle_create_raid(
    headers: HeaderMap,
    Json(payload): Json<CreateRaidRequest>,
) -> Json<ApiResponse<StorageJob>> {
    require_admin_or_err!(headers);
    match start_create_raid_job(payload) {
        Ok(job) => Json(ApiResponse {
            success: true,
            data: Some(job),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Supprime et désassemble une grappe RAID logicielle de manière contrôlée.
async fn handle_destroy_raid(
    headers: HeaderMap,
    Json(payload): Json<DestroyRaidRequest>,
) -> Json<ApiResponse<StorageJob>> {
    require_admin_or_err!(headers);
    match start_destroy_raid_job(payload) {
        Ok(job) => Json(ApiResponse {
            success: true,
            data: Some(job),
            message: None,
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Envoie un ordre d'arrêt de rotation (spindown) immédiat à un disque mécanique via `hdparm`.
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

// ============================================================================
// CONTRÔLEURS : MATÉRIEL, DIAGNOSTICS S.M.A.R.T. & TESTS DE DÉBIT
// ============================================================================

/// Analyse le matériel système (architecture processeur, barrettes mémoires DMI, températures).
async fn handle_hardware() -> Json<ApiResponse<HardwareOverview>> {
    let hw = tokio::task::spawn_blocking(get_hardware_overview).await.unwrap_or_else(|_| get_hardware_overview());
    Json(ApiResponse {
        success: true,
        data: Some(hw),
        message: None,
    })
}

/// Interroge les attributs de santé prédictive S.M.A.R.T. de tous les disques connectés.
async fn handle_smart() -> Json<ApiResponse<SmartOverview>> {
    let smart = tokio::task::spawn_blocking(get_smart_overview).await.unwrap_or_else(|_| get_smart_overview());
    Json(ApiResponse {
        success: true,
        data: Some(smart),
        message: None,
    })
}

/// Retourne le résultat du dernier test de bande passante réseau enregistré.
async fn handle_speedtest_latest() -> Json<ApiResponse<Option<SpeedtestResult>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_latest_speedtest()),
        message: None,
    })
}

/// Lance un test de débit réseau Internet (débit montant, descendant et latence de ping).
async fn handle_speedtest_run() -> Json<ApiResponse<SpeedtestResult>> {
    let res = tokio::task::spawn_blocking(run_speedtest).await.unwrap_or_else(|_| run_speedtest());
    Json(ApiResponse {
        success: true,
        data: Some(res),
        message: None,
    })
}


// ============================================================================
// CONTRÔLEURS : FICHIERS AVANCÉS (UPLOAD, FLUX, ÉDITEUR, IMAGES & ARCHIVES)
// ============================================================================

/// Paramètres de requête pour le téléversement de fichiers.
#[derive(Deserialize)]
pub struct UploadQuery {
    pub dir: Option<String>,
}

/// Description d'un fichier téléversé avec succès.
#[derive(Serialize)]
pub struct UploadedFileItem {
    pub name: String,
    pub bytes: u64,
}

/// Réceptionne un flux multipart pour téléverser un ou plusieurs fichiers dans un dossier cible.
async fn handle_files_upload(
    Query(params): Query<UploadQuery>,
    mut multipart: axum::extract::Multipart,
) -> Json<ApiResponse<Vec<UploadedFileItem>>> {
    use std::path::{Path, PathBuf};
    use tokio::io::AsyncWriteExt;

    let target_dir = crate::files::normalize_user_path(PathBuf::from(
        params.dir.unwrap_or_else(|| crate::updates::get_user_home(&crate::updates::target_user()).to_string_lossy().to_string())
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

/// Paramètres de requête pour la diffusion d'un fichier multimédia ou téléchargement brut.
#[derive(Deserialize)]
pub struct StreamQuery {
    pub path: String,
    #[allow(dead_code)]
    pub token: Option<String>,
}

/// Sert un fichier en streaming HTTP direct avec prise en charge native des plages d'octets (Range requests).
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

/// Lit le contenu texte/code d'un fichier pour l'éditeur du dashboard.
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

/// Enregistre les modifications textuelles apportées à un fichier.
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

/// Compresse une sélection de fichiers et répertoires dans le format spécifié (zip, 7z, tar.*).
async fn handle_files_compress(
    Json(payload): Json<crate::files::CompressRequest>,
) -> Json<ApiResponse<String>> {
    match crate::files::compress_items(payload) {
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

/// Décompresse une archive existante vers le répertoire de destination choisi.
async fn handle_files_extract(
    Json(payload): Json<crate::files::ExtractRequest>,
) -> Json<ApiResponse<String>> {
    match crate::files::extract_archive(payload) {
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

/// Analyse une archive sans l'extraire pour vérifier son format et son chiffrement éventuel.
async fn handle_files_archive_info(
    Json(payload): Json<crate::files::ArchiveInfoRequest>,
) -> Json<ApiResponse<crate::files::ArchiveInfoResponse>> {
    match crate::files::get_archive_info(&payload.archive_path) {
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

/// Diffuse une image ou sa miniature (vignette) générée à la volée avec cache disque.
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

/// Extrait les métadonnées photographiques complètes d'un fichier image (EXIF, modèle appareil, objectif, etc.).
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

// ============================================================================
// CONTRÔLEURS : PRÉVISUALISATION DE DOCUMENTS (PDF / BUREAUTIQUE)
// ============================================================================

/// Paramètres de requête pour la prévisualisation de documents bureautiques ou PDF.
#[derive(Debug, Deserialize)]
pub struct DocumentPreviewQuery {
    pub path: String,
}

/// Convertit à chaud un document bureautique (Office, LibreOffice) en PDF pour prévisualisation web.
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

/// Prévisualise un document identifié par son identifiant unique.
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

/// Retourne les métadonnées et le nombre de pages d'un document PDF/Office.
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


// ============================================================================
// CONTRÔLEURS : GESTION DOCKER (CONTENEURS & CATALOGUE APP STORE)
// ============================================================================

/// Liste tous les conteneurs Docker (en cours, arrêtés, utilisation mémoire et ports).
async fn handle_docker_containers() -> Json<ApiResponse<Vec<DockerContainer>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_docker_containers()),
        message: None,
    })
}

/// Paramètres de suppression d'un conteneur Docker.
#[derive(Debug, Deserialize)]
pub struct DeleteContainerQuery {
    #[serde(default)]
    pub delete_image: bool,
    #[serde(default)]
    pub delete_data: bool,
}

/// Supprime un conteneur Docker avec option de purge de son image sous-jacente et de ses données.
async fn handle_delete_docker_container(
    Path(name): Path<String>,
    Query(params): Query<DeleteContainerQuery>,
) -> Json<ApiResponse<String>> {
    match remove_docker_container(&name, params.delete_image, params.delete_data).await {
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

/// Déclenche une action sur un conteneur (start, stop, restart, pause, unpause).
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

/// Récupère les derniers logs d'exécution d'un conteneur Docker.
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

/// Fournit le catalogue complet des applications NAS préconfigurées du Docker Store.
async fn handle_docker_store() -> Json<ApiResponse<StoreCatalog>> {
    let catalog = get_store_catalog();
    Json(ApiResponse {
        success: true,
        data: Some(catalog),
        message: None,
    })
}

/// Installe une application du catalogue Docker Store en déployant son conteneur et ses volumes.
async fn handle_docker_store_install(
    Json(payload): Json<InstallAppRequest>,
) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || install_store_app(payload)).await;
    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur d'exécution de la tâche : {}", err)),
        }),
    }
}

/// Désinstalle une application du Docker Store avec option de conservation des données persistantes.
async fn handle_docker_store_uninstall(
    Json(payload): Json<UninstallAppRequest>,
) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || uninstall_store_app(&payload.app_id, payload.delete_data)).await;
    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg),
            message: None,
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur d'exécution de la tâche : {}", err)),
        }),
    }
}

/// Liste tous les déploiements d'applications du Docker Store actifs ou récents.
async fn handle_docker_store_deployments() -> Json<ApiResponse<Vec<DockerStoreDeployStatus>>> {
    let list = tokio::task::spawn_blocking(get_all_store_deployments).await.unwrap_or_default();
    Json(ApiResponse {
        success: true,
        data: Some(list),
        message: None,
    })
}

/// Récupère l'état d'avancement d'un déploiement spécifique dans le Docker Store.
async fn handle_docker_store_deployment_status(
    Path(app_id): Path<String>,
) -> Json<ApiResponse<DockerStoreDeployStatus>> {
    let clean_id = app_id.trim().to_lowercase();
    let st = tokio::task::spawn_blocking(move || get_store_deployment_status(&clean_id)).await.unwrap_or(None);
    Json(ApiResponse {
        success: st.is_some(),
        data: st,
        message: None,
    })
}

// ============================================================================
// CONTRÔLEURS : RÉSEAU, TRAFIC EN TEMPS RÉEL & RÉSOLVEUR DNS
// ============================================================================

/// Interroge la configuration des serveurs DNS système configurés dans resolv.conf ou systemd-resolved.
async fn handle_get_dns() -> Json<ApiResponse<DnsOverview>> {
    let overview = get_dns_overview().await;
    Json(ApiResponse {
        success: true,
        data: Some(overview),
        message: None,
    })
}

/// Met à jour les adresses des serveurs DNS utilisés par le NAS.
async fn handle_update_dns(
    headers: HeaderMap,
    Json(payload): Json<UpdateDnsRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
    match update_dns(&payload) {
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

/// Fournit la vue globale des interfaces réseau physiques, virtuelles et passerelles par défaut.
async fn handle_network() -> Json<ApiResponse<NetworkOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_network_overview()),
        message: None,
    })
}

/// Récupère le débit instantané (upload/download en Ko/s) par interface réseau.
async fn handle_network_traffic_live() -> Json<ApiResponse<LiveTrafficOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_live_traffic()),
        message: None,
    })
}

/// Fournit l'historique chronologique d'utilisation de la bande passante sur les dernières heures.
async fn handle_network_traffic_history() -> Json<ApiResponse<TrafficHistoryOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_traffic_history()),
        message: None,
    })
}

// ============================================================================
// CONTRÔLEURS : SÉCURITÉ & PARE-FEU (NFTABLES)
// ============================================================================

/// Paramètres de déblocage (débannissement) d'une adresse IP bloquée par fail2ban / nftables.
#[derive(Debug, Deserialize)]
pub struct UnbanRequest {
    pub ip: String,
}

/// Active ou désactive globalement le filtrage du pare-feu nftables.
async fn handle_firewall_toggle(
    headers: HeaderMap,
    Json(payload): Json<ToggleFirewallRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
    match toggle_firewall(payload.enable) {
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

/// Crée une nouvelle règle de filtrage ou d'ouverture de port personnalisée.
async fn handle_firewall_create_rule(
    headers: HeaderMap,
    Json(payload): Json<CreatePortRuleRequest>,
) -> Json<ApiResponse<CustomPortRule>> {
    require_admin_or_err!(headers);
    match create_custom_rule(payload) {
        Ok(rule) => Json(ApiResponse {
            success: true,
            data: Some(rule),
            message: Some("Règle de pare-feu créée avec succès".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Modifie les propriétés d'une règle de pare-feu existante.
async fn handle_firewall_update_rule(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<UpdatePortRuleRequest>,
) -> Json<ApiResponse<CustomPortRule>> {
    require_admin_or_err!(headers);
    match update_custom_rule(&id, payload) {
        Ok(rule) => Json(ApiResponse {
            success: true,
            data: Some(rule),
            message: Some("Règle de pare-feu mise à jour avec succès".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Supprime une règle de pare-feu personnalisée.
async fn handle_firewall_delete_rule(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
    match delete_custom_rule(&id) {
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

/// Débloque immédiatement une adresse IP de la liste noire du pare-feu.
async fn handle_firewall_unban(
    headers: HeaderMap,
    Json(payload): Json<UnbanRequest>,
) -> Json<ApiResponse<String>> {
    require_admin_or_err!(headers);
    match unban_ip(&payload.ip) {
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


// ============================================================================
// CONTRÔLEURS : VPN SÉCURISÉ WIREGUARD
// ============================================================================

/// Retourne l'état et les paramètres de configuration du serveur WireGuard (port, clé publique, interface wg0).
async fn handle_wireguard_server(headers: HeaderMap) -> Json<ApiResponse<WireguardServerInfo>> {
    require_admin_or_err!(headers);
    Json(ApiResponse {
        success: true,
        data: Some(get_wireguard_server_info()),
        message: None,
    })
}

/// Liste l'ensemble des profils clients (pairs / peers) WireGuard configurés.
async fn handle_wireguard_clients(headers: HeaderMap) -> Json<ApiResponse<Vec<WireguardClient>>> {
    require_admin_or_err!(headers);
    Json(ApiResponse {
        success: true,
        data: Some(load_wireguard_clients()),
        message: None,
    })
}

/// Génère une nouvelle paire de clés cryptographiques et provisionne un profil client WireGuard.
async fn handle_create_wireguard_client(
    headers: HeaderMap,
    Json(req): Json<CreateClientRequest>,
) -> Json<ApiResponse<WireguardClient>> {
    require_admin_or_err!(headers);
    match create_client(req) {
        Ok(client) => Json(ApiResponse {
            success: true,
            data: Some(client),
            message: Some("Profil client WireGuard créé avec succès.".to_string()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Révoque et supprime définitivement un profil client WireGuard de la configuration serveur.
async fn handle_delete_wireguard_client(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    require_admin_or_err!(headers);
    match delete_client(&id) {
        Ok(()) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some("Profil client WireGuard révoqué avec succès.".to_string()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

// ============================================================================
// CONTRÔLEURS : MACHINES VIRTUELLES (KVM / QEMU / CONSOLE VNC)
// ============================================================================

/// Liste toutes les machines virtuelles KVM enregistrées avec leur état d'exécution (Running, Stopped).
async fn handle_vms_list() -> Json<ApiResponse<Vec<VirtualMachine>>> {
    let list = list_vms();
    Json(ApiResponse {
        success: true,
        data: Some(list),
        message: None,
    })
}

/// Crée et configure une nouvelle machine virtuelle KVM/QEMU (vCPU, RAM, disque qcow2).
async fn handle_vms_create(
    Json(req): Json<CreateVmRequest>,
) -> Json<ApiResponse<String>> {
    match create_vm(req) {
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

/// Contrôle le cycle de vie d'une machine virtuelle (démarrage, arrêt ACPI, arrêt forcé, redémarrage).
async fn handle_vms_action(
    Path(name): Path<String>,
    Json(req): Json<VmActionRequest>,
) -> Json<ApiResponse<String>> {
    match control_vm(&name, &req.action) {
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

/// Retourne le port d'affichage graphique VNC alloué à la machine virtuelle.
async fn handle_vms_vnc_port(
    Path(name): Path<String>,
) -> Json<ApiResponse<Option<u16>>> {
    let port = get_vm_vnc_port(&name);
    let has_port = port.is_some();
    Json(ApiResponse {
        success: has_port,
        data: Some(port),
        message: if has_port { None } else { Some("La machine virtuelle n'est pas allumée ou ne dispose pas de port VNC actif.".into()) },
    })
}

/// Liste les images disques ISO d'installation disponibles sur le serveur.
async fn handle_vms_isos() -> Json<ApiResponse<Vec<IsoInfo>>> {
    let isos = list_isos();
    Json(ApiResponse {
        success: true,
        data: Some(isos),
        message: None,
    })
}

/// Déclenche le téléchargement en arrière-plan d'une image ISO officielle depuis une URL distante.
async fn handle_vms_iso_download(
    Json(req): Json<IsoDownloadRequest>,
) -> Json<ApiResponse<String>> {
    match start_iso_download(req).await {
        Ok(job_id) => Json(ApiResponse {
            success: true,
            data: Some(job_id),
            message: Some("Téléchargement de l'image ISO initié en arrière-plan.".into()),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
    }
}

/// Retourne la liste des téléchargements d'images ISO en cours ou terminés.
async fn handle_vms_iso_downloads() -> Json<ApiResponse<Vec<IsoDownloadJob>>> {
    let store = get_iso_job_store();
    let jobs = store.read().await;
    let list: Vec<IsoDownloadJob> = jobs.values().cloned().collect();
    Json(ApiResponse {
        success: true,
        data: Some(list),
        message: None,
    })
}

/// Permet à l'administrateur de téléverser directement une image ISO d'installation.
async fn handle_vms_iso_upload(
    mut multipart: axum::extract::Multipart,
) -> Json<ApiResponse<Vec<String>>> {
    use std::path::Path as StdPath;
    use tokio::io::AsyncWriteExt;

    let target_dir = crate::vms::get_isos_dir();
    let mut uploaded = Vec::new();

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let raw_name = field.file_name().unwrap_or("image.iso").to_string();
        let safe_name = StdPath::new(&raw_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.iso")
            .to_string();

        if !safe_name.ends_with(".iso") && !safe_name.ends_with(".img") {
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

        while let Ok(Some(chunk)) = field.chunk().await {
            if let Err(e) = file.write_all(&chunk).await {
                return Json(ApiResponse {
                    success: false,
                    data: Some(uploaded),
                    message: Some(format!("Erreur lors de l'écriture : {}", e)),
                });
            }
        }
        let _ = file.flush().await;
        uploaded.push(safe_name);
    }

    Json(ApiResponse {
        success: true,
        data: Some(uploaded),
        message: Some("Téléversement de l'image ISO terminé avec succès.".into()),
    })
}

/// Détecte les GPU et contrôleurs vidéo physiques disponibles pour le passthrough IOMMU (PCIe passthrough).
async fn handle_vms_gpus() -> Json<ApiResponse<Vec<GpuDeviceInfo>>> {
    let gpus = detect_gpus();
    Json(ApiResponse {
        success: true,
        data: Some(gpus),
        message: None,
    })
}

// ============================================================================
// CONTRÔLEURS : GESTION DES GÉNÉRATIONS NIXOS (ROLLBACK & SÉCURITÉ BOOT)
// ============================================================================

/// Liste l'ensemble des générations du système NixOS avec leur date, numéro et kernel associé.
async fn handle_generations_list() -> Json<ApiResponse<generations::GenerationsListResponse>> {
    let res = tokio::task::spawn_blocking(generations::list_generations).await;
    match res {
        Ok(Ok(data)) => Json(ApiResponse {
            success: true,
            data: Some(data),
            message: None,
        }),
        Ok(Err(e)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur d'exécution : {}", e)),
        }),
    }
}

/// Bascule la génération par défaut au prochain démarrage du système (Rollback / Pinning).
async fn handle_generations_boot(Json(req): Json<generations::SetBootRequest>) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || generations::set_boot_generation(req.generation_id)).await;
    match res {
        Ok(Ok(msg)) => Json(ApiResponse {
            success: true,
            data: Some(msg.clone()),
            message: Some(msg),
        }),
        Ok(Err(e)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur d'exécution : {}", e)),
        }),
    }
}

/// Supprime les générations NixOS obsolètes et lance le ramasse-miettes (nix-collect-garbage).
async fn handle_generations_cleanup(Json(req): Json<generations::CleanupRequest>) -> Json<ApiResponse<generations::CleanupResponse>> {
    let res = tokio::task::spawn_blocking(move || generations::cleanup_generations(req)).await;
    match res {
        Ok(Ok(resp)) => Json(ApiResponse {
            success: resp.success,
            data: Some(resp),
            message: None,
        }),
        Ok(Err(e)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(e),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur d'exécution : {}", e)),
        }),
    }
}


// ============================================================================
// CONTRÔLEURS : SERVEURS DE JEUX & MOTEUR D'EGGS (PTERODACTYL)
// ============================================================================

/// Liste tous les serveurs de jeux déployés et leur statut (en ligne, arrêté, ports de jeu).
async fn handle_games_servers() -> Json<ApiResponse<Vec<crate::games::GameServer>>> {
    let servers = tokio::task::spawn_blocking(crate::games::list_game_servers).await.unwrap_or_default();
    Json(ApiResponse {
        success: true,
        data: Some(servers),
        message: None,
    })
}

/// Fournit le catalogue complet des modèles d'installation (Eggs Pterodactyl).
async fn handle_games_catalog() -> Json<ApiResponse<Vec<crate::games::Egg>>> {
    let eggs = tokio::task::spawn_blocking(crate::games::load_all_eggs).await.unwrap_or_default();
    Json(ApiResponse {
        success: true,
        data: Some(eggs),
        message: None,
    })
}

/// Synchronise le catalogue d'Eggs avec le dépôt officiel GitHub STEvE_OS Eggs.
async fn handle_games_catalog_sync() -> Json<ApiResponse<Vec<crate::games::Egg>>> {
    let eggs = tokio::task::spawn_blocking(crate::games::sync_and_load_all_eggs).await.unwrap_or_default();
    Json(ApiResponse {
        success: true,
        data: Some(eggs),
        message: Some("Catalogue synchronisé avec succès depuis GitHub !".into()),
    })
}

/// Crée et lance l'installation d'un nouveau serveur de jeu à partir d'un Egg.
async fn handle_games_create(Json(req): Json<crate::games::CreateGameServerRequest>) -> Json<ApiResponse<crate::games::GameServer>> {
    let res = tokio::task::spawn_blocking(move || crate::games::create_game_server(req)).await;
    match res {
        Ok(Ok(server)) => Json(ApiResponse {
            success: true,
            data: Some(server),
            message: Some("Serveur de jeu déployé avec succès !".into()),
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

/// Contrôle l'état d'un serveur de jeu (démarrer, arrêter, redémarrer, forcer l'arrêt).
async fn handle_games_action(
    Path(id): Path<String>,
    Json(payload): Json<crate::games::GameServerActionRequest>,
) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || crate::games::control_game_server(&id, &payload.action)).await;
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

/// Paramètres de suppression d'un serveur de jeu.
#[derive(Deserialize)]
struct DeleteGameQuery {
    delete_data: Option<bool>,
}

/// Supprime un serveur de jeu avec option de suppression des sauvegardes et fichiers de monde.
async fn handle_games_delete(
    Path(id): Path<String>,
    Query(query): Query<DeleteGameQuery>,
) -> Json<ApiResponse<String>> {
    let del_data = query.delete_data.unwrap_or(false);
    let res = tokio::task::spawn_blocking(move || crate::games::delete_game_server(&id, del_data)).await;
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

/// Retourne la progression de l'installation et du premier déploiement d'un serveur de jeu.
async fn handle_games_deploy_status(
    Path(id): Path<String>,
) -> Json<ApiResponse<crate::games::GameDeployProgress>> {
    let res = tokio::task::spawn_blocking(move || crate::games::get_deployment_status(&id)).await;
    match res {
        Ok(Some(progress)) => Json(ApiResponse {
            success: true,
            data: Some(progress),
            message: None,
        }),
        _ => Json(ApiResponse {
            success: false,
            data: None,
            message: Some("Statut de déploiement introuvable.".into()),
        }),
    }
}

/// Paramètres de consultation des journaux de console d'un serveur de jeu.
#[derive(Deserialize)]
struct GameLogsQuery {
    lines: Option<usize>,
}

/// Lit les dernières lignes de la console d'un serveur de jeu en cours d'exécution.
async fn handle_games_logs(
    Path(id): Path<String>,
    Query(query): Query<GameLogsQuery>,
) -> Json<ApiResponse<String>> {
    let lines = query.lines.unwrap_or(150);
    let res = tokio::task::spawn_blocking(move || crate::games::get_game_server_logs(&id, lines)).await;
    match res {
        Ok(Ok(logs)) => Json(ApiResponse {
            success: true,
            data: Some(logs),
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

/// Envoie une commande shell/RCON à la console interactive d'un serveur de jeu actif.
async fn handle_games_command(
    Path(id): Path<String>,
    Json(payload): Json<crate::games::GameServerCommandRequest>,
) -> Json<ApiResponse<String>> {
    let res = tokio::task::spawn_blocking(move || crate::games::send_game_server_command(&id, &payload.command)).await;
    match res {
        Ok(Ok(resp)) => Json(ApiResponse {
            success: true,
            data: Some(resp),
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

/// Importe un fichier Egg JSON personnalisé téléversé par l'administrateur.
async fn handle_games_import_egg(
    Json(payload): Json<crate::games::ImportEggRequest>,
) -> Json<ApiResponse<crate::games::Egg>> {
    let res = tokio::task::spawn_blocking(move || crate::games::import_egg_file(payload)).await;
    match res {
        Ok(Ok(egg)) => Json(ApiResponse {
            success: true,
            data: Some(egg),
            message: Some("Egg importé avec succès !".into()),
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

/// Supprime un modèle d'Egg personnalisé du catalogue.
async fn handle_games_delete_egg(
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    let res = tokio::task::spawn_blocking(move || crate::games::delete_custom_egg(&id)).await;
    match res {
        Ok(Ok(_)) => Json(ApiResponse {
            success: true,
            data: None,
            message: Some("Egg personnalisé supprimé avec succès !".into()),
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

#[derive(serde::Deserialize)]
struct McVersionsQuery {
    loader: Option<String>,
}

/// Liste les chargeurs de mods et serveurs Minecraft pris en charge (Paper, Purpur, Fabric, Forge, Vanilla).
async fn handle_minecraft_loaders() -> Json<ApiResponse<Vec<crate::minecraft::MinecraftLoader>>> {
    let loaders = crate::minecraft::get_available_loaders();
    Json(ApiResponse {
        success: true,
        data: Some(loaders),
        message: None,
    })
}

/// Interroge les API officielles pour lister toutes les versions compatibles avec un chargeur Minecraft.
async fn handle_minecraft_versions(
    Query(q): Query<McVersionsQuery>,
) -> Json<ApiResponse<Vec<String>>> {
    let loader = q.loader.unwrap_or_else(|| "paper".to_string());
    let versions = tokio::task::spawn_blocking(move || {
        crate::minecraft::get_versions_for_loader(&loader)
    }).await.unwrap_or_default();

    Json(ApiResponse {
        success: true,
        data: Some(versions),
        message: None,
    })
}

/// Résout l'URL de téléchargement direct du binaire serveur JAR pour une version Minecraft donnée.
async fn handle_minecraft_resolve(
    Json(req): Json<crate::minecraft::ResolveMinecraftRequest>,
) -> Json<ApiResponse<crate::minecraft::ResolvedMinecraftServer>> {
    let res = tokio::task::spawn_blocking(move || {
        crate::minecraft::resolve_minecraft_server(&req.loader, &req.version)
    }).await;

    match res {
        Ok(Ok(resolved)) => Json(ApiResponse {
            success: true,
            data: Some(resolved),
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


// ============================================================================
// CONTRÔLEURS : GESTION DES IMAGES DOCKER
// ============================================================================

/// Liste l'ensemble des images Docker stockées localement avec leurs tags et tailles.
async fn handle_docker_images() -> Json<ApiResponse<DockerImagesOverview>> {
    let overview = list_docker_images();
    Json(ApiResponse {
        success: true,
        data: Some(overview),
        message: None,
    })
}

/// Paramètres de nettoyage des images Docker inutilisées.
#[derive(Debug, Deserialize)]
struct PruneImagesRequest {
    #[serde(default)]
    all: bool,
}

/// Purge les images Docker orphelines (dangling) ou non associées à un conteneur en cours.
async fn handle_docker_images_prune(
    payload: Option<Json<PruneImagesRequest>>,
) -> Json<ApiResponse<String>> {
    let all = payload.map(|Json(p)| p.all).unwrap_or(true);
    match prune_docker_images(all) {
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

/// Supprime une image Docker spécifique par son identifiant SHA256 ou son tag.
async fn handle_delete_docker_image(
    Path(id): Path<String>,
) -> Json<ApiResponse<String>> {
    match delete_docker_image(&id) {
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


// ============================================================================
// CONTRÔLEURS : PARTAGES RÉSEAU SAMBA (SMB / CIFS)
// ============================================================================

/// Fournit la vue globale des partages Samba actifs, sessions SMB connectées et diagnostics.
async fn handle_samba_overview() -> Json<ApiResponse<SambaOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_samba_overview()),
        message: None,
    })
}

/// Crée un nouveau partage SMB (nom de partage, chemin disque, droits d'accès et utilisateurs autorisés).
async fn handle_samba_create_share(
    Json(body): Json<CreateShareRequest>,
) -> Json<ApiResponse<SambaShare>> {
    match create_samba_share(body) {
        Ok(share) => Json(ApiResponse {
            success: true,
            data: Some(share),
            message: Some("Partage Samba créé avec succès !".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Met à jour les paramètres d'un partage Samba existant.
async fn handle_samba_update_share(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<UpdateShareRequest>,
) -> Json<ApiResponse<SambaShare>> {
    match update_samba_share(&id, body) {
        Ok(share) => Json(ApiResponse {
            success: true,
            data: Some(share),
            message: Some("Partage Samba mis à jour avec succès !".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Supprime un partage Samba et réécrit le fichier smb.conf dynamique.
async fn handle_samba_delete_share(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<()>> {
    match delete_samba_share(&id) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some("Partage Samba supprimé.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Enregistre les options de configuration globale du serveur Samba (Workgroup, Multi-channel, WSDD).
async fn handle_samba_update_global(
    Json(body): Json<UpdateGlobalConfigRequest>,
) -> Json<ApiResponse<SambaGlobalConfig>> {
    let mut cfg = crate::samba::load_samba_global_config();
    if let Some(w) = body.workgroup { cfg.workgroup = w; }
    if let Some(s) = body.server_string { cfg.server_string = s; }
    if let Some(min) = body.min_protocol { cfg.min_protocol = min; }
    if let Some(max) = body.max_protocol { cfg.max_protocol = max; }
    if let Some(wsdd) = body.wsdd_enabled { cfg.wsdd_enabled = wsdd; }
    if let Some(mc) = body.multi_channel { cfg.multi_channel = mc; }
    if let Some(aio) = body.aio_enabled { cfg.aio_enabled = aio; }

    match save_samba_global_config(&cfg) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(cfg),
            message: Some("Configuration globale Samba enregistrée.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Recharge le démon Samba (smbd / nmbd) sans interrompre les connexions actives (`smbcontrol all reload-config`).
async fn handle_samba_reload() -> Json<ApiResponse<String>> {
    match reload_samba_service() {
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

/// Exécute un diagnostic automatisé du service Samba via `testparm` et `smbstatus`.
async fn handle_samba_diagnostics() -> Json<ApiResponse<SambaDiagResult>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_samba_diagnostics()),
        message: None,
    })
}

/// Termine et déconnecte de force une session SMB cliente active par son identifiant de processus PID.
async fn handle_samba_disconnect_session(
    Json(body): Json<DisconnectSessionRequest>,
) -> Json<ApiResponse<()>> {
    match disconnect_samba_session(&body.pid) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some(format!("Session PID {} déconnectée.", body.pid)),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

// ============================================================================
// CONTRÔLEURS : PARTAGES SÉCURISÉS SFTP (SSH FILE TRANSFER)
// ============================================================================

/// Fournit la vue globale des partages SFTP configurés et des sessions SSH actives.
async fn handle_sftp_overview() -> Json<ApiResponse<SftpOverview>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_sftp_overview()),
        message: None,
    })
}

/// Liste les comptes utilisateurs système autorisés à se connecter en SFTP.
async fn handle_sftp_users() -> Json<ApiResponse<Vec<SftpUserAccess>>> {
    Json(ApiResponse {
        success: true,
        data: Some(get_sftp_allowed_users()),
        message: None,
    })
}

/// Crée un nouveau partage SFTP chrooté avec restriction de dossier.
async fn handle_sftp_create_share(
    Json(body): Json<CreateSftpShareRequest>,
) -> Json<ApiResponse<SftpShare>> {
    match create_sftp_share(body) {
        Ok(share) => Json(ApiResponse {
            success: true,
            data: Some(share),
            message: Some("Partage sFTP créé avec succès !".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Met à jour les options d'un partage SFTP existant.
async fn handle_sftp_update_share(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<CreateSftpShareRequest>,
) -> Json<ApiResponse<SftpShare>> {
    match update_sftp_share(&id, body) {
        Ok(share) => Json(ApiResponse {
            success: true,
            data: Some(share),
            message: Some("Partage sFTP mis à jour avec succès !".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Supprime un partage SFTP de la configuration.
async fn handle_sftp_delete_share(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<()>> {
    match delete_sftp_share(&id) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some("Partage sFTP supprimé.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Met à jour la configuration globale du démon SSH / SFTP (port d'écoute, chroot par défaut).
async fn handle_sftp_update_global(
    Json(body): Json<UpdateSftpGlobalRequest>,
) -> Json<ApiResponse<SftpGlobalConfig>> {
    let mut cfg = crate::sftp::load_sftp_global_config();
    if let Some(p) = body.port { cfg.port = p; }
    if let Some(r) = body.permit_root_login { cfg.permit_root_login = r; }
    if let Some(pw) = body.password_authentication { cfg.password_authentication = pw; }
    if let Some(pk) = body.pubkey_authentication { cfg.pubkey_authentication = pk; }
    if let Some(ch) = body.default_chroot_dir { cfg.default_chroot_dir = ch; }
    if let Some(m) = body.max_auth_tries { cfg.max_auth_tries = m; }
    if let Some(to) = body.idle_timeout_min { cfg.idle_timeout_min = to; }

    match save_sftp_global_config(&cfg) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(cfg),
            message: Some("Configuration globale sFTP enregistrée.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Recharge le service systemd `sshd` pour appliquer immédiatement les modifications de partage SFTP.
async fn handle_sftp_reload() -> Json<ApiResponse<String>> {
    if reload_sshd_service() {
        Json(ApiResponse {
            success: true,
            data: Some("Service SSH / sFTP rechargé avec succès.".into()),
            message: Some("Service SSH / sFTP rechargé.".into()),
        })
    } else {
        Json(ApiResponse {
            success: false,
            data: None,
            message: Some("Échec du rechargement du service SSH.".into()),
        })
    }
}

/// Déconnecte de force une session SFTP active.
async fn handle_sftp_disconnect_session(
    Json(body): Json<DisconnectSftpSessionRequest>,
) -> Json<ApiResponse<()>> {
    match disconnect_sftp_session(body.pid) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some(format!("Session PID {} déconnectée.", body.pid)),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

// ============================================================================
// CONTRÔLEURS : MONTAGES ÉPINGLÉS & PARTAGES RÉSEAU DISTANTS
// ============================================================================

/// Liste tous les points de montage locaux éligibles à l'affichage rapide dans la barre latérale.
async fn handle_storage_mounts_list() -> Json<ApiResponse<Vec<crate::remote_shares::StorageMountItem>>> {
    let mounts = crate::remote_shares::get_storage_mounts();
    Json(ApiResponse {
        success: true,
        data: Some(mounts),
        message: None,
    })
}

/// Liste les raccourcis de dossiers épinglés personnalisés par l'utilisateur.
async fn handle_pinned_mounts_list() -> Json<ApiResponse<Vec<crate::remote_shares::PinnedMount>>> {
    let pinned = crate::remote_shares::load_pinned_mounts();
    Json(ApiResponse {
        success: true,
        data: Some(pinned),
        message: None,
    })
}

/// Épingle un nouveau chemin de dossier dans le panneau de navigation latéral.
async fn handle_pinned_mounts_add(
    Json(body): Json<crate::remote_shares::PinMountRequest>,
) -> Json<ApiResponse<crate::remote_shares::PinnedMount>> {
    match crate::remote_shares::pin_mount(&body.path, body.label.as_deref(), body.icon.as_deref()) {
        Ok(pin) => Json(ApiResponse {
            success: true,
            data: Some(pin),
            message: Some("Point de montage épinglé au gestionnaire de fichiers.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Retire un point de montage de la liste des raccourcis épinglés.
async fn handle_pinned_mounts_remove(
    Json(body): Json<crate::remote_shares::UnpinMountRequest>,
) -> Json<ApiResponse<()>> {
    match crate::remote_shares::unpin_mount(&body.path) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some("Point de montage retiré des épinglés.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Paramètres de réorganisation de l'ordre d'affichage des montages épinglés.
#[derive(Debug, Deserialize)]
pub struct ReorderPinnedMountsRequest {
    pub paths: Vec<String>,
}

/// Enregistre le nouvel ordre d'affichage des raccourcis épinglés après un glisser-déposer (drag-and-drop).
async fn handle_pinned_mounts_reorder(
    Json(body): Json<ReorderPinnedMountsRequest>,
) -> Json<ApiResponse<Vec<crate::remote_shares::PinnedMount>>> {
    match crate::remote_shares::reorder_pinned_mounts(&body.paths) {
        Ok(list) => Json(ApiResponse {
            success: true,
            data: Some(list),
            message: Some("Ordre des épingles mis à jour avec succès.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Découvre automatiquement les serveurs de fichiers SMB/SFTP disponibles sur le réseau local via mDNS / WSDD / NetBIOS.
async fn handle_network_discover() -> Json<ApiResponse<Vec<crate::remote_shares::DiscoveredDevice>>> {
    let devices = tokio::task::spawn_blocking(crate::remote_shares::discover_network_devices)
        .await
        .unwrap_or_default();
    Json(ApiResponse {
        success: true,
        data: Some(devices),
        message: None,
    })
}

/// Liste l'ensemble des partages distants SMB ou SFTP montés vers l'arborescence locale du NAS.
async fn handle_remote_mounts_list() -> Json<ApiResponse<Vec<crate::remote_shares::RemoteMountConfig>>> {
    let mounts = crate::remote_shares::load_remote_mounts();
    Json(ApiResponse {
        success: true,
        data: Some(mounts),
        message: None,
    })
}

/// Connecte et monte un nouveau partage distant distant (CIFS/SMB ou SSHFS/SFTP).
async fn handle_remote_mounts_create(
    Json(body): Json<crate::remote_shares::CreateRemoteMountRequest>,
) -> Json<ApiResponse<crate::remote_shares::RemoteMountConfig>> {
    match tokio::task::spawn_blocking(move || crate::remote_shares::create_and_mount_remote(body)).await {
        Ok(Ok(cfg)) => Json(ApiResponse {
            success: true,
            data: Some(cfg),
            message: Some("Partage distant connecté et monté avec succès.".into()),
        }),
        Ok(Err(err)) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
        Err(join_err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("Erreur d execution : {}", join_err)),
        }),
    }
}

/// Supprime la configuration d'un partage distant et démonte son point d'accès local.
async fn handle_remote_mounts_delete(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<()>> {
    match crate::remote_shares::delete_remote_mount(&id) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some("Partage distant démonté et supprimé.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}

/// Remonte un partage distant préalablement configuré mais temporairement déconnecté.
async fn handle_remote_mounts_mount(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<()>> {
    let mounts = crate::remote_shares::load_remote_mounts();
    if let Some(m) = mounts.into_iter().find(|m| m.id == id) {
        match tokio::task::spawn_blocking(move || crate::remote_shares::execute_mount(&m)).await {
            Ok(Ok(_)) => Json(ApiResponse {
                success: true,
                data: Some(()),
                message: Some("Partage connecté.".into()),
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
    } else {
        Json(ApiResponse {
            success: false,
            data: None,
            message: Some("Partage introuvable".into()),
        })
    }
}

/// Démonte temporairement un partage distant sans supprimer sa configuration.
async fn handle_remote_mounts_unmount(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<()>> {
    match crate::remote_shares::unmount_remote(&id) {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: Some(()),
            message: Some("Partage démonté.".into()),
        }),
        Err(err) => Json(ApiResponse {
            success: false,
            data: None,
            message: Some(err),
        }),
    }
}
