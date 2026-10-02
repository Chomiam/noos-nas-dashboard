use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::{Read, Write};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub username: String,
    pub is_admin: bool,
    pub created_at: u64,
    pub expires_at: u64,
}


// Magasin de sessions avec persistance sur disque (/var/lib/noos/sessions.json)
static SESSIONS: OnceLock<Arc<RwLock<HashMap<String, Session>>>> = OnceLock::new();

fn get_sessions_file_path() -> std::path::PathBuf {
    if let Ok(env_path) = std::env::var("NOOS_SESSIONS_FILE") {
        return std::path::PathBuf::from(env_path);
    }
    let var_lib = std::path::Path::new("/var/lib/noos");
    if var_lib.exists() || std::fs::create_dir_all(var_lib).is_ok() {
        return var_lib.join("sessions.json");
    }
    std::path::PathBuf::from("/run/noos-sessions.json")
}

fn load_sessions_from_disk() -> HashMap<String, Session> {
    let path = get_sessions_file_path();
    if !path.exists() {
        return HashMap::new();
    }

    let now = now_secs();
    if let Ok(file_content) = std::fs::read_to_string(&path) {
        if let Ok(sessions) = serde_json::from_str::<HashMap<String, Session>>(&file_content) {
            let valid_sessions: HashMap<String, Session> = sessions
                .into_iter()
                .filter(|(_, s)| s.expires_at > now)
                .collect();
            return valid_sessions;
        }
    }

    HashMap::new()
}

fn save_sessions_to_disk(sessions: &HashMap<String, Session>) {
    let path = get_sessions_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let now = now_secs();
    let valid_sessions: HashMap<&String, &Session> = sessions
        .iter()
        .filter(|(_, s)| s.expires_at > now)
        .collect();

    if let Ok(json_bytes) = serde_json::to_vec_pretty(&valid_sessions) {
        let tmp_path = path.with_extension("json.tmp");
        if std::fs::write(&tmp_path, &json_bytes).is_ok() {
            let _ = std::fs::rename(&tmp_path, &path);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
        let _ = std::fs::create_dir_all("/var/lib/noos");
        let _ = std::fs::write("/var/lib/noos/sessions.json", &json_bytes);
    }
}

fn get_sessions() -> &'static Arc<RwLock<HashMap<String, Session>>> {
    SESSIONS.get_or_init(|| {
        let initial_sessions = load_sessions_from_disk();
        Arc::new(RwLock::new(initial_sessions))
    })
}


#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(alias = "remember_me")]
    pub remember: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub token: Option<String>,
    pub username: Option<String>,
    pub is_admin: Option<bool>,
    pub expires_at: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub auth_required: bool,
    pub active_sessions: usize,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub success: bool,
    pub authenticated: bool,
    pub username: String,
    pub is_admin: bool,
    pub home_dir: String,
    pub expires_at: u64,
}

pub fn auth_routes() -> Router {
    Router::new()
        .route("/login", post(handle_login))
        .route("/status", get(handle_status))
        .route("/logout", post(handle_logout))
        .route("/me", get(handle_me))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Génération de jeton cryptographique aléatoire 64 hex via /dev/urandom
fn generate_token() -> String {
    let mut buf = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    } else {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        return format!("{:032x}{:032x}", nanos, nanos.wrapping_mul(6364136223846793005));
    }

    let mut hex = String::with_capacity(64);
    for b in buf {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

// Vérification d'un mot de passe contre un hash cryptographique Linux
fn check_password_hash(password: &str, hash: &str) -> bool {
    if hash.starts_with('!') || hash.starts_with('*') || hash.is_empty() {
        return false;
    }

    let last_dollar = match hash.rfind('$') {
        Some(idx) => idx,
        None => return false,
    };

    let setting = &hash[..last_dollar];

    // Moteur 1 : openssl passwd (si openssl est disponible)
    for openssl_bin in &["openssl", "/run/current-system/sw/bin/openssl", "/usr/bin/openssl"] {
        // Si hash SHA-512 ($6$)
        if hash.starts_with("$6$") {
            let parts: Vec<&str> = hash.split('$').collect();
            // Format $6$salt$hash -> parts[0]="", parts[1]="6", parts[2]=salt, parts[3]=hash
            if parts.len() >= 4 {
                let salt = parts[2];
                let mut cmd = Command::new(openssl_bin);
                cmd.args(["passwd", "-6", "-salt", salt, password]);
                if let Ok(output) = cmd.output() {
                    if output.status.success() {
                        let computed = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if computed == hash {
                            return true;
                        }
                    }
                }
            }
        }

        // Essai universel avec le setting complet
        let mut cmd = Command::new(openssl_bin);
        cmd.args(["passwd", "-S", setting, password]);
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let computed = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if computed == hash {
                    return true;
                }
            }
        }
    }

    // Moteur 2 : mkpasswd (fourni par whois)
    for mkpasswd_bin in &["mkpasswd", "/run/current-system/sw/bin/mkpasswd", "/usr/bin/mkpasswd"] {
        let mut cmd = Command::new(mkpasswd_bin);
        cmd.args(["-s", "-S", setting]);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(password.as_bytes());
                let _ = stdin.write_all(b"\n");
            }
            if let Ok(output) = child.wait_with_output() {
                if output.status.success() {
                    let computed = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if computed == hash {
                        return true;
                    }
                }
            }
        }
    }

    // Moteur 3 : python3 -c "import crypt..." si présent
    for py_bin in &["python3", "/run/current-system/sw/bin/python3"] {
        let py_script = "import crypt, sys; sys.exit(0 if crypt.crypt(sys.argv[1], sys.argv[2]) == sys.argv[2] else 1)";
        let mut cmd = Command::new(py_bin);
        cmd.args(["-c", py_script, password, hash]);
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                return true;
            }
        }
    }

    false
}

