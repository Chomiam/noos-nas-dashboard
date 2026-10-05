fn target_user() -> String {
    crate::updates::target_user()
}
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpOverview {
    pub is_active: bool,
    pub unit: String,
    pub status_text: String,
    pub port: u16,
    pub fail2ban_active: bool,
    pub fail2ban_banned_count: u32,
    pub fail2ban_banned_ips: Vec<String>,
    pub primary_lan_ip: String,
    pub hostname: String,
    pub global_config: SftpGlobalConfig,
    pub shares: Vec<SftpShare>,
    pub allowed_users: Vec<SftpUserAccess>,
    pub active_sessions: Vec<SftpSession>,
    pub recent_logs: Vec<SftpLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpGlobalConfig {
    pub port: u16,
    pub permit_root_login: String, // "no", "prohibit-password", "yes"
    pub password_authentication: bool,
    pub pubkey_authentication: bool,
    pub default_chroot_dir: String, // "/mnt/storage/sftp" ou "%h"
    pub max_auth_tries: u32,
    pub idle_timeout_min: u32,
    pub subsystem_cmd: String,
}

impl Default for SftpGlobalConfig {
    fn default() -> Self {
        Self {
            port: 22,
            permit_root_login: "no".into(),
            password_authentication: true,
            pubkey_authentication: true,
            default_chroot_dir: "/mnt/storage/sftp".into(),
            max_auth_tries: 5,
            idle_timeout_min: 15,
            subsystem_cmd: "internal-sftp".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpShare {
    pub id: String,
    pub name: String,
    pub path: String,
    pub comment: String,
    pub read_only: bool,
    pub chroot_enforced: bool,
    pub allowed_users: Vec<String>,
    pub write_users: Vec<String>,
    pub exists: bool,
    pub quota_gb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpUserAccess {
    pub username: String,
    pub full_name: String,
    pub role: String,
    pub home_dir: String,
    pub shell: String,
    pub has_ssh_keys: bool,
    pub ssh_keys_count: usize,
    pub chroot_dir: Option<String>,
    pub is_sftp_enabled: bool,
    pub is_admin: bool,
    pub avatar_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpSession {
    pub pid: u32,
    pub user: String,
    pub client_ip: String,
    pub client_port: u16,
    pub login_time: String,
    pub session_type: String, // "sFTP (Transfert)" ou "SSH (Terminal)"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpLogEntry {
    pub timestamp: String,
    pub event_type: String, // "accepted", "failed", "closed", "banned"
    pub user: String,
    pub ip: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSftpShareRequest {
    pub id: Option<String>,
    pub name: String,
    pub path: String,
    pub comment: Option<String>,
    pub read_only: Option<bool>,
    pub chroot_enforced: Option<bool>,
    pub allowed_users: Option<Vec<String>>,
    pub write_users: Option<Vec<String>>,
    pub quota_gb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSftpGlobalRequest {
    pub port: Option<u16>,
    pub permit_root_login: Option<String>,
    pub password_authentication: Option<bool>,
    pub pubkey_authentication: Option<bool>,
    pub default_chroot_dir: Option<String>,
    pub max_auth_tries: Option<u32>,
    pub idle_timeout_min: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectSftpSessionRequest {
    pub pid: u32,
}

// =========================================================================
// GESTION DU RÉPERTOIRE DE CONFIGURATION & PERSISTANCE
// =========================================================================

#[allow(dead_code)]
fn resolve_sftp_dir() -> PathBuf {
    let var_lib_noos = Path::new("/var/lib/noos");
    let _ = fs::create_dir_all(var_lib_noos);

    if var_lib_noos.exists() {
        return var_lib_noos.to_path_buf();
    }
    PathBuf::from("/tmp")
}

fn get_sftp_shares_json_path() -> PathBuf {
    PathBuf::from("/var/lib/noos/sftp_shares.json")
}

fn get_sftp_global_json_path() -> PathBuf {
    PathBuf::from("/var/lib/noos/sftp_global.json")
}

#[allow(dead_code)]
fn get_sftp_shares_conf_path() -> PathBuf {
    PathBuf::from("/var/lib/noos/sftp_shares.conf")
}

pub fn load_sftp_global_config() -> SftpGlobalConfig {
    let path = get_sftp_global_json_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<SftpGlobalConfig>(&content) {
            return cfg;
        }
    }
    SftpGlobalConfig::default()
}

pub fn save_sftp_global_config(cfg: &SftpGlobalConfig) -> Result<(), String> {
    let path = get_sftp_global_json_path();
    let content = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Erreur sérialisation sftp_global.json: {}", e))?;
    fs::write(&path, content)
        .map_err(|e| format!("Erreur écriture sftp_global.json: {}", e))?;

    let shares = load_sftp_shares();
    save_sftp_shares_and_generate_conf(&shares, cfg)?;
    Ok(())
}

pub fn load_sftp_shares() -> Vec<SftpShare> {
    let path = get_sftp_shares_json_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(shares) = serde_json::from_str::<Vec<SftpShare>>(&content) {
            return shares;
        }
    }

    let primary_user = target_user();
    let default_shares = vec![
        SftpShare {
            id: "data".into(),
            name: "Données Générales NAS".into(),
            path: "/storage/data".into(),
            comment: "Dépôt central de données chiffré sFTP".into(),
            read_only: false,
            chroot_enforced: true,
            allowed_users: vec![primary_user.clone()],
            write_users: vec![primary_user.clone()],
            exists: Path::new("/storage/data").exists(),
            quota_gb: None,
        },
        SftpShare {
            id: "sftp-vault".into(),
            name: "Coffre-fort sFTP Chroot".into(),
            path: "/mnt/storage/sftp".into(),
            comment: "Espace isolé haute sécurité pour transferts distants".into(),
            read_only: false,
            chroot_enforced: true,
            allowed_users: vec![primary_user.clone()],
            write_users: vec![primary_user.clone()],
            exists: Path::new("/mnt/storage/sftp").exists(),
            quota_gb: None,
        },
    ];

    let _ = save_sftp_shares_and_generate_conf(&default_shares, &load_sftp_global_config());
    default_shares
}

pub fn is_user_shell_allowed(username: &str) -> bool {
    let primary = target_user();
    if username == "root" || username == primary || username == "chomiam" || username == "admin" {
        return true;
    }

    // 1. Vérification dans users-registry.json
    let registry_path = PathBuf::from("/var/lib/noos/users-registry.json");
    if let Ok(c) = fs::read_to_string(&registry_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&c) {
            if let Some(user_obj) = v.get("users").and_then(|u| u.get(username)) {
                if let Some(allow_sh) = user_obj.get("allow_shell").and_then(|b| b.as_bool()) {
                    if allow_sh {
                        return true;
                    }
                }
                if let Some(is_sftp_only) = user_obj.get("sftp_only").and_then(|b| b.as_bool()) {
                    if is_sftp_only {
                        return false;
                    }
                }
            }
        }
    }

    // 2. Vérification dans /etc/passwd
    if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 7 && parts[0] == username {
                let shell = parts[6];
                let is_nologin = shell.ends_with("nologin") || shell.ends_with("false");
                return !is_nologin && (shell.contains("sh") || shell.contains("bash") || shell.contains("zsh"));
            }
        }
    }

    false
}

/// Nettoie automatiquement /var/lib/noos/sftp_shares.conf pour supprimer tout bloc Match User
/// accidentel bloquant l'accès SSH des administrateurs ou utilisateurs shell.
pub fn heal_sshd_config() -> Result<bool, String> {
    let conf_path = Path::new("/var/lib/noos/sftp_shares.conf");
    if !conf_path.exists() {
        return Ok(false);
    }

    let content = match fs::read_to_string(conf_path) {
        Ok(c) => c,
        Err(e) => return Err(e.to_string()),
    };

    let mut modified = false;
    let mut sanitized_lines = Vec::new();
    let mut in_forbidden_match = false;

    let primary = target_user();
    let forbidden_users = ["root", "admin", "chomiam", primary.as_str()];

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Match User ") {
            let matched_user = trimmed.trim_start_matches("Match User ").trim();
            if forbidden_users.iter().any(|u| u.eq_ignore_ascii_case(matched_user))
                || is_user_shell_allowed(matched_user)
            {
                in_forbidden_match = true;
                modified = true;
                continue;
            } else {
                in_forbidden_match = false;
            }
        } else if trimmed.starts_with("Match ") {
            in_forbidden_match = false;
        }

        if in_forbidden_match {
            if trimmed.is_empty() {
                in_forbidden_match = false;
            }
            continue;
        }

        sanitized_lines.push(line);
    }

    if modified {
        let mut new_conf = sanitized_lines.join("\n");
        if !new_conf.ends_with('\n') {
            new_conf.push('\n');
        }
        let _ = fs::write(conf_path, new_conf);
        reload_sshd_service();
    }

    Ok(modified)
}

pub fn save_sftp_shares_and_generate_conf(
    shares: &[SftpShare],
    _global_cfg: &SftpGlobalConfig,
) -> Result<(), String> {
    let json_content = serde_json::to_string_pretty(shares)
        .map_err(|e| format!("Erreur sérialisation sftp_shares.json: {}", e))?;

    let _ = fs::create_dir_all("/var/lib/noos");
    let _ = fs::write("/var/lib/noos/sftp_shares.json", &json_content);

    let mut conf = String::new();
    conf.push_str("# =========================================================================\n");
    conf.push_str("# 📁 Noos NAS — Configuration Dynamique des Partages sFTP & Chroot\n");
    conf.push_str("# Généré automatiquement par le Dashboard Noos (Catppuccin Mocha)\n");
    conf.push_str("# =========================================================================\n\n");

    conf.push_str("Subsystem sftp internal-sftp\n\n");

    let mut user_rules: HashMap<String, (String, bool, bool)> = HashMap::new();

    for share in shares {
        if !Path::new(&share.path).exists() {
            let _ = fs::create_dir_all(&share.path);
        }

        for user in &share.allowed_users {
            let is_write = share.write_users.contains(user) && !share.read_only;
            let read_only = !is_write;
            user_rules.insert(user.clone(), (share.path.clone(), read_only, share.chroot_enforced));
        }
    }

    for (user, (path, read_only, chroot)) in user_rules {
        // IMPORTANT : Ne JAMAIS appliquer ForceCommand internal-sftp ou ChrootDirectory
        // aux utilisateurs possédant un accès shell (administrateurs, chomiam, root, utilisateurs shell).
        // Ils disposent déjà d'un accès sFTP natif via le sous-système global OpenSSH ("Subsystem sftp internal-sftp").
        // Forcer internal-sftp sur leur compte brise immédiatement leur session interactive SSH (Broken pipe).
        if is_user_shell_allowed(&user) {
            conf.push_str(&format!("# Compte shell '{}' : accès sFTP natif autorisé (session SSH terminal préservée)\n", user));
            continue;
        }

        conf.push_str(&format!("# Règle d'isolation sFTP pour compte restreint {}\n", user));
        conf.push_str(&format!("Match User {}\n", user));
        if chroot {
            conf.push_str(&format!("    ChrootDirectory {}\n", path));
        }
        if read_only {
            conf.push_str("    ForceCommand internal-sftp -R -u 002\n");
        } else {
            conf.push_str("    ForceCommand internal-sftp -u 002\n");
        }
        conf.push_str("    AllowTcpForwarding no\n");
        conf.push_str("    X11Forwarding no\n");
        conf.push_str("    PasswordAuthentication yes\n\n");
    }

    let _ = fs::write("/var/lib/noos/sftp_shares.conf", &conf);

    reload_sshd_service();

    Ok(())
}


pub fn create_sftp_share(req: CreateSftpShareRequest) -> Result<SftpShare, String> {
    let mut shares = load_sftp_shares();
    let id = req.id.unwrap_or_else(|| {
        req.name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    });

    if shares.iter().any(|s| s.id == id) {
        return Err(format!("Un partage avec l'ID '{}' existe déjà.", id));
    }

    let share = SftpShare {
        id,
        name: req.name,
        path: req.path.clone(),
        comment: req.comment.unwrap_or_default(),
        read_only: req.read_only.unwrap_or(false),
        chroot_enforced: req.chroot_enforced.unwrap_or(true),
        allowed_users: req.allowed_users.unwrap_or_default(),
        write_users: req.write_users.unwrap_or_default(),
        exists: Path::new(&req.path).exists(),
        quota_gb: req.quota_gb,
    };

    shares.push(share.clone());
    save_sftp_shares_and_generate_conf(&shares, &load_sftp_global_config())?;
    Ok(share)
}

pub fn update_sftp_share(id: &str, req: CreateSftpShareRequest) -> Result<SftpShare, String> {
    let mut shares = load_sftp_shares();
    let idx = shares
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Partage '{}' introuvable.", id))?;

    shares[idx].name = req.name;
    shares[idx].path = req.path.clone();
    if let Some(c) = req.comment { shares[idx].comment = c; }
    if let Some(ro) = req.read_only { shares[idx].read_only = ro; }
    if let Some(ch) = req.chroot_enforced { shares[idx].chroot_enforced = ch; }
    if let Some(au) = req.allowed_users { shares[idx].allowed_users = au; }
    if let Some(wu) = req.write_users { shares[idx].write_users = wu; }
    shares[idx].quota_gb = req.quota_gb;
    shares[idx].exists = Path::new(&req.path).exists();

    let updated = shares[idx].clone();
    save_sftp_shares_and_generate_conf(&shares, &load_sftp_global_config())?;
    Ok(updated)
}

pub fn delete_sftp_share(id: &str) -> Result<(), String> {
    let mut shares = load_sftp_shares();
    let before_len = shares.len();
    shares.retain(|s| s.id != id);

    if shares.len() == before_len {
        return Err(format!("Partage '{}' introuvable.", id));
    }

    save_sftp_shares_and_generate_conf(&shares, &load_sftp_global_config())?;
    Ok(())
}

// =========================================================================
// INSPECTION DES UTILISATEURS AUTORISÉS SSH / sFTP
// =========================================================================

pub fn get_sftp_allowed_users() -> Vec<SftpUserAccess> {
    let mut allowed_users = Vec::new();
    let primary_user = target_user();

    let registry_path = PathBuf::from("/var/lib/noos/users-registry.json");
    let registry_meta: HashMap<String, (String, String, bool)> = fs::read_to_string(&registry_path)
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .map(|v| {
            let mut map = HashMap::new();
            if let Some(users_obj) = v.get("users").and_then(|u| u.as_object()) {
                for (u, obj) in users_obj {
                    let fn_name = obj.get("full_name").and_then(|s| s.as_str()).unwrap_or(u).to_string();
                    let color = obj.get("avatar_color").and_then(|s| s.as_str()).unwrap_or("sapphire").to_string();
                    let allow_sh = obj.get("allow_shell").and_then(|b| b.as_bool()).unwrap_or(false);
                    map.insert(u.clone(), (fn_name, color, allow_sh));
                }
            }
            map
        })
        .unwrap_or_default();

    if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() < 7 {
                continue;
            }
            let username = parts[0].to_string();
            let uid: u32 = parts[2].parse().unwrap_or(9999);
            let gecos = parts[4].to_string();
            let home_dir = parts[5].to_string();
            let shell = parts[6].to_string();

            if uid < 1000 && username != "root" {
                continue;
            }
            if username.starts_with("nixbld") {
                continue;
            }

            let is_primary = username == primary_user;
            let (reg_fn, reg_color, reg_allow_shell) = registry_meta
                .get(&username)
                .cloned()
                .unwrap_or_else(|| (username.clone(), "sapphire".into(), false));

            let is_nologin = shell.ends_with("nologin") || shell.ends_with("false");
            let is_ssh_allowed = is_primary || (!is_nologin && (shell.contains("sh") || reg_allow_shell));

            if !is_ssh_allowed {
                continue;
            }

            let auth_keys_path = Path::new(&home_dir).join(".ssh/authorized_keys");
            let mut keys_count = 0;
            if let Ok(keys_content) = fs::read_to_string(&auth_keys_path) {
                keys_count = keys_content
                    .lines()
                    .filter(|l| {
                        let trim = l.trim();
                        !trim.is_empty() && !trim.starts_with('#')
                    })
                    .count();
            }

            let full_name = if !reg_fn.is_empty() {
                reg_fn
            } else if !gecos.is_empty() {
                gecos.split(',').next().unwrap_or(&username).to_string()
            } else {
                username.clone()
            };

            let is_admin = is_primary || username == "root";

            allowed_users.push(SftpUserAccess {
                username,
                full_name,
                role: if is_admin { "Administrateur NAS".into() } else { "Utilisateur Standard".into() },
                home_dir,
                shell,
                has_ssh_keys: keys_count > 0,
                ssh_keys_count: keys_count,
                chroot_dir: Some("/mnt/storage/sftp".into()),
                is_sftp_enabled: true,
                is_admin,
                avatar_color: if is_admin { "mauve".into() } else { reg_color },
            });
        }
    }

    // Trier les utilisateurs : utilisateur principal en premier, puis utilisateurs normaux par ordre alphabétique, root en dernier
    allowed_users.sort_by(|a, b| {
        if a.username == primary_user {
            std::cmp::Ordering::Less
        } else if b.username == primary_user {
            std::cmp::Ordering::Greater
        } else if a.username == "root" {
            std::cmp::Ordering::Greater
        } else if b.username == "root" {
            std::cmp::Ordering::Less
        } else {
            a.username.cmp(&b.username)
        }
    });

    allowed_users
}

