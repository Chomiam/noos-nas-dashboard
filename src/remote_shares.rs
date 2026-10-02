use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use serde::{Deserialize, Serialize};

// =========================================================================
// STRUCTURES : POINTS DE MONTAGE DU SYSTÈME & DISQUES ÉPINGLÉS
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMountItem {
    pub mount_point: String,
    pub device: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f32,
    pub total_human: String,
    pub used_human: String,
    pub free_human: String,
    pub label: String,
    pub icon: String,
    pub is_pinned: bool,
    pub is_removable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedMount {
    pub path: String,
    pub label: String,
    pub icon: String,
    pub date_added: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinMountRequest {
    pub path: String,
    pub label: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpinMountRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PinnedMountsConfig {
    pub pins: Vec<PinnedMount>,
    #[serde(default)]
    pub hidden_paths: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum PinnedStore {
    Config(PinnedMountsConfig),
    List(Vec<PinnedMount>),
}

fn get_pinned_mounts_file() -> PathBuf {
    let var_lib = Path::new("/var/lib/steveos");
    if var_lib.exists() || fs::create_dir_all(var_lib).is_ok() {
        var_lib.join("pinned_mounts.json")
    } else {
        PathBuf::from("/tmp/steveos_pinned_mounts.json")
    }
}

pub fn load_pinned_mounts_config() -> PinnedMountsConfig {
    let path = get_pinned_mounts_file();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(store) = serde_json::from_str::<PinnedStore>(&content) {
            match store {
                PinnedStore::Config(c) => return c,
                PinnedStore::List(list) => return PinnedMountsConfig {
                    pins: list,
                    hidden_paths: HashSet::new(),
                },
            }
        }
    }
    PinnedMountsConfig::default()
}

pub fn save_pinned_mounts_config(config: &PinnedMountsConfig) -> Result<(), String> {
    let path = get_pinned_mounts_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("Impossible de sauvegarder les favoris : {}", e))
}

pub fn get_storage_mounts_raw() -> Vec<StorageMountItem> {
    let mut items = Vec::new();

    if let Ok(output) = Command::new("df").args(["-B1", "-T"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 7 {
                let device = parts[0].to_string();
                let fstype = parts[1].to_string();
                let total_bytes: u64 = parts[2].parse().unwrap_or(0);
                let used_bytes: u64 = parts[3].parse().unwrap_or(0);
                let free_bytes: u64 = parts[4].parse().unwrap_or(0);
                let mount_point = parts[6].to_string();

                // Filtrer les montages virtuels système (cgroup, devtmpfs, tmpfs système, overlay docker)
                let is_virtual = fstype.contains("tmpfs") && !mount_point.starts_with("/run/media")
                    || fstype == "devtmpfs"
                    || fstype == "squashfs"
                    || fstype == "overlay"
                    || mount_point.starts_with("/nix")
                    || mount_point.starts_with("/boot")
                    || mount_point.starts_with("/sys")
                    || mount_point.starts_with("/proc")
                    || mount_point.starts_with("/dev");

                if is_virtual && mount_point != "/" {
                    continue;
                }

                if total_bytes == 0 {
                    continue;
                }

                let usage_percent = if total_bytes > 0 {
                    ((used_bytes as f64 / total_bytes as f64) * 100.0) as f32
                } else {
                    0.0
                };

                let is_removable = mount_point.starts_with("/run/media") || mount_point.starts_with("/media");
                let is_remote = fstype.contains("fuse.sshfs") || fstype.contains("cifs") || fstype.contains("nfs") || mount_point.starts_with("/mnt/remote");

                let (label, icon) = if mount_point == "/" {
                    ("Système Root (NixOS)".into(), "🗄️".into())
                } else if mount_point == "/home" {
                    ("Dossiers Utilisateurs (/home)".into(), "🏠".into())
                } else if mount_point == "/mnt/storage" {
                    ("Stockage Principal (/mnt/storage)".into(), "💾".into())
                } else if mount_point.contains("raid") {
                    (format!("Grappe RAID ({})", mount_point.split('/').last().unwrap_or("raid")), "💽".into())
                } else if is_remote {
                    (format!("Partage Distant ({})", mount_point.split('/').last().unwrap_or("distant")), "🌐".into())
                } else if is_removable {
                    (format!("Disque Amovible USB ({})", mount_point.split('/').last().unwrap_or("usb")), "🔌".into())
                } else {
                    (mount_point.split('/').last().unwrap_or("Disque").to_string(), "💾".into())
                };

                items.push(StorageMountItem {
                    mount_point,
                    device,
                    filesystem: fstype,
                    total_bytes,
                    used_bytes,
                    free_bytes,
                    usage_percent: (usage_percent * 10.0).round() / 10.0,
                    total_human: format_bytes(total_bytes),
                    used_human: format_bytes(used_bytes),
                    free_human: format_bytes(free_bytes),
                    label,
                    icon,
                    is_pinned: false,
                    is_removable,
                });
            }
        }
    }

    items
}

pub fn load_pinned_mounts() -> Vec<PinnedMount> {
    let config = load_pinned_mounts_config();
    let raw_mounts = get_storage_mounts_raw();
    let mut active_mounts_map: HashMap<String, StorageMountItem> = HashMap::new();
    for sm in raw_mounts {
        active_mounts_map.insert(sm.mount_point.clone(), sm);
    }

    let mut all_available: HashMap<String, PinnedMount> = HashMap::new();

    // 1. Dossiers système de base (avec vérification stricte de l'existence physique)
    if !config.hidden_paths.contains("/") {
        all_available.insert("/".into(), PinnedMount {
            path: "/".into(),
            label: "Système Root (NixOS)".into(),
            icon: "🗄️".into(),
            date_added: "Système".into(),
        });
    }

    if Path::new("/mnt/storage").exists() && !config.hidden_paths.contains("/mnt/storage") {
        all_available.insert("/mnt/storage".into(), PinnedMount {
            path: "/mnt/storage".into(),
            label: "Stockage Principal (/mnt/storage)".into(),
            icon: "💾".into(),
            date_added: "Système".into(),
        });
    }

    if Path::new("/home").exists() && !config.hidden_paths.contains("/home") {
        all_available.insert("/home".into(), PinnedMount {
            path: "/home".into(),
            label: "Dossiers Utilisateurs (/home)".into(),
            icon: "🏠".into(),
            date_added: "Système".into(),
        });
    }

    // 2. Détection dynamique de tous les volumes/disques actuellement montés
    for (mount_point, item) in &active_mounts_map {
        if mount_point == "/" || config.hidden_paths.contains(mount_point) {
            continue;
        }
        all_available.entry(mount_point.clone()).or_insert_with(|| {
            PinnedMount {
                path: mount_point.clone(),
                label: item.label.clone(),
                icon: item.icon.clone(),
                date_added: "Montage Détecté".into(),
            }
        });
    }

    // 3. Épingles personnalisées enregistrées : n'inclure que si le chemin existe
    // et s'il s'agit d'un point de montage (ex: USB démonté sous /run/media ou /media),
    // vérifier qu'il est toujours actif
    for sp in &config.pins {
        let p = &sp.path;
        if config.hidden_paths.contains(p) {
            continue;
        }
        if !Path::new(p).exists() {
            // Le disque ou dossier n'existe plus physiquement -> éliminé
            continue;
        }
        if (p.starts_with("/run/media") || p.starts_with("/media")) && !active_mounts_map.contains_key(p) {
            // Disque amovible démonté -> masqué automatiquement
            continue;
        }

        if let Some(entry) = all_available.get_mut(p) {
            entry.label = sp.label.clone();
            entry.icon = sp.icon.clone();
        } else {
            all_available.insert(p.clone(), sp.clone());
        }
    }

    // 4. Ordonner selon l'ordre personnalisé de l'utilisateur
    let mut result = Vec::new();
    let mut used_keys = HashSet::new();

    for sp in &config.pins {
        if let Some(m) = all_available.get(&sp.path) {
            if used_keys.insert(sp.path.clone()) {
                result.push(m.clone());
            }
        }
    }

    // Ajouter les éléments restants selon l'ordre canonique
    let mut remaining: Vec<PinnedMount> = all_available
        .into_values()
        .filter(|m| !used_keys.contains(&m.path))
        .collect();

    remaining.sort_by(|a, b| {
        let priority = |p: &str| match p {
            "/" => 0,
            "/mnt/storage" => 1,
            "/home" => 2,
            _ => 3,
        };
        let pa = priority(&a.path);
        let pb = priority(&b.path);
        if pa != pb {
            pa.cmp(&pb)
        } else {
            a.path.cmp(&b.path)
        }
    });

    result.extend(remaining);
    result
}

#[allow(dead_code)]
pub fn save_pinned_mounts(mounts: &[PinnedMount]) -> Result<(), String> {
    let mut config = load_pinned_mounts_config();
    config.pins = mounts.to_vec();
    save_pinned_mounts_config(&config)
}

pub fn reorder_pinned_mounts(ordered_paths: &[String]) -> Result<Vec<PinnedMount>, String> {
    let current_list = load_pinned_mounts();
    let current_map: HashMap<String, PinnedMount> = current_list.into_iter().map(|m| (m.path.clone(), m)).collect();

    let mut config = load_pinned_mounts_config();
    let mut reordered = Vec::new();
    let mut seen = HashSet::new();

    for path in ordered_paths {
        let clean = path.trim().trim_end_matches('/');
        let final_path = if clean.is_empty() { "/" } else { clean };
        if let Some(m) = current_map.get(final_path) {
            if seen.insert(final_path.to_string()) {
                reordered.push(m.clone());
            }
        }
    }

    for (path, m) in current_map {
        if seen.insert(path) {
            reordered.push(m);
        }
    }

    config.pins = reordered.clone();
    save_pinned_mounts_config(&config)?;
    Ok(reordered)
}

pub fn pin_mount(path_str: &str, custom_label: Option<&str>, custom_icon: Option<&str>) -> Result<PinnedMount, String> {
    let mut config = load_pinned_mounts_config();
    let clean_path = path_str.trim().trim_end_matches('/');
    let final_path = if clean_path.is_empty() { "/" } else { clean_path };

    config.hidden_paths.remove(final_path);

    if let Some(existing) = config.pins.iter_mut().find(|m| m.path == final_path) {
        if let Some(lbl) = custom_label { existing.label = lbl.to_string(); }
        if let Some(ic) = custom_icon { existing.icon = ic.to_string(); }
        let res = existing.clone();
        save_pinned_mounts_config(&config)?;
        return Ok(res);
    }

    let default_icon = if final_path == "/" {
        "🗄️"
    } else if final_path.contains("raid") {
        "💽"
    } else if final_path.contains("remote") || final_path.contains("sftp") {
        "🌐"
    } else if final_path.contains("media") {
        "🎬"
    } else {
        "💾"
    };

    let default_label = final_path.split('/').last().unwrap_or("Disque").to_string();
    let now = chrono_timestamp_approx();

    let new_pin = PinnedMount {
        path: final_path.to_string(),
        label: custom_label.unwrap_or(&default_label).to_string(),
        icon: custom_icon.unwrap_or(default_icon).to_string(),
        date_added: now,
    };

    config.pins.push(new_pin.clone());
    save_pinned_mounts_config(&config)?;
    Ok(new_pin)
}

pub fn unpin_mount(path_str: &str) -> Result<(), String> {
    let mut config = load_pinned_mounts_config();
    let clean_path = path_str.trim().trim_end_matches('/');
    let final_path = if clean_path.is_empty() { "/" } else { clean_path };

    config.pins.retain(|m| m.path != final_path);
    config.hidden_paths.insert(final_path.to_string());

    save_pinned_mounts_config(&config)
}

pub fn get_storage_mounts() -> Vec<StorageMountItem> {
    let mut items = get_storage_mounts_raw();
    let pinned = load_pinned_mounts();
    let pinned_set: HashSet<String> = pinned.iter().map(|p| p.path.clone()).collect();

    for item in &mut items {
        item.is_pinned = pinned_set.contains(&item.mount_point);
    }

    items.sort_by(|a, b| {
        if a.is_pinned && !b.is_pinned {
            std::cmp::Ordering::Less
        } else if !a.is_pinned && b.is_pinned {
            std::cmp::Ordering::Greater
        } else {
            a.mount_point.cmp(&b.mount_point)
        }
    });

    items
}

// =========================================================================
// STRUCTURES & LOGIQUE : DÉCOUVERTE DU RÉSEAU LOCAL (LAN) & PARTAGES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub ip: String,
    pub hostname: String,
    pub is_local: bool,
    pub protocols: Vec<String>, // "smb", "sftp", "nfs", "http"
    pub smb_shares: Vec<String>,
    pub sftp_available: bool,
    pub sftp_port: u16,
    pub smb_available: bool,
    pub nfs_available: bool,
    pub device_type: String, // "NAS / Serveur", "Machine Windows", "Serveur SSH/sFTP", "Routeur / Passerelle", "Inconnu"
}

pub fn discover_network_devices() -> Vec<DiscoveredDevice> {
    let mut devices_map: HashMap<String, DiscoveredDevice> = HashMap::new();
    let local_lan_ip = crate::network::get_primary_lan_ip();

    // 1. Collecter les adresses IP actives sur le réseau local via la table ARP (/proc/net/arp)
    let mut ips_to_probe: HashSet<String> = HashSet::new();
    if let Ok(arp_content) = fs::read_to_string("/proc/net/arp") {
        for line in arp_content.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let ip = parts[0].to_string();
                let mac = parts[3];
                if mac != "00:00:00:00:00:00" && !ip.starts_with("127.") && ip != local_lan_ip {
                    ips_to_probe.insert(ip);
                }
            }
        }
    }

    // 2. Compléter via `ip -4 neigh show`
    if let Ok(output) = Command::new("ip").args(["-4", "neigh", "show"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                let ip = parts[0].to_string();
                if ip.contains('.') && ip != local_lan_ip && !line.contains("FAILED") {
                    ips_to_probe.insert(ip);
                }
            }
        }
    }

    // 3. Scanner via mDNS / Avahi si le démon est présent
    if let Ok(output) = Command::new("avahi-browse").args(["-a", "-t", "-r", "-p"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            // Ex: =;eth0;IPv4;TrueNAS;_smb._tcp;local;truenas.local;192.168.1.42;445;
            let parts: Vec<&str> = line.split(';').collect();
            if parts.len() >= 8 && parts[0] == "=" {
                let ip = parts[7].to_string();
                let host = parts[6].replace(".local", "");
                let service = parts[4];
                if !ip.is_empty() && ip != local_lan_ip {
                    ips_to_probe.insert(ip.clone());
                    let entry = devices_map.entry(ip.clone()).or_insert_with(|| DiscoveredDevice {
                        ip: ip.clone(),
                        hostname: host.clone(),
                        is_local: false,
                        protocols: Vec::new(),
                        smb_shares: Vec::new(),
                        sftp_available: false,
                        sftp_port: 22,
                        smb_available: false,
                        nfs_available: false,
                        device_type: "Périphérique Réseau".into(),
                    });

                    if service.contains("_smb") {
                        entry.smb_available = true;
                        if !entry.protocols.contains(&"smb".to_string()) {
                            entry.protocols.push("smb".into());
                        }
                    } else if service.contains("_sftp") || service.contains("_ssh") {
                        entry.sftp_available = true;
                        if !entry.protocols.contains(&"sftp".to_string()) {
                            entry.protocols.push("sftp".into());
                        }
                    } else if service.contains("_nfs") {
                        entry.nfs_available = true;
                        if !entry.protocols.contains(&"nfs".to_string()) {
                            entry.protocols.push("nfs".into());
                        }
                    }
                }
            }
        }
    }

    // 4. Sonde TCP rapide (SMB 445, SFTP/SSH 22, NFS 2049) sur les IPs détectées
    let timeout = Duration::from_millis(220);
    for ip in ips_to_probe {
        let mut smb_open = false;
        let mut sftp_open = false;
        let mut nfs_open = false;

        if let Ok(addr) = format!("{}:445", ip).parse::<SocketAddr>() {
            if TcpStream::connect_timeout(&addr, timeout).is_ok() {
                smb_open = true;
            }
        }

        if let Ok(addr) = format!("{}:22", ip).parse::<SocketAddr>() {
            if TcpStream::connect_timeout(&addr, timeout).is_ok() {
                sftp_open = true;
            }
        }

        if let Ok(addr) = format!("{}:2049", ip).parse::<SocketAddr>() {
            if TcpStream::connect_timeout(&addr, timeout).is_ok() {
                nfs_open = true;
            }
        }

        if smb_open || sftp_open || nfs_open {
            let entry = devices_map.entry(ip.clone()).or_insert_with(|| DiscoveredDevice {
                ip: ip.clone(),
                hostname: String::new(),
                is_local: false,
                protocols: Vec::new(),
                smb_shares: Vec::new(),
                sftp_available: false,
                sftp_port: 22,
                smb_available: false,
                nfs_available: false,
                device_type: "Hôte Réseau".into(),
            });

            if smb_open {
                entry.smb_available = true;
                if !entry.protocols.contains(&"smb".to_string()) {
                    entry.protocols.push("smb".into());
                }
            }
            if sftp_open {
                entry.sftp_available = true;
                if !entry.protocols.contains(&"sftp".to_string()) {
                    entry.protocols.push("sftp".into());
                }
            }
            if nfs_open {
                entry.nfs_available = true;
                if !entry.protocols.contains(&"nfs".to_string()) {
                    entry.protocols.push("nfs".into());
                }
            }

            // Découverte des partages SMB anonymes/invités via smbclient
            if smb_open && entry.smb_shares.is_empty() {
                if let Ok(smb_out) = Command::new("smbclient")
                    .args(["-N", "-g", "-L", &format!("//{}", ip)])
                    .output()
                {
                    let smb_txt = String::from_utf8_lossy(&smb_out.stdout);
                    for smb_line in smb_txt.lines() {
                        let fields: Vec<&str> = smb_line.split('|').collect();
                        if fields.len() >= 2 && fields[0] == "Disk" {
                            let share_name = fields[1].to_string();
                            if !share_name.ends_with('$') {
                                entry.smb_shares.push(share_name);
                            }
                        }
                    }
                }
            }

            // Résolution du hostname si vide
            if entry.hostname.is_empty() {
                if let Ok(nmb_out) = Command::new("nmblookup").args(["-A", &ip]).output() {
                    let nmb_txt = String::from_utf8_lossy(&nmb_out.stdout);
                    for nmb_line in nmb_txt.lines() {
                        let trimmed = nmb_line.trim();
                        if trimmed.contains("<00>") && !trimmed.contains("<GROUP>") {
                            let h = trimmed.split_whitespace().next().unwrap_or("").to_string();
                            if !h.is_empty() {
                                entry.hostname = h;
                                break;
                            }
                        }
                    }
                }
            }

            if entry.hostname.is_empty() {
                entry.hostname = format!("Hôte {}", ip);
            }

            // Déduction du type de périphérique
            let h_lower = entry.hostname.to_lowercase();
            if h_lower.contains("nas") || h_lower.contains("synology") || h_lower.contains("truenas") || h_lower.contains("qnap") || (smb_open && sftp_open) {
                entry.device_type = "NAS / Serveur de Fichiers".into();
            } else if smb_open {
                entry.device_type = "Partage Windows / Samba".into();
            } else if sftp_open {
                entry.device_type = "Serveur SSH / sFTP".into();
            } else if nfs_open {
                entry.device_type = "Serveur de Partage NFS".into();
            }
        }
    }

    let mut list: Vec<DiscoveredDevice> = devices_map.into_values().collect();
    list.sort_by(|a, b| {
        let a_score = (a.smb_shares.len() * 2) + if a.smb_available { 2 } else { 0 } + if a.sftp_available { 1 } else { 0 };
        let b_score = (b.smb_shares.len() * 2) + if b.smb_available { 2 } else { 0 } + if b.sftp_available { 1 } else { 0 };
        b_score.cmp(&a_score)
    });

    list
}

