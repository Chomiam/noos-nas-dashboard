pub fn detect_device_filesystem(dev_path: &str) -> Option<String> {
    if let Ok(out) = Command::new("blkid").args(["-o", "value", "-s", "TYPE", dev_path]).output() {
        if out.status.success() && !out.stdout.is_empty() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    if let Ok(out) = Command::new("lsblk").args(["-no", "FSTYPE", dev_path]).output() {
        if out.status.success() && !out.stdout.is_empty() {
            let s = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}


use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMount {
    pub id: String,
    pub name: String,
    pub device: String,
    #[serde(default, rename = "deviceUuid")]
    pub device_uuid: Option<String>,
    #[serde(rename = "mountPoint")]
    pub mount_point: String,
    #[serde(rename = "fsType")]
    pub fs_type: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovableDevice {
    pub name: String,
    pub path: String,
    pub model: String,
    pub vendor: Option<String>,
    pub size_human: String,
    pub fstype: Option<String>,
    pub label: Option<String>,
    pub mountpoint: Option<String>,
    pub is_optical: bool,
    pub is_mounted: bool,
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageOverview {
    pub pools: Vec<StoragePool>,
    pub physical_disks: Vec<PhysicalDiskInfo>,
    pub logical_raids: Vec<LogicalRaidInfo>,
    pub active_sync: Option<RaidSyncProgress>,
    #[serde(default)]
    pub persisted_mounts: Vec<PersistedMount>,
    #[serde(default)]
    pub removable_devices: Vec<RemovableDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePool {
    #[allow(dead_code)]
    pub name: String,
    pub mountpoint: String,
    pub filesystem: String,
    pub health: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f32,
    pub total_human: String,
    pub used_human: String,
    pub free_human: String,
    pub owner_user: String,
    pub owner_group: String,
    pub permissions_mode: String,
    pub is_user_writable: bool,
    pub needs_permission_repair: bool,
    #[serde(default)]
    pub is_persisted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalDiskInfo {
    pub name: String,
    pub path: String,
    pub bay_label: String, // "Baie 1 (SATA)", "Slot M.2 NVMe #1"
    pub size_human: String,
    pub model: String,
    pub serial: String,
    pub disk_type: String, // "HDD (SATA)" ou "SSD NVMe PCIe"
    pub is_rotational: bool,
    pub is_system: bool,
    pub fstype: Option<String>,
    pub role: String, // "Système NixOS", "Membre de md0 (RAID5)", "Libre (Non alloué)", etc.
    pub power_state: String, // "Actif / En ligne", "Veille (Standby)"
    pub smart_status: String,
    pub temperature_c: f32,
    pub partitions: Vec<PartitionInfo>,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub is_removable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub path: String,
    pub size_human: String,
    pub fstype: Option<String>,
    pub mountpoint: Option<String>,
    pub is_system: bool,
    #[serde(default)]
    pub is_free_space: bool,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicalRaidInfo {
    pub name: String,
    pub device: String, // ex: "/dev/md0" ou "/dev/md/storage"
    pub level: String, // "RAID5", "RAID1", "RAID0", "RAID6", "RAID10", "JBOD"
    pub status: String, // "clean", "active", "degraded", "rebuilding"
    pub health: String, // "Sain", "En resynchronisation", "Dégradé"
    pub size_human: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f32,
    pub filesystem: String,
    pub mountpoint: Option<String>,
    pub members: Vec<String>,
    pub sync_progress: Option<RaidSyncProgress>,
    #[serde(default)]
    pub fs_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidSyncProgress {
    pub array: String,
    pub action: String, // "resync", "recovery", "check"
    pub percent: f32,
    pub speed_mb_s: f32,
    pub finish_minutes: f32,
    pub finish_human: String,
}

#[derive(Debug, Deserialize)]
pub struct RepairPermissionsRequest {
    pub path: String,
    pub target_user: Option<String>,
    pub target_group: Option<String>,
    pub password: Option<String>,
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct MountVolumeRequest {
    pub name: String,
    pub device: String,
    pub mountpoint: Option<String>,
    pub fs_type: Option<String>,
    pub raid_type: Option<String>,
    pub lv_name: Option<String>,
    pub options: Option<Vec<String>>,
    pub persist: Option<bool>,
    pub force_format: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePartitionRequest {
    pub disk_path: String,
    pub size_mb: Option<u64>,
    pub fs_type: String,
    pub label: Option<String>,
    pub mountpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeletePartitionRequest {
    pub partition_path: String,
}

#[derive(Debug, Deserialize)]
pub struct EjectRemovableRequest {
    pub device_path: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UmountVolumeRequest {
    pub mountpoint: String,
    pub remove_persist: Option<bool>,
}

// --------------------------------------------------------------------------
// PERSISTANCE DÉCLARATIVE NIXOS (mounts.json)
// --------------------------------------------------------------------------
pub fn get_mounts_json_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    // 1. Emplacement persistant standardisé
    paths.push(PathBuf::from("/var/lib/noos/mounts.json"));
    paths.push(PathBuf::from("/var/lib/steveos/mounts.json"));

    // 2. Dossier de configuration résolu
    let cfg_dir = crate::updates::resolve_config_dir();
    let cfg_mounts = cfg_dir.join("mounts.json");
    if !paths.contains(&cfg_mounts) {
        paths.push(cfg_mounts);
    }

    // 3. Emplacements connus
    let u = target_user();
    let user_dev_noos = PathBuf::from(format!("/home/{}/Projects/noos-nas/mounts.json", u));
    if !paths.contains(&user_dev_noos) {
        paths.push(user_dev_noos);
    }
    let user_dev_steve = PathBuf::from(format!("/home/{}/Projects/steveos-nas/mounts.json", u));
    if !paths.contains(&user_dev_steve) {
        paths.push(user_dev_steve);
    }

    for p in &[
        "/etc/nixos/mounts.json",
        "/etc/nixos/noos-nas/mounts.json",
        "/etc/nixos/steveos-nas/mounts.json",
        "./mounts.json",
        "../mounts.json",
    ] {
        let pb = PathBuf::from(p);
        if !paths.contains(&pb) {
            paths.push(pb);
        }
    }
    paths
}

pub fn load_persisted_mounts() -> Vec<PersistedMount> {
    for path in get_mounts_json_paths() {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(mounts) = serde_json::from_str::<Vec<PersistedMount>>(&content) {
                    return mounts;
                }
            }
        }
    }
    Vec::new()
}

pub fn save_persisted_mounts(mounts: &[PersistedMount]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(mounts).map_err(|e| e.to_string())?;
    let _ = std::fs::create_dir_all("/var/lib/noos");
    let _ = std::fs::create_dir_all("/var/lib/steveos");

    let mut written = false;
    for path in get_mounts_json_paths() {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        if std::fs::write(&path, &json).is_ok() {
            written = true;
        }
    }

    if written {
        Ok(())
    } else {
        Err("Impossible de sauvegarder mounts.json sur le disque".into())
    }
}

fn sanitize_runtime_mount_options(options: &[String]) -> String {
    let filtered: Vec<&str> = options
        .iter()
        .map(|s| s.trim())
        .filter(|&opt| {
            !opt.is_empty()
                && opt != "nofail"
                && !opt.starts_with("x-systemd.")
                && opt != "auto"
                && opt != "noauto"
        })
        .collect();

    if filtered.is_empty() {
        "defaults,noatime".to_string()
    } else {
        filtered.join(",")
    }
}

#[derive(Debug, Deserialize)]
pub struct FormatDiskRequest {
    pub device: String,
    pub fs_type: String, // "btrfs", "ext4", "xfs", "vfat"
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CreateRaidRequest {
    pub name: String,
    pub level: String, // "raid0", "raid1", "raid5", "raid6", "raid10", "linear"
    pub devices: Vec<String>,
    pub fs_type: String, // "btrfs", "ext4"
    pub mountpoint: String, // ex: "/mnt/storage"
}

#[derive(Debug, Deserialize, Clone)]
pub struct DestroyRaidRequest {
    pub name: String,
    pub device: String,
    #[serde(default)]
    pub wipe_members: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageJobType {
    #[serde(rename = "destroy_raid")]
    DestroyRaid,
    #[serde(rename = "create_raid")]
    CreateRaid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageJobStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "resumed")]
    Resumed,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestroyRaidParams {
    pub name: String,
    pub device: String,
    pub wipe_members: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRaidParams {
    pub name: String,
    pub level: String,
    pub devices: Vec<String>,
    pub fs_type: String,
    pub mountpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageJob {
    pub id: String,
    pub job_type: StorageJobType,
    pub status: StorageJobStatus,
    pub current_step: usize,
    pub total_steps: usize,
    pub step_name: String,
    pub step_detail: String,
    pub progress_percent: u8,
    pub target_name: String,
    pub target_device: String,
    pub member_devices: Vec<String>,
    #[serde(default)]
    pub destroy_params: Option<DestroyRaidParams>,
    #[serde(default)]
    pub create_params: Option<CreateRaidParams>,
    pub started_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub result_message: Option<String>,
}

// --------------------------------------------------------------------------
// LECTURE ET SCAN CONSOLIDÉ DU STOCKAGE
// --------------------------------------------------------------------------
pub fn get_storage_overview() -> StorageOverview {
    let mut pools = scan_storage_pools();
    let logical_raids = scan_logical_raids(&pools);
    let physical_disks = scan_physical_disks(&logical_raids);
    let active_sync = logical_raids.iter().find_map(|r| r.sync_progress.clone());
    let persisted_mounts = load_persisted_mounts();
    let removable_devices = scan_removable_devices();

    for pool in &mut pools {
        if persisted_mounts.iter().any(|m| m.mount_point == pool.mountpoint) {
            pool.is_persisted = true;
        }
    }

    StorageOverview {
        pools,
        physical_disks,
        logical_raids,
        active_sync,
        persisted_mounts,
        removable_devices,
    }
}

pub fn get_raid_sync_progress() -> Option<RaidSyncProgress> {
    parse_mdstat_sync()
}

fn scan_storage_pools() -> Vec<StoragePool> {
    let mut pools = Vec::new();

    if let Ok(output) = Command::new("df").arg("-B1").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                let mountpoint = parts[5].to_string();
                let fs = parts[0].to_string();
                // Garder les points de montages pertinents (/, /mnt/*, /home, /storage)
                if mountpoint == "/" || mountpoint.starts_with("/mnt") || mountpoint.starts_with("/home") || mountpoint.starts_with("/storage") {
                    let total_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let used_bytes = parts[2].parse::<u64>().unwrap_or(0);
                    let free_bytes = parts[3].parse::<u64>().unwrap_or(0);

                    if total_bytes > 0 {
                        let usage_percent = ((used_bytes as f64 / total_bytes as f64) * 100.0) as f32;
                        let pool_name = if mountpoint == "/" {
                            "Système Root (NixOS)".into()
                        } else if mountpoint == "/home" {
                            "Dossiers Utilisateurs (/home)".into()
                        } else {
                            mountpoint.split('/').last().unwrap_or("pool").to_string()
                        };

                        let active_user = target_user();
                        let (owner_user, owner_group, permissions_mode, is_user_writable, needs_permission_repair) = check_path_permissions(&mountpoint, &active_user);

                        pools.push(StoragePool {
                            name: pool_name,
                            mountpoint,
                            filesystem: fs,
                            health: "ONLINE".into(),
                            total_bytes,
                            used_bytes,
                            free_bytes,
                            usage_percent: (usage_percent * 10.0).round() / 10.0,
                            total_human: format_bytes(total_bytes),
                            used_human: format_bytes(used_bytes),
                            free_human: format_bytes(free_bytes),
                            owner_user,
                            owner_group,
                            permissions_mode,
                            is_user_writable,
                            needs_permission_repair,
                            is_persisted: false,
                        });
                    }
                }
            }
        }
    }
    pools
}

fn scan_logical_raids(pools: &[StoragePool]) -> Vec<LogicalRaidInfo> {
    let mut raids = Vec::new();

    // 1. Scanner /proc/mdstat pour les grappes mdadm Linux
    if let Ok(content) = std::fs::read_to_string("/proc/mdstat") {
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if line.contains(':') && (line.contains("active") || line.contains("inactive")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let md_name = parts[0].to_string();
                    let status_str = parts[2].to_string();
                    let raid_level = if parts.len() >= 4 {
                        parts[3].to_uppercase()
                    } else {
                        "RAID".to_string()
                    };

                    let mut members = Vec::new();
                    for p in parts.iter().skip(4) {
                        let member = p.split('[').next().unwrap_or("").to_string();
                        if !member.is_empty() {
                            members.push(format!("/dev/{}", member));
                        }
                    }

                    let mut size_blocks = 0u64;
                    let mut sync_prog = None;

                    if i + 1 < lines.len() {
                        let next_line = lines[i + 1];
                        if let Some(blocks_part) = next_line.split_whitespace().next() {
                            size_blocks = blocks_part.parse::<u64>().unwrap_or(0);
                        }
                    }

                    if i + 2 < lines.len() {
                        let sync_line = lines[i + 2];
                        if sync_line.contains("resync") || sync_line.contains("recovery") || sync_line.contains("check") {
                            sync_prog = parse_sync_line(&md_name, sync_line);
                        }
                    }

                    let dev_path = format!("/dev/{}", md_name);
                    let total_bytes = size_blocks * 1024;

                    let matched_pool = pools.iter().find(|p| p.filesystem.contains(&md_name));
                    let probed_fs = detect_device_filesystem(&dev_path);
                    let (mountpoint, used_bytes, free_bytes, usage_percent, fs_name) = if let Some(p) = matched_pool {
                        (Some(p.mountpoint.clone()), p.used_bytes, p.free_bytes, p.usage_percent, probed_fs.clone().unwrap_or_else(|| p.filesystem.clone()))
                    } else {
                        (None, 0, total_bytes, 0.0, probed_fs.clone().unwrap_or_else(|| "Non formaté".to_string()))
                    };

                    let health = if sync_prog.is_some() {
                        "En resynchronisation".to_string()
                    } else if status_str == "active" {
                        "Sain".to_string()
                    } else {
                        "Dégradé".to_string()
                    };

                    raids.push(LogicalRaidInfo {
                        name: md_name,
                        device: dev_path,
                        level: raid_level,
                        status: status_str,
                        health,
                        size_human: format_bytes(total_bytes),
                        total_bytes,
                        used_bytes,
                        free_bytes,
                        usage_percent,
                        filesystem: fs_name,
                        mountpoint,
                        members,
                        sync_progress: sync_prog,
                        fs_type: probed_fs,
                    });
                }
            }
            i += 1;
        }
    }

    // 2. Scanner LVM2 : Volumes Physiques (pvs), Volumes Logiques (lvs) et Groupes de Volumes (vgs)
    let mut vg_to_pvs: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(output) = Command::new("pvs").args(["--reportformat", "json"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(reports) = json.get("report").and_then(|r| r.as_array()) {
                for rep in reports {
                    if let Some(pvs) = rep.get("pv").and_then(|p| p.as_array()) {
                        for pv in pvs {
                            let pv_name = pv.get("pv_name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                            let vg_name = pv.get("vg_name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                            if !pv_name.is_empty() && !vg_name.is_empty() {
                                vg_to_pvs.entry(vg_name).or_default().push(pv_name);
                            }
                        }
                    }
                }
            }
        }
    }

    // Mapping LVs par VG
    struct LvParsed {
        name: String,
        path: String,
        size: u64,
        segtype: String,
    }
    let mut vg_to_lvs: HashMap<String, Vec<LvParsed>> = HashMap::new();
    if let Ok(output) = Command::new("lvs")
        .args(["--units", "b", "--nosuffix", "-o", "lv_name,vg_name,lv_size,segtype,lv_path", "--reportformat", "json"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(reports) = json.get("report").and_then(|r| r.as_array()) {
                for rep in reports {
                    if let Some(lvs) = rep.get("lv").and_then(|l| l.as_array()) {
                        for lv in lvs {
                            let lv_name = lv.get("lv_name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                            let vg_name = lv.get("vg_name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                            let lv_path = lv.get("lv_path").and_then(|s| s.as_str()).unwrap_or("").to_string();
                            let segtype = lv.get("segtype").and_then(|s| s.as_str()).unwrap_or("linear").to_string();
                            let lv_size = lv.get("lv_size").and_then(|s| s.as_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);

                            if !lv_name.is_empty() && !vg_name.is_empty() {
                                vg_to_lvs.entry(vg_name.clone()).or_default().push(LvParsed {
                                    name: lv_name.clone(),
                                    path: if lv_path.is_empty() { format!("/dev/{}/{}", vg_name, lv_name) } else { lv_path },
                                    size: lv_size,
                                    segtype,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Scanner VGs
    if let Ok(output) = Command::new("vgs")
        .args(["--units", "b", "--nosuffix", "--reportformat", "json"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(reports) = json.get("report").and_then(|r| r.as_array()) {
                for rep in reports {
                    if let Some(vgs) = rep.get("vg").and_then(|v| v.as_array()) {
                        for vg in vgs {
                            let vg_name = vg.get("vg_name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                            if vg_name.is_empty() {
                                continue;
                            }
                            let pv_count = vg.get("pv_count").and_then(|s| s.as_str()).and_then(|s| s.parse::<usize>().ok()).unwrap_or(1);
                            let vg_size = vg.get("vg_size").and_then(|s| s.as_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                            let vg_free = vg.get("vg_free").and_then(|s| s.as_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                            let pvs = vg_to_pvs.get(&vg_name).cloned().unwrap_or_default();

                            let has_lvs = vg_to_lvs.get(&vg_name).map(|l| !l.is_empty()).unwrap_or(false);

                            if has_lvs {
                                if let Some(lvs_list) = vg_to_lvs.get(&vg_name) {
                                    for lv in lvs_list {
                                        let matched_pool = pools.iter().find(|p| p.filesystem.contains(&lv.name) || p.filesystem.contains(&lv.path));
                                        let probed_lv_fs = detect_device_filesystem(&lv.path);
                                        let (mountpoint, used_bytes, free_bytes, usage_percent, fs_name) = if let Some(p) = matched_pool {
                                            (Some(p.mountpoint.clone()), p.used_bytes, p.free_bytes, p.usage_percent, probed_lv_fs.clone().unwrap_or_else(|| p.filesystem.clone()))
                                        } else {
                                            (None, 0, lv.size, 0.0, probed_lv_fs.clone().unwrap_or_else(|| "Non formaté".to_string()))
                                        };

                                        let level_label = match lv.segtype.to_lowercase().as_str() {
                                            "raid5" => "LVM2 RAID 5".to_string(),
                                            "raid6" => "LVM2 RAID 6".to_string(),
                                            "raid1" => "LVM2 RAID 1 (Miroir)".to_string(),
                                            "raid0" | "striped" => "LVM2 RAID 0 (Agrégat)".to_string(),
                                            "raid10" => "LVM2 RAID 10".to_string(),
                                            "thin" => "LVM2 Thin Pool".to_string(),
                                            _ => format!("LVM2 {}", lv.segtype.to_uppercase()),
                                        };

                                        raids.push(LogicalRaidInfo {
                                            name: format!("{}/{}", vg_name, lv.name),
                                            device: lv.path.clone(),
                                            level: level_label,
                                            status: "active".to_string(),
                                            health: "Sain".to_string(),
                                            size_human: format_bytes(lv.size),
                                            total_bytes: lv.size,
                                            used_bytes,
                                            free_bytes,
                                            usage_percent,
                                            filesystem: fs_name,
                                            mountpoint,
                                            members: pvs.clone(),
                                            sync_progress: None,
                                            fs_type: probed_lv_fs,
                                        });
                                    }
                                }
                            } else {
                                // Volume Group LVM2 sans LV encore alloué : exposer la grappe globale de stockage
                                let used_bytes = vg_size.saturating_sub(vg_free);
                                let usage_percent = if vg_size > 0 {
                                    ((used_bytes as f64 / vg_size as f64) * 100.0) as f32
                                } else {
                                    0.0
                                };

                                let matched_pool = pools.iter().find(|p| p.filesystem.contains(&vg_name));
                                let probed_vg_fs = detect_device_filesystem(&format!("/dev/{}", vg_name));
                                let (mountpoint, fs_name) = if let Some(p) = matched_pool {
                                    (Some(p.mountpoint.clone()), probed_vg_fs.clone().unwrap_or_else(|| p.filesystem.clone()))
                                } else {
                                    (None, probed_vg_fs.clone().unwrap_or_else(|| "LVM2 Volume Group".to_string()))
                                };

                                raids.push(LogicalRaidInfo {
                                    name: vg_name.clone(),
                                    device: format!("/dev/{}", vg_name),
                                    level: format!("Grappe LVM2 (Pool {} disques)", pvs.len().max(pv_count)),
                                    status: "active".to_string(),
                                    health: "Sain".to_string(),
                                    size_human: format_bytes(vg_size),
                                    total_bytes: vg_size,
                                    used_bytes,
                                    free_bytes: vg_free,
                                    usage_percent: (usage_percent * 10.0).round() / 10.0,
                                    filesystem: fs_name,
                                    mountpoint,
                                    members: pvs,
                                    sync_progress: None,
                                    fs_type: probed_vg_fs,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Scanner Btrfs multi-device / pools si pertinent
    if let Ok(output) = Command::new("btrfs").args(["filesystem", "show"]).output() {
        let str_out = String::from_utf8_lossy(&output.stdout);
        for block in str_out.split("\n\n") {
            if block.contains("Label:") || block.contains("uuid:") {
                let mut label = "btrfs-pool".to_string();
                let mut dev_count = 0;
                let mut members = Vec::new();
                let total_bytes = 0u64;

                for l in block.lines() {
                    if l.contains("Label:") {
                        if let Some(lbl) = l.split('\'').nth(1) {
                            if !lbl.is_empty() {
                                label = lbl.to_string();
                            }
                        }
                    }
                    if l.contains("Total devices") {
                        dev_count = l.split_whitespace().last().and_then(|c| c.parse::<usize>().ok()).unwrap_or(1);
                    }
                    if l.contains("devid") {
                        if let Some(path) = l.split("path").nth(1) {
                            members.push(path.trim().to_string());
                        }
                    }
                }

                if dev_count > 1 && !members.is_empty() {
                    if !raids.iter().any(|r| r.name == label) {
                        let matched_pool = pools.iter().find(|p| p.mountpoint == "/" || p.name.contains(&label));
                        raids.push(LogicalRaidInfo {
                            name: label.clone(),
                            device: members.first().cloned().unwrap_or_else(|| "/dev/btrfs".into()),
                            level: format!("Btrfs Multi-Disques ({} dev)", dev_count),
                            status: "active".into(),
                            health: "Sain".into(),
                            size_human: matched_pool.map(|p| p.total_human.clone()).unwrap_or_else(|| "Multi-To".into()),
                            total_bytes,
                            used_bytes: matched_pool.map(|p| p.used_bytes).unwrap_or(0),
                            free_bytes: matched_pool.map(|p| p.free_bytes).unwrap_or(0),
                            usage_percent: matched_pool.map(|p| p.usage_percent).unwrap_or(0.0),
                            filesystem: "btrfs".into(),
                            mountpoint: matched_pool.map(|p| p.mountpoint.clone()),
                            members,
                            sync_progress: None,
                            fs_type: Some("btrfs".into()),
                        });
                    }
                }
            }
        }
    }

    raids
}

pub fn scan_removable_devices() -> Vec<RemovableDevice> {
    let mut list = Vec::new();
    let output = Command::new("lsblk")
        .args(["-b", "--json", "-o", "NAME,PATH,SIZE,FSTYPE,LABEL,MOUNTPOINT,TYPE,TRAN,MODEL,SERIAL,ROTA,RM,HOTPLUG,VENDOR"])
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(devices) = json.get("blockdevices").and_then(|d| d.as_array()) {
                for dev in devices {
                    let dev_type = dev.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let tran = dev.get("tran").and_then(|v| v.as_str()).unwrap_or("");
                    let rm = dev.get("rm").and_then(|v| v.as_bool()).unwrap_or(false);
                    let hotplug = dev.get("hotplug").and_then(|v| v.as_bool()).unwrap_or(false);
                    let is_optical = dev_type == "rom" || dev.get("name").and_then(|v| v.as_str()).unwrap_or("").starts_with("sr");

                    let is_removable = is_optical || tran == "usb" || rm || hotplug;
                    if !is_removable {
                        continue;
                    }

                    let name = dev.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let path = dev.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let model = dev.get("model").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).unwrap_or_else(|| {
                        if is_optical { "Lecteur Optique (CD/DVD/Blu-ray)".to_string() } else { format!("Périphérique USB ({})", name) }
                    });
                    let vendor = dev.get("vendor").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
                    let size_bytes = dev.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                    let fstype = dev.get("fstype").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let label = dev.get("label").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let mountpoint = dev.get("mountpoint").and_then(|v| v.as_str()).map(|s| s.to_string());

                    let mut parts = Vec::new();
                    let mut any_mounted = mountpoint.is_some();
                    if let Some(children) = dev.get("children").and_then(|c| c.as_array()) {
                        for child in children {
                            let c_name = child.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let c_path = child.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let c_size = child.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                            let c_fs = child.get("fstype").and_then(|v| v.as_str()).map(|s| s.to_string());
                            let c_lbl = child.get("label").and_then(|v| v.as_str()).map(|s| s.to_string());
                            let c_mnt = child.get("mountpoint").and_then(|v| v.as_str()).map(|s| s.to_string());
                            if c_mnt.is_some() { any_mounted = true; }

                            parts.push(PartitionInfo {
                                name: c_name,
                                path: c_path,
                                size_human: format_bytes(c_size),
                                fstype: c_fs,
                                mountpoint: c_mnt,
                                is_system: false,
                                is_free_space: false,
                                size_bytes: c_size,
                                label: c_lbl,
                            });
                        }
                    }

                    list.push(RemovableDevice {
                        name,
                        path,
                        model,
                        vendor,
                        size_human: format_bytes(size_bytes),
                        fstype,
                        label,
                        mountpoint,
                        is_optical,
                        is_mounted: any_mounted,
                        partitions: parts,
                    });
                }
            }
        }
    }
    list
}

fn scan_physical_disks(logical_raids: &[LogicalRaidInfo]) -> Vec<PhysicalDiskInfo> {
    let mut physical_disks = Vec::new();

    if let Ok(output) = Command::new("lsblk")
        .args(["-b", "-J", "-o", "NAME,PATH,SIZE,ROTA,TYPE,MOUNTPOINTS,MODEL,SERIAL,FSTYPE,TRAN,RM,HOTPLUG,LABEL"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(devices) = json.get("blockdevices").and_then(|b| b.as_array()) {
                for dev in devices {
                    let dev_type = dev.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if dev_type != "disk" {
                        continue;
                    }

                    let name = dev.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    if name.starts_with("loop") || name.starts_with("zram") {
                        continue;
                    }

                    let path = dev.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
                    let total_size_bytes = dev.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                    let size_human = format_bytes(total_size_bytes);
                    let is_rotational = dev.get("rota").and_then(|r| r.as_bool()).unwrap_or(true);
                    let mut model = dev.get("model").and_then(|m| m.as_str()).unwrap_or("").trim().to_string();
                    if model.is_empty() {
                        model = if path.contains("nvme") { "SSD NVMe PCIe".into() } else { "Disque SATA".into() };
                    }
                    let serial = dev.get("serial").and_then(|s| s.as_str()).unwrap_or("N/A").trim().to_string();
                    let fstype = dev.get("fstype").and_then(|f| f.as_str()).map(|f| f.to_string());

                    let tran = dev.get("tran").and_then(|t| t.as_str()).unwrap_or("");
                    let rm = dev.get("rm").and_then(|r| r.as_bool()).unwrap_or(false);
                    let hotplug = dev.get("hotplug").and_then(|h| h.as_bool()).unwrap_or(false);
                    let is_removable = tran == "usb" || rm || hotplug;

                    let disk_type = if path.contains("nvme") {
                        "SSD NVMe PCIe".to_string()
                    } else if is_rotational {
                        "HDD 3.5\" SATA".to_string()
                    } else {
                        "SSD SATA Flash".to_string()
                    };

                    let bay_label = if name == "sda" {
                        "Baie 1 (SATA)".to_string()
                    } else if name == "sdb" {
                        "Baie 2 (SATA)".to_string()
                    } else if name == "sdc" {
                        "Baie 3 (SATA)".to_string()
                    } else if name == "sdd" {
                        "Baie 4 (SATA)".to_string()
                    } else if name.starts_with("sd") && name.len() >= 3 {
                        let letter = name.chars().nth(2).unwrap_or('a');
                        let bay_num = (letter as u8).saturating_sub(b'a') + 1;
                        format!("Baie {} (SATA)", bay_num)
                    } else if name == "nvme0n1" {
                        "Slot M.2 NVMe #1".to_string()
                    } else if name == "nvme1n1" {
                        "Slot M.2 NVMe #2 (Système)".to_string()
                    } else if name.starts_with("nvme") {
                        format!("Slot NVMe {}", name)
                    } else {
                        format!("Disque {}", name)
                    };

                    let mut partitions = Vec::new();
                    let contains_system = path.contains("nvme1n1") || is_system_device(&path);
                    let mut allocated_bytes: u64 = 0;

                    if let Some(children) = dev.get("children").and_then(|c| c.as_array()) {
                        for child in children {
                            let part_name = child.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                            let part_path = child.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
                            let part_size_bytes = child.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                            allocated_bytes += part_size_bytes;
                            let part_size = format_bytes(part_size_bytes);
                            let part_fs = child.get("fstype").and_then(|f| f.as_str()).map(|s| s.to_string());
                            let part_lbl = child.get("label").and_then(|l| l.as_str()).map(|s| s.to_string());
                            let mut part_mount = None;
                            let mut part_is_sys = false;

                            if let Some(mounts) = child.get("mountpoints").and_then(|m| m.as_array()) {
                                for m_val in mounts {
                                    if let Some(m0) = m_val.as_str() {
                                        if part_mount.is_none() {
                                            part_mount = Some(m0.to_string());
                                        }
                                        if m0 == "/" || m0 == "/boot" || m0.starts_with("/nix") {
                                            part_is_sys = true;
                                        }
                                    }
                                }
                            }

                            partitions.push(PartitionInfo {
                                name: part_name,
                                path: part_path,
                                size_human: part_size,
                                fstype: part_fs,
                                mountpoint: part_mount,
                                is_system: part_is_sys,
                                is_free_space: false,
                                size_bytes: part_size_bytes,
                                label: part_lbl,
                            });
                        }
                    }

                    if total_size_bytes > allocated_bytes && (total_size_bytes - allocated_bytes) >= 50 * 1024 * 1024 {
                        let free_bytes = total_size_bytes - allocated_bytes;
                        partitions.push(PartitionInfo {
                            name: "Espace non alloué".to_string(),
                            path: String::new(),
                            size_human: format_bytes(free_bytes),
                            fstype: None,
                            mountpoint: None,
                            is_system: false,
                            is_free_space: true,
                            size_bytes: free_bytes,
                            label: None,
                        });
                    }

                    let role = if contains_system {
                        "Système NixOS (Verrouillé)".to_string()
                    } else {
                        let member_of = logical_raids.iter().find(|r| {
                            r.members.iter().any(|m| m.contains(&name) || m.contains(&path))
                        });

                        if let Some(r) = member_of {
                            format!("Membre de {} ({})", r.name, r.level)
                        } else if let Some(fs) = &fstype {
                            if fs.contains("LVM") {
                                "Membre LVM2 (Pool)".to_string()
                            } else {
                                format!("Volume simple ({})", fs)
                            }
                        } else if !partitions.is_empty() {
                            "Partitionné".to_string()
                        } else {
                            "✨ Libre (Non alloué)".to_string()
                        }
                    };

                                        let mut power_state = if is_rotational { "Actif / En rotation".to_string() } else { "Actif / En ligne".to_string() };
                    let mut smart_status = "Sain (PASS)".to_string();
                    let mut temperature_c = if is_rotational { 32.0 } else { 35.0 };

                    if let Ok(smart_out) = Command::new("smartctl")
                        .args(["-n", "standby", "-j", "-H", "-A", &path])
                        .output()
                    {
                        if smart_out.status.code() == Some(2) {
                            power_state = "Veille (Standby)".to_string();
                            smart_status = "Veille (Préservé)".to_string();
                        } else {
                            let smart_json_str = String::from_utf8_lossy(&smart_out.stdout);
                            if let Ok(sj) = serde_json::from_str::<serde_json::Value>(&smart_json_str) {
                                if let Some(passed) = sj.get("smart_status").and_then(|s| s.get("passed")).and_then(|p| p.as_bool()) {
                                    smart_status = if passed { "Sain (PASS)".to_string() } else { "Attention (ÉCHEC)".to_string() };
                                }
                                if let Some(t) = sj.get("temperature").and_then(|t| t.get("current")).and_then(|c| c.as_f64()) {
                                    temperature_c = t as f32;
                                }
                            }
                        }
                    }

                    if temperature_c <= 0.0 && path.contains("nvme") {
                        if let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") {
                            for e in entries.flatten() {
                                let hpath = e.path();
                                if let Ok(name) = std::fs::read_to_string(hpath.join("name")) {
                                    if name.trim().contains("nvme") {
                                        if let Ok(temp_str) = std::fs::read_to_string(hpath.join("temp1_input")) {
                                            if let Ok(val) = temp_str.trim().parse::<f32>() {
                                                temperature_c = (val / 1000.0).round();
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if temperature_c <= 0.0 {
                        if let Ok(zones) = std::fs::read_dir("/sys/class/thermal") {
                            for z in zones.flatten() {
                                let zpath = z.path();
                                if let Ok(typ) = std::fs::read_to_string(zpath.join("type")) {
                                    let t_low = typ.to_lowercase();
                                    if t_low.contains("drive") || t_low.contains("ssd") || t_low.contains("nvme") || t_low.contains("hdd") {
                                        if let Ok(t_str) = std::fs::read_to_string(zpath.join("temp")) {
                                            if let Ok(val) = t_str.trim().parse::<i32>() {
                                                temperature_c = val as f32;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    physical_disks.push(PhysicalDiskInfo {
                        name,
                        path,
                        bay_label,
                        size_human,
                        model,
                        serial,
                        disk_type,
                        is_rotational,
                        is_system: contains_system,
                        fstype,
                        role,
                        power_state,
                        smart_status,
                        temperature_c,
                        partitions,
                        size_bytes: total_size_bytes,
                        is_removable,
                    });
                }
            }
        }
    }

    physical_disks.sort_by(|a, b| {
        let order_a = if a.name.starts_with("sd") { 0 } else { 1 };
        let order_b = if b.name.starts_with("sd") { 0 } else { 1 };
        if order_a != order_b {
            order_a.cmp(&order_b)
        } else {
            a.name.cmp(&b.name)
        }
    });

    physical_disks
}


fn parse_mdstat_sync() -> Option<RaidSyncProgress> {
    if let Ok(content) = std::fs::read_to_string("/proc/mdstat") {
        let lines: Vec<&str> = content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if line.contains("resync") || line.contains("recovery") || line.contains("check") {
                // Remonter en arrière pour trouver la ligne du périphérique md (ex: "md127 : active...")
                let mut md_name = "md0";
                for prev in lines[..idx].iter().rev() {
                    let trimmed = prev.trim();
                    if trimmed.starts_with("md") && trimmed.contains(':') {
                        if let Some(name) = trimmed.split(':').next() {
                            md_name = name.trim();
                            break;
                        }
                    }
                }
                return parse_sync_line(md_name, line);
            }
        }
    }
    None
}

fn parse_sync_line(array_name: &str, line: &str) -> Option<RaidSyncProgress> {
    // Exemple : [=================>...]  resync = 88.8% (3471048704/3906886144) finish=145.3min speed=49992K/sec
    let action = if line.contains("recovery") {
        "Reconstruction".to_string()
    } else if line.contains("check") {
        "Vérification".to_string()
    } else {
        "Synchronisation initiale".to_string()
    };

    // Extraire le pourcentage en isolant le token numérique précédant le caractère '%'
    // Ne JAMAIS faire .split('=') car la barre de progression ASCII [=======>...] contient des '=' !
    let percent = line
        .split('%')
        .next()
        .and_then(|before_pct| before_pct.split_whitespace().last())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);

    let finish_minutes = line
        .split("finish=")
        .nth(1)
        .and_then(|f| f.split("min").next())
        .and_then(|s| s.trim().parse::<f32>().ok())
        .unwrap_or(0.0);

    let speed_kb = line
        .split("speed=")
        .nth(1)
        .and_then(|s| s.split("K/sec").next())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let speed_mb_s = (speed_kb as f32 / 1024.0 * 10.0).round() / 10.0;

    let finish_human = if finish_minutes >= 60.0 {
        let h = (finish_minutes / 60.0).floor() as u32;
        let m = (finish_minutes % 60.0).round() as u32;
        format!("{}h {}m", h, m)
    } else {
        format!("{}m", finish_minutes.round() as u32)
    };

    Some(RaidSyncProgress {
        array: array_name.to_string(),
        action,
        percent,
        speed_mb_s,
        finish_minutes,
        finish_human,
    })
}

// --------------------------------------------------------------------------
// FORMATAGE ET CRÉATION DE POOLS RAID
// --------------------------------------------------------------------------
pub fn is_system_device(dev: &str) -> bool {
    let clean = dev.trim_start_matches("/dev/");
    // Protection absolue du disque système
    if clean.starts_with("nvme1n1") {
        return true;
    }
    // Vérifier si une partition du disque est montée sur /, /boot, /nix
    if let Ok(output) = Command::new("findmnt")
        .args(["-n", "-o", "SOURCE,TARGET"])
        .output()
    {
        let str_out = String::from_utf8_lossy(&output.stdout);
        for line in str_out.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let src = parts[0];
                let target = parts[1];
                if target == "/" || target == "/boot" || target == "/nix" || target == "/nix/store" {
                    if src.contains(clean) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub fn format_disk(req: &FormatDiskRequest) -> Result<String, String> {
    if is_system_device(&req.device) {
        return Err("Interdiction absolue : Impossible de formater un disque contenant le système d'exploitation NixOS.".into());
    }

    let dev = req.device.trim();
    if !dev.starts_with("/dev/") {
        return Err("Chemin de périphérique invalide.".into());
    }

    // Démonter s'il est monté
    let _ = Command::new("umount").args(["-f", dev]).output();

    // Effacer les signatures existantes
    let _ = Command::new("wipefs").args(["-a", dev]).output();

    let default_label = "STORAGE";
    let label = req.label.as_deref().unwrap_or(default_label);

    let status = match req.fs_type.to_lowercase().as_str() {
        "btrfs" => Command::new("mkfs.btrfs")
            .args(["-f", "-L", label, dev])
            .output(),
        "ext4" => Command::new("mkfs.ext4")
            .args(["-F", "-L", label, dev])
            .output(),
        "xfs" => Command::new("mkfs.xfs")
            .args(["-f", "-L", label, dev])
            .output(),
        "vfat" => Command::new("mkfs.vfat")
            .args(["-F", "32", "-n", label, dev])
            .output(),
        other => return Err(format!("Système de fichiers non supporté : {}", other)),
    };

    match status {
        Ok(out) if out.status.success() => {
            Ok(format!("Le périphérique {} a été formaté avec succès en {} (Label: {}).", dev, req.fs_type.to_uppercase(), label))
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("Erreur lors du formatage : {}", err))
        }
        Err(e) => Err(format!("Échec d'exécution de la commande de formatage : {}", e)),
    }
}

static ACTIVE_STORAGE_JOB: std::sync::Mutex<Option<StorageJob>> = std::sync::Mutex::new(None);

pub fn get_storage_jobs_file_path() -> PathBuf {
    let noos_p = PathBuf::from("/var/lib/noos/storage_jobs.json");
    if noos_p.exists() {
        return noos_p;
    }
    let steve_p = PathBuf::from("/var/lib/steveos/storage_jobs.json");
    if steve_p.exists() {
        return steve_p;
    }
    let p = PathBuf::from("/var/lib/noos/storage_jobs.json");
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    p
}

pub fn load_persisted_storage_job() -> Option<StorageJob> {
    let candidate_paths = [
        PathBuf::from("/var/lib/noos/storage_jobs.json"),
        PathBuf::from("/var/lib/steveos/storage_jobs.json"),
        PathBuf::from("/tmp/noos_storage_jobs.json"),
        PathBuf::from("/tmp/steveos_storage_jobs.json"),
    ];

    for p in &candidate_paths {
        if p.exists() {
            if let Ok(c) = std::fs::read_to_string(p) {
                if let Ok(job) = serde_json::from_str::<StorageJob>(&c) {
                    return Some(job);
                }
            }
        }
    }
    None
}

pub fn save_persisted_storage_job(job: &StorageJob) {
    if let Ok(json) = serde_json::to_string_pretty(job) {
        let _ = std::fs::create_dir_all("/var/lib/noos");
        let _ = std::fs::create_dir_all("/var/lib/steveos");
        let _ = std::fs::write("/var/lib/noos/storage_jobs.json", &json);
        let _ = std::fs::write("/var/lib/steveos/storage_jobs.json", &json);
        let _ = std::fs::write("/tmp/noos_storage_jobs.json", &json);
        let _ = std::fs::write("/tmp/steveos_storage_jobs.json", json);
    }
}

pub fn get_active_storage_job() -> Option<StorageJob> {
    if let Ok(guard) = ACTIVE_STORAGE_JOB.lock() {
        if let Some(ref j) = *guard {
            return Some(j.clone());
        }
    }
    let loaded = load_persisted_storage_job();
    if let Some(ref j) = loaded {
        if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
            *guard = Some(j.clone());
        }
    }
    loaded
}

pub fn dismiss_storage_job() {
    if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
        *guard = None;
    }
    let _ = std::fs::remove_file("/var/lib/noos/storage_jobs.json");
    let _ = std::fs::remove_file("/var/lib/steveos/storage_jobs.json");
    let _ = std::fs::remove_file("/tmp/noos_storage_jobs.json");
    let _ = std::fs::remove_file("/tmp/steveos_storage_jobs.json");
}

fn update_job_step(
    job_id: &str,
    step: usize,
    total: usize,
    percent: u8,
    name: &str,
    detail: &str,
) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
        if let Some(ref mut j) = *guard {
            if j.id == job_id {
                j.current_step = step;
                j.total_steps = total;
                j.progress_percent = percent;
                j.step_name = name.to_string();
                j.step_detail = detail.to_string();
                j.updated_at = now;
                save_persisted_storage_job(j);
            }
        }
    }
}

fn complete_job(job_id: &str, result_message: &str) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
        if let Some(ref mut j) = *guard {
            if j.id == job_id {
                j.status = StorageJobStatus::Completed;
                j.progress_percent = 100;
                j.completed_at = Some(now);
                j.updated_at = now;
                j.step_name = "Opération terminée avec succès".to_string();
                j.step_detail = result_message.to_string();
                j.result_message = Some(result_message.to_string());
                save_persisted_storage_job(j);
            }
        }
    }
}

fn fail_job(job_id: &str, error_message: &str) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
        if let Some(ref mut j) = *guard {
            if j.id == job_id {
                j.status = StorageJobStatus::Failed;
                j.completed_at = Some(now);
                j.updated_at = now;
                j.step_name = "Échec de l'opération".to_string();
                j.step_detail = error_message.to_string();
                j.error = Some(error_message.to_string());
                save_persisted_storage_job(j);
            }
        }
    }
}

pub fn start_destroy_raid_job(req: DestroyRaidRequest) -> Result<StorageJob, String> {
    let clean_name = req.name.trim().to_string();
    let clean_dev = req.device.trim().to_string();

    if clean_name.is_empty() && clean_dev.is_empty() {
        return Err("Identifiant ou chemin de la grappe RAID manquant.".into());
    }

    if is_system_device(&clean_dev) || is_system_device(&clean_name) {
        return Err("Interdiction absolue : Impossible de détruire une grappe ou un volume contenant le système NixOS.".into());
    }

    if let Some(existing) = get_active_storage_job() {
        if existing.status == StorageJobStatus::Running || existing.status == StorageJobStatus::Resumed {
            return Err(format!("Une opération de stockage est déjà en cours d'exécution : {} ({})", existing.step_name, existing.target_name));
        }
    }

    let wipe = req.wipe_members.unwrap_or(true);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let job_id = format!("job-destroy-{}", now);

    let mut members = Vec::new();
    if clean_dev.starts_with("/dev/md") || clean_name.starts_with("md") {
        let md_target = if clean_dev.starts_with("/dev/md") { &clean_dev } else { &format!("/dev/{}", clean_name) };
        if let Ok(detail_out) = Command::new("mdadm").args(["--detail", md_target]).output() {
            let str_out = String::from_utf8_lossy(&detail_out.stdout);
            for line in str_out.lines() {
                let trimmed = line.trim();
                if trimmed.contains("/dev/") {
                    for token in trimmed.split_whitespace() {
                        if token.starts_with("/dev/") && !token.starts_with("/dev/md") {
                            members.push(token.to_string());
                        }
                    }
                }
            }
        }
    } else {
        let stripped = clean_dev.trim_start_matches("/dev/");
        let vg_cand = if clean_dev.starts_with("/dev/mapper/") {
            clean_dev.trim_start_matches("/dev/mapper/").split('-').next().unwrap_or("").to_string()
        } else if stripped.contains('/') {
            stripped.split('/').next().unwrap_or("").to_string()
        } else {
            clean_name.clone()
        };
        if !vg_cand.is_empty() {
            if let Ok(pv_out) = Command::new("pvs").args(["--noheadings", "-o", "pv_name,vg_name"]).output() {
                let str_out = String::from_utf8_lossy(&pv_out.stdout);
                for line in str_out.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 && parts[1] == vg_cand {
                        members.push(parts[0].to_string());
                    }
                }
            }
        }
    }

    let job = StorageJob {
        id: job_id.clone(),
        job_type: StorageJobType::DestroyRaid,
        status: StorageJobStatus::Running,
        current_step: 1,
        total_steps: 6,
        step_name: "Initialisation et démontage".to_string(),
        step_detail: format!("Préparation de la dissolution de la grappe '{}'...", clean_name),
        progress_percent: 5,
        target_name: clean_name.clone(),
        target_device: clean_dev.clone(),
        member_devices: members,
        destroy_params: Some(DestroyRaidParams {
            name: clean_name,
            device: clean_dev,
            wipe_members: wipe,
        }),
        create_params: None,
        started_at: now,
        updated_at: now,
        completed_at: None,
        error: None,
        result_message: None,
    };

    save_persisted_storage_job(&job);
    if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
        *guard = Some(job.clone());
    }

    let job_clone = job.clone();
    std::thread::spawn(move || {
        run_destroy_raid_worker(job_clone);
    });

    Ok(job)
}

pub fn start_create_raid_job(req: CreateRaidRequest) -> Result<StorageJob, String> {
    for dev in &req.devices {
        if is_system_device(dev) {
            return Err(format!("Interdiction : Le périphérique {} fait partie du système NixOS.", dev));
        }
    }

    if req.devices.len() < 2 && req.level != "linear" {
        return Err("Une grappe RAID nécessite au moins 2 disques.".into());
    }

    if let Some(existing) = get_active_storage_job() {
        if existing.status == StorageJobStatus::Running || existing.status == StorageJobStatus::Resumed {
            return Err(format!("Une opération de stockage est déjà en cours d'exécution : {} ({})", existing.step_name, existing.target_name));
        }
    }

    let clean_name = req.name.trim().to_lowercase().replace(' ', "-");
    let md_device = format!("/dev/md/{}", clean_name);
    let mountpoint = if req.mountpoint.is_empty() {
        format!("/mnt/{}", clean_name)
    } else {
        req.mountpoint.clone()
    };

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let job_id = format!("job-create-{}", now);

    let job = StorageJob {
        id: job_id.clone(),
        job_type: StorageJobType::CreateRaid,
        status: StorageJobStatus::Running,
        current_step: 1,
        total_steps: 5,
        step_name: "Préparation des disques".to_string(),
        step_detail: format!("Nettoyage préalable de {} disque(s) pour la grappe '{}'...", req.devices.len(), clean_name),
        progress_percent: 5,
        target_name: clean_name.clone(),
        target_device: md_device.clone(),
        member_devices: req.devices.clone(),
        destroy_params: None,
        create_params: Some(CreateRaidParams {
            name: clean_name,
            level: req.level,
            devices: req.devices,
            fs_type: req.fs_type,
            mountpoint,
        }),
        started_at: now,
        updated_at: now,
        completed_at: None,
        error: None,
        result_message: None,
    };

    save_persisted_storage_job(&job);
    if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
        *guard = Some(job.clone());
    }

    let job_clone = job.clone();
    std::thread::spawn(move || {
        run_create_raid_worker(job_clone);
    });

    Ok(job)
}

fn run_destroy_raid_worker(job: StorageJob) {
    let job_id = &job.id;
    let params = match job.destroy_params {
        Some(ref p) => p,
        None => {
            fail_job(job_id, "Paramètres de destruction manquants.");
            return;
        }
    };

    let clean_name = &params.name;
    let clean_dev = &params.device;
    let do_wipe = params.wipe_members;

    if is_system_device(clean_dev) || is_system_device(clean_name) {
        fail_job(job_id, "Interdiction : Le volume ciblé contient le système NixOS.");
        return;
    }

    // Étape 1 : Démontage forcé propre
    if job.current_step <= 1 {
        update_job_step(job_id, 1, 6, 15, "Démontage des partitions", &format!("Démontage automatique de '{}'...", clean_name));
        if let Ok(output) = Command::new("findmnt").args(["-n", "-o", "SOURCE,TARGET"]).output() {
            let str_out = String::from_utf8_lossy(&output.stdout);
            for line in str_out.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let src = parts[0];
                    let mnt = parts[1];
                    if mnt != "/" && mnt != "/boot" && !mnt.starts_with("/nix") {
                        let matches_dev = src == clean_dev || (!clean_dev.is_empty() && src.contains(clean_dev.trim_start_matches("/dev/")));
                        let matches_name = !clean_name.is_empty() && (src.contains(clean_name) || mnt.ends_with(clean_name));
                        if matches_dev || matches_name {
                            let _ = Command::new("umount").args(["-f", mnt]).output();
                        }
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Étape 2 : Nettoyage déclaratif mounts.json
    if job.current_step <= 2 {
        update_job_step(job_id, 2, 6, 30, "Nettoyage déclaratif", "Suppression des entrées déclaratives dans mounts.json...");
        let mut mounts = load_persisted_mounts();
        let init_len = mounts.len();
        mounts.retain(|m| {
            m.device != *clean_dev
                && !m.name.contains(clean_name)
                && (!clean_dev.is_empty() && !m.device.contains(clean_dev.trim_start_matches("/dev/")))
        });
        if mounts.len() != init_len {
            let _ = save_persisted_mounts(&mounts);
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    // Détermination de la technologie (LVM2, mdadm, ou générique)
    let stripped = clean_dev.trim_start_matches("/dev/");
    let (vg_cand, lv_cand) = if clean_dev.starts_with("/dev/mapper/") {
        let mapper_name = clean_dev.trim_start_matches("/dev/mapper/");
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut chars = mapper_name.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '-' {
                if chars.peek() == Some(&'-') {
                    chars.next();
                    current.push('-');
                } else {
                    parts.push(current);
                    current = String::new();
                }
            } else {
                current.push(c);
            }
        }
        parts.push(current);
        if parts.len() == 2 {
            (parts[0].clone(), Some(parts[1].clone()))
        } else {
            (mapper_name.to_string(), None)
        }
    } else if stripped.contains('/') && !stripped.starts_with("disk/") {
        let parts: Vec<&str> = stripped.split('/').collect();
        (parts[0].to_string(), if parts.len() > 1 { Some(parts[1].to_string()) } else { None })
    } else if clean_name.contains('/') {
        let parts: Vec<&str> = clean_name.split('/').collect();
        (parts[0].to_string(), if parts.len() > 1 { Some(parts[1].to_string()) } else { None })
    } else {
        (if clean_name.is_empty() { stripped.to_string() } else { clean_name.to_string() }, None)
    };

    let is_vg = Command::new("vgs").args([&vg_cand]).output().map(|o| o.status.success()).unwrap_or(false);
    let mut members_to_wipe = job.member_devices.clone();

    // Cas A : LVM2
    if is_vg {
        let vg_name = &vg_cand;

        if members_to_wipe.is_empty() {
            if let Ok(pv_out) = Command::new("pvs").args(["--noheadings", "-o", "pv_name,vg_name"]).output() {
                let str_out = String::from_utf8_lossy(&pv_out.stdout);
                for line in str_out.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 && parts[1] == vg_name {
                        members_to_wipe.push(parts[0].to_string());
                    }
                }
            }
        }

        // Étape 3 : Suppression logique LVM
        if job.current_step <= 3 {
            update_job_step(job_id, 3, 6, 55, "Suppression logique LVM", &format!("Désactivation et suppression des volumes LVM de '{}'...", vg_name));
            if let Some(ref lv) = lv_cand {
                let _ = Command::new("lvchange").args(["-an", "-f", &format!("{}/{}", vg_name, lv)]).output();
                let _ = Command::new("lvremove").args(["-y", "-f", &format!("{}/{}", vg_name, lv)]).output();
            }

            let remaining_lvs = Command::new("lvs").args(["-o", "lv_name", "--noheadings", vg_name]).output();
            let has_other_lvs = remaining_lvs.map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                s.lines().any(|l| !l.trim().is_empty())
            }).unwrap_or(false);

            if !has_other_lvs || lv_cand.is_none() {
                let _ = Command::new("vgchange").args(["-an", "-f", vg_name]).output();
                let _ = Command::new("vgremove").args(["-y", "-f", vg_name]).output();
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        // Étape 4 : Suppression des Physical Volumes LVM
        if job.current_step <= 4 {
            update_job_step(job_id, 4, 6, 75, "Suppression des Physical Volumes", "Libération des PVs membres LVM2...");
            for pv in &members_to_wipe {
                if !is_system_device(pv) {
                    let _ = Command::new("pvremove").args(["-y", "-ff", pv]).output();
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
        }

        // Étape 5 : Effacement signatures
        if job.current_step <= 5 {
            update_job_step(job_id, 5, 6, 90, "Effacement des signatures", "Nettoyage des superblocks et tables de partition (wipefs)...");
            if do_wipe {
                for pv in &members_to_wipe {
                    if !is_system_device(pv) {
                        let _ = Command::new("wipefs").args(["-a", "-f", pv]).output();
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        // Étape 6 : Settle
        update_job_step(job_id, 6, 6, 98, "Synchronisation du système", "Actualisation des nœuds de périphériques et udev...");
        let _ = Command::new("vgmknodes").output();
        let _ = Command::new("udevadm").args(["settle", "--timeout=3"]).output();

        complete_job(job_id, &format!("Grappe LVM2 '{}' dissoute avec succès. {} disque(s) libéré(s) : {}.", vg_name, members_to_wipe.len(), members_to_wipe.join(", ")));
        return;
    }

    // Cas B : mdadm
    if clean_dev.starts_with("/dev/md") || clean_name.starts_with("md") {
        let md_target = if clean_dev.starts_with("/dev/md") { clean_dev } else { &format!("/dev/{}", clean_name) };

        if members_to_wipe.is_empty() {
            if let Ok(detail_out) = Command::new("mdadm").args(["--detail", md_target]).output() {
                let str_out = String::from_utf8_lossy(&detail_out.stdout);
                for line in str_out.lines() {
                    let trimmed = line.trim();
                    if trimmed.contains("/dev/") {
                        for token in trimmed.split_whitespace() {
                            if token.starts_with("/dev/") && !token.starts_with("/dev/md") {
                                members_to_wipe.push(token.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Étape 3 : Arrêt mdadm
        if job.current_step <= 3 {
            update_job_step(job_id, 3, 6, 55, "Arrêt de la grappe logicielle", &format!("Arrêt de la grappe logicielle mdadm '{}'...", md_target));
            let _ = Command::new("mdadm").args(["--stop", md_target]).output();
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        // Étape 4 : Suppression superblocks mdadm
        if job.current_step <= 4 {
            update_job_step(job_id, 4, 6, 75, "Suppression des superblocks", "Effacement des superblocks mdadm sur chaque membre...");
            for m in &members_to_wipe {
                if !is_system_device(m) {
                    let _ = Command::new("mdadm").args(["--zero-superblock", "--force", m]).output();
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
        }

        // Étape 5 : Wipefs
        if job.current_step <= 5 {
            update_job_step(job_id, 5, 6, 90, "Effacement des signatures", "Nettoyage des signatures de partitions (wipefs)...");
            if do_wipe {
                for m in &members_to_wipe {
                    if !is_system_device(m) {
                        let _ = Command::new("wipefs").args(["-a", "-f", m]).output();
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        // Étape 6 : Settle
        update_job_step(job_id, 6, 6, 98, "Synchronisation du système", "Actualisation des périphériques blocs udev...");
        let _ = Command::new("udevadm").args(["settle", "--timeout=3"]).output();

        complete_job(job_id, &format!("Grappe RAID mdadm '{}' dissoute avec succès. Disques libérés : {}.", md_target, members_to_wipe.join(", ")));
        return;
    }

    // Cas C : Périphérique bloc générique ou Btrfs multi-disques
    if Path::new(clean_dev).exists() {
        if do_wipe && !is_system_device(clean_dev) {
            update_job_step(job_id, 5, 6, 90, "Effacement des signatures", "Nettoyage du périphérique bloc...");
            let _ = Command::new("wipefs").args(["-a", "-f", clean_dev]).output();
            let _ = Command::new("udevadm").args(["settle", "--timeout=2"]).output();
            complete_job(job_id, &format!("Périphérique '{}' réinitialisé et libéré avec succès.", clean_dev));
            return;
        }
    }

    fail_job(job_id, &format!("Impossible d'identifier la grappe RAID '{}' ({}) pour dissolution.", clean_name, clean_dev));
}

fn run_create_raid_worker(job: StorageJob) {
    let job_id = &job.id;
    let params = match job.create_params {
        Some(ref p) => p,
        None => {
            fail_job(job_id, "Paramètres de création manquants.");
            return;
        }
    };

    let clean_name = &params.name;
    let md_device = format!("/dev/md/{}", clean_name);
    let mountpoint = &params.mountpoint;
    let fs_type = params.fs_type.to_lowercase();
    let raid_level = match params.level.to_lowercase().as_str() {
        "raid0" | "0" => "0",
        "raid1" | "1" => "1",
        "raid5" | "5" => "5",
        "raid6" | "6" => "6",
        "raid10" | "10" => "10",
        "linear" => "linear",
        _ => {
            fail_job(job_id, "Niveau de RAID invalide.");
            return;
        }
    };

    // Étape 1 : Nettoyage préalable des disques sélectionnés
    if job.current_step <= 1 {
        update_job_step(job_id, 1, 5, 15, "Nettoyage des disques", "Démontage et suppression des anciennes signatures...");
        for dev in &params.devices {
            let _ = Command::new("umount").args(["-f", dev]).output();
            let _ = Command::new("wipefs").args(["-a", dev]).output();
            let _ = Command::new("mdadm").args(["--zero-superblock", "--force", dev]).output();
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Étape 2 : Assemblage de la grappe logicielle mdadm
    if job.current_step <= 2 {
        update_job_step(job_id, 2, 5, 40, "Assemblage de la grappe", &format!("Création de la grappe RAID{} ({}) avec mdadm...", raid_level, clean_name));
        let _ = Command::new("modprobe").args(["raid0", "raid1", "raid456", "raid10"]).output();

        let dev_count = params.devices.len().to_string();
        let mut args = vec![
            "--create".to_string(),
            md_device.clone(),
            format!("--level={}", raid_level),
            format!("--raid-devices={}", dev_count),
        ];
        for d in &params.devices {
            args.push(d.clone());
        }
        args.push("--run".to_string());

        let create_out = Command::new("mdadm").args(&args).output();
        match create_out {
            Ok(ref o) if !o.status.success() => {
                let err = String::from_utf8_lossy(&o.stderr);
                if !err.contains("already") && !Path::new(&md_device).exists() {
                    fail_job(job_id, &format!("Échec de création du RAID mdadm : {}", err));
                    return;
                }
            }
            Err(e) => {
                fail_job(job_id, &format!("Impossible d'exécuter mdadm : {}", e));
                return;
            }
            _ => {}
        }

        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = Command::new("udevadm").args(["settle", "--timeout=3"]).output();
    }

    // Étape 3 : Formatage du système de fichiers
    if job.current_step <= 3 {
        update_job_step(job_id, 3, 5, 65, "Formatage du volume", &format!("Formatage du volume {} en {}...", md_device, fs_type.to_uppercase()));
        let label = clean_name.to_uppercase();
        let fmt_status = if fs_type == "btrfs" {
            Command::new("mkfs.btrfs").args(["-f", "-L", &label, &md_device]).output()
        } else if fs_type == "xfs" {
            Command::new("mkfs.xfs").args(["-f", "-L", &label, &md_device]).output()
        } else {
            Command::new("mkfs.ext4").args(["-F", "-L", &label, &md_device]).output()
        };

        if let Ok(out) = fmt_status {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                fail_job(job_id, &format!("Le RAID a été créé mais le formatage a échoué : {}", err));
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Étape 4 : Point de montage et permissions
    if job.current_step <= 4 {
        update_job_step(job_id, 4, 5, 85, "Montage et permissions", &format!("Montage sur {} et configuration des droits...", mountpoint));
        let _ = Command::new("mkdir").args(["-p", mountpoint]).output();
        let _ = Command::new("mount").args([&md_device, mountpoint]).output();

        let primary_user = target_user();
        let _ = Command::new("chown").args(["-R", &format!("{}:storage", primary_user), mountpoint]).output();
        let _ = Command::new("chmod").args(["2775", mountpoint]).output();
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    // Étape 5 : Persistance déclarative mounts.json
    update_job_step(job_id, 5, 5, 98, "Persistance déclarative", "Enregistrement dans mounts.json et synchronisation...");
    let mut opts = vec!["defaults".to_string(), "noatime".to_string(), "nofail".to_string()];
    if fs_type == "btrfs" {
        opts.push("compress=zstd".to_string());
    }

    let _ = Command::new("udevadm").args(["settle", "--timeout=3"]).output();
    let uuid = Command::new("blkid")
        .args(["-s", "UUID", "-o", "value", &md_device])
        .output()
        .ok()
        .and_then(|o| {
            let u = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if u.is_empty() { None } else { Some(u) }
        });
    let effective_device = if let Some(ref u) = uuid {
        format!("/dev/disk/by-uuid/{}", u)
    } else {
        md_device.clone()
    };

    let mut mounts = load_persisted_mounts();
    mounts.retain(|m| m.mount_point != *mountpoint && m.device != md_device && m.device != effective_device);
    let mount_id = format!("mount-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());

    mounts.push(PersistedMount {
        id: mount_id,
        name: clean_name.clone(),
        device: effective_device,
        device_uuid: uuid,
        mount_point: mountpoint.clone(),
        fs_type: fs_type.clone(),
        options: opts,
        enabled: true,
        created_at: Some("Aujourd'hui".to_string()),
    });
    let _ = save_persisted_mounts(&mounts);

    complete_job(job_id, &format!(
        "Pool RAID {} ({}) créé avec succès avec {} disque(s), formaté en {} et monté sur {} !",
        clean_name,
        params.level.to_uppercase(),
        params.devices.len(),
        fs_type.to_uppercase(),
        mountpoint
    ));
}

pub fn init_storage_tasks_tracker() {
    if let Some(mut job) = load_persisted_storage_job() {
        if job.status == StorageJobStatus::Running || job.status == StorageJobStatus::Pending || job.status == StorageJobStatus::Resumed {
            eprintln!("[STORAGE] Tâche de stockage interrompue détectée au démarrage : {} (type: {:?}, étape: {}/{})", job.id, job.job_type, job.current_step, job.total_steps);
            job.status = StorageJobStatus::Resumed;
            job.step_detail = format!("[Reprise suite au redémarrage] Reprise à l'étape {}/{} : {}", job.current_step, job.total_steps, job.step_name);
            job.updated_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            save_persisted_storage_job(&job);

            if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
                *guard = Some(job.clone());
            }

            let job_clone = job.clone();
            std::thread::spawn(move || {
                match job_clone.job_type {
                    StorageJobType::DestroyRaid => run_destroy_raid_worker(job_clone),
                    StorageJobType::CreateRaid => run_create_raid_worker(job_clone),
                }
            });
        } else if job.status == StorageJobStatus::Completed {
            if let Some(completed_at) = job.completed_at {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                if now.saturating_sub(completed_at) > 30 {
                    dismiss_storage_job();
                } else if let Ok(mut guard) = ACTIVE_STORAGE_JOB.lock() {
                    *guard = Some(job);
                }
            }
        }
    }
}

#[allow(dead_code)]
pub fn create_raid(req: &CreateRaidRequest) -> Result<String, String> {
    let job = start_create_raid_job(req.clone())?;
    Ok(format!("Tâche de création de grappe lancée avec succès (ID: {}).", job.id))
}

#[allow(dead_code)]
pub fn destroy_raid(req: &DestroyRaidRequest) -> Result<String, String> {
    let job = start_destroy_raid_job(req.clone())?;
    Ok(format!("Tâche de dissolution de grappe lancée avec succès (ID: {}).", job.id))
}

pub fn trigger_disk_spindown(disk_name: &str) -> Result<String, String> {
    let clean_name = disk_name.trim_start_matches("/dev/");
    let path = format!("/dev/{}", clean_name);

    let output = Command::new("hdparm")
        .args(["-y", &path])
        .output()
        .map_err(|e| format!("Impossible d'exécuter hdparm : {}", e))?;

    if output.status.success() {
        Ok(format!("Le disque {} a été placé en mode veille (standby).", path))
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Erreur lors de la mise en veille : {}", err))
    }
}


pub fn target_user() -> String {
    if let Ok(u) = std::env::var("NOOS_USER").or_else(|_| std::env::var("STEVEOS_USER")) {
        let trimmed = u.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 7 {
                if let Ok(uid) = parts[2].parse::<u32>() {
                    if uid >= 1000 && uid < 65534 {
                        let shell = parts[6];
                        if !shell.ends_with("nologin") && !shell.ends_with("false") {
                            return parts[0].to_string();
                        }
                    }
                }
            }
        }
    }
    crate::updates::target_user()
}

fn resolve_uid_name(uid: u32) -> String {
    if uid == 0 {
        return "root".to_string();
    }
    if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 && parts[2] == uid.to_string() {
                return parts[0].to_string();
            }
        }
    }
    uid.to_string()
}

fn resolve_gid_name(gid: u32) -> String {
    if gid == 0 {
        return "root".to_string();
    }
    if let Ok(group) = std::fs::read_to_string("/etc/group") {
        for line in group.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 && parts[2] == gid.to_string() {
                return parts[0].to_string();
            }
        }
    }
    gid.to_string()
}

fn get_user_uid_gid(username: &str) -> (u32, u32) {
    if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 && parts[0] == username {
                let uid = parts[2].parse::<u32>().unwrap_or(1000);
                let gid = parts[3].parse::<u32>().unwrap_or(100);
                return (uid, gid);
            }
        }
    }
    (1000, 100)
}

fn get_storage_gid() -> Option<u32> {
    if let Ok(group) = std::fs::read_to_string("/etc/group") {
        for line in group.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 && parts[0] == "storage" {
                if let Ok(gid) = parts[2].parse::<u32>() {
                    return Some(gid);
                }
            }
        }
    }
    None
}

pub fn check_path_permissions(path: &str, target_user: &str) -> (String, String, String, bool, bool) {
    use std::os::unix::fs::MetadataExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let uid = meta.uid();
        let gid = meta.gid();
        let mode = meta.mode() & 0o7777;
        let mode_str = format!("{:04o}", mode);

        // Récupérer le nom utilisateur et groupe
        let user_name = resolve_uid_name(uid);
        let group_name = resolve_gid_name(gid);

        let writable = Command::new("runuser")
            .args(["-u", target_user, "--", "test", "-w", path])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let is_sys = path == "/" || path == "/boot" || path.starts_with("/nix");
        let needs_repair = !writable && !is_sys;

        (user_name, group_name, mode_str, writable, needs_repair)
    } else {
        ("N/A".into(), "N/A".into(), "0000".into(), false, false)
    }
}

pub fn verify_user_password(username: &str, password: &str) -> Result<(), String> {
    if crate::auth::verify_linux_credentials(username, password)? {
        Ok(())
    } else {
        Err("Mot de passe incorrect ou utilisateur invalide.".into())
    }
}

pub fn repair_path_permissions(req: &RepairPermissionsRequest) -> Result<String, String> {
    let path = req.path.trim();
    if path.is_empty() || !path.starts_with('/') {
        return Err("Chemin de volume invalide.".into());
    }

    if path == "/" || path == "/boot" || path.starts_with("/nix") || path == "/etc" || path == "/usr" || path == "/bin" {
        return Err("Interdiction : Impossible de modifier les permissions des répertoires système.".into());
    }

    if !std::path::Path::new(path).exists() {
        return Err(format!("Le chemin '{}' n'existe pas ou n'est pas monté.", path));
    }

    let default_user = target_user();
    let target_user = req.target_user.as_deref().unwrap_or(&default_user);
    let target_group = req.target_group.as_deref().unwrap_or("storage");

    // Vérification du mot de passe si fourni
    if let Some(pwd) = &req.password {
        if !pwd.trim().is_empty() {
            verify_user_password(target_user, pwd.trim())?;
        }
    }

    // 1. Chown récursif
    let owner_str = format!("{}:{}", target_user, target_group);
    let chown_out = Command::new("chown")
        .args(["-R", &owner_str, path])
        .output()
        .map_err(|e| format!("Impossible d'exécuter chown : {}", e))?;

    if !chown_out.status.success() {
        let err = String::from_utf8_lossy(&chown_out.stderr);
        return Err(format!("Échec de modification du propriétaire ({}) : {}", owner_str, err));
    }

    // 2. Chmod 2775 (setgid) sur la racine
    let _ = Command::new("chmod").args(["2775", path]).output();

    if req.recursive.unwrap_or(true) {
        let _ = Command::new("find").args([path, "-type", "d", "-exec", "chmod", "2775", "{}", "+"]).output();
        let _ = Command::new("find").args([path, "-type", "f", "-exec", "chmod", "664", "{}", "+"]).output();
    }

    // 3. Test d'écriture en conditions réelles
    let test_file = format!("{}/.noos_perm_test_{}", path, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
    let write_test = Command::new("runuser")
        .args(["-u", target_user, "--", "touch", &test_file])
        .output();

    match write_test {
        Ok(out) if out.status.success() => {
            let _ = Command::new("runuser")
                .args(["-u", target_user, "--", "rm", "-f", &test_file])
                .output();
            Ok(format!(
                "Permissions réparées avec succès ! Le volume '{}' appartient à {} (mode 2775 / rwxrwsr-x). Test d'écriture pour '{}' validé avec succès.",
                path, owner_str, target_user
            ))
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!(
                "Les permissions ont été appliquées mais le test d'écriture pour l'utilisateur '{}' a échoué : {}",
                target_user, err
            ))
        }
        Err(e) => Err(format!("Échec de validation du test d'écriture : {}", e)),
    }
}

fn resolve_and_activate_block_device(clean_dev: &str, req: &MountVolumeRequest) -> Result<String, String> {
    // 0. Si le périphérique existe déjà directement comme nœud de bloc valide
    if Path::new(clean_dev).exists() {
        return Ok(clean_dev.to_string());
    }

    // Charger systématiquement les modules noyau RAID & Device Mapper indispensables
    let _ = Command::new("modprobe")
        .args(["dm-mod", "dm-raid", "raid0", "raid1", "raid456", "raid10"])
        .output();

    let stripped = clean_dev.trim_start_matches("/dev/");

    // Identifier si la cible fait référence à du LVM (ex: "vg1", "/dev/vg1", "vg1/storage", "/dev/vg1/storage", "/dev/mapper/vg1-storage")
    let (vg_candidate, lv_candidate) = if clean_dev.starts_with("/dev/mapper/") {
        let mapper_name = clean_dev.trim_start_matches("/dev/mapper/");
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut chars = mapper_name.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '-' {
                if chars.peek() == Some(&'-') {
                    chars.next();
                    current.push('-');
                } else {
                    parts.push(current);
                    current = String::new();
                }
            } else {
                current.push(c);
            }
        }
        parts.push(current);
        if parts.len() == 2 {
            (parts[0].clone(), Some(parts[1].clone()))
        } else {
            (mapper_name.to_string(), None)
        }
    } else if stripped.contains('/') && !stripped.starts_with("disk/") {
        let parts: Vec<&str> = stripped.split('/').collect();
        (parts[0].to_string(), if parts.len() > 1 { Some(parts[1].to_string()) } else { None })
    } else {
        (stripped.to_string(), None)
    };

    // Vérifier si vg_candidate est un Volume Group LVM valide
    let is_vg = Command::new("vgs")
        .args([&vg_candidate])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if is_vg {
        let vg_name = &vg_candidate;
        // Activation préalable globale du Volume Group
        let _ = Command::new("vgchange")
            .args(["-ay", "-K", "--activationmode", "degraded", vg_name])
            .output();

        let chosen_lv = if let Some(ref lv) = lv_candidate {
            // Un LV précis a été demandé dans le chemin (ex: "/dev/vg1/storage")
            let lv_exists = Command::new("lvs")
                .args(["-o", "lv_name", "--noheadings", &format!("{}/{}", vg_name, lv)])
                .output()
                .map(|o| o.status.success() && !o.stdout.is_empty())
                .unwrap_or(false);

            if lv_exists {
                // Activer spécifiquement le volume logique
                let act_res = Command::new("lvchange")
                    .args(["-ay", "-K", "--activationmode", "degraded", &format!("{}/{}", vg_name, lv)])
                    .output();
                if let Ok(ref out) = act_res {
                    if !out.status.success() {
                        let err = String::from_utf8_lossy(&out.stderr);
                        eprintln!("[storage] Avertissement lvchange: {}", err);
                    }
                }
                lv.clone()
            } else if req.force_format == Some(true) {
                // Le LV n'existe pas encore mais un formatage/création a été demandé
                let raid_type = req.raid_type.as_deref().unwrap_or("raid5");
                let create_args = if raid_type == "raid5" {
                    vec!["--type", "raid5", "-l", "100%FREE", "-n", lv.as_str(), vg_name]
                } else if raid_type == "linear" {
                    vec!["-l", "100%FREE", "-n", lv.as_str(), vg_name]
                } else {
                    vec!["--type", raid_type, "-l", "100%FREE", "-n", lv.as_str(), vg_name]
                };

                let lv_create_out = Command::new("lvcreate")
                    .args(&create_args)
                    .output()
                    .map_err(|e| format!("Impossible d'exécuter lvcreate : {}", e))?;

                if !lv_create_out.status.success() {
                    let err = String::from_utf8_lossy(&lv_create_out.stderr);
                    return Err(format!("Échec de création du volume logique '{}/{}' : {}", vg_name, lv, err));
                }

                let _ = Command::new("vgchange")
                    .args(["-ay", "-K", "--activationmode", "degraded", vg_name])
                    .output();
                lv.clone()
            } else {
                return Err(format!(
                    "Le volume logique '{}/{}' n'existe pas dans le groupe '{}'. Cochez l'option de formatage pour l'allouer et l'initialiser.",
                    vg_name, lv, vg_name
                ));
            }
        } else {
            // Aucun LV dans le chemin (ex: "/dev/vg1") : rechercher le premier LV existant
            let lv_out = Command::new("lvs")
                .args(["-o", "lv_name", "--noheadings", vg_name])
                .output()
                .map_err(|e| format!("Erreur lvs : {}", e))?;
            let lvs_str = String::from_utf8_lossy(&lv_out.stdout);
            let first_lv = lvs_str.lines().map(|l| l.trim()).find(|l| !l.is_empty()).map(|s| s.to_string());

            if let Some(lv) = first_lv {
                let _ = Command::new("lvchange")
                    .args(["-ay", "-K", "--activationmode", "degraded", &format!("{}/{}", vg_name, lv)])
                    .output();
                lv
            } else {
                let chosen_lv_name = req.lv_name.as_deref().unwrap_or("storage");
                let raid_type = req.raid_type.as_deref().unwrap_or("raid5");

                let create_args = if raid_type == "raid5" {
                    vec!["--type", "raid5", "-l", "100%FREE", "-n", chosen_lv_name, vg_name]
                } else if raid_type == "linear" {
                    vec!["-l", "100%FREE", "-n", chosen_lv_name, vg_name]
                } else {
                    vec!["--type", raid_type, "-l", "100%FREE", "-n", chosen_lv_name, vg_name]
                };

                let lv_create_out = Command::new("lvcreate")
                    .args(&create_args)
                    .output()
                    .map_err(|e| format!("Impossible d'exécuter lvcreate : {}", e))?;

                if !lv_create_out.status.success() {
                    let err = String::from_utf8_lossy(&lv_create_out.stderr);
                    return Err(format!("Échec de création du volume logique : {}", err));
                }

                let _ = Command::new("vgchange")
                    .args(["-ay", "-K", "--activationmode", "degraded", vg_name])
                    .output();
                chosen_lv_name.to_string()
            }
        };

        // Forcer la création synchrone des nœuds /dev avec vgmknodes et settle
        let _ = Command::new("vgmknodes").output();
        let _ = Command::new("udevadm").args(["settle", "--timeout=3"]).output();

        let candidate_dev = format!("/dev/{}/{}", vg_name, chosen_lv);
        let candidate_mapper = format!("/dev/mapper/{}-{}", vg_name.replace('-', "--"), chosen_lv.replace('-', "--"));

        if Path::new(&candidate_dev).exists() {
            return Ok(candidate_dev);
        } else if Path::new(&candidate_mapper).exists() {
            return Ok(candidate_mapper);
        } else {
            // Deuxième tentative avec léger délai de synchronisation
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = Command::new("vgmknodes").output();
            let _ = Command::new("udevadm").args(["settle", "--timeout=2"]).output();

            if Path::new(&candidate_dev).exists() {
                return Ok(candidate_dev);
            } else if Path::new(&candidate_mapper).exists() {
                return Ok(candidate_mapper);
            }

            // Ultime recherche via dmsetup pour un mappeur actif
            let dm_out = Command::new("dmsetup")
                .args(["info", "-C", "--noheadings", "-o", "devname", &format!("{}-{}", vg_name.replace('-', "--"), chosen_lv.replace('-', "--"))])
                .output();
            if let Ok(dm) = dm_out {
                let dm_name = String::from_utf8_lossy(&dm.stdout).trim().to_string();
                if !dm_name.is_empty() {
                    let dm_path = format!("/dev/{}", dm_name);
                    if Path::new(&dm_path).exists() {
                        return Ok(dm_path);
                    }
                }
            }

            return Err(format!(
                "Volume logique '{}/{}' activé mais périphérique de bloc introuvable ('{}' ou '{}').",
                vg_name, chosen_lv, candidate_dev, candidate_mapper
            ));
        }
    }

    // Cas C : périphérique standard ou /dev/mapper existant
    if clean_dev.starts_with("/dev/mapper/") {
        let _ = Command::new("udevadm").args(["settle", "--timeout=2"]).output();
        if Path::new(clean_dev).exists() {
            return Ok(clean_dev.to_string());
        }
    }

    if Path::new(clean_dev).exists() {
        return Ok(clean_dev.to_string());
    }

    let _ = Command::new("udevadm").args(["settle", "--timeout=2"]).output();
    if Path::new(clean_dev).exists() {
        return Ok(clean_dev.to_string());
    }

    Err(format!("Le périphérique bloc '{}' est introuvable sur le système. Vérifiez qu'il est bien connecté ou actif.", clean_dev))
}

pub fn mount_volume(req: &MountVolumeRequest) -> Result<String, String> {
    let _ = &req.name;
    let clean_dev = req.device.trim();
    let mount_target = req.mountpoint.as_deref().unwrap_or("/mnt/storage").trim();
    if mount_target.is_empty() || !mount_target.starts_with('/') {
        return Err("Point de montage invalide.".into());
    }

    if mount_target == "/" || mount_target == "/boot" || mount_target.starts_with("/nix") {
        return Err("Interdiction : Impossible de monter sur un répertoire système.".into());
    }

    let final_block_device = resolve_and_activate_block_device(clean_dev, req)?;

    // 1. Contrôle strict de présence physique du fichier de périphérique bloc
    if !Path::new(&final_block_device).exists() {
        return Err(format!(
            "Le périphérique bloc '{}' est introuvable sur le système. Vérifiez qu'il est bien connecté et actif.",
            final_block_device
        ));
    }

    // 2. Détection du système de fichiers existant (blkid + lsblk en fallback)
    let blkid_out = Command::new("blkid").args(["-o", "value", "-s", "TYPE", &final_block_device]).output();
    let blkid_fs = blkid_out.as_ref().ok().and_then(|o| {
        if o.status.success() && !o.stdout.is_empty() {
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            None
        }
    });

    let lsblk_fs = if blkid_fs.is_none() {
        Command::new("lsblk")
            .args(["-no", "FSTYPE", &final_block_device])
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            })
    } else {
        None
    };

        let has_fs = blkid_fs.is_some() || lsblk_fs.is_some();
    let fs_type = req.fs_type.as_deref().unwrap_or("btrfs");
    let wants_format = req.force_format == Some(true);

    if wants_format {
        // Purger toute signature de système de fichiers résiduelle (ex: anciens blocs XFS/Ext4)
        let _ = Command::new("wipefs").args(["-a", &final_block_device]).output();
        let _ = Command::new("udevadm").args(["settle", "--timeout=2"]).output();

        let fmt_status = if fs_type == "btrfs" {
            Command::new("mkfs.btrfs").args(["-f", "-L", "STORAGE", &final_block_device]).output()
        } else if fs_type == "ext4" {
            Command::new("mkfs.ext4").args(["-F", "-L", "STORAGE", &final_block_device]).output()
        } else {
            Command::new("mkfs.xfs").args(["-f", "-L", "STORAGE", &final_block_device]).output()
        };

        if let Ok(out) = fmt_status {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return Err(format!("Échec du formatage en {} : {}", fs_type, err));
            }
        }
        let _ = Command::new("udevadm").args(["settle", "--timeout=2"]).output();
    } else if !has_fs {
        return Err("Aucun système de fichiers valide n'a été détecté sur ce volume. Si vous souhaitez l'initialiser et effacer toutes les données existantes, utilisez l'option 'Formater le volume'.".into());
    }

    let detected_fs = if wants_format {
        fs_type.to_string()
    } else if let Some(fs) = blkid_fs {
        fs
    } else if let Some(fs) = lsblk_fs {
        fs
    } else {
        fs_type.to_string()
    };

    let primary_user = target_user();
    let (p_uid, p_gid) = get_user_uid_gid(&primary_user);
    let storage_gid = get_storage_gid().unwrap_or(p_gid);

    let fs_lower = detected_fs.to_lowercase();
    let is_non_posix_fs = fs_lower == "vfat" || fs_lower == "fat" || fs_lower == "msdos" || fs_lower == "exfat" || fs_lower.starts_with("ntfs");

    let mut effective_options: Vec<String> = req.options.clone().unwrap_or_else(|| {
        let mut opts = vec!["defaults".to_string(), "noatime".to_string(), "nofail".to_string()];
        if detected_fs == "btrfs" {
            opts.push("compress=zstd".to_string());
        }
        opts
    });

    if is_non_posix_fs {
        if !effective_options.iter().any(|o| o.starts_with("uid=")) {
            effective_options.push(format!("uid={}", p_uid));
        }
        if !effective_options.iter().any(|o| o.starts_with("gid=")) {
            effective_options.push(format!("gid={}", storage_gid));
        }
        if !effective_options.iter().any(|o| o.starts_with("dmask=") || o.starts_with("umask=")) {
            effective_options.push("dmask=0002".to_string());
            effective_options.push("fmask=0113".to_string());
        }
        if fs_lower.starts_with("ntfs") && !effective_options.iter().any(|o| o == "rw") {
            effective_options.push("rw".to_string());
        }
    }

    let runtime_opts_str = sanitize_runtime_mount_options(&effective_options);

    let _ = Command::new("mkdir").args(["-p", mount_target]).output();

    let mut mount_cmd = Command::new("mount");
    if !detected_fs.is_empty() && detected_fs != "auto" {
        mount_cmd.args(["-t", &detected_fs]);
    }
    mount_cmd.args(["-o", &runtime_opts_str, &final_block_device, mount_target]);
    let mount_out = mount_cmd
        .output()
        .map_err(|e| format!("Impossible d'exécuter mount : {}", e))?;

    if !mount_out.status.success() {
        let err = String::from_utf8_lossy(&mount_out.stderr);
        return Err(format!("Échec du montage sur {} : {}", mount_target, err));
    }

    // Application automatique des permissions pour l'utilisateur NAS et le groupe storage
    let primary_user = target_user();
    let _ = Command::new("chown").args(["-R", &format!("{}:storage", primary_user), mount_target]).output();
    let _ = Command::new("chmod").args(["2775", mount_target]).output();

    // Persistance déclarative dans mounts.json
    let should_persist = req.persist.unwrap_or(true);
    if should_persist {
        let uuid = Command::new("blkid")
            .args(["-s", "UUID", "-o", "value", &final_block_device])
            .output()
            .ok()
            .and_then(|o| {
                let u = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if u.is_empty() { None } else { Some(u) }
            });

        let effective_device = if let Some(ref u) = uuid {
            format!("/dev/disk/by-uuid/{}", u)
        } else {
            final_block_device.clone()
        };

        let mut mounts = load_persisted_mounts();
        mounts.retain(|m| m.mount_point != mount_target && m.device != final_block_device && m.device != effective_device);

        let pool_name = mount_target.trim_start_matches("/mnt/").trim_start_matches("/media/").trim_start_matches('/').to_string();
        let mount_id = format!("mount-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());

        mounts.push(PersistedMount {
            id: mount_id,
            name: if pool_name.is_empty() { "storage".to_string() } else { pool_name },
            device: effective_device,
            device_uuid: uuid,
            mount_point: mount_target.to_string(),
            fs_type: if detected_fs.is_empty() { "auto".to_string() } else { detected_fs },
            options: effective_options,
            enabled: true,
            created_at: Some("Aujourd'hui".to_string()),
        });

        if let Err(e) = save_persisted_mounts(&mounts) {
            eprintln!("[STORAGE] Avertissement: échec sauvegarde mounts.json: {}", e);
        }
    }

    Ok(format!(
        "Le volume {} a été monté avec succès sur {} !",
        final_block_device, mount_target
    ))
}

pub fn umount_volume(req: &UmountVolumeRequest) -> Result<String, String> {
    let target = req.mountpoint.trim();
    if target == "/" || target == "/boot" || target.starts_with("/nix") {
        return Err("Interdiction : Impossible de démonter un répertoire système.".into());
    }

    let out = Command::new("umount")
        .args(["-l", target])
        .output()
        .map_err(|e| format!("Impossible d'exécuter umount : {}", e))?;

    if out.status.success() {
        let should_remove = req.remove_persist.unwrap_or(true);
        if should_remove {
            let mut mounts = load_persisted_mounts();
            let initial_len = mounts.len();
            mounts.retain(|m| m.mount_point != target && m.device != target);
            if mounts.len() != initial_len {
                if let Err(e) = save_persisted_mounts(&mounts) {
                    eprintln!("[STORAGE] Avertissement: échec suppression mounts.json: {}", e);
                }
            }
        }
        Ok(format!("Le point de montage {} a été démonté avec succès.", target))
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("Échec du démontage : {}", err))
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    const TIB: u64 = 1024 * GIB;

    if bytes >= TIB {
        format!("{:.1} To", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.1} Go", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} Mo", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} Ko", bytes as f64 / KIB as f64)
    } else {
        format!("{} o", bytes)
    }
}


pub fn create_partition(req: &CreatePartitionRequest) -> Result<String, String> {
    let disk = req.disk_path.trim();
    if !disk.starts_with("/dev/") || is_system_device(disk) {
        return Err("Périphérique invalide ou protégé (Système NixOS).".into());
    }

    if !Path::new(disk).exists() {
        return Err(format!("Le disque '{}' est introuvable.", disk));
    }

    let ptable_check = Command::new("sfdisk").args(["-d", disk]).output();
    let has_table = if let Ok(out) = ptable_check {
        out.status.success() && !out.stdout.is_empty()
    } else {
        false
    };

    if !has_table {
        let init_cmd = Command::new("parted").args(["-s", disk, "mklabel", "gpt"]).output();
        if init_cmd.is_err() || !init_cmd.as_ref().unwrap().status.success() {
            let _ = Command::new("sfdisk").arg(disk)
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(b"label: gpt\n");
                    }
                    child.wait()
                });
        }
    }

    let fs_type = req.fs_type.to_lowercase();
    let part_fs = match fs_type.as_str() {
        "ext4" => "ext4",
        "btrfs" => "btrfs",
        "xfs" => "xfs",
        "vfat" | "fat32" => "vfat",
        "exfat" => "exfat",
        _ => "btrfs",
    };

    let sfdisk_input = if let Some(mb) = req.size_mb {
        if mb > 0 { format!(",{}MiB\n", mb) } else { ",,\n".to_string() }
    } else {
        ",,\n".to_string()
    };

    let mut success = false;
    if let Ok(mut cmd) = Command::new("sfdisk")
        .args(["--append", disk])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        use std::io::Write;
        if let Some(mut stdin) = cmd.stdin.take() {
            let _ = stdin.write_all(sfdisk_input.as_bytes());
        }
        if let Ok(out) = cmd.wait_with_output() {
            success = out.status.success();
        }
    }

    if !success {
        let mkpart_res = if let Some(mb) = req.size_mb {
            if mb == 0 {
                Command::new("parted").args(["-s", "-a", "optimal", disk, "mkpart", "primary", part_fs, "0%", "100%"]).output()
            } else {
                Command::new("parted").args(["-s", "-a", "optimal", disk, "mkpart", "primary", part_fs, "0%", &format!("{}MiB", mb)]).output()
            }
        } else {
            Command::new("parted").args(["-s", "-a", "optimal", disk, "mkpart", "primary", part_fs, "0%", "100%"]).output()
        };

        if let Ok(out) = mkpart_res {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                return Err(format!("Échec de création de la partition : {}", err));
            }
        } else {
            return Err("Impossible de créer la partition (sfdisk et parted ont échoué).".into());
        }
    }

    let _ = Command::new("udevadm").args(["settle"]).output();
    std::thread::sleep(std::time::Duration::from_millis(800));

    let mut new_part_path = String::new();
    if let Ok(out) = Command::new("lsblk").args(["-no", "PATH", disk]).output() {
        let lines: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(|s| s.trim().to_string()).collect();
        if lines.len() > 1 {
            new_part_path = lines.last().unwrap().clone();
        }
    }

    if new_part_path.is_empty() || new_part_path == disk {
        new_part_path = if disk.ends_with(|c: char| c.is_ascii_digit()) {
            format!("{}p1", disk)
        } else {
            format!("{}1", disk)
        };
    }

    let label = req.label.as_deref().unwrap_or("DATA").trim();
    let fmt_status = match part_fs {
        "btrfs" => Command::new("mkfs.btrfs").args(["-f", "-L", label, &new_part_path]).output(),
        "ext4" => Command::new("mkfs.ext4").args(["-F", "-L", label, &new_part_path]).output(),
        "vfat" => Command::new("mkfs.vfat").args(["-F", "32", "-n", label, &new_part_path]).output(),
        "exfat" => Command::new("mkfs.exfat").args(["-n", label, &new_part_path]).output(),
        "xfs" => Command::new("mkfs.xfs").args(["-f", "-L", label, &new_part_path]).output(),
        _ => Command::new("mkfs.btrfs").args(["-f", "-L", label, &new_part_path]).output(),
    };

    if let Ok(out) = fmt_status {
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("Partition créée ({}) mais échec du formatage : {}", new_part_path, err));
        }
    }

    if let Some(ref mnt) = req.mountpoint {
        let clean_mnt = mnt.trim();
        if !clean_mnt.is_empty() && clean_mnt.starts_with("/mnt") {
            let _ = Command::new("mkdir").args(["-p", clean_mnt]).output();
            let _ = Command::new("mount").args([&new_part_path, clean_mnt]).output();
            let user = target_user();
            let _ = Command::new("chown").args(["-R", &format!("{}:storage", user), clean_mnt]).output();
            let _ = Command::new("chmod").args(["2775", clean_mnt]).output();

            let _ = Command::new("udevadm").args(["settle", "--timeout=3"]).output();
            let uuid = Command::new("blkid")
                .args(["-s", "UUID", "-o", "value", &new_part_path])
                .output()
                .ok()
                .and_then(|o| {
                    let u = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if u.is_empty() { None } else { Some(u) }
                });
            let effective_device = if let Some(ref u) = uuid {
                format!("/dev/disk/by-uuid/{}", u)
            } else {
                new_part_path.clone()
            };

            let mut mounts = load_persisted_mounts();
            mounts.retain(|m| m.mount_point != clean_mnt && m.device != new_part_path && m.device != effective_device);
            let mount_id = format!("mount-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
            let mut opts = vec!["defaults".to_string(), "noatime".to_string(), "nofail".to_string()];
            if part_fs == "btrfs" { opts.push("compress=zstd".to_string()); }

            mounts.push(PersistedMount {
                id: mount_id,
                name: label.to_string(),
                device: effective_device,
                device_uuid: uuid,
                mount_point: clean_mnt.to_string(),
                fs_type: part_fs.to_string(),
                options: opts,
                enabled: true,
                created_at: Some("Aujourd'hui".to_string()),
            });
            let _ = save_persisted_mounts(&mounts);
        }
    }

    Ok(format!("Partition {} créée avec succès et formatée en {} !", new_part_path, part_fs.to_uppercase()))
}

pub fn delete_partition(req: &DeletePartitionRequest) -> Result<String, String> {
    let part = req.partition_path.trim();
    if !part.starts_with("/dev/") || is_system_device(part) {
        return Err("Interdiction absolue : Impossible de supprimer une partition système NixOS.".into());
    }

    let _ = Command::new("umount").args(["-f", part]).output();
    let _ = Command::new("wipefs").args(["-a", part]).output();

    let (disk, num) = if let Some(idx) = part.rfind('p') {
        if part.starts_with("/dev/nvme") || part.starts_with("/dev/mmcblk") {
            let d = &part[..idx];
            let n = &part[idx+1..];
            (d, n)
        } else {
            let num_start = part.chars().rev().take_while(|c| c.is_ascii_digit()).count();
            let split_pos = part.len() - num_start;
            (&part[..split_pos], &part[split_pos..])
        }
    } else {
        let num_start = part.chars().rev().take_while(|c| c.is_ascii_digit()).count();
        let split_pos = part.len() - num_start;
        (&part[..split_pos], &part[split_pos..])
    };

    let part_num = num.parse::<u32>().map_err(|_| format!("Impossible d'extraire le numéro de partition de {}", part))?;

    let rm_out = Command::new("sfdisk").args(["--delete", disk, &part_num.to_string()]).output();
    let success = match rm_out {
        Ok(ref o) if o.status.success() => true,
        _ => {
            let parted_out = Command::new("parted").args(["-s", disk, "rm", &part_num.to_string()]).output();
            parted_out.map(|o| o.status.success()).unwrap_or(false)
        }
    };

    if !success {
        return Err(format!("Échec de la suppression de la partition {} sur {}", part_num, disk));
    }

    let _ = Command::new("udevadm").args(["settle"]).output();

    let mut mounts = load_persisted_mounts();
    let initial_len = mounts.len();
    mounts.retain(|m| m.device != part);
    if mounts.len() != initial_len {
        let _ = save_persisted_mounts(&mounts);
    }

    Ok(format!("Partition {} supprimée avec succès.", part))
}

pub fn eject_removable(req: &EjectRemovableRequest) -> Result<String, String> {
    let dev = req.device_path.trim();
    if !dev.starts_with("/dev/") || is_system_device(dev) {
        return Err("Périphérique invalide ou protégé.".into());
    }

    let dev_name = req.name.as_deref().unwrap_or(dev);

    let _ = Command::new("sync").output();

    if let Ok(out) = Command::new("lsblk").args(["-no", "PATH,MOUNTPOINT", dev]).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let mnt = parts[1];
                let _ = Command::new("umount").args(["-f", mnt]).output();
            }
        }
    }
    let _ = Command::new("umount").args(["-f", dev]).output();

    if dev.contains("sr") {
        let _ = Command::new("eject").arg(dev).output();
    } else {
        let udisks_res = Command::new("udisksctl").args(["power-off", "-b", dev]).output();
        if udisks_res.is_err() || !udisks_res.as_ref().unwrap().status.success() {
            let _ = Command::new("eject").arg(dev).output();
        }
    }

    Ok(format!("Votre périphérique '{}' peut être déconnecté en toute sécurité !", dev_name))
}
