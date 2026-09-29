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

// Magasin de sessions en mémoire partagé (utilisant std::sync::OnceLock)
static SESSIONS: OnceLock<Arc<RwLock<HashMap<String, Session>>>> = OnceLock::new();

fn get_sessions() -> &'static Arc<RwLock<HashMap<String, Session>>> {
    SESSIONS.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
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
    pub username: String,
    pub is_admin: bool,
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

// Vérification de mot de passe contre /etc/shadow ou /etc/nixos/vars.nix
pub fn verify_linux_credentials(username: &str, password: &str) -> Result<bool, String> {
    if username.is_empty() || password.is_empty() {
        return Ok(false);
    }

    let mut stored_hash: Option<String> = None;

    // 1. Recherche dans /etc/shadow
    if let Ok(content) = std::fs::read_to_string("/etc/shadow") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 && parts[0] == username {
                stored_hash = Some(parts[1].to_string());
                break;
            }
        }
    }

    // 2. Fallback dans /etc/nixos/vars.nix si shadow n'est pas accessible
    if stored_hash.is_none() {
        for vars_path in &["/etc/nixos/vars.nix", "/etc/nixos/steveos-nas/vars.nix"] {
            if let Ok(content) = std::fs::read_to_string(vars_path) {
                if content.contains(&format!("\"{}\"", username)) {
                    for line in content.lines() {
                        if line.contains("hashedPassword") {
                            if let Some(start) = line.find('"') {
                                if let Some(end) = line[start + 1..].find('"') {
                                    stored_hash = Some(line[start + 1..start + 1 + end].to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let hash = match stored_hash {
        Some(h) => h,
        None => return Ok(false),
    };

    if hash.starts_with('!') || hash.starts_with('*') || hash.is_empty() {
        return Ok(false);
    }

    let last_dollar = match hash.rfind('$') {
        Some(idx) => idx,
        None => return Ok(false),
    };

    let setting = &hash[..last_dollar];

    let mut cmd = Command::new("mkpasswd");
    cmd.args(["-s", "-S", setting]);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Err(format!("Impossible d'exécuter mkpasswd : {}", e)),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(password.as_bytes());
        let _ = stdin.write_all(b"\n");
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return Err(format!("Erreur lors de l'attente de mkpasswd : {}", e)),
    };

    if !output.status.success() {
        return Ok(false);
    }

    let computed = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(computed == hash)
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
            }

            (
                StatusCode::OK,
                Json(LoginResponse {
                    success: true,
                    token: Some(token),
                    username: Some(username.to_string()),
                    is_admin: Some(is_admin),
                    expires_at: Some(expires_at),
                    message: Some("Connexion réussie".into()),
                }),
            )
                .into_response()
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

    // 2. Cookie: steveos_token=<token>
    if let Some(cookie_header) = req.headers().get(header::COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for c in cookies.split(';') {
                let parts: Vec<&str> = c.trim().split('=').collect();
                if parts.len() == 2 && parts[0] == "steveos_token" {
                    return Some(parts[1].trim().to_string());
                }
            }
        }
    }

    // 3. Query param ?token=<token>
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            let parts: Vec<&str> = pair.split('=').collect();
            if parts.len() == 2 && parts[0] == "token" {
                return Some(parts[1].to_string());
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
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": "Déconnexion effectuée avec succès."
        })),
    )
        .into_response()
}

async fn handle_me(req: Request) -> Response {
    let token = match extract_token(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "success": false,
                    "message": "Jeton d'authentification manquant."
                })),
            )
                .into_response();
        }
    };

    let now = now_secs();
    let sessions_lock = get_sessions();
    let sessions = sessions_lock.read().await;

    if let Some(session) = sessions.get(&token) {
        if session.expires_at > now {
            return (
                StatusCode::OK,
                Json(MeResponse {
                    success: true,
                    username: session.username.clone(),
                    is_admin: session.is_admin,
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
            "message": "Session expirée ou invalide."
        })),
    )
        .into_response()
}

// Middleware de protection globale des routes /api
pub async fn auth_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path();

    // Endpoints publics exemptés d'authentification
    if path.ends_with("/auth/login") || path.ends_with("/auth/status") {
        return next.run(req).await;
    }

    // Vérification du jeton
    if let Some(token) = extract_token(&req) {
        let now = now_secs();
        let sessions_lock = get_sessions();
        let sessions = sessions_lock.read().await;
        if let Some(session) = sessions.get(&token) {
            if session.expires_at > now {
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
