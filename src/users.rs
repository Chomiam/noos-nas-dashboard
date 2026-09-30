use axum::{
    extract::{Path, Query},
    http::HeaderMap,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path as StdPath, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub full_name: String,
    pub email: String,
    pub avatar_color: String,
    pub role: String,
    pub is_admin: bool,
    pub is_system: bool,
    pub locked: bool,
    pub shell: String,
    pub home_dir: String,
    pub disk_usage_bytes: u64,
    pub groups: Vec<String>,
    pub samba_enabled: bool,
    pub ssh_keys_count: usize,
    pub active_sessions_count: usize,
    pub created_at: Option<u64>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub gid: u32,
    pub is_system: bool,
    pub description: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserRegistryMetadata {
    pub full_name: String,
    pub email: String,
    pub avatar_color: String,
    pub role: String,
    pub samba_enabled: bool,
    pub allow_shell: bool,
    pub created_at: u64,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsersRegistry {
    pub users: HashMap<String, UserRegistryMetadata>,
    pub custom_group_descriptions: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub full_name: Option<String>,
    pub email: Option<String>,
    pub avatar_color: Option<String>,
    pub password: String,
    pub role: Option<String>,
    pub groups: Option<Vec<String>>,
    pub allow_shell: Option<bool>,
    pub shell: Option<String>,
    pub samba_access: Option<bool>,
    pub create_dedicated_share: Option<bool>,
    pub ssh_keys: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub full_name: Option<String>,
    pub email: Option<String>,
    pub avatar_color: Option<String>,
    pub role: Option<String>,
    pub groups: Option<Vec<String>>,
    pub allow_shell: Option<bool>,
    pub shell: Option<String>,
    pub samba_access: Option<bool>,
    pub ssh_keys: Option<Vec<String>>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub password: String,
    pub revoke_sessions: Option<bool>,
    pub update_samba: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteUserParams {
    pub delete_home: Option<bool>,
    pub delete_share: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
    pub members: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupMembersRequest {
    pub members: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SecurityAuditReport {
    pub total_users: usize,
    pub admin_count: usize,
    pub storage_count: usize,
    pub docker_count: usize,
    pub kvm_count: usize,
    pub locked_count: usize,
    pub nologin_count: usize,
    pub warnings: Vec<String>,
    pub users: Vec<UserInfo>,
}

// Emplacement du registre persistant
fn get_registry_path() -> PathBuf {
    let var_lib = StdPath::new("/var/lib/steveos");
    if var_lib.exists() || std::fs::create_dir_all(var_lib).is_ok() {
        return var_lib.join("users-registry.json");
    }
    PathBuf::from("/run/steveos-users-registry.json")
}

fn load_registry() -> UsersRegistry {
    let path = get_registry_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(reg) = serde_json::from_str::<UsersRegistry>(&content) {
            return reg;
        }
    }
    UsersRegistry::default()
}

fn save_registry(registry: &UsersRegistry) {
    let path = get_registry_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(registry) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

// Validation POSIX stricte : nom minuscules, chiffres, tirets, underscores
pub fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() || name.len() > 32 {
        return false;
    }
    let bytes = name.as_bytes();
    // Doit commencer par une lettre minuscule ou un underscore
    if !bytes[0].is_ascii_lowercase() && bytes[0] != b'_' {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

// Groupes réservés système protégés contre la suppression ou altération
pub fn is_protected_system_group(group: &str) -> bool {
    matches!(
        group,
        "root"
            | "wheel"
            | "sudo"
            | "users"
            | "storage"
            | "docker"
            | "podman"
            | "kvm"
            | "libvirtd"
            | "video"
            | "render"
            | "disk"
            | "audio"
            | "input"
            | "networkmanager"
            | "systemd-journal"
            | "nogroup"
            | "nobody"
    )
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Lecture des comptes verrouillés dans /etc/shadow
fn get_locked_users() -> HashSet<String> {
    let mut locked = HashSet::new();
    if let Ok(content) = std::fs::read_to_string("/etc/shadow") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                let user = parts[0].trim();
                let hash = parts[1].trim();
                if hash.starts_with('!') || hash.starts_with('*') || hash.is_empty() {
                    locked.insert(user.to_string());
                }
            }
        }
    }
    locked
}

// Lecture des utilisateurs enregistrés dans Samba (pdbedit -L)
fn get_samba_users() -> HashSet<String> {
    let mut smb_users = HashSet::new();
    if let Ok(out) = Command::new("pdbedit").arg("-L").output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some(user) = line.split(':').next() {
                    let u = user.trim();
                    if !u.is_empty() {
                        smb_users.insert(u.to_string());
                    }
                }
            }
        }
    }
    smb_users
}

// Lecture des groupes d'un utilisateur (id -Gn)
fn get_user_groups(username: &str) -> Vec<String> {
    if let Ok(out) = Command::new("id").args(["-Gn", username]).output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            return text
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
        }
    }
    Vec::new()
}

