use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaOverview {
    pub is_active: bool,
    pub unit: String,
    pub status_text: String,
    pub workgroup: String,
    pub server_string: String,
    pub netbios_name: String,
    pub primary_lan_ip: String,
    pub protocol_version: String,
    pub wsdd_active: bool,
    pub avahi_active: bool,
    pub shares: Vec<SambaShare>,
    pub active_sessions: Vec<SambaSession>,
    pub locked_files: Vec<SambaLockedFile>,
    pub global_config: SambaGlobalConfig,
    pub available_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaShare {
    pub id: String,
    pub name: String,
    pub path: String,
    pub comment: String,
    pub read_only: bool,
    pub browseable: bool,
    pub guest_ok: bool,
    pub valid_users: Vec<String>,
    pub write_list: Vec<String>,
    pub read_list: Vec<String>,
    pub is_time_machine: bool,
    pub time_machine_max_size_gb: Option<u64>,
    pub recycle_bin: bool,
    pub shadow_copy: bool,
    pub smb_encrypt: bool,
    pub create_mask: String,
    pub directory_mask: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaSession {
    pub pid: String,
    pub user: String,
    pub group: String,
    pub machine: String,
    pub client_ip: String,
    pub protocol: String,
    pub encryption: String,
    pub signing: String,
    pub share: String,
    pub connected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaLockedFile {
    pub pid: String,
    pub user: String,
    pub share: String,
    pub filename: String,
    pub access_mode: String,
    pub oplock: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaGlobalConfig {
    pub workgroup: String,
    pub server_string: String,
    pub netbios_name: String,
    pub min_protocol: String,
    pub max_protocol: String,
    pub wsdd_enabled: bool,
    pub multi_channel: bool,
    pub aio_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaDiagResult {
    pub is_valid: bool,
    pub output: String,
    pub errors: Vec<String>,
    pub loaded_services: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateShareRequest {
    pub name: String,
    pub path: String,
    pub comment: Option<String>,
    pub read_only: Option<bool>,
    pub browseable: Option<bool>,
    pub guest_ok: Option<bool>,
    pub valid_users: Option<Vec<String>>,
    pub write_list: Option<Vec<String>>,
    pub read_list: Option<Vec<String>>,
    pub is_time_machine: Option<bool>,
    pub time_machine_max_size_gb: Option<u64>,
    pub recycle_bin: Option<bool>,
    pub shadow_copy: Option<bool>,
    pub smb_encrypt: Option<bool>,
    pub create_mask: Option<String>,
    pub directory_mask: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateShareRequest {
    pub name: Option<String>,
    pub path: Option<String>,
    pub comment: Option<String>,
    pub read_only: Option<bool>,
    pub browseable: Option<bool>,
    pub guest_ok: Option<bool>,
    pub valid_users: Option<Vec<String>>,
    pub write_list: Option<Vec<String>>,
    pub read_list: Option<Vec<String>>,
    pub is_time_machine: Option<bool>,
    pub time_machine_max_size_gb: Option<u64>,
    pub recycle_bin: Option<bool>,
    pub shadow_copy: Option<bool>,
    pub smb_encrypt: Option<bool>,
    pub create_mask: Option<String>,
    pub directory_mask: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateGlobalConfigRequest {
    pub workgroup: Option<String>,
    pub server_string: Option<String>,
    pub min_protocol: Option<String>,
    pub max_protocol: Option<String>,
    pub wsdd_enabled: Option<bool>,
    pub multi_channel: Option<bool>,
    pub aio_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisconnectSessionRequest {
    pub pid: String,
}

// --------------------------------------------------------------------------
// PERSISTANCE DES PARTAGES & CONFIGURATION SAMBA
// --------------------------------------------------------------------------

pub fn get_samba_shares_json_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(PathBuf::from("/var/lib/steveos/samba_shares.json"));
    let cfg_dir = crate::updates::resolve_config_dir();
    let cfg_shares = cfg_dir.join("samba_shares.json");
    if !paths.contains(&cfg_shares) {
        paths.push(cfg_shares);
    }
    let u = target_user();
    let dev_shares = PathBuf::from(format!("/home/{}/Projects/steveos-nas/samba_shares.json", u));
    if !paths.contains(&dev_shares) {
        paths.push(dev_shares);
    }
    paths.push(PathBuf::from("/tmp/steveos_samba_shares.json"));
    paths
}

pub fn get_samba_conf_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(PathBuf::from("/var/lib/steveos/samba_shares.conf"));
    paths.push(PathBuf::from("/tmp/steveos_samba_shares.conf"));
    paths
}

fn target_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "chomiam".to_string())
}

pub fn load_samba_shares() -> Vec<SambaShare> {
    for p in get_samba_shares_json_paths() {
        if p.exists() {
            if let Ok(c) = fs::read_to_string(&p) {
                if let Ok(shares) = serde_json::from_str::<Vec<SambaShare>>(&c) {
                    return shares;
                }
            }
        }
    }

    // Partages par défaut sains pour un NAS moderne
    vec![
        SambaShare {
            id: "data".into(),
            name: "data".into(),
            path: "/storage/data".into(),
            comment: "Partage Général NAS (Lecture & Écriture)".into(),
            read_only: false,
            browseable: true,
            guest_ok: false,
            valid_users: vec![],
            write_list: vec![],
            read_list: vec![],
            is_time_machine: false,
            time_machine_max_size_gb: None,
            recycle_bin: true,
            shadow_copy: true,
            smb_encrypt: false,
            create_mask: "0664".into(),
            directory_mask: "0775".into(),
            exists: std::path::Path::new("/storage/data").exists(),
        },
        SambaShare {
            id: "media".into(),
            name: "media".into(),
            path: "/storage/media".into(),
            comment: "Médiathèque (Films, Séries, Musique - Jellyfin & DLNA)".into(),
            read_only: false,
            browseable: true,
            guest_ok: true,
            valid_users: vec![],
            write_list: vec![],
            read_list: vec![],
            is_time_machine: false,
            time_machine_max_size_gb: None,
            recycle_bin: false,
            shadow_copy: false,
            smb_encrypt: false,
            create_mask: "0664".into(),
            directory_mask: "0775".into(),
            exists: std::path::Path::new("/storage/media").exists(),
        },
        SambaShare {
            id: "backups".into(),
            name: "backups".into(),
            path: "/storage/backups".into(),
            comment: "Dépôt Sauvegardes & Snapshots (Accès Restreint)".into(),
            read_only: false,
            browseable: false,
            guest_ok: false,
            valid_users: vec![],
            write_list: vec![],
            read_list: vec![],
            is_time_machine: false,
            time_machine_max_size_gb: None,
            recycle_bin: true,
            shadow_copy: false,
            smb_encrypt: true,
            create_mask: "0660".into(),
            directory_mask: "0770".into(),
            exists: std::path::Path::new("/storage/backups").exists(),
        },
    ]
}

pub fn save_samba_shares(shares: &[SambaShare]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(shares).map_err(|e| e.to_string())?;
    let _ = fs::create_dir_all("/var/lib/steveos");

    let mut json_saved = false;
    for p in get_samba_shares_json_paths() {
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(&p, &json).is_ok() {
            json_saved = true;
        }
    }

    if !json_saved {
        let _ = fs::write("/tmp/steveos_samba_shares.json", &json);
    }

    // Génération du fichier smb_shares.conf
    let conf_text = generate_smb_shares_conf(shares);
    for cp in get_samba_conf_paths() {
        if let Some(parent) = cp.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(cp, &conf_text);
    }

    // Rechargement Samba à chaud
    let _ = reload_samba_service();

    Ok(())
}

pub fn generate_smb_shares_conf(shares: &[SambaShare]) -> String {
    let mut out = String::new();
    out.push_str("# =========================================================================\n");
    out.push_str("# 📁 STEvE_OS NAS Edition — Configuration Dynamique des Partages Samba (SMB)\n");
    out.push_str("# Généré automatiquement par STEvE_OS Dashboard. Ne pas éditer manuellement.\n");
    out.push_str("# =========================================================================\n\n");

    for s in shares {
        out.push_str(&format!("[{}]\n", s.name));
        out.push_str(&format!("   comment = {}\n", s.comment));
        out.push_str(&format!("   path = {}\n", s.path));
        out.push_str(&format!("   browseable = {}\n", if s.browseable { "yes" } else { "no" }));
        out.push_str(&format!("   read only = {}\n", if s.read_only { "yes" } else { "no" }));
        out.push_str(&format!("   guest ok = {}\n", if s.guest_ok { "yes" } else { "no" }));
        out.push_str(&format!("   create mask = {}\n", if !s.create_mask.is_empty() { &s.create_mask } else { "0664" }));
        out.push_str(&format!("   directory mask = {}\n", if !s.directory_mask.is_empty() { &s.directory_mask } else { "0775" }));

        if !s.valid_users.is_empty() {
            out.push_str(&format!("   valid users = {}\n", s.valid_users.join(" ")));
        }
        if !s.write_list.is_empty() {
            out.push_str(&format!("   write list = {}\n", s.write_list.join(" ")));
        }
        if !s.read_list.is_empty() {
            out.push_str(&format!("   read list = {}\n", s.read_list.join(" ")));
        }

        // VFS Objects (Time Machine, Corbeille, Shadow Copy)
        let mut vfs_objects = Vec::new();
        if s.recycle_bin {
            vfs_objects.push("recycle");
        }
        if s.is_time_machine {
            vfs_objects.push("fruit");
            vfs_objects.push("streams_xattr");
        }
        if s.shadow_copy {
            vfs_objects.push("shadow_copy2");
        }

        if !vfs_objects.is_empty() {
            out.push_str(&format!("   vfs objects = {}\n", vfs_objects.join(" ")));
        }

        if s.recycle_bin {
            out.push_str("   recycle:repository = .recycle\n");
            out.push_str("   recycle:keeptree = yes\n");
            out.push_str("   recycle:versions = yes\n");
            out.push_str("   recycle:touch = yes\n");
            out.push_str("   recycle:maxsize = 0\n");
        }

        if s.is_time_machine {
            out.push_str("   fruit:time machine = yes\n");
            if let Some(quota) = s.time_machine_max_size_gb {
                if quota > 0 {
                    out.push_str(&format!("   fruit:time machine max size = {}G\n", quota));
                }
            }
        }

        if s.shadow_copy {
            out.push_str("   shadow:snapdir = .snapshots\n");
            out.push_str("   shadow:sort = desc\n");
            out.push_str("   shadow:localtime = yes\n");
            out.push_str("   shadow:format = @GMT-%Y.%m.%d-%H.%M.%S\n");
        }

        if s.smb_encrypt {
            out.push_str("   smb encrypt = required\n");
        }

        out.push('\n');
    }

    out
}

pub fn load_samba_global_config() -> SambaGlobalConfig {
    let p = PathBuf::from("/var/lib/steveos/samba_global.json");
    if p.exists() {
        if let Ok(c) = fs::read_to_string(&p) {
            if let Ok(cfg) = serde_json::from_str::<SambaGlobalConfig>(&c) {
                return cfg;
            }
        }
    }

    SambaGlobalConfig {
        workgroup: "WORKGROUP".into(),
        server_string: "STEvE_OS NAS".into(),
        netbios_name: crate::network::get_hostname(),
        min_protocol: "SMB2_02".into(),
        max_protocol: "SMB3_11".into(),
        wsdd_enabled: true,
        multi_channel: true,
        aio_enabled: true,
    }
}

pub fn save_samba_global_config(cfg: &SambaGlobalConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    let _ = fs::create_dir_all("/var/lib/steveos");
    let _ = fs::write("/var/lib/steveos/samba_global.json", &json);
    let _ = fs::write("/tmp/steveos_samba_global.json", &json);
    let _ = reload_samba_service();
    Ok(())
}

// --------------------------------------------------------------------------
// VUE D'ENSEMBLE SAMBA & TÉLÉMÉTRIE
// --------------------------------------------------------------------------

pub fn get_samba_overview() -> SambaOverview {
    let is_active = Command::new("systemctl")
        .args(["is-active", "samba-smbd"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let wsdd_active = Command::new("systemctl")
        .args(["is-active", "samba-wsdd"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let avahi_active = Command::new("systemctl")
        .args(["is-active", "avahi-daemon"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let mut shares = load_samba_shares();
    for s in &mut shares {
        s.exists = std::path::Path::new(&s.path).exists();
    }

    let active_sessions = get_detailed_smb_sessions();
    let locked_files = get_detailed_smb_locks();
    let global_config = load_samba_global_config();

    let available_users: Vec<String> = crate::users::get_samba_users().into_iter().collect();

    SambaOverview {
        is_active,
        unit: "samba-smbd".into(),
        status_text: if is_active {
            "Actif (Partages SMB en ligne)".into()
        } else {
            "Inactif / Arrêté".into()
        },
        workgroup: global_config.workgroup.clone(),
        server_string: global_config.server_string.clone(),
        netbios_name: global_config.netbios_name.clone(),
        primary_lan_ip: crate::network::get_primary_lan_ip(),
        protocol_version: "SMB 3.1.1".into(),
        wsdd_active,
        avahi_active,
        shares,
        active_sessions,
        locked_files,
        global_config,
        available_users,
    }
}

pub fn get_detailed_smb_sessions() -> Vec<SambaSession> {
    let mut sessions = Vec::new();
    if let Ok(output) = Command::new("smbstatus").arg("-b").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Format typique smbstatus -b :
        // PID     Username     Group        Machine                        Protocol Version  Encryption           Signing
        // ------------------------------------------------------------------------------------------------------------------
        let mut in_data = false;
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("---") {
                in_data = true;
                continue;
            }
            if !in_data || trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 4 {
                let pid = parts[0].to_string();
                let user = parts[1].to_string();
                let group = parts[2].to_string();
                let machine = parts[3].to_string();
                let client_ip = if machine.contains('(') {
                    machine.trim_matches(|c| c == '(' || c == ')').to_string()
                } else {
                    machine.clone()
                };
                let protocol = if parts.len() >= 5 { parts[4].to_string() } else { "SMB3_11".into() };
                let encryption = if parts.len() >= 6 { parts[5].to_string() } else { "-".into() };
                let signing = if parts.len() >= 7 { parts[6].to_string() } else { "actif".into() };

                sessions.push(SambaSession {
                    pid,
                    user,
                    group,
                    machine,
                    client_ip,
                    protocol,
                    encryption,
                    signing,
                    share: "Global".into(),
                    connected_at: "Actif".into(),
                });
            }
        }
    }
    sessions
}

pub fn get_detailed_smb_locks() -> Vec<SambaLockedFile> {
    let mut locks = Vec::new();
    if let Ok(output) = Command::new("smbstatus").arg("-L").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Format typique smbstatus -L :
        // Pid          User(ID)   DenyMode   Access      R/W        Oplock           SharePath   Name   Time
        // --------------------------------------------------------------------------------------------------
        let mut in_data = false;
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("---") {
                in_data = true;
                continue;
            }
            if !in_data || trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 7 {
                let pid = parts[0].to_string();
                let user = parts[1].to_string();
                let access_mode = format!("{} / {}", parts.get(2).unwrap_or(&"-"), parts.get(4).unwrap_or(&"-"));
                let oplock = parts.get(5).unwrap_or(&"-").to_string();
                let share = parts.get(6).unwrap_or(&"-").to_string();
                let filename = parts.get(7..).map(|p| p.join(" ")).unwrap_or_else(|| "-".into());

                locks.push(SambaLockedFile {
                    pid,
                    user,
                    share,
                    filename,
                    access_mode,
                    oplock,
                });
            }
        }
    }
    locks
}

// --------------------------------------------------------------------------
// OPÉRATIONS CRUD SUR LES PARTAGES
// --------------------------------------------------------------------------

pub fn create_samba_share(req: CreateShareRequest) -> Result<SambaShare, String> {
    let clean_name = req.name.trim().to_lowercase().replace(' ', "-").replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "");
    if clean_name.is_empty() {
        return Err("Le nom du partage ne peut pas être vide.".into());
    }

    let mut shares = load_samba_shares();
    if shares.iter().any(|s| s.name.eq_ignore_ascii_case(&clean_name)) {
        return Err(format!("Un partage nommé '{}' existe déjà.", clean_name));
    }

    let clean_path = req.path.trim().to_string();
    if clean_path.is_empty() || !clean_path.starts_with('/') {
        return Err("Veuillez renseigner un chemin absolu valide (ex: /storage/data).".into());
    }

    // Création automatique du dossier cible si inexistant
    if !std::path::Path::new(&clean_path).exists() {
        let _ = fs::create_dir_all(&clean_path);
        let u = target_user();
        let _ = Command::new("chown").args(["-R", &format!("{}:storage", u), &clean_path]).output();
        let _ = Command::new("chmod").args(["2775", &clean_path]).output();
    }

    let share = SambaShare {
        id: clean_name.clone(),
        name: clean_name,
        path: clean_path.clone(),
        comment: req.comment.unwrap_or_else(|| "Partage réseau STEvE_OS".into()),
        read_only: req.read_only.unwrap_or(false),
        browseable: req.browseable.unwrap_or(true),
        guest_ok: req.guest_ok.unwrap_or(false),
        valid_users: req.valid_users.unwrap_or_default(),
        write_list: req.write_list.unwrap_or_default(),
        read_list: req.read_list.unwrap_or_default(),
        is_time_machine: req.is_time_machine.unwrap_or(false),
        time_machine_max_size_gb: req.time_machine_max_size_gb,
        recycle_bin: req.recycle_bin.unwrap_or(true),
        shadow_copy: req.shadow_copy.unwrap_or(false),
        smb_encrypt: req.smb_encrypt.unwrap_or(false),
        create_mask: req.create_mask.unwrap_or_else(|| "0664".into()),
        directory_mask: req.directory_mask.unwrap_or_else(|| "0775".into()),
        exists: std::path::Path::new(&clean_path).exists(),
    };

    shares.push(share.clone());
    save_samba_shares(&shares)?;

    Ok(share)
}

pub fn update_samba_share(id: &str, req: UpdateShareRequest) -> Result<SambaShare, String> {
    let mut shares = load_samba_shares();
    let idx = shares.iter().position(|s| s.id == id || s.name == id)
        .ok_or_else(|| format!("Partage '{}' introuvable.", id))?;

    if let Some(name) = req.name {
        let clean = name.trim().to_lowercase().replace(' ', "-").replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "");
        if !clean.is_empty() {
            shares[idx].name = clean;
        }
    }

    if let Some(path) = req.path {
        let clean = path.trim().to_string();
        if clean.starts_with('/') {
            shares[idx].path = clean;
        }
    }

    if let Some(comment) = req.comment {
        shares[idx].comment = comment;
    }
    if let Some(ro) = req.read_only {
        shares[idx].read_only = ro;
    }
    if let Some(br) = req.browseable {
        shares[idx].browseable = br;
    }
    if let Some(g) = req.guest_ok {
        shares[idx].guest_ok = g;
    }
    if let Some(vu) = req.valid_users {
        shares[idx].valid_users = vu;
    }
    if let Some(wl) = req.write_list {
        shares[idx].write_list = wl;
    }
    if let Some(rl) = req.read_list {
        shares[idx].read_list = rl;
    }
    if let Some(tm) = req.is_time_machine {
        shares[idx].is_time_machine = tm;
    }
    if let Some(quota) = req.time_machine_max_size_gb {
        shares[idx].time_machine_max_size_gb = Some(quota);
    }
    if let Some(rb) = req.recycle_bin {
        shares[idx].recycle_bin = rb;
    }
    if let Some(sc) = req.shadow_copy {
        shares[idx].shadow_copy = sc;
    }
    if let Some(enc) = req.smb_encrypt {
        shares[idx].smb_encrypt = enc;
    }
    if let Some(cm) = req.create_mask {
        shares[idx].create_mask = cm;
    }
    if let Some(dm) = req.directory_mask {
        shares[idx].directory_mask = dm;
    }

    shares[idx].exists = std::path::Path::new(&shares[idx].path).exists();
    let updated = shares[idx].clone();

    save_samba_shares(&shares)?;

    Ok(updated)
}

pub fn delete_samba_share(id: &str) -> Result<(), String> {
    let mut shares = load_samba_shares();
    let initial_len = shares.len();
    shares.retain(|s| s.id != id && s.name != id);

    if shares.len() == initial_len {
        return Err(format!("Partage '{}' introuvable.", id));
    }

    save_samba_shares(&shares)?;
    Ok(())
}

pub fn reload_samba_service() -> Result<String, String> {
    let _ = Command::new("sudo").args(["systemctl", "reload-or-restart", "samba-smbd"]).output();
    let _ = Command::new("smbcontrol").args(["smbd", "reload-config"]).output();
    Ok("Configuration Samba rechargée avec succès.".into())
}

pub fn get_samba_diagnostics() -> SambaDiagResult {
    let output_str;
    let mut is_valid = false;
    let mut errors = Vec::new();
    let mut loaded_services = Vec::new();

    if let Ok(out) = Command::new("testparm").args(["-s"]).output() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        output_str = format!("{}\n{}", stdout, stderr);

        if stdout.contains("Loaded services file OK") || stderr.contains("Loaded services file OK") {
            is_valid = true;
        }

        for line in output_str.lines() {
            let t = line.trim();
            if t.starts_with("ERROR") || t.starts_with("WARNING") {
                errors.push(t.to_string());
            } else if t.starts_with('[') && t.ends_with(']') && t != "[global]" {
                loaded_services.push(t.trim_matches(|c| c == '[' || c == ']').to_string());
            }
        }
    } else {
        output_str = "Impossible d'exécuter testparm sur le système.".into();
    }

    SambaDiagResult {
        is_valid,
        output: output_str,
        errors,
        loaded_services,
    }
}

pub fn disconnect_samba_session(pid: &str) -> Result<(), String> {
    let clean_pid = pid.trim();
    if clean_pid.is_empty() || !clean_pid.chars().all(|c| c.is_ascii_digit()) {
        return Err("PID invalide.".into());
    }

    let _ = Command::new("sudo").args(["kill", "-TERM", clean_pid]).output();
    Ok(())
}
