use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use crate::files::{
    calculate_aspect_ratio, categorize_file, format_duration, format_size, get_mime_type,
    FileEntry, FilePropertiesResponse, MediaMetadata,
};

#[derive(Debug, Clone)]
pub struct KdPropertiesCacheEntry {
    pub instant: Instant,
    pub data: FilePropertiesResponse,
}

static KDRIVE_PROPERTIES_CACHE: Mutex<Option<HashMap<String, KdPropertiesCacheEntry>>> = Mutex::new(None);
static KDRIVE_CHILDREN_CACHE: Mutex<Option<HashMap<(String, u64), (Instant, Vec<FileEntry>)>>> = Mutex::new(None);
static KDRIVE_LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

/// Pacing anti-burst pour respecter les quotas stricts d'Infomaniak API (min_interval_ms par requête)
pub fn pace_kdrive_request(min_interval_ms: u64) {
    if let Ok(mut guard) = KDRIVE_LAST_REQUEST.lock() {
        let now = Instant::now();
        if let Some(last) = *guard {
            let elapsed = now.duration_since(last);
            let min_dur = std::time::Duration::from_millis(min_interval_ms);
            if elapsed < min_dur {
                let sleep_dur = min_dur - elapsed;
                std::thread::sleep(sleep_dur);
            }
        }
        *guard = Some(Instant::now());
    }
}

// =========================================================================
// STRUCTURES & CONFIGURATION : COMPTES KDRIVE INFOMANIAK
// =========================================================================

/// Compte kDrive complet persisté localement sur disque avec permissions strictes (0600).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KDriveAccount {
    pub id: String,
    pub name: String,
    pub token: String,
    pub drive_id: u64,
    pub drive_name: String,
    pub total_size: u64,
    pub used_size: u64,
    pub created_at: String,
}

/// DTO public exposé à l'API HTTP du dashboard (le jeton complet n'est JAMAIS divulgué).
#[derive(Debug, Clone, Serialize)]
pub struct KDriveAccountPublic {
    pub id: String,
    pub name: String,
    pub drive_id: u64,
    pub drive_name: String,
    pub total_size: u64,
    pub used_size: u64,
    pub total_size_human: String,
    pub used_size_human: String,
    pub created_at: String,
    pub token_masked: String,
}

impl KDriveAccount {
    pub fn to_public(&self) -> KDriveAccountPublic {
        KDriveAccountPublic {
            id: self.id.clone(),
            name: self.name.clone(),
            drive_id: self.drive_id,
            drive_name: self.drive_name.clone(),
            total_size: self.total_size,
            used_size: self.used_size,
            total_size_human: format_size(self.total_size),
            used_size_human: format_size(self.used_size),
            created_at: self.created_at.clone(),
            token_masked: mask_token(&self.token),
        }
    }
}