// =========================================================================
// SURVEILLANCE DES SESSIONS SSH & sFTP EN TEMPS RÉEL
// =========================================================================

pub fn get_active_sftp_sessions() -> Vec<SftpSession> {
    let mut sessions = Vec::new();

    let mut conn_map: HashMap<u32, (String, u16)> = HashMap::new();
    if let Ok(out) = Command::new("ss").args(["-tnp", "state", "established", "( sport = :22 or sport = :2222 )"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 4 {
                let peer = cols[3];
                let (peer_ip, peer_port) = if let Some(idx) = peer.rfind(':') {
                    let ip = peer[..idx].trim_matches('[').trim_matches(']').to_string();
                    let port: u16 = peer[idx + 1..].parse().unwrap_or(0);
                    (ip, port)
                } else {
                    (peer.to_string(), 0)
                };

                for col in cols.iter().skip(4) {
                    if col.contains("pid=") {
                        if let Some(pos) = col.find("pid=") {
                            let sub = &col[pos + 4..];
                            let pid_str: String = sub.chars().take_while(|c| c.is_ascii_digit()).collect();
                            if let Ok(pid) = pid_str.parse::<u32>() {
                                conn_map.insert(pid, (peer_ip.clone(), peer_port));
                            }
                        }
                    }
                }
            }
        }
    }

    if let Ok(out) = Command::new("ps").args(["-eo", "pid,user,args"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("sshd:") && (line.contains("@") || line.contains("[priv]")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(pid) = parts[0].parse::<u32>() {
                        let proc_user = parts[1].to_string();
                        let args = parts[2..].join(" ");

                        let session_user = if let Some(idx) = args.find("sshd: ") {
                            let after = &args[idx + 6..];
                            let u: String = after.chars().take_while(|c| *c != '@' && *c != ' ' && *c != '[').collect();
                            if !u.is_empty() { u } else { proc_user }
                        } else {
                            proc_user
                        };

                        if session_user == "root" && args.contains("[listener]") {
                            continue;
                        }

                        let is_sftp = args.contains("internal-sftp") || args.contains("sftp-server") || args.contains("notty");
                        let (client_ip, client_port) = conn_map.get(&pid).cloned().unwrap_or_else(|| ("127.0.0.1".into(), 22));

                        sessions.push(SftpSession {
                            pid,
                            user: session_user,
                            client_ip,
                            client_port,
                            login_time: "En direct".into(),
                            session_type: if is_sftp { "sFTP (Transfert)".into() } else { "SSH (Terminal)".into() },
                        });
                    }
                }
            }
        }
    }

    let mut seen = HashSet::new();
    sessions.retain(|s| seen.insert(s.pid));

    sessions
}