// Calcul de la taille du répertoire personnel
fn get_directory_size(path: &str) -> u64 {
    let p = StdPath::new(path);
    if !p.exists() {
        return 0;
    }
    // Appel rapide 'du -sb <path>' avec délai
    if let Ok(out) = Command::new("du").args(["-sb", path]).output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(first) = text.split_whitespace().next() {
                if let Ok(bytes) = first.parse::<u64>() {
                    return bytes;
                }
            }
        }
    }
    0
}

// Compter les clés SSH publiques autorisées
fn count_ssh_keys(home_dir: &str) -> usize {
    let keys_file = StdPath::new(home_dir).join(".ssh/authorized_keys");
    if let Ok(content) = std::fs::read_to_string(keys_file) {
        return content
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with('#')
            })
            .count();
    }
    0
}

// Liste de tous les utilisateurs gérés
pub async fn list_all_users() -> Vec<UserInfo> {
    let registry = load_registry();
    let locked_set = get_locked_users();
    let samba_set = get_samba_users();
    let active_sessions = auth::get_all_active_sessions().await;

    let mut session_counts: HashMap<String, usize> = HashMap::new();
    for s in active_sessions {
        *session_counts.entry(s.username).or_insert(0) += 1;
    }

    let mut users = Vec::new();
    let passwd_content = std::fs::read_to_string("/etc/passwd").unwrap_or_default();

    for line in passwd_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 7 {
            continue;
        }

        let username = parts[0].to_string();
        let uid: u32 = parts[2].parse().unwrap_or(9999);
        let gid: u32 = parts[3].parse().unwrap_or(9999);
        let gecos = parts[4].to_string();
        let home_dir = parts[5].to_string();
        let shell = parts[6].to_string();

        let is_system = uid < 1000 && username != "root";
        // On masque les démons système purs (UID < 1000 hors root)
        if is_system {
            continue;
        }

        let groups = get_user_groups(&username);
        let is_admin = username == "root" || groups.iter().any(|g| g == "wheel" || g == "sudo");
        let is_locked = locked_set.contains(&username);
        let samba_enabled = samba_set.contains(&username);
        let ssh_keys = count_ssh_keys(&home_dir);
        let disk_usage = get_directory_size(&home_dir);
        let active_count = *session_counts.get(&username).unwrap_or(&0);

        let meta = registry.users.get(&username);

        let full_name = meta
            .map(|m| m.full_name.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if !gecos.is_empty() {
                    gecos.split(',').next().unwrap_or(&username).to_string()
                } else {
                    username.clone()
                }
            });

        let email = meta.map(|m| m.email.clone()).unwrap_or_default();
        let avatar_color = meta
            .map(|m| m.avatar_color.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if is_admin {
                    "mauve".to_string()
                } else if samba_enabled {
                    "teal".to_string()
                } else {
                    "sapphire".to_string()
                }
            });

        let role = meta
            .map(|m| m.role.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if is_admin {
                    "admin".to_string()
                } else if shell.ends_with("nologin") || shell.ends_with("false") {
                    "share_only".to_string()
                } else {
                    "standard".to_string()
                }
            });

        let created_at = meta.map(|m| m.created_at);
        let notes = meta.map(|m| m.notes.clone()).unwrap_or_default();

        users.push(UserInfo {
            username,
            uid,
            gid,
            full_name,
            email,
            avatar_color,
            role,
            is_admin,
            is_system: uid < 1000,
            locked: is_locked,
            shell,
            home_dir,
            disk_usage_bytes: disk_usage,
            groups,
            samba_enabled,
            ssh_keys_count: ssh_keys,
            active_sessions_count: active_count,
            created_at,
            notes,
        });
    }

    // Tri : Administrateurs d'abord, puis ordre alphabétique
    users.sort_by(|a, b| {
        if a.is_admin != b.is_admin {
            b.is_admin.cmp(&a.is_admin)
        } else {
            a.username.cmp(&b.username)
        }
    });

    users
}