/// Masque un jeton sensible (ex: "inf_api_1234...5678" -> "inf_••••••••5678")
pub fn mask_token(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.len() <= 8 {
        "••••••••".to_string()
    } else {
        let prefix: String = trimmed.chars().take(4).collect();
        let suffix: String = trimmed.chars().rev().take(4).collect::<String>().chars().rev().collect();
        format!("{}••••••••{}", prefix, suffix)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KDriveDriveInfo {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub used_size: u64,
    pub size_human: String,
    pub used_size_human: String,
}

#[derive(Debug, Deserialize)]
pub struct DetectTokenRequest {
    pub token: String,
    pub drive_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateKDriveAccountRequest {
    pub name: String,
    pub token: String,
    pub drive_id: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateKDriveAccountRequest {
    pub name: Option<String>,
    pub token: Option<String>,
    pub drive_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KDriveBreadcrumb {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KDriveFolderListing {
    pub account_id: String,
    pub account_name: String,
    pub drive_id: u64,
    pub current_folder_id: u64,
    pub current_folder_name: String,
    pub parent_folder_id: Option<u64>,
    pub breadcrumbs: Vec<KDriveBreadcrumb>,
    pub entries: Vec<FileEntry>,
    pub total_items: usize,
    pub total_size_bytes: u64,
    pub drive_used_size: u64,
    pub drive_total_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct KDriveListQuery {
    pub folder_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct KDriveMkdirRequest {
    pub parent_id: u64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct KDriveCopyToNasRequest {
    pub file_id: u64,
    pub file_name: String,
    pub dest_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct KDriveUploadFromNasRequest {
    pub source_path: String,
    pub target_folder_id: Option<u64>,
}

// =========================================================================
// STOCKAGE LOCAL DES COMPTES AVEC PERMISSIONS RESTREINTES
// =========================================================================

fn get_accounts_file_path() -> PathBuf {
    let p = PathBuf::from("/var/lib/noos/kdrive_accounts.json");
    if let Some(parent) = p.parent() {
        if parent.exists() {
            return p;
        }
    }
    PathBuf::from("/tmp/noos_kdrive_accounts.json")
}

pub fn load_kdrive_accounts() -> Vec<KDriveAccount> {
    let path = get_accounts_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(accounts) = serde_json::from_str::<Vec<KDriveAccount>>(&content) {
            return accounts;
        }
    }
    Vec::new()
}

pub fn save_kdrive_accounts(accounts: &[KDriveAccount]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(accounts).map_err(|e| e.to_string())?;
    let path = get_accounts_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, &json).map_err(|e| format!("Échec sauvegarde comptes kDrive : {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// =========================================================================
// APPELS API INFOMANIAK SÉCURISÉS (INJECTION DU TOKEN VIA STDIN)
// =========================================================================

/// Exécute une requête curl sécurisée en passant le token Bearer via stdin (-K -)
/// afin qu'aucun secret n'apparaisse dans les arguments CLI (`ps aux` / `/proc`).
fn curl_secure_request(
    token: &str,
    method: &str,
    url: &str,
    json_body: Option<&str>,
    timeout_sec: u64,
    output_file: Option<&str>,
) -> Result<Vec<u8>, String> {
    pace_kdrive_request(50);

    let mut args: Vec<String> = vec![
        "-s".into(),
        "-L".into(),
        "--connect-timeout".into(),
        "30".into(),
        "--retry".into(),
        "3".into(),
        "--retry-delay".into(),
        "2".into(),
        "--max-time".into(),
        timeout_sec.to_string(),
        "-K".into(),
        "-".into(),
    ];

    if method != "GET" {
        args.push("-X".into());
        args.push(method.into());
    }

    if let Some(body) = json_body {
        args.push("-H".into());
        args.push("Content-Type: application/json".into());
        args.push("-d".into());
        args.push(body.into());
    }

    if let Some(out_path) = output_file {
        args.push("-o".into());
        args.push(out_path.into());
    }

    args.push(url.into());

    let mut cmd = Command::new("curl");
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PATH", "/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/run/wrappers/bin:/usr/local/bin:/usr/bin:/bin");

    let mut child = cmd.spawn()
        .map_err(|e| format!("Erreur lancement curl : {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let config = format!(
            "header = \"Authorization: Bearer {}\"\nheader = \"Accept: application/json\"\n",
            token.trim()
        );
        let _ = stdin.write_all(config.as_bytes());
    }

    let output = child.wait_with_output()
        .map_err(|e| format!("Erreur exécution curl : {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Échec requête API Infomaniak : {}", err));
    }

    Ok(output.stdout)
}

fn curl_api_get(token: &str, url: &str) -> Result<serde_json::Value, String> {
    let raw = curl_secure_request(token, "GET", url, None, 20, None)?;
    let text = String::from_utf8_lossy(&raw);
    let val: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Réponse JSON Infomaniak invalide : {} (extrait: {})", e, text.chars().take(200).collect::<String>()))?;

    if let Some(res) = val.get("result").and_then(|r| r.as_str()) {
        if res == "error" {
            let desc = val.get("error")
                .and_then(|e| e.get("description").or_else(|| e.get("message")))
                .and_then(|m| m.as_str())
                .unwrap_or("Erreur API Infomaniak");
            return Err(desc.to_string());
        }
    }

    Ok(val)
}

fn curl_api_post_json(token: &str, url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let body_str = serde_json::to_string(body).map_err(|e| e.to_string())?;
    let raw = curl_secure_request(token, "POST", url, Some(&body_str), 20, None)?;
    let text = String::from_utf8_lossy(&raw);
    let val: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Réponse JSON invalide : {}", e))?;

    if let Some(res) = val.get("result").and_then(|r| r.as_str()) {
        if res == "error" {
            let desc = val.get("error")
                .and_then(|e| e.get("description").or_else(|| e.get("message")))
                .and_then(|m| m.as_str())
                .unwrap_or("Erreur API Infomaniak");
            return Err(desc.to_string());
        }
    }

    Ok(val)
}

fn curl_api_delete(token: &str, url: &str) -> Result<(), String> {
    let raw = curl_secure_request(token, "DELETE", url, None, 20, None)?;
    let text = String::from_utf8_lossy(&raw);
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(res) = val.get("result").and_then(|r| r.as_str()) {
            if res == "error" {
                let desc = val.get("error")
                    .and_then(|e| e.get("description"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Erreur de suppression sur kDrive");
                return Err(desc.to_string());
            }
        }
    }

    Ok(())
}

// =========================================================================
// MÉTHODES MÉTIER
// =========================================================================

/// Teste un jeton API Infomaniak et renvoie la liste des drives accessibles.
/// Si un drive_id est fourni, teste directement l'accès à ce kDrive.
pub fn test_and_fetch_drives(token: &str, drive_id_opt: Option<u64>) -> Result<Vec<KDriveDriveInfo>, String> {
    let clean_token = token.trim();
    if clean_token.is_empty() {
        return Err("Le jeton API ne peut pas être vide.".into());
    }

    // 0. Si un Drive ID spécifique a été fourni, le tester directement via GET /2/drive/{drive_id}
    if let Some(drive_id) = drive_id_opt {
        if drive_id > 0 {
            let res = curl_api_get(clean_token, &format!("https://api.infomaniak.com/2/drive/{}", drive_id));
            match res {
                Ok(val) => {
                    if let Some(data) = val.get("data") {
                        let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("Mon kDrive").to_string();
                        let size = data.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                        let used_size = data.get("used_size").and_then(|v| v.as_u64()).unwrap_or(0);
                        return Ok(vec![KDriveDriveInfo {
                            id: drive_id,
                            name,
                            size,
                            used_size,
                            size_human: format_size(size),
                            used_size_human: format_size(used_size),
                        }]);
                    }
                }
                Err(e) => {
                    return Err(format!("Le kDrive #{} est inaccessible avec ce jeton : {}", drive_id, e));
                }
            }
        }
    }

    // 0b. Tenter l'auto-détection via la liste des comptes : GET /1/accounts puis GET /2/drive?account_id={id}
    let res_accounts = curl_api_get(clean_token, "https://api.infomaniak.com/1/accounts");
    if let Ok(ref val) = res_accounts {
        if let Some(acc_arr) = val.get("data").and_then(|d| d.as_array()) {
            let mut drives = Vec::new();
            for acc in acc_arr {
                if let Some(acc_id) = acc.get("id").and_then(|v| v.as_u64()) {
                    let drive_url = format!("https://api.infomaniak.com/2/drive?account_id={}", acc_id);
                    if let Ok(ref drive_val) = curl_api_get(clean_token, &drive_url) {
                        if let Some(data_arr) = drive_val.get("data").and_then(|d| d.as_array()) {
                            for item in data_arr {
                                let id = item.get("id").or_else(|| item.get("drive_id")).and_then(|v| v.as_u64()).unwrap_or(0);
                                if id == 0 {
                                    continue;
                                }
                                let name = item.get("name").or_else(|| item.get("title")).and_then(|v| v.as_str()).unwrap_or("Mon kDrive").to_string();
                                let size = item.get("size").or_else(|| item.get("total_size")).and_then(|v| v.as_u64()).unwrap_or(0);
                                let used_size = item.get("used_size").and_then(|v| v.as_u64()).unwrap_or(0);

                                drives.push(KDriveDriveInfo {
                                    id,
                                    name,
                                    size,
                                    used_size,
                                    size_human: format_size(size),
                                    used_size_human: format_size(used_size),
                                });
                            }
                        }
                    }
                }
            }
            if !drives.is_empty() {
                return Ok(drives);
            }
        }
    }

    // 1. Essayer en priorité l'API officielle kDrive v2 : GET /2/drive
    let res_v2 = curl_api_get(clean_token, "https://api.infomaniak.com/2/drive");
    if let Ok(ref val) = res_v2 {
        let mut drives = Vec::new();
        if let Some(data_arr) = val.get("data").and_then(|d| d.as_array()) {
            for item in data_arr {
                let id = item.get("id").or_else(|| item.get("drive_id")).and_then(|v| v.as_u64()).unwrap_or(0);
                if id == 0 {
                    continue;
                }
                let name = item.get("name").or_else(|| item.get("title")).and_then(|v| v.as_str()).unwrap_or("Mon kDrive").to_string();
                let size = item.get("size").or_else(|| item.get("total_size")).and_then(|v| v.as_u64()).unwrap_or(0);
                let used_size = item.get("used_size").and_then(|v| v.as_u64()).unwrap_or(0);

                drives.push(KDriveDriveInfo {
                    id,
                    name,
                    size,
                    used_size,
                    size_human: format_size(size),
                    used_size_human: format_size(used_size),
                });
            }
        }
        if !drives.is_empty() {
            return Ok(drives);
        }
    }

    // 2. Repli : GET /1/accounts/current/products
    let res_prod = curl_api_get(clean_token, "https://api.infomaniak.com/1/accounts/current/products");
    if let Ok(ref val) = res_prod {
        let mut drives = Vec::new();
        if let Some(data_arr) = val.get("data").and_then(|d| d.as_array()) {
            for item in data_arr {
                let p_name = item.get("product_name").or_else(|| item.get("service_name")).and_then(|v| v.as_str()).unwrap_or("");
                let is_kdrive = p_name.to_lowercase().contains("drive") || item.get("type").and_then(|v| v.as_str()).map(|t| t.to_lowercase().contains("drive")).unwrap_or(false);
                if is_kdrive {
                    let id = item.get("id").or_else(|| item.get("product_id")).and_then(|v| v.as_u64()).unwrap_or(0);
                    if id > 0 {
                        let name = item.get("name").or_else(|| item.get("customer_name")).and_then(|v| v.as_str()).unwrap_or("kDrive").to_string();
                        let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                        let used_size = item.get("used_size").and_then(|v| v.as_u64()).unwrap_or(0);
                        drives.push(KDriveDriveInfo {
                            id,
                            name,
                            size,
                            used_size,
                            size_human: format_size(size),
                            used_size_human: format_size(used_size),
                        });
                    }
                }
            }
        }
        if !drives.is_empty() {
            return Ok(drives);
        }
    }

    // 3. Diagnostic précis et pédagogique de la cause de l'erreur
    let err_str = res_v2.err().unwrap_or_default();
    let prod_err = res_prod.err().unwrap_or_default();
    if err_str.contains("account id field is required") || prod_err.contains("require this specific scope: \"accounts\"") {
        Err("Ce jeton possède la permission kDrive mais pas la permission 'Comptes' nécessaire à la détection automatique de l'ID. Renseignez directement l'ID de votre kDrive (le numéro dans l'URL de votre navigateur : ksuite.infomaniak.com/.../drive/ID).".into())
    } else if !err_str.is_empty() {
        Err(format!("Erreur API Infomaniak : {}", err_str))
    } else {
        Err("Aucun kDrive n'a pu être détecté pour ce compte. Vérifiez les permissions du jeton API.".into())
    }
}

/// Enregistre un nouveau compte kDrive sur le NAS et renvoie son profil public masqué.
pub fn add_kdrive_account(name: &str, token: &str, drive_id: u64) -> Result<KDriveAccountPublic, String> {
    let clean_token = token.trim();
    if clean_token.is_empty() {
        return Err("Le jeton API ne peut pas être vide.".into());
    }

    let mut drive_name = name.trim().to_string();
    let mut total_size = 0;
    let mut used_size = 0;

    // Tenter de récupérer les métadonnées via GET /2/drive/{drive_id}
    let drive_info_res = curl_api_get(clean_token, &format!("https://api.infomaniak.com/2/drive/{}", drive_id));
    if let Ok(ref val) = drive_info_res {
        if let Some(data) = val.get("data") {
            if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
                if drive_name.is_empty() {
                    drive_name = n.to_string();
                }
            }
            total_size = data.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            used_size = data.get("used_size").and_then(|v| v.as_u64()).unwrap_or(0);
        }
    } else if let Ok(drives) = test_and_fetch_drives(clean_token, Some(drive_id)) {
        if let Some(d) = drives.iter().find(|d| d.id == drive_id) {
            if drive_name.is_empty() {
                drive_name = d.name.clone();
            }
            total_size = d.size;
            used_size = d.used_size;
        }
    }

    if drive_name.is_empty() {
        drive_name = format!("kDrive #{}", drive_id);
    }

    let slug = drive_name.to_lowercase().replace(|c: char| !c.is_alphanumeric() && c != '-', "-");
    let account_id = format!("kdrive-{}", if slug.is_empty() { format!("{}", drive_id) } else { slug });

    let now_str = chrono_or_date();
    let account = KDriveAccount {
        id: account_id.clone(),
        name: if name.trim().is_empty() { drive_name.clone() } else { name.trim().to_string() },
        token: clean_token.to_string(),
        drive_id,
        drive_name,
        total_size,
        used_size,
        created_at: now_str,
    };

    let mut accounts = load_kdrive_accounts();
    accounts.retain(|a| a.id != account_id);
    accounts.push(account.clone());
    save_kdrive_accounts(&accounts)?;

    Ok(account.to_public())
}

/// Met à jour les paramètres d'un compte kDrive existant (jeton, ID du drive, nom d'affichage).
pub fn update_kdrive_account(account_id: &str, req: UpdateKDriveAccountRequest) -> Result<KDriveAccountPublic, String> {
    let mut accounts = load_kdrive_accounts();
    let account = accounts.iter_mut().find(|a| a.id == account_id)
        .ok_or_else(|| format!("Compte kDrive '{}' introuvable.", account_id))?;

    if let Some(t) = req.token {
        let clean = t.trim();
        if !clean.is_empty() {
            account.token = clean.to_string();
        }
    }

    if let Some(d_id) = req.drive_id {
        if d_id > 0 {
            account.drive_id = d_id;
        }
    }

    if let Some(n) = req.name {
        let clean = n.trim();
        if !clean.is_empty() {
            account.name = clean.to_string();
        }
    }

    // Tenter de rafraîchir les métadonnées (quota, nom distant)
    let info_url = format!("https://api.infomaniak.com/2/drive/{}", account.drive_id);
    if let Ok(ref val) = curl_api_get(&account.token, &info_url) {
        if let Some(data) = val.get("data") {
            if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
                account.drive_name = n.to_string();
            }
            if let Some(sz) = data.get("size").and_then(|v| v.as_u64()) {
                account.total_size = sz;
            }
            if let Some(used) = data.get("used_size").and_then(|v| v.as_u64()) {
                account.used_size = used;
            }
        }
    }

    let public_account = account.to_public();
    save_kdrive_accounts(&accounts)?;
    Ok(public_account)
}

/// Supprime / Déconnecte un compte kDrive du NAS.
pub fn delete_kdrive_account(account_id: &str) -> Result<(), String> {
    let mut accounts = load_kdrive_accounts();
    let before_len = accounts.len();
    accounts.retain(|a| a.id != account_id);
    if accounts.len() == before_len {
        return Err("Compte kDrive introuvable.".into());
    }
    save_kdrive_accounts(&accounts)?;
    Ok(())
}

/// Liste le contenu d'un dossier kDrive et le traduit en `FileEntry` standard.
pub fn list_kdrive_folder(account_id: &str, folder_id_opt: Option<u64>) -> Result<KDriveFolderListing, String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| format!("Compte kDrive '{}' introuvable.", account_id))?;

    let folder_id = folder_id_opt.unwrap_or(0);
    // En API Infomaniak kDrive, la racine est représentée par file_id = 1
    let target_folder_id = if folder_id == 0 { 1 } else { folder_id };

    let url = format!("https://api.infomaniak.com/3/drive/{}/files/{}/files?limit=1000", account.drive_id, target_folder_id);

    let val = match curl_api_get(&account.token, &url) {
        Ok(v) => v,
        Err(e) => {
            // Repli 1 : tester search avec directory_id
            let fb_search_url = format!(
                "https://api.infomaniak.com/3/drive/{}/files/search?directory_id={}&depth=child&limit=1000",
                account.drive_id, target_folder_id
            );
            if let Ok(v2) = curl_api_get(&account.token, &fb_search_url) {
                v2
            } else if target_folder_id == 1 {
                // Repli 2 pour la racine : tester /files/root/files
                let fb_root_url = format!("https://api.infomaniak.com/3/drive/{}/files/root/files?limit=1000", account.drive_id);
                curl_api_get(&account.token, &fb_root_url)
                    .map_err(|e2| format!("Impossible d'accéder au kDrive ({}) : {}", e, e2))?
            } else {
                return Err(format!("Impossible d'accéder au dossier kDrive : {}", e));
            }
        }
    };

    let items = val.get("data").and_then(|d| d.as_array());
    let mut entries = Vec::new();
    let mut total_size_bytes: u64 = 0;

    let mut current_name = "Racine".to_string();
    let mut parent_folder_id: Option<u64> = None;

    if folder_id != 0 {
        let meta_url = format!("https://api.infomaniak.com/3/drive/{}/files/{}", account.drive_id, folder_id);
        if let Ok(meta_val) = curl_api_get(&account.token, &meta_url) {
            if let Some(data) = meta_val.get("data") {
                if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
                    current_name = n.to_string();
                }
                parent_folder_id = data.get("parent_id").and_then(|v| v.as_u64());
            }
        }
    }

    if let Some(arr) = items {
        for item in arr {
            let id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("Sans nom").to_string();
            let is_dir = item.get("type").and_then(|v| v.as_str()).map(|t| t == "dir").unwrap_or(false)
                || item.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
            let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            
            let mod_time = item.get("modtime").or_else(|| item.get("updated_at"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let modified = if mod_time > 0 {
                format_timestamp(mod_time)
            } else {
                "Inconnue".to_string()
            };

            let category = if is_dir {
                "folder".to_string()
            } else {
                categorize_file(&name)
            };

            if !is_dir {
                total_size_bytes += size;
            }

            let virt_path = format!("kdrive://{}/{}", account.id, id);

            let ext = Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            let mime_type = if is_dir {
                "inode/directory".to_string()
            } else {
                get_mime_type(&ext).to_string()
            };

            // Pré-alimentation du cache des propriétés pour éviter tout appel HTTP ultérieur
            let cache_key = format!("{}:{}", account.id, id);
            let item_props = FilePropertiesResponse {
                name: name.clone(),
                path: virt_path.clone(),
                parent_path: Some(format!("kdrive://{}/{}", account.id, target_folder_id)),
                is_dir,
                is_symlink: false,
                is_mount_point: false,
                size_bytes: size,
                size_human: if is_dir { "--".to_string() } else { format_size(size) },
                category: category.clone(),
                mime_type,
                permissions_octal: if is_dir { "0755".to_string() } else { "0644".to_string() },
                permissions_mode: if is_dir { "drwxr-xr-x".to_string() } else { "-rw-r--r--".to_string() },
                owner: format!("kDrive ({})", account.name),
                group: "cloud".to_string(),
                modified: modified.clone(),
                created: None,
                accessed: None,
                media_info: None,
            };

            if let Ok(mut guard) = KDRIVE_PROPERTIES_CACHE.lock() {
                let map = guard.get_or_insert_with(HashMap::new);
                map.insert(cache_key, KdPropertiesCacheEntry {
                    instant: Instant::now(),
                    data: item_props,
                });
            }

            entries.push(FileEntry {
                name,
                path: virt_path,
                is_dir,
                size_bytes: size,
                size_human: if is_dir { "--".to_string() } else { format_size(size) },
                modified,
                permissions: if is_dir { "drwxr-xr-x".to_string() } else { "-rw-r--r--".to_string() },
                category,
                is_mount_point: false,
            });
        }
    }

    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    let total_items = entries.len();

    // Mémoriser la liste des enfants dans le cache anti-flood
    if let Ok(mut guard) = KDRIVE_CHILDREN_CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert((account.id.clone(), target_folder_id), (Instant::now(), entries.clone()));
    }

    let listing = KDriveFolderListing {
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        drive_id: account.drive_id,
        current_folder_id: folder_id,
        current_folder_name: current_name,
        parent_folder_id,
        breadcrumbs: vec![
            KDriveBreadcrumb { id: 0, name: account.name.clone() },
        ],
        entries,
        total_items,
        total_size_bytes,
        drive_used_size: account.used_size,
        drive_total_size: account.total_size,
    };

    Ok(listing)
}

/// Crée un sous-dossier dans kDrive.
pub fn create_kdrive_folder(account_id: &str, parent_id: u64, name: &str) -> Result<(), String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| "Compte kDrive introuvable.".to_string())?;

    let clean_name = name.trim();
    if clean_name.is_empty() {
        return Err("Le nom du dossier ne peut pas être vide.".into());
    }

    let target_parent_id = if parent_id == 0 { 1 } else { parent_id };
    let url = format!("https://api.infomaniak.com/3/drive/{}/files/{}/directory", account.drive_id, target_parent_id);
    let body = serde_json::json!({
        "name": clean_name
    });

    let _ = curl_api_post_json(&account.token, &url, &body)?;
    Ok(())
}

/// Supprime un élément (fichier ou dossier) dans kDrive (vers la corbeille kDrive).
pub fn delete_kdrive_item(account_id: &str, file_id: u64) -> Result<(), String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| "Compte kDrive introuvable.".to_string())?;

    let del_url = format!("https://api.infomaniak.com/2/drive/{}/files/{}", account.drive_id, file_id);
    curl_api_delete(&account.token, &del_url)?;

    Ok(())
}

/// Télécharge un fichier kDrive dans un fichier temporaire sécurisé pour le servir au navigateur.
pub fn download_kdrive_file_to_temp(account_id: &str, file_id: u64) -> Result<(PathBuf, String), String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| "Compte kDrive introuvable.".to_string())?;

    let meta_url = format!("https://api.infomaniak.com/3/drive/{}/files/{}", account.drive_id, file_id);
    let bytes = curl_secure_request(&account.token, "GET", &meta_url, None, 15, None)?;
    let val: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Réponse métadonnées invalide : {}", e))?;

    let file_name = val.get("data")
        .and_then(|d| d.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("fichier_kdrive")
        .to_string();

    let sanitized_name = Path::new(&file_name).file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("kdrive_{}", file_id));

    let temp_dir = PathBuf::from("/tmp/noos_kdrive_downloads");
    let _ = fs::create_dir_all(&temp_dir);
    let temp_file = temp_dir.join(format!("{}_{}", file_id, sanitized_name));

    let dl_url = format!("https://api.infomaniak.com/2/drive/{}/files/{}/download", account.drive_id, file_id);
    curl_secure_request(&account.token, "GET", &dl_url, None, 600, Some(&temp_file.display().to_string()))?;

    Ok((temp_file, sanitized_name))
}

/// Copie un fichier depuis kDrive directement vers un répertoire du NAS en tâche de fond.
pub fn copy_kdrive_file_to_nas(account_id: &str, file_id: u64, file_name: &str, dest_dir: &str) -> Result<String, String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| "Compte kDrive introuvable.".to_string())?;

    let target_dest_dir = crate::files::normalize_user_path(PathBuf::from(dest_dir));
    if !target_dest_dir.is_dir() {
        return Err(format!("Le dossier de destination '{}' n'existe pas sur le NAS.", dest_dir));
    }

    let mut resolved_name = file_name.trim().to_string();
    if resolved_name.is_empty() {
        let meta_url = format!("https://api.infomaniak.com/3/drive/{}/files/{}", account.drive_id, file_id);
        if let Ok(bytes) = curl_secure_request(&account.token, "GET", &meta_url, None, 15, None) {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(n) = val.get("data").and_then(|d| d.get("name")).and_then(|n| n.as_str()) {
                    resolved_name = n.to_string();
                }
            }
        }
    }

    let sanitized_name = Path::new(&resolved_name).file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("kdrive_file_{}", file_id));

    let final_dest_path = target_dest_dir.join(&sanitized_name);
    let final_path_str = final_dest_path.display().to_string();

    let dl_api_url = format!("https://api.infomaniak.com/2/drive/{}/files/{}/download", account.drive_id, file_id);

    curl_secure_request(
        &account.token,
        "GET",
        &dl_api_url,
        None,
        3600, // 1 heure max pour les téléchargements de gros volumes
        Some(&final_path_str),
    )?;

    Ok(final_path_str)
}