// =========================================================================
// FAIL2BAN & LOGS DE SÉCURITÉ
// =========================================================================

pub fn get_sftp_fail2ban_info() -> (bool, u32, Vec<String>) {
    let is_active = Command::new("systemctl")
        .args(["is-active", "fail2ban"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    if !is_active {
        return (false, 0, Vec::new());
    }

    let mut banned_ips = Vec::new();
    if let Ok(out) = Command::new("fail2ban-client").args(["status", "sshd"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("Banned IP list:") {
                if let Some(idx) = line.find(':') {
                    let ips_str = &line[idx + 1..];
                    for ip in ips_str.split_whitespace() {
                        let t = ip.trim();
                        if !t.is_empty() {
                            banned_ips.push(t.to_string());
                        }
                    }
                }
            }
        }
    }

    let count = banned_ips.len() as u32;
    (true, count, banned_ips)
}

pub fn get_recent_sftp_logs() -> Vec<SftpLogEntry> {
    let mut logs = Vec::new();
    if let Ok(out) = Command::new("journalctl")
        .args(["-u", "sshd", "-n", "20", "--no-pager", "-o", "short-iso"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let timestamp = parts[0].to_string();
                let msg = parts[3..].join(" ");

                let mut event_type = "info".to_string();
                let mut user = "-".to_string();
                let mut ip = "-".to_string();

                if msg.contains("Accepted") {
                    event_type = "accepted".to_string();
                    if let Some(pos) = msg.find("for ") {
                        let sub = &msg[pos + 4..];
                        user = sub.chars().take_while(|c| *c != ' ').collect();
                    }
                    if let Some(pos) = msg.find("from ") {
                        let sub = &msg[pos + 5..];
                        ip = sub.chars().take_while(|c| *c != ' ').collect();
                    }
                } else if msg.contains("Failed") || msg.contains("Invalid") {
                    event_type = "failed".to_string();
                    if let Some(pos) = msg.find("user ") {
                        let sub = &msg[pos + 5..];
                        user = sub.chars().take_while(|c| *c != ' ').collect();
                    }
                    if let Some(pos) = msg.find("from ") {
                        let sub = &msg[pos + 5..];
                        ip = sub.chars().take_while(|c| *c != ' ').collect();
                    }
                } else if msg.contains("Closed") || msg.contains("Disconnected") {
                    event_type = "closed".to_string();
                }

                logs.push(SftpLogEntry {
                    timestamp,
                    event_type,
                    user,
                    ip,
                    message: msg,
                });
            }
        }
    }
    logs.reverse();
    logs.truncate(8);
    logs
}

pub fn reload_sshd_service() -> bool {
    let res = Command::new("systemctl")
        .args(["reload-or-restart", "sshd"])
        .output();
    res.map(|o| o.status.success()).unwrap_or(false)
}

pub fn disconnect_sftp_session(pid: u32) -> Result<(), String> {
    if pid <= 1 {
        return Err("PID système non autorisé".into());
    }
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .map_err(|e| format!("Erreur kill: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Impossible de terminer le PID {}", pid))
    }
}