// Vérification de mot de passe contre /etc/shadow ou /etc/nixos/vars.nix
pub fn verify_linux_credentials(username: &str, password: &str) -> Result<bool, String> {
    if username.is_empty() || password.is_empty() {
        return Ok(false);
    }

    let mut stored_hash: Option<String> = None;
    let mut shadow_had_locked_account = false;

    // 1. Recherche dans /etc/shadow
    if let Ok(content) = std::fs::read_to_string("/etc/shadow") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 && parts[0].eq_ignore_ascii_case(username) {
                let h = parts[1].trim();
                if h.starts_with('!') || h.starts_with('*') || h.is_empty() {
                    shadow_had_locked_account = true;
                } else {
                    stored_hash = Some(h.to_string());
                }
                break;
            }
        }
    }

    // 2. Fallback dans /etc/nixos/vars.nix si shadow n'a pas de hash valide
    if stored_hash.is_none() {
        for vars_path in &[
            "/etc/nixos/vars.nix",
            "/etc/nixos/noos-nas/vars.nix",
            "./vars.nix",
            "../vars.nix",
        ] {
            if let Ok(content) = std::fs::read_to_string(vars_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.contains("hashedPassword") || trimmed.contains("initialHashedPassword") {
                        if let Some(first_quote) = trimmed.find('"') {
                            if let Some(second_quote) = trimmed[first_quote + 1..].find('"') {
                                let val = &trimmed[first_quote + 1..first_quote + 1 + second_quote];
                                if val.starts_with('$') {
                                    stored_hash = Some(val.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
                if stored_hash.is_some() {
                    break;
                }
            }
        }
    }

    let hash = match stored_hash {
        Some(h) => h,
        None => return Ok(false),
    };

    let is_valid = check_password_hash(password, &hash);

    // 3. Auto-réparation système : si le compte était verrouillé dans /etc/shadow mais que le mot de passe est bon
    if is_valid && shadow_had_locked_account {
        let chpasswd_input = format!("{}:{}\n", username, password);
        for chpasswd_bin in &["chpasswd", "/run/current-system/sw/bin/chpasswd"] {
            if let Ok(mut child) = Command::new(chpasswd_bin).stdin(std::process::Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(chpasswd_input.as_bytes());
                }
                let _ = child.wait();
                break;
            }
        }
    }

    Ok(is_valid)
}

fn check_is_admin(username: &str) -> bool {
    if username == "root" {
        return true;
    }

    // Vérifier via 'id -Gn <user>'
    if let Ok(out) = Command::new("id").args(["-Gn", username]).output() {
        if out.status.success() {
            let groups = String::from_utf8_lossy(&out.stdout);
            for g in groups.split_whitespace() {
                if g == "wheel" || g == "sudo" || g == "storage" || g == "root" {
                    return true;
                }
            }
        }
    }

    false
}

async fn handle_login(Json(req): Json<LoginRequest>) -> Response {
    let username = req.username.trim();
    let password = req.password;

    match verify_linux_credentials(username, &password) {
        Ok(true) => {
            let is_admin = check_is_admin(username);
            let token = generate_token();
            let now = now_secs();
            // 7 jours si 'se souvenir de moi' coché, sinon 24 heures
            let duration = if req.remember.unwrap_or(true) {
                7 * 24 * 3600
            } else {
                24 * 3600
            };
            let expires_at = now + duration;

            let session = Session {
                token: token.clone(),
                username: username.to_string(),
                is_admin,
                created_at: now,
                expires_at,
            };

            {
                let sessions_lock = get_sessions();
                let mut sessions = sessions_lock.write().await;
                // Nettoyage périodique des sessions expirées
                sessions.retain(|_, s| s.expires_at > now);
                sessions.insert(token.clone(), session);
                save_sessions_to_disk(&sessions);
            }

            let mut response = (
                StatusCode::OK,
                Json(LoginResponse {
                    success: true,
                    token: Some(token.clone()),
                    username: Some(username.to_string()),
                    is_admin: Some(is_admin),
                    expires_at: Some(expires_at),
                    message: Some("Connexion réussie".into()),
                }),
            )
                .into_response();

            let cookie_noos = format!(
                "noos_token={}; Path=/; Max-Age={}; SameSite=Lax",
                token, duration
            );
            if let Ok(hv) = header::HeaderValue::from_str(&cookie_noos) {
                response.headers_mut().append(header::SET_COOKIE, hv);
            }

            response
        }
        Ok(false) => (
            StatusCode::UNAUTHORIZED,
            Json(LoginResponse {
                success: false,
                token: None,
                username: None,
                is_admin: None,
                expires_at: None,
                message: Some("Nom d'utilisateur ou mot de passe incorrect.".into()),
            }),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(LoginResponse {
                success: false,
                token: None,
                username: None,
                is_admin: None,
                expires_at: None,
                message: Some(format!("Erreur d'authentification système : {}", err)),
            }),
        )
            .into_response(),
    }
}

async fn handle_status() -> Json<StatusResponse> {
    let now = now_secs();
    let sessions_lock = get_sessions();
    let sessions = sessions_lock.read().await;
    let active = sessions.values().filter(|s| s.expires_at > now).count();

    Json(StatusResponse {
        auth_required: true,
        active_sessions: active,
    })
}

fn extract_token(req: &Request) -> Option<String> {
    // 1. Header Authorization: Bearer <token>
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(val) = auth_header.to_str() {
            if val.starts_with("Bearer ") {
                return Some(val[7..].trim().to_string());
            }
        }
    }

    // 2. Cookie: noos_token=<token>, noos_auth_token=<token>
    if let Some(cookie_header) = req.headers().get(header::COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for c in cookies.split(';') {
                let parts: Vec<&str> = c.trim().split('=').collect();
                if parts.len() == 2 && (parts[0] == "noos_token" || parts[0] == "noos_auth_token") {
                    return Some(parts[1].trim().to_string());
                }
            }
        }
    }

    // 3. Query param ?token=<token> ou ?auth_token=<token>
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            let parts: Vec<&str> = pair.split('=').collect();
            if parts.len() == 2 && (parts[0] == "token" || parts[0] == "auth_token") {
                let clean = parts[1].trim();
                if !clean.is_empty() {
                    return Some(clean.to_string());
                }
            }
        }
    }

    None
}