/// Téléverse un fichier local présent sur le NAS directement dans kDrive.
pub fn upload_nas_file_to_kdrive(
    account_id: &str,
    nas_file_path: &str,
    target_folder_id_opt: Option<u64>,
) -> Result<FileEntry, String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| format!("Compte kDrive '{}' introuvable.", account_id))?;

    let raw_path = PathBuf::from(nas_file_path);
    let path = crate::files::normalize_user_path(raw_path);
    if !path.exists() {
        return Err(format!("Le fichier local '{}' n'existe pas sur le NAS.", nas_file_path));
    }
    if !path.is_file() {
        return Err(format!("'{}' n'est pas un fichier standard (les dossiers ne peuvent pas être téléversés directement).", nas_file_path));
    }

    let metadata = fs::metadata(&path).map_err(|e| format!("Impossible de lire le fichier : {}", e))?;
    let file_size = metadata.len();

    // Limitation de l'API Infomaniak : l'upload direct simple est limité à 1 Go par fichier.
    if file_size > 1024 * 1024 * 1024 {
        return Err("Le fichier dépasse 1 Go. L'API d'upload direct d'Infomaniak est limitée à 1 Go par fichier pour garantir l'intégrité du transfert. Pour les fichiers plus volumineux, découpez l'archive ou utilisez la synchronisation WebDAV.".to_string());
    }

    let file_name = path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .ok_or_else(|| "Nom de fichier invalide.".to_string())?;

    // Détermination du dossier cible sur kDrive.
    // kDrive interdit l'upload direct à la racine globale (ID 1) : il faut cibler un sous-dossier inscriptible.
    let mut target_dir_id = target_folder_id_opt.unwrap_or(0);
    if target_dir_id <= 1 {
        // Interroger les sous-dossiers de la racine pour trouver un dossier inscriptible (comme 'Private')
        let root_files_url = format!("https://api.infomaniak.com/3/drive/{}/files/1/files", account.drive_id);
        if let Ok(ref val) = curl_api_get(&account.token, &root_files_url) {
            if let Some(arr) = val.get("data").and_then(|d| d.as_array()) {
                for item in arr {
                    let is_dir = item.get("type").and_then(|v| v.as_str()).map(|t| t == "dir").unwrap_or(false);
                    let vis = item.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
                    let id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                    if is_dir && id > 0 {
                        if vis == "is_private_space" || target_dir_id == 0 {
                            target_dir_id = id;
                            if vis == "is_private_space" {
                                break;
                            }
                        }
                    }
                }
            }
        }
        if target_dir_id <= 1 {
            // Repli conventionnel kDrive : dossier 5 = Private
            target_dir_id = 5;
        }
    }

    let encoded_name = percent_encode(&file_name);
    let upload_url = format!(
        "https://api.infomaniak.com/3/drive/{}/upload?directory_id={}&file_name={}&total_size={}",
        account.drive_id, target_dir_id, encoded_name, file_size
    );

    // Timeout proportionnel à la taille (minimum 1800s / 30 min, jusqu'à 2 heures pour 1 Go)
    let timeout_sec = 1800.max(file_size / (50 * 1024));

    let mut child = Command::new("curl")
        .args([
            "-s",
            "-L",
            "--connect-timeout", "30",
            "--retry", "3",
            "--retry-delay", "2",
            "--max-time", &timeout_sec.to_string(),
            "-X", "POST",
            "-H", "Content-Type: application/octet-stream",
            "-K", "-",
            "--data-binary", &format!("@{}", path.display()),
            &upload_url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Erreur lancement du processus de téléversement : {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let config = format!(
            "header = \"Authorization: Bearer {}\"\nheader = \"Accept: application/json\"\n",
            account.token.trim()
        );
        let _ = stdin.write_all(config.as_bytes());
    }

    let output = child.wait_with_output()
        .map_err(|e| format!("Erreur exécution du téléversement : {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Échec du téléversement vers kDrive (erreur réseau ou timeout) : {}", err));
    }

    let resp_text = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("Réponse JSON invalide lors de l'upload : {} (extrait: {})", e, resp_text.chars().take(200).collect::<String>()))?;

    if let Some(res) = val.get("result").and_then(|r| r.as_str()) {
        if res == "error" {
            let code = val.get("error").and_then(|e| e.get("code")).and_then(|c| c.as_str()).unwrap_or("");
            if code == "too_many_requests" {
                return Err("Limite de requêtes Infomaniak atteinte (60 requêtes/minute max). Veuillez patienter quelques instants avant de relancer.".to_string());
            }
            let desc = val.get("error")
                .and_then(|e| e.get("description").or_else(|| e.get("message")))
                .and_then(|m| m.as_str())
                .unwrap_or("Erreur de téléversement kDrive");
            return Err(desc.to_string());
        }
    }

    let data = val.get("data");
    let file_id = data.and_then(|d| d.get("id")).and_then(|v| v.as_u64()).unwrap_or(0);
    let res_name = data.and_then(|d| d.get("name")).and_then(|v| v.as_str()).unwrap_or(&file_name).to_string();
    let res_size = data.and_then(|d| d.get("size")).and_then(|v| v.as_u64()).unwrap_or(file_size);

    Ok(FileEntry {
        name: res_name,
        path: format!("kdrive://{}/{}", account.id, file_id),
        is_dir: false,
        size_bytes: res_size,
        size_human: format_size(res_size),
        modified: "À l'instant".to_string(),
        permissions: "-rw-r--r--".to_string(),
        category: categorize_file(&file_name),
        is_mount_point: false,
    })
}