// =========================================================================
// STRUCTURES & LOGIQUE : MONTAGES RÉSEAU DISTANTS (sFTP & SMB)
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMountConfig {
    pub id: String,
    pub name: String,
    pub protocol: String, // "sftp", "smb"
    pub host: String,
    pub port: u16,
    pub remote_path: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub ssh_key_path: Option<String>,
    pub mount_point: String,
    pub auto_mount: bool,
    pub is_mounted: bool,
    pub status_text: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRemoteMountRequest {
    pub id: Option<String>,
    pub name: String,
    pub protocol: String, // "sftp", "smb"
    pub host: String,
    pub port: Option<u16>,
    pub remote_path: Option<String>,
    pub username: String,
    pub password: Option<String>,
    pub ssh_key_path: Option<String>,
    pub auto_mount: Option<bool>,
}

fn get_remote_mounts_file() -> PathBuf {
    let var_lib = Path::new("/var/lib/steveos");
    if var_lib.exists() || fs::create_dir_all(var_lib).is_ok() {
        var_lib.join("remote_mounts.json")
    } else {
        PathBuf::from("/tmp/steveos_remote_mounts.json")
    }
}

pub fn load_remote_mounts() -> Vec<RemoteMountConfig> {
    let path = get_remote_mounts_file();
    let mut mounts: Vec<RemoteMountConfig> = if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Actualiser le statut de montage en direct via /proc/mounts
    let active_mounts = get_active_mountpoints();
    for m in &mut mounts {
        m.is_mounted = active_mounts.contains(&m.mount_point);
        m.status_text = if m.is_mounted {
            "Connecté & Monté".into()
        } else {
            "Déconnecté".into()
        };
    }

    mounts
}

pub fn save_remote_mounts(mounts: &[RemoteMountConfig]) -> Result<(), String> {
    let path = get_remote_mounts_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(mounts).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("Impossible de sauvegarder les partages distants : {}", e))
}