// Liste de tous les groupes
pub fn list_all_groups() -> Vec<GroupInfo> {
    let registry = load_registry();
    let mut groups = Vec::new();
    let group_content = std::fs::read_to_string("/etc/group").unwrap_or_default();

    for line in group_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 3 {
            continue;
        }

        let name = parts[0].to_string();
        let gid: u32 = parts[2].parse().unwrap_or(9999);
        let explicit_members: Vec<String> = if parts.len() >= 4 && !parts[3].trim().is_empty() {
            parts[3].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        } else {
            Vec::new()
        };

        let is_system = is_protected_system_group(&name) || gid < 1000;

        let description = registry
            .custom_group_descriptions
            .get(&name)
            .cloned()
            .unwrap_or_else(|| match name.as_str() {
                "wheel" => "Administrateurs système avec accès root et sudo".to_string(),
                "storage" => "Accès complet aux partages et disques du NAS (/mnt/storage)".to_string(),
                "docker" => "Contrôle des conteneurs Docker et accès au daemon".to_string(),
                "kvm" | "libvirtd" => "Gestion des machines virtuelles et hyperviseur QEMU".to_string(),
                "video" | "render" => "Accès matériel direct au GPU pour le transcodage vidéo".to_string(),
                "users" => "Groupe principal par défaut pour tous les utilisateurs standard".to_string(),
                "disk" => "Accès direct aux disques et périphériques de stockage bruts".to_string(),
                "networkmanager" => "Configuration des interfaces réseau et connexions".to_string(),
                _ => {
                    if is_system {
                        "Groupe système Linux".to_string()
                    } else {
                        "Groupe personnalisé NAS".to_string()
                    }
                }
            });

        // Filtrer les groupes pertinents pour le NAS (ignorer les démons internes sans membre)
        if !is_system || explicit_members.len() > 0 || matches!(name.as_str(), "wheel" | "storage" | "docker" | "kvm" | "libvirtd" | "video" | "render" | "users") {
            groups.push(GroupInfo {
                name,
                gid,
                is_system,
                description,
                members: explicit_members,
            });
        }
    }

    groups.sort_by(|a, b| {
        if a.is_system != b.is_system {
            a.is_system.cmp(&b.is_system)
        } else {
            a.name.cmp(&b.name)
        }
    });

    groups
}

// ---------------------------------------------------------------------------
// HANDLERS AXUM REST
// ---------------------------------------------------------------------------

// GET /api/users
pub async fn handle_users_list(headers: HeaderMap) -> Response {
    let session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    let users = list_all_users().await;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "current_user": session.username,
            "users": users
        })),
    )
        .into_response()
}