// Helpers
fn percent_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn chrono_or_date() -> String {
    let output = Command::new("date")
        .args(["+%d/%m/%Y %H:%M"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "Récemment".to_string());
    output
}

fn format_timestamp(sec: i64) -> String {
    let output = Command::new("date")
        .args(["-d", &format!("@{}", sec), "+%d/%m/%Y %H:%M"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "Inconnue".to_string());
    output
}

// =========================================================================
// ÉVALUATION DES PROPRIÉTÉS & CRAWLER KDRIVE AVEC PROTECTION ANTI-FLOOD
// =========================================================================

/// Extrait les propriétés complètes d'un élément kDrive (fichier ou dossier)
/// avec mise en cache transparente et protection anti-rate-limit.
pub fn get_kdrive_item_properties(kdrive_uri: &str) -> Result<FilePropertiesResponse, String> {
    let clean = kdrive_uri.trim().strip_prefix("kdrive://")
        .ok_or_else(|| format!("L'URI '{}' n'est pas un chemin kDrive valide.", kdrive_uri))?;
    let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Err("Identifiant de compte kDrive manquant.".to_string());
    }
    let account_id = parts[0];
    let item_id: u64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    let cache_key = format!("{}:{}", account_id, item_id);
    if let Ok(guard) = KDRIVE_PROPERTIES_CACHE.lock() {
        if let Some(ref map) = *guard {
            if let Some(entry) = map.get(&cache_key) {
                if entry.instant.elapsed().as_secs() < 90 {
                    return Ok(entry.data.clone());
                }
            }
        }
    }

    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| format!("Compte kDrive '{}' introuvable.", account_id))?;

    // Cas Racine du kDrive
    if item_id == 0 || item_id == 1 {
        let props = FilePropertiesResponse {
            name: account.drive_name.clone(),
            path: format!("kdrive://{}/{}", account.id, item_id),
            parent_path: None,
            is_dir: true,
            is_symlink: false,
            is_mount_point: false,
            size_bytes: account.used_size,
            size_human: format_size(account.used_size),
            category: "folder".to_string(),
            mime_type: "inode/directory".to_string(),
            permissions_octal: "0755".to_string(),
            permissions_mode: "drwxr-xr-x".to_string(),
            owner: format!("kDrive ({})", account.name),
            group: "cloud".to_string(),
            modified: account.created_at.clone(),
            created: Some(account.created_at.clone()),
            accessed: None,
            media_info: None,
        };

        if let Ok(mut guard) = KDRIVE_PROPERTIES_CACHE.lock() {
            let map = guard.get_or_insert_with(HashMap::new);
            map.insert(cache_key, KdPropertiesCacheEntry {
                instant: Instant::now(),
                data: props.clone(),
            });
        }

        return Ok(props);
    }

    // Récupération des métadonnées détaillées du fichier ou sous-dossier
    let meta_url = format!("https://api.infomaniak.com/3/drive/{}/files/{}", account.drive_id, item_id);
    let val = curl_api_get(&account.token, &meta_url)
        .map_err(|e| format!("Impossible de lire les métadonnées de l'élément kDrive #{} : {}", item_id, e))?;

    let data = val.get("data").ok_or_else(|| "Données kDrive introuvables.".to_string())?;

    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("Sans nom").to_string();
    let is_dir = data.get("type").and_then(|v| v.as_str()).map(|t| t == "dir").unwrap_or(false)
        || data.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
    let size = data.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
    let parent_id = data.get("parent_id").and_then(|v| v.as_u64());
    let parent_path = parent_id.map(|pid| format!("kdrive://{}/{}", account.id, pid));

    let created_ts = data.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0);
    let mod_ts = data.get("modtime").or_else(|| data.get("updated_at")).and_then(|v| v.as_i64()).unwrap_or(0);
    let modified = if mod_ts > 0 { format_timestamp(mod_ts) } else { "Inconnue".to_string() };
    let created = if created_ts > 0 { Some(format_timestamp(created_ts)) } else { None };

    let ext = Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let category = if is_dir {
        "folder".to_string()
    } else {
        categorize_file(&name)
    };

    let mime_type = if is_dir {
        "inode/directory".to_string()
    } else if let Some(m) = data.get("mimetype").and_then(|v| v.as_str()) {
        if !m.is_empty() { m.to_string() } else { get_mime_type(&ext).to_string() }
    } else {
        get_mime_type(&ext).to_string()
    };

    let mut media_info = None;
    if !is_dir && (category == "video" || category == "audio" || category == "image") {
        let mut mi = MediaMetadata {
            is_media: true,
            media_type: category.clone(),
            video_codec: data.get("video_codec").and_then(|v| v.as_str()).map(|s| s.to_string()),
            resolution: None,
            width: None,
            height: None,
            aspect_ratio: None,
            framerate: None,
            video_bitrate: None,
            pixel_format: None,
            audio_codec: data.get("audio_codec").and_then(|v| v.as_str()).map(|s| s.to_string()),
            audio_channels: None,
            sample_rate: None,
            audio_bitrate: None,
            duration_seconds: None,
            duration_human: None,
            overall_bitrate: None,
            container_format: if !ext.is_empty() { Some(ext.to_uppercase()) } else { None },
            title: None,
            artist: None,
            album: None,
            year: None,
            genre: None,
            comment: None,
        };

        let w = data.get("width").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(0);
        let h = data.get("height").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(0);
        if w > 0 && h > 0 {
            mi.width = Some(w);
            mi.height = Some(h);
            mi.resolution = Some(format!("{} × {}", w, h));
            mi.aspect_ratio = calculate_aspect_ratio(w, h);
        }

        if let Some(dur_val) = data.get("duration").and_then(|v| v.as_f64()).or_else(|| data.get("duration").and_then(|v| v.as_u64()).map(|v| v as f64)) {
            if dur_val > 0.0 {
                mi.duration_seconds = Some(dur_val);
                mi.duration_human = Some(format_duration(dur_val));
            }
        }

        let has_media_data = mi.resolution.is_some() || mi.duration_seconds.is_some() || mi.video_codec.is_some() || mi.audio_codec.is_some();
        if has_media_data {
            media_info = Some(mi);
        }
    }

    let props = FilePropertiesResponse {
        name,
        path: format!("kdrive://{}/{}", account.id, item_id),
        parent_path,
        is_dir,
        is_symlink: false,
        is_mount_point: false,
        size_bytes: size,
        size_human: if is_dir { "--".to_string() } else { format_size(size) },
        category,
        mime_type,
        permissions_octal: if is_dir { "0755".to_string() } else { "0644".to_string() },
        permissions_mode: if is_dir { "drwxr-xr-x".to_string() } else { "-rw-r--r--".to_string() },
        owner: format!("kDrive ({})", account.name),
        group: "cloud".to_string(),
        modified,
        created,
        accessed: None,
        media_info,
    };

    if let Ok(mut guard) = KDRIVE_PROPERTIES_CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(cache_key, KdPropertiesCacheEntry {
            instant: Instant::now(),
            data: props.clone(),
        });
    }

    Ok(props)
}