pub fn is_dir_mounted(mount_point: &str) -> bool {
    let clean = mount_point.trim_end_matches('/');
    get_active_mountpoints().contains(&clean.to_string())
}

fn get_active_mountpoints() -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(content) = fs::read_to_string("/proc/mounts") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                set.insert(parts[1].trim_end_matches('/').to_string());
            }
        }
    }
    set
}

pub fn create_and_mount_remote(req: CreateRemoteMountRequest) -> Result<RemoteMountConfig, String> {
    let protocol = req.protocol.to_lowercase();
    if protocol != "sftp" && protocol != "smb" {
        return Err("Protocole invalide. Seuls 'sftp' et 'smb' sont supportés.".into());
    }

    let port = req.port.unwrap_or_else(|| if protocol == "sftp" { 22 } else { 445 });
    let remote_path = req.remote_path.unwrap_or_else(|| "/".into());
    let clean_remote_path = if remote_path.starts_with('/') { remote_path } else { format!("/{}", remote_path) };

    let slug = req.name.to_lowercase().replace(|c: char| !c.is_alphanumeric() && c != '-', "-");
    let id = req.id.unwrap_or_else(|| format!("{}-{}", protocol, slug));
    let mount_point = format!("/mnt/remote/{}/{}", protocol, slug);

    // Créer le point de montage local
    if let Err(e) = fs::create_dir_all(&mount_point) {
        return Err(format!("Échec création du dossier de montage local '{}' : {}", mount_point, e));
    }

    let mut mount_cfg = RemoteMountConfig {
        id: id.clone(),
        name: req.name,
        protocol: protocol.clone(),
        host: req.host,
        port,
        remote_path: clean_remote_path,
        username: req.username,
        password: req.password.clone(),
        ssh_key_path: req.ssh_key_path,
        mount_point: mount_point.clone(),
        auto_mount: req.auto_mount.unwrap_or(true),
        is_mounted: false,
        status_text: "En cours de connexion...".into(),
        error: None,
    };

    // Exécuter le montage physique
    match execute_mount(&mount_cfg) {
        Ok(_) => {
            mount_cfg.is_mounted = true;
            mount_cfg.status_text = "Connecté & Monté".into();
            mount_cfg.error = None;
        }
        Err(err) => {
            mount_cfg.is_mounted = false;
            mount_cfg.status_text = "Erreur de connexion".into();
            mount_cfg.error = Some(err.clone());
        }
    }

    // Sauvegarder dans la liste des partages distants
    let mut mounts = load_remote_mounts();
    mounts.retain(|m| m.id != id);
    mounts.push(mount_cfg.clone());
    save_remote_mounts(&mounts)?;

    // Épingler automatiquement le point de montage dans le gestionnaire de fichiers
    let _ = pin_mount(&mount_point, Some(&mount_cfg.name), Some("🌐"));

    if let Some(err) = mount_cfg.error {
        Err(err)
    } else {
        Ok(mount_cfg)
    }
}