// POST /api/users
pub async fn handle_users_create(headers: HeaderMap, Json(body): Json<CreateUserRequest>) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    let username = body.username.trim().to_lowercase();
    if !is_valid_identifier(&username) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Nom d'utilisateur invalide. Utilisez 1 à 32 caractères minuscules, chiffres, tirets ou underscores (commençant par une lettre)."
            })),
        )
            .into_response();
    }

    if is_protected_system_group(&username) || username == "root" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Ce nom d'utilisateur est réservé pour le système."
            })),
        )
            .into_response();
    }

    // Vérifier si l'utilisateur existe déjà
    if let Ok(out) = Command::new("id").arg(&username).output() {
        if out.status.success() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("L'utilisateur '{}' existe déjà sur le système.", username)
                })),
            )
                .into_response();
        }
    }

    if body.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Le mot de passe doit comporter au moins 6 caractères."
            })),
        )
            .into_response();
    }

    // Détermination du shell
    let allow_shell = body.allow_shell.unwrap_or(false);
    let shell = if allow_shell {
        body.shell.unwrap_or_else(|| {
            if StdPath::new("/run/current-system/sw/bin/fish").exists() {
                "/run/current-system/sw/bin/fish".to_string()
            } else {
                "/run/current-system/sw/bin/bash".to_string()
            }
        })
    } else {
        if StdPath::new("/run/current-system/sw/bin/nologin").exists() {
            "/run/current-system/sw/bin/nologin".to_string()
        } else {
            "/bin/false".to_string()
        }
    };

    // Préparation des groupes
    let mut initial_groups: Vec<String> = body.groups.unwrap_or_default();
    // Par défaut, donner le groupe storage pour tout compte sur le NAS
    if !initial_groups.contains(&"storage".to_string()) && StdPath::new("/mnt/storage").exists() {
        initial_groups.push("storage".to_string());
    }
    // Nettoyage des doublons
    initial_groups.retain(|g| is_valid_identifier(g));

    // Exécution de useradd
    let mut useradd_cmd = Command::new("useradd");
    useradd_cmd
        .arg("-m")
        .arg("-s")
        .arg(&shell)
        .arg("-g")
        .arg("users");

    if !initial_groups.is_empty() {
        useradd_cmd.arg("-G").arg(initial_groups.join(","));
    }

        // Mise à jour des clés SSH si spécifiées
    if let Some(ref keys) = body.ssh_keys {
        let home_path = format!("/home/{}", username);
        let ssh_dir = format!("{}/.ssh", home_path);
        let _ = std::fs::create_dir_all(&ssh_dir);
        let _ = Command::new("chmod").args(["0700", &ssh_dir]).status();
        let _ = Command::new("chown")
            .args([format!("{}:users", username).as_str(), &ssh_dir])
            .status();

        let auth_keys_path = format!("{}/authorized_keys", ssh_dir);
        let content = keys.join("\n") + "\n";
        if std::fs::write(&auth_keys_path, content).is_ok() {
            let _ = Command::new("chmod").args(["0600", &auth_keys_path]).status();
            let _ = Command::new("chown")
                .args([format!("{}:users", username).as_str(), &auth_keys_path])
                .status();
        }
    }

    if let Some(ref fn_str) = body.full_name {
        if !fn_str.trim().is_empty() {
            useradd_cmd.arg("-c").arg(fn_str.trim());
        }
    }

    useradd_cmd.arg(&username);

    match useradd_cmd.output() {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Échec de la création du compte Linux : {}", err)
                })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Impossible d'exécuter useradd : {}", e)
                })),
            )
                .into_response();
        }
    }

    // Définition du mot de passe via flux standard stdin vers chpasswd
    if let Ok(mut child) = Command::new("chpasswd")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let payload = format!("{}:{}\n", username, body.password);
            let _ = stdin.write_all(payload.as_bytes());
        }
        let _ = child.wait();
    }

    // Permissions strictes 0750 sur le répertoire personnel
    let home_path = format!("/home/{}", username);
    let _ = Command::new("chmod").args(["0750", &home_path]).status();

    // Configuration Samba si demandée
    let samba_access = body.samba_access.unwrap_or(true);
    if samba_access {
        if let Ok(mut child) = Command::new("smbpasswd")
            .args(["-s", "-a", &username])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let payload = format!("{}\n{}\n", body.password, body.password);
                let _ = stdin.write_all(payload.as_bytes());
            }
            let _ = child.wait();
        }
    }

    // Création d'un partage dédié dans /mnt/storage/shares/<username> si demandé
    if body.create_dedicated_share.unwrap_or(false) {
        let user_share = format!("/mnt/storage/shares/{}", username);
        let _ = std::fs::create_dir_all(&user_share);
        let _ = Command::new("chown")
            .args([format!("{}:storage", username).as_str(), &user_share])
            .status();
        let _ = Command::new("chmod").args(["2770", &user_share]).status();
    }

    // Injection des clés SSH si spécifiées
    if let Some(ref keys) = body.ssh_keys {
        if !keys.is_empty() {
            let ssh_dir = format!("{}/.ssh", home_path);
            let _ = std::fs::create_dir_all(&ssh_dir);
            let _ = Command::new("chmod").args(["0700", &ssh_dir]).status();
            let _ = Command::new("chown")
                .args([format!("{}:users", username).as_str(), &ssh_dir])
                .status();

            let auth_keys_path = format!("{}/authorized_keys", ssh_dir);
            let content = keys.join("\n") + "\n";
            if std::fs::write(&auth_keys_path, content).is_ok() {
                let _ = Command::new("chmod").args(["0600", &auth_keys_path]).status();
                let _ = Command::new("chown")
                    .args([format!("{}:users", username).as_str(), &auth_keys_path])
                    .status();
            }
        }
    }

    // Sauvegarde des métadonnées dans le registre persistant
    let mut reg = load_registry();
    reg.users.insert(
        username.clone(),
        UserRegistryMetadata {
            full_name: body.full_name.unwrap_or_else(|| username.clone()),
            email: body.email.unwrap_or_default(),
            avatar_color: body.avatar_color.unwrap_or_else(|| "teal".to_string()),
            role: body.role.unwrap_or_else(|| "standard".to_string()),
            samba_enabled: samba_access,
            allow_shell,
            created_at: now_secs(),
            notes: String::new(),
        },
    );
    save_registry(&reg);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Utilisateur '{}' créé avec succès.", username),
            "username": username
        })),
    )
        .into_response()
}

