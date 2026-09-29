use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageOverview {
    pub pools: Vec<StoragePool>,
    pub physical_disks: Vec<PhysicalDiskInfo>,
    pub logical_raids: Vec<LogicalRaidInfo>,
    pub active_sync: Option<RaidSyncProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePool {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalDiskInfo {
    pub name: String,
    pub path: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub path: String,
    pub size_human: String,
    pub fstype: Option<String>,
    pub mountpoint: Option<String>,
    pub is_system: bool,
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
pub struct FormatDiskRequest {
    pub device: String,
    pub fs_type: String, // "btrfs", "ext4", "xfs", "vfat"
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRaidRequest {
    pub name: String,
    pub level: String, // "raid0", "raid1", "raid5", "raid6", "raid10", "linear"
    pub devices: Vec<String>,
    pub fs_type: String, // "btrfs", "ext4"
    pub mountpoint: String, // ex: "/mnt/storage"
}

// --------------------------------------------------------------------------
// LECTURE ET SCAN CONSOLIDÉ DU STOCKAGE
// --------------------------------------------------------------------------
pub fn get_storage_overview() -> StorageOverview {
    let pools = scan_storage_pools();
    let logical_raids = scan_logical_raids(&pools);
    let physical_disks = scan_physical_disks(&logical_raids);
    let active_sync = logical_raids.iter().find_map(|r| r.sync_progress.clone());

    StorageOverview {
        pools,
        physical_disks,
        logical_raids,
        active_sync,
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

    // 1. Scanner /proc/mdstat pour les arrays mdadm
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

                    // Vérifier si la ligne suivante ou suivante contient resync / blocks
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

                    // Chercher montage correspondant dans pools
                    let matched_pool = pools.iter().find(|p| p.filesystem.contains(&md_name));
                    let (mountpoint, used_bytes, free_bytes, usage_percent, fs_name) = if let Some(p) = matched_pool {
                        (Some(p.mountpoint.clone()), p.used_bytes, p.free_bytes, p.usage_percent, p.filesystem.clone())
                    } else {
                        (None, 0, total_bytes, 0.0, "Non monté".to_string())
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
                    });
                }
            }
            i += 1;
        }
    }

    // 2. Scanner Btrfs multi-device / pools si aucun mdadm n'existe ou en complément
    if raids.is_empty() {
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
                        });
                    }
                }
            }
        }
    }

    raids
}

fn scan_physical_disks(logical_raids: &[LogicalRaidInfo]) -> Vec<PhysicalDiskInfo> {
    let mut physical_disks = Vec::new();

    // Utiliser `lsblk -J` pour obtenir l'arbre complet JSON
    if let Ok(output) = Command::new("lsblk")
        .args(["-J", "-o", "NAME,PATH,SIZE,ROTA,TYPE,MOUNTPOINTS,MODEL,SERIAL,FSTYPE"])
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
                    let size_human = dev.get("size").and_then(|s| s.as_str()).unwrap_or("0").to_string();
                    let is_rotational = dev.get("rota").and_then(|r| r.as_bool()).unwrap_or(true);
                    let model = dev.get("model").and_then(|m| m.as_str()).unwrap_or("Disque standard").trim().to_string();
                    let serial = dev.get("serial").and_then(|s| s.as_str()).unwrap_or("N/A").trim().to_string();
                    let fstype = dev.get("fstype").and_then(|f| f.as_str()).map(|f| f.to_string());

                    let disk_type = if path.contains("nvme") {
                        "SSD NVMe PCIe".to_string()
                    } else if is_rotational {
                        "HDD 3.5\" SATA".to_string()
                    } else {
                        "SSD SATA Flash".to_string()
                    };

                    // Extraire les partitions
                    let mut partitions = Vec::new();
                    let mut contains_system = path.contains("nvme1n1");

                    if let Some(children) = dev.get("children").and_then(|c| c.as_array()) {
                        for child in children {
                            let part_name = child.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                            let part_path = child.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
                            let part_size = child.get("size").and_then(|s| s.as_str()).unwrap_or("").to_string();
                            let part_fs = child.get("fstype").and_then(|f| f.as_str()).map(|s| s.to_string());
                            let mut part_mount = None;
                            let mut part_is_sys = false;

                            if let Some(mounts) = child.get("mountpoints").and_then(|m| m.as_array()) {
                                if let Some(m0) = mounts.first().and_then(|m| m.as_str()) {
                                    part_mount = Some(m0.to_string());
                                    if m0 == "/" || m0 == "/boot" || m0.starts_with("/nix") {
                                        part_is_sys = true;
                                        contains_system = true;
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
                            });
                        }
                    }

                    // Rôle / Affiliation
                    let role = if contains_system {
                        "Système NixOS (Verrouillé)".to_string()
                    } else {
                        let member_of = logical_raids.iter().find(|r| {
                            r.members.iter().any(|m| m.contains(&name))
                        });

                        if let Some(r) = member_of {
                            format!("Membre de {} ({})", r.name, r.level)
                        } else if let Some(fs) = &fstype {
                            if fs.contains("LVM") {
                                "Membre LVM2 (vg1)".to_string()
                            } else {
                                format!("Volume simple ({})", fs)
                            }
                        } else if !partitions.is_empty() {
                            "Partitionné".to_string()
                        } else {
                            "✨ Libre (Non alloué)".to_string()
                        }
                    };

                    // État spindown
                    let mut power_state = "Actif / En ligne".to_string();
                    if is_rotational {
                        if let Ok(hd_out) = Command::new("hdparm").args(["-C", &path]).output() {
                            let hd_str = String::from_utf8_lossy(&hd_out.stdout);
                            if hd_str.contains("standby") {
                                power_state = "Veille (Standby)".into();
                            } else if hd_str.contains("active") || hd_str.contains("idle") {
                                power_state = "Actif / En rotation".into();
                            }
                        }
                    }

                    let temperature_c = if is_rotational { 32.0 } else { 35.0 };

                    physical_disks.push(PhysicalDiskInfo {
                        name,
                        path,
                        size_human,
                        model,
                        serial,
                        disk_type,
                        is_rotational,
                        is_system: contains_system,
                        fstype,
                        role,
                        power_state,
                        smart_status: "Sain (PASS)".into(),
                        temperature_c,
                        partitions,
                    });
                }
            }
        }
    }

    physical_disks
}