pub fn execute_mount(cfg: &RemoteMountConfig) -> Result<(), String> {
    if is_dir_mounted(&cfg.mount_point) {
        return Ok(());
    }

    let _ = fs::create_dir_all(&cfg.mount_point);

    if cfg.protocol == "sftp" {
        // Montage sFTP via sshfs
        let target = format!("{}@{}:{}", cfg.username, cfg.host, cfg.remote_path);
        let mut cmd = Command::new("sshfs");
        cmd.arg(&target)
            .arg(&cfg.mount_point)
            .args(["-p", &cfg.port.to_string()])
            .args([
                "-o", "reconnect",
                "-o", "ServerAliveInterval=15",
                "-o", "ServerAliveCountMax=3",
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "allow_other",
                "-o", "idmap=user",
            ]);

        if let Some(key_path) = &cfg.ssh_key_path {
            cmd.args(["-o", &format!("IdentityFile={}", key_path)]);
        }

        if let Some(pwd) = &cfg.password {
            // Passer le mot de passe via stdin pour ne pas l'exposer
            cmd.arg("-o").arg("password_stdin");
            let mut child = cmd.stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("Impossible de lancer 'sshfs' (vérifiez si sshfs est installé) : {}", e))?;

            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(pwd.as_bytes());
                let _ = stdin.write_all(b"\n");
            }

            let output = child.wait_with_output().map_err(|e| e.to_string())?;
            if !output.status.success() {
                let err_str = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Échec montage sFTP : {}", err_str.trim()));
            }
        } else {
            let output = cmd.output().map_err(|e| format!("Erreur exécution sshfs : {}", e))?;
            if !output.status.success() {
                let err_str = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Échec montage sFTP : {}", err_str.trim()));
            }
        }
    } else if cfg.protocol == "smb" {
        // Montage SMB via mount -t cifs
        let target = format!("//{}/{}", cfg.host, cfg.remote_path.trim_start_matches('/'));
        let mut opts = format!("vers=3.0,iocharset=utf8,username={}", cfg.username);
        if let Some(pwd) = &cfg.password {
            opts.push_str(&format!(",password={}", pwd));
        } else {
            opts.push_str(",guest");
        }

        let output = Command::new("mount")
            .args(["-t", "cifs", &target, &cfg.mount_point, "-o", &opts])
            .output()
            .map_err(|e| format!("Erreur exécution mount.cifs : {}", e))?;

        if !output.status.success() {
            let err_str = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Échec montage SMB : {}", err_str.trim()));
        }
    }

    if is_dir_mounted(&cfg.mount_point) {
        Ok(())
    } else {
        Err("Le montage a retourné un code succès mais le dossier n'apparaît pas dans /proc/mounts.".into())
    }
}