// GET /api/users/:username
pub async fn handle_users_get(Path(username): Path<String>, headers: HeaderMap) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    let users = list_all_users().await;
    if let Some(user) = users.into_iter().find(|u| u.username == username) {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "user": user
            })),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Utilisateur '{}' introuvable.", username)
            })),
        )
            .into_response()
    }
}

// PUT /api/users/:username
pub async fn handle_users_update(
    Path(username): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateUserRequest>,
) -> Response {
    let session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    // Protection anti-auto-éviction : un admin ne peut pas retirer son propre groupe wheel
    if session.username == username {
        if let Some(ref req_groups) = body.groups {
            if !req_groups.contains(&"wheel".to_string()) && !req_groups.contains(&"sudo".to_string()) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "success": false,
                        "error": "Sécurité : vous ne pouvez pas retirer votre propre compte du groupe administrateur (wheel)."
                    })),
                )
                    .into_response();
            }
        }
    }

    // Mise à jour des groupes si spécifiés
    if let Some(ref new_groups) = body.groups {
        let valid_groups: Vec<&str> = new_groups
            .iter()
            .map(|s| s.as_str())
            .filter(|g| is_valid_identifier(g))
            .collect();

        let _ = Command::new("usermod")
            .args(["-G", &valid_groups.join(","), &username])
            .status();
    }

    // Mise à jour du shell
    if let Some(allow_shell) = body.allow_shell {
        let new_shell = if allow_shell {
            body.shell.clone().unwrap_or_else(|| {
                if StdPath::new("/run/current-system/sw/bin/fish").exists() {
                    "/run/current-system/sw/bin/fish".to_string()
                } else {
                    "/run/current-system/sw/bin/bash".to_string()
                }
            })
        } else {
            if StdPath::new("/run/current-system/sw/bin/nologin").exists() {
                "/run/current-system/sw/bin/nologin".to_string()
            } else {
                "/bin/false".to_string()
            }
        };
        let _ = Command::new("usermod")
            .args(["-s", &new_shell, &username])
            .status();
    }

    // Mise à jour du nom complet
        // Mise à jour des clés SSH si spécifiées
    if let Some(ref keys) = body.ssh_keys {
        let home_path = format!("/home/{}", username);
        let ssh_dir = format!("{}/.ssh", home_path);
        let _ = std::fs::create_dir_all(&ssh_dir);
        let _ = Command::new("chmod").args(["0700", &ssh_dir]).status();
        let _ = Command::new("chown")
            .args([format!("{}:users", username).as_str(), &ssh_dir])
            .status();

        let auth_keys_path = format!("{}/authorized_keys", ssh_dir);
        let content = keys.join("\n") + "\n";
        if std::fs::write(&auth_keys_path, content).is_ok() {
            let _ = Command::new("chmod").args(["0600", &auth_keys_path]).status();
            let _ = Command::new("chown")
                .args([format!("{}:users", username).as_str(), &auth_keys_path])
                .status();
        }
    }

    if let Some(ref fn_str) = body.full_name {
        let _ = Command::new("usermod")
            .args(["-c", fn_str.trim(), &username])
            .status();
    }

    // Mise à jour de l'accès Samba
    if let Some(samba_access) = body.samba_access {
        if samba_access {
            let _ = Command::new("smbpasswd").args(["-e", &username]).status();
        } else {
            let _ = Command::new("smbpasswd").args(["-d", &username]).status();
        }
    }

    // Mise à jour des métadonnées
    let mut reg = load_registry();
    let entry = reg.users.entry(username.clone()).or_insert_with(|| UserRegistryMetadata {
        full_name: username.clone(),
        avatar_color: "sapphire".to_string(),
        created_at: now_secs(),
        ..Default::default()
    });

    if let Some(f) = body.full_name {
        entry.full_name = f;
    }
    if let Some(e) = body.email {
        entry.email = e;
    }
    if let Some(a) = body.avatar_color {
        entry.avatar_color = a;
    }
    if let Some(r) = body.role {
        entry.role = r;
    }
    if let Some(s) = body.samba_access {
        entry.samba_enabled = s;
    }
    if let Some(sh) = body.allow_shell {
        entry.allow_shell = sh;
    }
    if let Some(n) = body.notes {
        entry.notes = n;
    }

    save_registry(&reg);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Utilisateur '{}' mis à jour avec succès.", username)
        })),
    )
        .into_response()
}