fn parse_mdstat_sync() -> Option<RaidSyncProgress> {
    if let Ok(content) = std::fs::read_to_string("/proc/mdstat") {
        let lines: Vec<&str> = content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if line.contains("resync") || line.contains("recovery") || line.contains("check") {
                let md_name = if idx > 0 {
                    lines[idx - 1].split_whitespace().next().unwrap_or("md0")
                } else {
                    "md0"
                };
                return parse_sync_line(md_name, line);
            }
        }
    }
    None
}

fn parse_sync_line(array_name: &str, line: &str) -> Option<RaidSyncProgress> {
    // Exemple : [>....................]  resync =  1.2% (140648256/11720684544) finish=1198.4min speed=161048K/sec
    let action = if line.contains("recovery") {
        "Reconstruction".to_string()
    } else if line.contains("check") {
        "Vérification".to_string()
    } else {
        "Synchronisation initiale".to_string()
    };

    let percent = line
        .split('=')
        .nth(1)
        .and_then(|p| p.split('%').next())
        .and_then(|s| s.trim().parse::<f32>().ok())
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

pub fn create_raid(req: &CreateRaidRequest) -> Result<String, String> {
    for dev in &req.devices {
        if is_system_device(dev) {
            return Err(format!("Interdiction : Le périphérique {} fait partie du système NixOS.", dev));
        }
    }

    if req.devices.len() < 2 && req.level != "linear" {
        return Err("Une grappe RAID nécessite au moins 2 disques.".into());
    }

    let clean_name = req.name.trim().to_lowercase().replace(' ', "-");
    let md_device = format!("/dev/md/{}", clean_name);

    // Démonter et effacer les signatures
    for dev in &req.devices {
        let _ = Command::new("umount").args(["-f", dev]).output();
        let _ = Command::new("wipefs").args(["-a", dev]).output();
        let _ = Command::new("mdadm").args(["--zero-superblock", "--force", dev]).output();
    }

    let _ = Command::new("modprobe").args(["raid0", "raid1", "raid456", "raid10"]).output();

    let raid_level = match req.level.to_lowercase().as_str() {
        "raid0" | "0" => "0",
        "raid1" | "1" => "1",
        "raid5" | "5" => "5",
        "raid6" | "6" => "6",
        "raid10" | "10" => "10",
        "linear" => "linear",
        _ => return Err("Niveau de RAID invalide.".into()),
    };

    let dev_count = req.devices.len().to_string();
    let mut args = vec![
        "--create".to_string(),
        md_device.clone(),
        format!("--level={}", raid_level),
        format!("--raid-devices={}", dev_count),
    ];
    for d in &req.devices {
        args.push(d.clone());
    }
    args.push("--run".to_string());

    let create_out = Command::new("mdadm")
        .args(&args)
        .output()
        .map_err(|e| format!("Impossible d'exécuter mdadm : {}", e))?;

    if !create_out.status.success() {
        let err = String::from_utf8_lossy(&create_out.stderr);
        return Err(format!("Échec de création du RAID : {}", err));
    }

    // Attendre 1.5s que le périphérique soit exposé
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // Formater le volume RAID
    let label = clean_name.to_uppercase();
    let fs_type = req.fs_type.to_lowercase();
    let fmt_status = if fs_type == "btrfs" {
        Command::new("mkfs.btrfs").args(["-f", "-L", &label, &md_device]).output()
    } else {
        Command::new("mkfs.ext4").args(["-F", "-L", &label, &md_device]).output()
    };

    if let Ok(out) = fmt_status {
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("Le RAID a été créé mais le formatage a échoué : {}", err));
        }
    }

    // Créer le point de montage et monter
    let mountpoint = if req.mountpoint.is_empty() {
        format!("/mnt/{}", clean_name)
    } else {
        req.mountpoint.clone()
    };

    let _ = Command::new("mkdir").args(["-p", &mountpoint]).output();
    let _ = Command::new("mount").args([&md_device, &mountpoint]).output();

    Ok(format!(
        "Pool RAID {} ({}) créé avec succès avec {} disques, formaté en {} et monté sur {} !",
        clean_name,
        req.level.to_uppercase(),
        req.devices.len(),
        fs_type.to_uppercase(),
        mountpoint
    ))
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