async fn handle_logout(req: Request) -> Response {
    if let Some(token) = extract_token(&req) {
        let sessions_lock = get_sessions();
        let mut sessions = sessions_lock.write().await;
        sessions.remove(&token);
        save_sessions_to_disk(&sessions);
    }

    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": "Déconnexion effectuée avec succès."
        })),
    )
        .into_response();

    if let Ok(hv) = header::HeaderValue::from_str("noos_token=; Path=/; Max-Age=0; SameSite=Lax") {
        response.headers_mut().append(header::SET_COOKIE, hv);
    }

    response
}

async fn handle_me(req: Request) -> Response {
    let token = match extract_token(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "success": false,
                    "authenticated": false,
                    "message": "Jeton d'authentification manquant."
                })),
            )
                .into_response();
        }
    };

    let now = now_secs();
    let sessions_lock = get_sessions();

    // 1. Vérification en mémoire (rapide)
    {
        let sessions = sessions_lock.read().await;
        if let Some(session) = sessions.get(&token) {
            if session.expires_at > now {
                return (
                    StatusCode::OK,
                    Json(MeResponse {
                        success: true,
                        authenticated: true,
                        username: session.username.clone(),
                        is_admin: session.is_admin,
                        home_dir: crate::updates::get_user_home(&session.username).to_string_lossy().to_string(),
                        expires_at: session.expires_at,
                    }),
                )
                    .into_response();
            }
        }
    }

    // 2. Vérification sur disque si absent de la mémoire (ex: redémarrage service)
    let disk_session = {
        let disk_sessions = load_sessions_from_disk();
        disk_sessions.get(&token).cloned()
    };
    if let Some(session) = disk_session {
        if session.expires_at > now {
            {
                let mut sessions = sessions_lock.write().await;
                sessions.insert(token.clone(), session.clone());
            }
            return (
                StatusCode::OK,
                Json(MeResponse {
                    success: true,
                    authenticated: true,
                    username: session.username.clone(),
                    is_admin: session.is_admin,
                    home_dir: crate::updates::get_user_home(&session.username).to_string_lossy().to_string(),
                    expires_at: session.expires_at,
                }),
            )
                .into_response();
        }
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "success": false,
            "authenticated": false,
            "message": "Session expirée ou invalide."
        })),
    )
        .into_response()
}