// POST /api/users/:username/password
pub async fn handle_users_change_password(
    Path(username): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ChangePasswordRequest>,
) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin || s.username == username => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges insuffisants."
                })),
            )
                .into_response();
        }
    };

    if body.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Le mot de passe doit comporter au moins 6 caractères."
            })),
        )
            .into_response();
    }

    // Mise à jour Linux via chpasswd
    let mut success = false;
    if let Ok(mut child) = Command::new("chpasswd")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let payload = format!("{}:{}\n", username, body.password);
            let _ = stdin.write_all(payload.as_bytes());
        }
        if let Ok(status) = child.wait() {
            success = status.success();
        }
    }

    if !success {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": "Échec de l'application du mot de passe système."
            })),
        )
            .into_response();
    }

    // Synchronisation Samba si demandée
    if body.update_samba.unwrap_or(true) {
        if let Ok(mut child) = Command::new("smbpasswd")
            .args(["-s", &username])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let payload = format!("{}\n{}\n", body.password, body.password);
                let _ = stdin.write_all(payload.as_bytes());
            }
            let _ = child.wait();
        }
    }

    // Révocation des sessions actives si demandée
    if body.revoke_sessions.unwrap_or(true) {
        auth::revoke_user_sessions(&username).await;
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Mot de passe mis à jour avec succès pour '{}'.", username)
        })),
    )
        .into_response()
}

