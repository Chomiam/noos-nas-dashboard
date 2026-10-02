use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use crate::files::{categorize_file, format_size, FileEntry};

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
}

#[derive(Debug, Deserialize)]
pub struct CreateKDriveAccountRequest {
    pub name: String,
    pub token: String,
    pub drive_id: u64,
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
    let mut args: Vec<String> = vec![
        "-s".into(),
        "-L".into(),
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

    let mut child = Command::new("curl")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
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
pub fn test_and_fetch_drives(token: &str) -> Result<Vec<KDriveDriveInfo>, String> {
    let clean_token = token.trim();
    if clean_token.is_empty() {
        return Err("Le jeton API ne peut pas être vide.".into());
    }

    let val = curl_api_get(clean_token, "https://api.infomaniak.com/1/drive")?;
    let data_arr = val.get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "Aucun kDrive trouvé pour ce compte Infomaniak.".to_string())?;

    let mut drives = Vec::new();
    for item in data_arr {
        let id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        if id == 0 {
            continue;
        }
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("Mon kDrive").to_string();
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

    if drives.is_empty() {
        return Err("Le jeton est valide mais aucun kDrive n'est associé à ce compte.".into());
    }

    Ok(drives)
}

/// Enregistre un nouveau compte kDrive sur le NAS et renvoie son profil public masqué.
pub fn add_kdrive_account(name: &str, token: &str, drive_id: u64) -> Result<KDriveAccountPublic, String> {
    let clean_token = token.trim();
    let drives = test_and_fetch_drives(clean_token)?;
    let drive = drives.iter().find(|d| d.id == drive_id)
        .ok_or_else(|| format!("Le kDrive avec l'ID {} est introuvable sur ce compte.", drive_id))?;

    let slug = name.to_lowercase().replace(|c: char| !c.is_alphanumeric() && c != '-', "-");
    let account_id = format!("kdrive-{}", if slug.is_empty() { format!("{}", drive_id) } else { slug });

    let now_str = chrono_or_date();
    let account = KDriveAccount {
        id: account_id.clone(),
        name: if name.trim().is_empty() { drive.name.clone() } else { name.trim().to_string() },
        token: clean_token.to_string(),
        drive_id,
        drive_name: drive.name.clone(),
        total_size: drive.size,
        used_size: drive.used_size,
        created_at: now_str,
    };

    let mut accounts = load_kdrive_accounts();
    accounts.retain(|a| a.id != account_id);
    accounts.push(account.clone());
    save_kdrive_accounts(&accounts)?;

    Ok(account.to_public())
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

    let url = if folder_id == 0 {
        format!("https://api.infomaniak.com/1/drive/{}/files/search?limit=1000", account.drive_id)
    } else {
        format!("https://api.infomaniak.com/1/drive/{}/files/{}/files?limit=1000", account.drive_id, folder_id)
    };

    let val = match curl_api_get(&account.token, &url) {
        Ok(v) => v,
        Err(_) => {
            let fb_url = format!("https://api.infomaniak.com/1/drive/{}/files/search?parent_id={}&limit=1000", account.drive_id, folder_id);
            curl_api_get(&account.token, &fb_url)?
        }
    };

    let items = val.get("data").and_then(|d| d.as_array());
    let mut entries = Vec::new();
    let mut total_size_bytes: u64 = 0;

    let mut current_name = "Racine".to_string();
    let mut parent_folder_id: Option<u64> = None;

    if folder_id != 0 {
        let meta_url = format!("https://api.infomaniak.com/1/drive/{}/files/{}", account.drive_id, folder_id);
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

    let url = format!("https://api.infomaniak.com/1/drive/{}/files/directory", account.drive_id);
    let body = serde_json::json!({
        "name": clean_name,
        "parent_id": if parent_id == 0 { serde_json::Value::Null } else { serde_json::json!(parent_id) }
    });

    let _ = curl_api_post_json(&account.token, &url, &body)?;
    Ok(())
}

/// Supprime un élément (fichier ou dossier) dans kDrive (vers la corbeille kDrive).
pub fn delete_kdrive_item(account_id: &str, file_id: u64) -> Result<(), String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| "Compte kDrive introuvable.".to_string())?;

    let trash_url = format!("https://api.infomaniak.com/1/drive/{}/files/{}/trash", account.drive_id, file_id);
    if curl_api_post_json(&account.token, &trash_url, &serde_json::json!({})).is_err() {
        let del_url = format!("https://api.infomaniak.com/1/drive/{}/files/{}", account.drive_id, file_id);
        curl_api_delete(&account.token, &del_url)?;
    }

    Ok(())
}

/// Télécharge un fichier kDrive dans un fichier temporaire sécurisé pour le servir au navigateur.
pub fn download_kdrive_file_to_temp(account_id: &str, file_id: u64) -> Result<(PathBuf, String), String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| "Compte kDrive introuvable.".to_string())?;

    let meta_url = format!("https://api.infomaniak.com/1/drive/{}/files/{}", account.drive_id, file_id);
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

    let dl_url = format!("https://api.infomaniak.com/1/drive/{}/files/{}/download", account.drive_id, file_id);
    curl_secure_request(&account.token, "GET", &dl_url, None, 600, Some(&temp_file.display().to_string()))?;

    Ok((temp_file, sanitized_name))
}

/// Copie un fichier depuis kDrive directement vers un répertoire du NAS en tâche de fond.
pub fn copy_kdrive_file_to_nas(account_id: &str, file_id: u64, file_name: &str, dest_dir: &str) -> Result<String, String> {
    let accounts = load_kdrive_accounts();
    let account = accounts.iter().find(|a| a.id == account_id)
        .ok_or_else(|| "Compte kDrive introuvable.".to_string())?;

    let target_dest_dir = PathBuf::from(dest_dir);
    if !target_dest_dir.is_dir() {
        return Err(format!("Le dossier de destination '{}' n'existe pas sur le NAS.", dest_dir));
    }

    let sanitized_name = Path::new(file_name).file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("kdrive_file_{}", file_id));

    let final_dest_path = target_dest_dir.join(&sanitized_name);
    let final_path_str = final_dest_path.display().to_string();

    let dl_api_url = format!("https://api.infomaniak.com/1/drive/{}/files/{}/download", account.drive_id, file_id);

    curl_secure_request(
        &account.token,
        "GET",
        &dl_api_url,
        None,
        600,
        Some(&final_path_str),
    )?;

    Ok(final_path_str)
}

// Helpers
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