pub async fn auth_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path();

    // Endpoints publics exemptés d'authentification
    if path.ends_with("/auth/login") || path.ends_with("/auth/status") || path.ends_with("/auth/me") || path.ends_with("/vnc") {
        return next.run(req).await;
    }

    // Vérification du jeton
    if let Some(token) = extract_token(&req) {
        let now = now_secs();
        let sessions_lock = get_sessions();
        
        // 1. Vérification en mémoire (instantanée)
        // Le verrou de lecture DOIT être libéré avant tout appel à next.run() pour éviter l'interblocage
        let is_valid_in_memory = {
            let sessions = sessions_lock.read().await;
            sessions.get(&token).map(|s| s.expires_at > now).unwrap_or(false)
        };
        if is_valid_in_memory {
            return next.run(req).await;
        }

        // 2. Si non trouvé en mémoire (ex: service redémarré après une mise à jour), vérifier sur disque
        let disk_session = {
            let disk_sessions = load_sessions_from_disk();
            disk_sessions.get(&token).cloned()
        };
        if let Some(session) = disk_session {
            if session.expires_at > now {
                {
                    let mut sessions = sessions_lock.write().await;
                    sessions.insert(token.clone(), session);
                }
                // Le verrou d'écriture DOIT être libéré avant next.run() sous peine d'interblocage immédiat !
                return next.run(req).await;
            }
        }
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "success": false,
            "error": "Non authentifié",
            "message": "Authentification requise pour accéder aux fonctionnalités du NAS."
        })),
    )
        .into_response()
}


pub async fn revoke_user_sessions(username: &str) {
    let sessions_lock = get_sessions();
    let mut sessions = sessions_lock.write().await;
    sessions.retain(|_, s| s.username != username);
    save_sessions_to_disk(&sessions);
}

pub async fn revoke_token(token: &str) {
    let sessions_lock = get_sessions();
    let mut sessions = sessions_lock.write().await;
    sessions.remove(token);
    save_sessions_to_disk(&sessions);
}

pub async fn get_all_active_sessions() -> Vec<Session> {
    let now = now_secs();
    let sessions_lock = get_sessions();
    let sessions = sessions_lock.read().await;
    sessions.values().filter(|s| s.expires_at > now).cloned().collect()
}

pub fn extract_token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
        if let Ok(val) = auth_header.to_str() {
            if val.starts_with("Bearer ") {
                return Some(val[7..].trim().to_string());
            }
        }
    }
    if let Some(cookie_header) = headers.get(header::COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for c in cookies.split(';') {
                let parts: Vec<&str> = c.trim().split('=').collect();
                if parts.len() == 2 && (parts[0] == "noos_token" || parts[0] == "noos_auth_token") {
                    return Some(parts[1].trim().to_string());
                }
            }
        }
    }
    None
}

pub async fn get_session_from_headers(headers: &axum::http::HeaderMap) -> Option<Session> {
    let token = extract_token_from_headers(headers)?;
    let now = now_secs();
    let sessions_lock = get_sessions();
    {
        let sessions = sessions_lock.read().await;
        if let Some(session) = sessions.get(&token) {
            if session.expires_at > now {
                return Some(session.clone());
            }
        }
    }
    let disk_session = {
        let disk_sessions = load_sessions_from_disk();
        disk_sessions.get(&token).cloned()
    };
    if let Some(session) = disk_session {
        if session.expires_at > now {
            let session_clone = session.clone();
            {
                let mut sessions = sessions_lock.write().await;
                sessions.insert(token, session_clone);
            }
            return Some(session);
        }
    }
    None
}