// POST /api/users/:username/toggle-lock
pub async fn handle_users_toggle_lock(Path(username): Path<String>, headers: HeaderMap) -> Response {
    let session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    if session.username == username {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Sécurité : vous ne pouvez pas verrouiller votre propre compte administrateur."
            })),
        )
            .into_response();
    }

    if username == "root" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Le compte root système ne peut pas être verrouillé depuis cette interface."
            })),
        )
            .into_response();
    }

    let locked_set = get_locked_users();
    let is_currently_locked = locked_set.contains(&username);

    let (cmd_arg, new_status_msg) = if is_currently_locked {
        ("-U", "déverrouillé")
    } else {
        ("-L", "verrouillé")
    };

    match Command::new("usermod").args([cmd_arg, &username]).status() {
        Ok(s) if s.success() => {
            if !is_currently_locked {
                // Purger les sessions dès le verrouillage
                auth::revoke_user_sessions(&username).await;
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "locked": !is_currently_locked,
                    "message": format!("Compte '{}' {} avec succès.", username, new_status_msg)
                })),
            )
                .into_response()
        }
        Ok(_) | Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": "Échec de l'exécution de usermod pour modifier l'état de verrouillage."
            })),
        )
            .into_response(),
    }
}

// DELETE /api/users/:username
pub async fn handle_users_delete(
    Path(username): Path<String>,
    Query(params): Query<DeleteUserParams>,
    headers: HeaderMap,
) -> Response {
    let session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    if session.username == username {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Sécurité : vous ne pouvez pas supprimer votre propre compte connecté."
            })),
        )
            .into_response();
    }

    if username == "root" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Le compte root système ne peut pas être supprimé."
            })),
        )
            .into_response();
    }

    // Révoquer immédiatement toutes les sessions
    auth::revoke_user_sessions(&username).await;

    // Supprimer de Samba
    let _ = Command::new("smbpasswd").args(["-x", &username]).status();

    // Suppression Linux
    let mut userdel_cmd = Command::new("userdel");
    if params.delete_home.unwrap_or(false) {
        userdel_cmd.arg("-r");
    }
    userdel_cmd.arg(&username);

    match userdel_cmd.output() {
        Ok(out) if out.status.success() => {
            // Nettoyer du registre
            let mut reg = load_registry();
            reg.users.remove(&username);
            save_registry(&reg);

            // Supprimer le partage dédié si demandé
            if params.delete_share.unwrap_or(false) {
                let share_path = format!("/mnt/storage/shares/{}", username);
                let _ = std::fs::remove_dir_all(&share_path);
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "message": format!("Utilisateur '{}' supprimé avec succès.", username)
                })),
            )
                .into_response()
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Échec de userdel : {}", err)
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Impossible d'exécuter userdel : {}", e)
            })),
        )
            .into_response(),
    }
}

// GET /api/groups
pub async fn handle_groups_list(headers: HeaderMap) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    let groups = list_all_groups();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "groups": groups
        })),
    )
        .into_response()
}

// POST /api/groups
pub async fn handle_groups_create(headers: HeaderMap, Json(body): Json<CreateGroupRequest>) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    let group_name = body.name.trim().to_lowercase();
    if !is_valid_identifier(&group_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Nom de groupe invalide. Utilisez 1 à 32 caractères minuscules, chiffres, tirets ou underscores."
            })),
        )
            .into_response();
    }

    match Command::new("groupadd").arg(&group_name).output() {
        Ok(out) if out.status.success() => {
            // Ajouter les membres initiaux si spécifiés
            if let Some(members) = body.members {
                for m in members {
                    if is_valid_identifier(&m) {
                        let _ = Command::new("gpasswd").args(["-a", &m, &group_name]).status();
                    }
                }
            }

            // Enregistrer la description si fournie
            if let Some(desc) = body.description {
                let mut reg = load_registry();
                reg.custom_group_descriptions.insert(group_name.clone(), desc);
                save_registry(&reg);
            }

            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "success": true,
                    "message": format!("Groupe '{}' créé avec succès.", group_name),
                    "group": group_name
                })),
            )
                .into_response()
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Impossible de créer le groupe : {}", err)
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Erreur système groupadd : {}", e)
            })),
        )
            .into_response(),
    }
}