/// Résout instantanément les éléments directs d'une sélection kDrive (séparant fichiers et dossiers).
pub fn resolve_kdrive_selection_direct_items(paths: &[String]) -> (Vec<(String, u64)>, Vec<(String, u64, String)>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();

    for p in paths {
        if let Ok(props) = get_kdrive_item_properties(p) {
            let clean = p.trim().strip_prefix("kdrive://").unwrap_or("");
            let parts: Vec<&str> = clean.split('/').collect();
            let account_id = parts.first().unwrap_or(&"").to_string();
            let item_id: u64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

            if props.is_dir {
                dirs.push((account_id, item_id, props.name));
            } else {
                files.push((props.name, props.size_bytes));
            }
        }
    }

    (files, dirs)
}

/// Explore récursivement les dossiers kDrive de manière asynchrone avec pacing anti-rate-limit,
/// protection de mémoire et seuil d'appels API.
pub async fn crawl_kdrive_dirs(
    dirs: Vec<(String, u64, String)>,
    total_files: Arc<AtomicU64>,
    total_dirs: Arc<AtomicU64>,
    total_bytes: Arc<AtomicU64>,
    current_path: Arc<Mutex<Option<String>>>,
    is_cancelled: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    let mut queue = dirs.into_iter().map(|(acc, id, name)| (acc, id, name, 1usize)).collect::<Vec<_>>();
    let mut api_calls_count = 0usize;
    const MAX_API_CALLS: usize = 20;
    const MAX_DEPTH: usize = 3;

    while let Some((acc_id, f_id, f_name, depth)) = queue.pop() {
        if is_cancelled.load(Ordering::Relaxed) {
            break;
        }

        if let Ok(mut cp) = current_path.lock() {
            *cp = Some(format!("kDrive: {}", f_name));
        }

        let cached_children = {
            let guard = KDRIVE_CHILDREN_CACHE.lock().unwrap();
            guard.as_ref().and_then(|m| m.get(&(acc_id.clone(), f_id)).cloned())
        };

        let entries_to_process = if let Some((instant, entries)) = cached_children {
            if instant.elapsed().as_secs() < 120 {
                Some(entries)
            } else {
                None
            }
        } else {
            None
        };

        let entries = match entries_to_process {
            Some(e) => e,
            None => {
                if api_calls_count >= MAX_API_CALLS {
                    if let Ok(mut err_guard) = error.lock() {
                        if err_guard.is_none() {
                            *err_guard = Some("Quota API préservé : calcul limité aux 20 premiers sous-dossiers kDrive.".to_string());
                        }
                    }
                    continue;
                }

                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                if is_cancelled.load(Ordering::Relaxed) {
                    break;
                }

                api_calls_count += 1;
                let acc_id_clone = acc_id.clone();
                let list_res = tokio::task::spawn_blocking(move || {
                    list_kdrive_folder(&acc_id_clone, Some(f_id))
                }).await;

                match list_res {
                    Ok(Ok(listing)) => listing.entries,
                    Ok(Err(e)) => {
                        if e.contains("429") || e.to_lowercase().contains("too many requests") || e.to_lowercase().contains("rate limit") {
                            if let Ok(mut err_guard) = error.lock() {
                                *err_guard = Some("Limite de requêtes kDrive atteinte (HTTP 429). Pause de protection activée.".to_string());
                            }
                            break;
                        }
                        continue;
                    }
                    Err(_) => continue,
                }
            }
        };

        for item in entries {
            if is_cancelled.load(Ordering::Relaxed) {
                break;
            }

            if item.is_dir {
                total_dirs.fetch_add(1, Ordering::Relaxed);
                if depth < MAX_DEPTH {
                    let child_id: u64 = item.path.replace("kdrive://", "")
                        .split('/')
                        .nth(1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    if child_id > 0 {
                        queue.push((acc_id.clone(), child_id, item.name, depth + 1));
                    }
                }
            } else {
                total_files.fetch_add(1, Ordering::Relaxed);
                total_bytes.fetch_add(item.size_bytes, Ordering::Relaxed);
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    }
}