pub fn unmount_remote(id: &str) -> Result<(), String> {
    let mut mounts = load_remote_mounts();
    let mount_cfg = mounts.iter_mut().find(|m| m.id == id).ok_or_else(|| "Partage distant introuvable".to_string())?;

    if is_dir_mounted(&mount_cfg.mount_point) {
        // Tenter fusermount3 / fusermount d'abord, puis umount
        let res = Command::new("fusermount3").args(["-u", &mount_cfg.mount_point]).output()
            .or_else(|_| Command::new("fusermount").args(["-u", &mount_cfg.mount_point]).output())
            .or_else(|_| Command::new("umount").arg(&mount_cfg.mount_point).output());

        if let Ok(out) = res {
            if !out.status.success() {
                // Tenter un démontage forcé (lazy unmount)
                let _ = Command::new("umount").args(["-l", &mount_cfg.mount_point]).output();
            }
        }
    }

    mount_cfg.is_mounted = is_dir_mounted(&mount_cfg.mount_point);
    mount_cfg.status_text = "Déconnecté".into();
    save_remote_mounts(&mounts)?;
    Ok(())
}

pub fn delete_remote_mount(id: &str) -> Result<(), String> {
    let _ = unmount_remote(id);
    let mut mounts = load_remote_mounts();
    let mut target_mount_point = None;
    if let Some(m) = mounts.iter().find(|m| m.id == id) {
        target_mount_point = Some(m.mount_point.clone());
    }

    mounts.retain(|m| m.id != id);
    save_remote_mounts(&mounts)?;

    // Retirer des favoris si présent
    if let Some(mp) = target_mount_point {
        let _ = unpin_mount(&mp);
        let _ = fs::remove_dir(&mp);
    }

    Ok(())
}

// =========================================================================
// UTILITAIRES
// =========================================================================

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} To", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} Go", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} Mo", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} Ko", bytes as f64 / KB as f64)
    } else {
        format!("{} o", bytes)
    }
}

fn chrono_timestamp_approx() -> String {
    if let Ok(output) = Command::new("date").arg("+%Y-%m-%d %H:%M").output() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        "2026-10-01".into()
    }
}