// POST /api/groups/:group/members
pub async fn handle_groups_update_members(
    Path(group): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateGroupMembersRequest>,
) -> Response {
    let session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    if group == "wheel" && !body.members.contains(&session.username) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Sécurité : vous ne pouvez pas retirer votre propre compte du groupe wheel."
            })),
        )
            .into_response();
    }

    let all_groups = list_all_groups();
    let current_members = all_groups
        .iter()
        .find(|g| g.name == group)
        .map(|g| g.members.clone())
        .unwrap_or_default();

    let new_members_set: HashSet<String> = body.members.into_iter().filter(|m| is_valid_identifier(m)).collect();
    let current_members_set: HashSet<String> = current_members.into_iter().collect();

    // Retraits
    for m in current_members_set.difference(&new_members_set) {
        let _ = Command::new("gpasswd").args(["-d", m, &group]).status();
    }

    // Ajouts
    for m in new_members_set.difference(&current_members_set) {
        let _ = Command::new("gpasswd").args(["-a", m, &group]).status();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Membres du groupe '{}' mis à jour avec succès.", group)
        })),
    )
        .into_response()
}

// DELETE /api/groups/:group
pub async fn handle_groups_delete(Path(group): Path<String>, headers: HeaderMap) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    if is_protected_system_group(&group) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Le groupe '{}' est un groupe système protégé et ne peut pas être supprimé.", group)
            })),
        )
            .into_response();
    }

    match Command::new("groupdel").arg(&group).output() {
        Ok(out) if out.status.success() => {
            let mut reg = load_registry();
            reg.custom_group_descriptions.remove(&group);
            save_registry(&reg);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "message": format!("Groupe '{}' supprimé avec succès.", group)
                })),
            )
                .into_response()
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Échec de groupdel : {}", err)
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Erreur système groupdel : {}", e)
            })),
        )
            .into_response(),
    }
}

// GET /api/users/audit
pub async fn handle_users_audit(headers: HeaderMap) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    let users = list_all_users().await;
    let mut warnings = Vec::new();

    let admin_count = users.iter().filter(|u| u.is_admin).count();
    let storage_count = users.iter().filter(|u| u.groups.iter().any(|g| g == "storage")).count();
    let docker_count = users.iter().filter(|u| u.groups.iter().any(|g| g == "docker")).count();
    let kvm_count = users.iter().filter(|u| u.groups.iter().any(|g| g == "kvm" || g == "libvirtd")).count();
    let locked_count = users.iter().filter(|u| u.locked).count();
    let nologin_count = users.iter().filter(|u| u.shell.ends_with("nologin") || u.shell.ends_with("false")).count();

    // Analyse des anomalies de sécurité
    if admin_count == 0 {
        warnings.push("Avertissement critique : Aucun compte administrateur identifié dans le groupe wheel.".to_string());
    }

    for u in &users {
        if !u.is_admin && u.groups.iter().any(|g| g == "docker") {
            warnings.push(format!(
                "Privilège élevé : L'utilisateur non-admin '{}' appartient au groupe 'docker' (équivalent root potentiel).",
                u.username
            ));
        }
        if u.role == "share_only" && !u.shell.ends_with("nologin") && !u.shell.ends_with("false") {
            warnings.push(format!(
                "Moindre privilège : Le compte de partage pur '{}' dispose d'un shell interactif ('{}').",
                u.username, u.shell
            ));
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "report": SecurityAuditReport {
                total_users: users.len(),
                admin_count,
                storage_count,
                docker_count,
                kvm_count,
                locked_count,
                nologin_count,
                warnings,
                users
            }
        })),
    )
        .into_response()
}

// GET /api/users/sessions
pub async fn handle_users_sessions(headers: HeaderMap) -> Response {
    let _session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    let sessions = auth::get_all_active_sessions().await;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "sessions": sessions
        })),
    )
        .into_response()
}

// POST /api/users/sessions/:token/revoke
pub async fn handle_users_revoke_session(Path(token): Path<String>, headers: HeaderMap) -> Response {
    let session = match auth::get_session_from_headers(&headers).await {
        Some(s) if s.is_admin => s,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Accès refusé. Privilèges administrateur requis."
                })),
            )
                .into_response();
        }
    };

    if session.token == token {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Vous ne pouvez pas révoquer votre propre session active actuelle. Utilisez la déconnexion."
            })),
        )
            .into_response();
    }

    auth::revoke_token(&token).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": "Session révoquée avec succès."
        })),
    )
        .into_response()
}