// =========================================================================
// SYNTHÈSE GLOBALE SFTP
// =========================================================================

pub fn get_sftp_overview() -> SftpOverview {
    // Auto-guérison immédiate des blocages SSH résiduels à chaque interrogation
    let _ = heal_sshd_config();

    let is_active = Command::new("systemctl")
        .args(["is-active", "sshd"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let (fail2ban_active, fail2ban_banned_count, fail2ban_banned_ips) = get_sftp_fail2ban_info();
    let global_config = load_sftp_global_config();
    let mut shares = load_sftp_shares();
    for share in &mut shares {
        share.exists = Path::new(&share.path).exists();
    }

    let allowed_users = get_sftp_allowed_users();
    let active_sessions = get_active_sftp_sessions();
    let recent_logs = get_recent_sftp_logs();

    let primary_lan_ip = crate::network::get_primary_lan_ip();
    let hostname = crate::network::get_hostname();

    SftpOverview {
        is_active,
        unit: "sshd".into(),
        status_text: if is_active {
            format!("Actif (OpenSSH écoute sur le port {})", global_config.port)
        } else {
            "Inactif / Arrêté".into()
        },
        port: global_config.port,
        fail2ban_active,
        fail2ban_banned_count,
        fail2ban_banned_ips,
        primary_lan_ip,
        hostname,
        global_config,
        shares,
        allowed_users,
        active_sessions,
        recent_logs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_user_shell_allowed_for_root_and_admin() {
        assert!(is_user_shell_allowed("root"));
        assert!(is_user_shell_allowed("admin"));
        assert!(is_user_shell_allowed("chomiam"));
    }

    #[test]
    fn test_heal_sshd_config_sanitizes_admin_match_block() {
        let content = r#"
# Règle générale
Subsystem sftp internal-sftp

# Règle sFTP pour l'utilisateur chomiam
Match User chomiam
    ChrootDirectory /mnt/storage/data
    ForceCommand internal-sftp -u 002
    AllowTcpForwarding no

# Règle sFTP pour l'utilisateur restreint invité
Match User invite
    ChrootDirectory /mnt/storage/invite
    ForceCommand internal-sftp -R -u 002
"#;
        let forbidden = ["root", "admin", "chomiam"];
        let mut lines = Vec::new();
        let mut in_forbidden = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Match User ") {
                let user = trimmed.trim_start_matches("Match User ").trim();
                if forbidden.contains(&user) {
                    in_forbidden = true;
                    continue;
                } else {
                    in_forbidden = false;
                }
            } else if trimmed.starts_with("Match ") {
                in_forbidden = false;
            }
            if in_forbidden {
                if trimmed.is_empty() {
                    in_forbidden = false;
                }
                continue;
            }
            lines.push(line);
        }

        let result = lines.join("\n");
        assert!(!result.contains("Match User chomiam"));
        assert!(result.contains("Match User invite"));
        assert!(result.contains("Subsystem sftp internal-sftp"));
    }
}

