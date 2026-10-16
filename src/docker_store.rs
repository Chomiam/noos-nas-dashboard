use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreVolume {
    pub host: String,
    pub container: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreEnv {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreApp {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub category: String,
    #[serde(default)]
    pub tagline: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub default_port: u32,
    #[serde(default)]
    pub extra_ports: Vec<u32>,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub media_support: bool,
    #[serde(default)]
    pub volumes: Vec<StoreVolume>,
    #[serde(default)]
    pub compose_file: String,
    #[serde(default)]
    pub env: Vec<StoreEnv>,

    // Champs calculés dynamiquement
    #[serde(default)]
    pub is_installed: bool,
    #[serde(default)]
    pub is_running: bool,
    #[serde(default)]
    pub container_id: Option<String>,
    #[serde(default)]
    pub container_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreCatalog {
    pub version: String,
    pub updated_at: String,
    pub repository: String,
    #[serde(default)]
    pub total_apps: usize,
    pub categories: Vec<String>,
    pub apps: Vec<StoreApp>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct InstallAppRequest {
    pub app_id: String,
    pub port: Option<u32>,
    pub data_dir: Option<String>,
    pub media_dir: Option<String>,
    pub gpu_device: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    #[serde(default)]
    pub extra_volumes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UninstallAppRequest {
    pub app_id: String,
    #[serde(default)]
    pub delete_data: bool,
}

#[derive(Debug, Deserialize)]
pub struct ContainerActionRequest {
    pub action: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct DockerActionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerStoreDeployStatus {
    pub app_id: String,
    pub status: String, // "installing" | "ready" | "failed"
    pub step: u8,
    pub progress_percent: u8,
    pub message: String,
    pub started_at: u64,
    pub is_running: bool,
    #[serde(default)]
    pub error: Option<String>,
}

static STORE_DEPLOY_TRACKER: LazyLock<Mutex<HashMap<String, DockerStoreDeployStatus>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn get_all_store_deployments() -> Vec<DockerStoreDeployStatus> {
    let tracker = STORE_DEPLOY_TRACKER.lock().unwrap();
    let mut list: Vec<DockerStoreDeployStatus> = tracker.values().cloned().collect();
    drop(tracker);

    let containers = get_running_containers_map();
    for d in &mut list {
        if containers.contains_key(&d.app_id) {
            d.is_running = true;
        }
    }
    list
}

pub fn get_store_deployment_status(app_id: &str) -> Option<DockerStoreDeployStatus> {
    let clean_id = app_id.trim().to_lowercase();
    let tracker = STORE_DEPLOY_TRACKER.lock().unwrap();
    if let Some(mut st) = tracker.get(&clean_id).cloned() {
        drop(tracker);
        let containers = get_running_containers_map();
        if containers.contains_key(&clean_id) {
            st.is_running = true;
        }
        return Some(st);
    }
    drop(tracker);

    let containers = get_running_containers_map();
    if containers.contains_key(&clean_id) {
        return Some(DockerStoreDeployStatus {
            app_id: clean_id,
            status: "ready".into(),
            step: 4,
            progress_percent: 100,
            message: "Application active".into(),
            started_at: 0,
            is_running: true,
            error: None,
        });
    }
    None
}

fn get_config_dir() -> PathBuf {
    crate::updates::resolve_config_dir()
}

fn get_target_user() -> String {
    crate::updates::target_user()
}

pub fn resolve_store_app_info(project_or_name: &str) -> Option<(String, String, String)> {
    let clean = project_or_name.trim().trim_start_matches('/').to_lowercase();
    let stripped = clean.strip_prefix("docker-").unwrap_or(&clean);

    let cache_file_noos = Path::new("/var/cache/noos-nas-dashboard/store_cache.json");
    let catalog: StoreCatalog = if let Ok(txt) = std::fs::read_to_string(cache_file_noos) {
        serde_json::from_str(&txt).unwrap_or_else(|_| get_default_catalog())
    } else {
        get_default_catalog()
    };

    for app in catalog.apps {
        let app_id = app.id.to_lowercase();
        if app_id == clean
            || app_id == stripped
            || stripped.starts_with(&format!("{}-", app_id))
            || stripped.starts_with(&format!("{}_", app_id))
        {
            return Some((app.id, app.name, app.icon));
        }
    }
    None
}

pub fn get_running_containers_map() -> HashMap<String, (String, String, bool)> {
    let mut map = HashMap::new();
    if let Ok(output) = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.Status}}\t{{.State}}\t{{.Labels}}"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                let id = parts[0].to_string();
                let name = parts[1].trim_start_matches('/').to_string();
                let status = parts[2].to_string();
                let is_running = parts[3].to_lowercase() == "running";

                let clean_name = name.strip_prefix("docker-").unwrap_or(&name).to_string();
                map.insert(clean_name.to_lowercase(), (id.clone(), status.clone(), is_running));
                map.insert(name.to_lowercase(), (id.clone(), status.clone(), is_running));

                // Extraire le label com.docker.compose.project
                if parts.len() >= 5 {
                    for label in parts[4].split(',') {
                        if let Some((k, v)) = label.split_once('=') {
                            if k.trim() == "com.docker.compose.project" {
                                let proj = v.trim().to_lowercase();
                                if !proj.is_empty() {
                                    map.insert(proj, (id.clone(), status.clone(), is_running));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    map
}

pub fn get_store_catalog() -> StoreCatalog {
    let cache_dir_noos = Path::new("/var/cache/noos-nas-dashboard");
    let cache_file_noos = cache_dir_noos.join("store_cache.json");

    // 1. Tenter la récupération depuis GitHub (noos_nas_store)
    let url = "https://raw.githubusercontent.com/Chomiam/noos_nas_store/main/store.json";

    let mut json_text = None;
    if let Ok(out) = Command::new("curl")
        .args(["-s", "-L", "--connect-timeout", "4", "--max-time", "8", url])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        if text.trim().starts_with('{') {
            let _ = std::fs::create_dir_all(cache_dir_noos);
            let _ = std::fs::write(&cache_file_noos, &text);
            json_text = Some(text);
        }
    }

    // 2. Repli sur le cache local
    let json_text = json_text
        .or_else(|| std::fs::read_to_string(&cache_file_noos).ok());

    // 3. Parser ou repli sur catalogue par défaut
    let mut catalog: StoreCatalog = if let Some(txt) = json_text {
        serde_json::from_str(&txt).unwrap_or_else(|_| get_default_catalog())
    } else {
        get_default_catalog()
    };

    // 4. Enrichir avec l'état dynamique des conteneurs et dossiers
    let config_dir = get_config_dir();
    let nix_docker_dir = config_dir.join("docker");
    let containers_map = get_running_containers_map();

    for app in &mut catalog.apps {
        let nix_file = nix_docker_dir.join(format!("{}.nix", app.id));
        let container_info = containers_map.get(&app.id.to_lowercase());

        // Une application est installée UNIQUEMENT si un conteneur existe dans Docker
        // ou si elle est déclarée dans un module NixOS.
        if let Some((cid, status, is_running)) = container_info {
            app.is_installed = true;
            app.is_running = *is_running;
            app.container_id = Some(cid.clone());
            app.container_status = Some(status.clone());
        } else if nix_file.exists() {
            app.is_installed = true;
            app.is_running = false;
            app.container_id = None;
            app.container_status = None;
        } else {
            app.is_installed = false;
            app.is_running = false;
            app.container_id = None;
            app.container_status = None;
        }
    }

    catalog
}

/// Vérifie si un fichier docker-compose possède une section de périphériques matériels (GPU/devices).
pub fn check_compose_has_device_section(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        let code_part = trimmed.trim_start_matches('#').trim();
        if code_part.starts_with("devices:")
            || code_part == "devices:"
            || code_part.starts_with("device:")
            || code_part.contains("/dev/dri")
            || code_part.contains("device_ids:")
            || code_part.contains("capabilities: [gpu]")
            || code_part.contains("capabilities: [\"gpu\"]")
            || (code_part.starts_with("- ") && code_part.contains("/dev/"))
        {
            return true;
        }
    }
    false
}

/// Récupère le contenu d'un compose d'application Store (local ou distant) et détecte la présence de devices GPU.
pub fn get_app_compose_info(app_id: &str) -> (bool, Option<String>) {
    let clean_id = app_id.trim().to_lowercase();
    let user = get_target_user();

    // 1. Vérifier si un fichier compose.yaml local existe déjà dans le home de l'utilisateur
    let local_compose = PathBuf::from(format!("/home/{}/docker/{}/compose.yaml", user, clean_id));
    if local_compose.is_file() {
        if let Ok(txt) = std::fs::read_to_string(&local_compose) {
            let has_device = check_compose_has_device_section(&txt);
            return (has_device, Some(txt));
        }
    }

    let local_yml = PathBuf::from(format!("/home/{}/docker/{}/docker-compose.yml", user, clean_id));
    if local_yml.is_file() {
        if let Ok(txt) = std::fs::read_to_string(&local_yml) {
            let has_device = check_compose_has_device_section(&txt);
            return (has_device, Some(txt));
        }
    }

    // 2. Vérifier le cache dans /var/cache/noos-nas-dashboard/
    let cache_dir = Path::new("/var/cache/noos-nas-dashboard");
    let cache_file = cache_dir.join(format!("compose_{}.yaml", clean_id));
    if cache_file.is_file() {
        if let Ok(txt) = std::fs::read_to_string(&cache_file) {
            let has_device = check_compose_has_device_section(&txt);
            return (has_device, Some(txt));
        }
    }

    // 3. Télécharger depuis noos_nas_store GitHub
    let url = format!("https://raw.githubusercontent.com/Chomiam/noos_nas_store/main/apps/{}/compose.yaml", clean_id);
    if let Ok(out) = Command::new("curl")
        .args(["-s", "-L", "--connect-timeout", "4", "--max-time", "8", &url])
        .output()
    {
        let txt = String::from_utf8_lossy(&out.stdout).to_string();
        if txt.contains("services:") {
            let _ = std::fs::create_dir_all(cache_dir);
            let _ = std::fs::write(&cache_file, &txt);
            let has_device = check_compose_has_device_section(&txt);
            return (has_device, Some(txt));
        }
    }

    (false, None)
}

/// Génère une clé ou un mot de passe cryptographiquement sécurisé selon le type demandé.
pub fn generate_secret_key(key_type: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let pid = std::process::id();
    let mut random_bytes = [0u8; 64];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut random_bytes);
    } else {
        for (i, b) in random_bytes.iter_mut().enumerate() {
            *b = ((nanos.wrapping_add((i as u128).wrapping_mul(31)).wrapping_add(pid as u128)) & 0xFF) as u8;
        }
    }

    let lower_type = key_type.to_lowercase();
    if lower_type.contains("hex64") || lower_type.contains("hex_64") {
        // 32 octets = 64 caractères hexadécimaux
        random_bytes[..32].iter().map(|b| format!("{:02x}", b)).collect()
    } else if lower_type.contains("hex128") || lower_type.contains("hex_128") {
        // 64 octets = 128 caractères hexadécimaux
        random_bytes.iter().map(|b| format!("{:02x}", b)).collect()
    } else if lower_type.contains("hex") {
        // 16 octets = 32 caractères hexadécimaux (hex32 ou hex par défaut)
        random_bytes[..16].iter().map(|b| format!("{:02x}", b)).collect()
    } else if lower_type.contains("base64") {
        const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut res = String::new();
        for chunk in random_bytes[..32].chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
            res.push(B64_CHARS[(b0 >> 2) & 0x3F] as char);
            res.push(B64_CHARS[((b0 & 0x03) << 4) | ((b1 >> 4) & 0x0F)] as char);
            if chunk.len() > 1 {
                res.push(B64_CHARS[((b1 & 0x0F) << 2) | ((b2 >> 6) & 0x03)] as char);
            } else {
                res.push('=');
            }
            if chunk.len() > 2 {
                res.push(B64_CHARS[b2 & 0x3F] as char);
            } else {
                res.push('=');
            }
        }
        res
    } else if lower_type.contains("uuid") {
        let mut u = [0u8; 16];
        u.copy_from_slice(&random_bytes[..16]);
        u[6] = (u[6] & 0x0F) | 0x40; // Version 4
        u[8] = (u[8] & 0x3F) | 0x80; // Variant RFC 4122
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            u[0], u[1], u[2], u[3], u[4], u[5], u[6], u[7], u[8], u[9], u[10], u[11], u[12], u[13], u[14], u[15]
        )
    } else {
        // Mot de passe alphanumérique robuste de 24 caractères (A-Z, a-z, 0-9) sans caractères spéciaux brisant docker
        const ALPHA_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut res = String::new();
        for &b in &random_bytes[..24] {
            res.push(ALPHA_CHARS[(b as usize) % ALPHA_CHARS.len()] as char);
        }
        res
    }
}

/// Détermine si une variable d'environnement doit être auto-générée et génère sa valeur le cas échéant.
pub fn should_generate_secret(key_name: &str, key_type: Option<&str>, current_val: &str) -> Option<String> {
    let val = current_val.trim();
    if val.starts_with("{{GENERATE_") || val == "generate" || val == "auto" {
        let requested_type = if val.contains("HEX_64") || val.contains("HEX64") {
            "hex64"
        } else if val.contains("HEX") {
            "hex32"
        } else if val.contains("BASE64") {
            "base64"
        } else if val.contains("UUID") {
            "uuid"
        } else if val.contains("SECRET") || val.contains("JWT") {
            "base64"
        } else {
            key_type.unwrap_or("password")
        };
        return Some(generate_secret_key(requested_type));
    }

    if let Some(t) = key_type {
        let lt = t.to_lowercase();
        if (val.is_empty() || val == "changeme" || val == "secret" || val == "password")
            && (lt == "password" || lt == "secret" || lt == "hex" || lt == "hex32" || lt == "hex64" || lt == "base64" || lt == "uuid")
        {
            return Some(generate_secret_key(&lt));
        }
    }

    let kn = key_name.to_uppercase();
    if val.is_empty() {
        if kn.contains("PASSWORD") || kn.contains("PASSWD") || kn.ends_with("_PASS") {
            return Some(generate_secret_key("password"));
        } else if kn.contains("SECRET_KEY") || kn.contains("JWT_SECRET") || kn.contains("AUTH_SECRET") || kn.contains("ENCRYPTION_KEY") {
            return Some(generate_secret_key("base64"));
        } else if kn.contains("UUID") || kn.contains("GUID") {
            return Some(generate_secret_key("uuid"));
        }
    }

    None
}

fn customize_compose_yaml(
    base_compose: &str,
    new_port: Option<u32>,
    env_vars: Option<&HashMap<String, String>>,
    gpu_device: Option<&str>,
    extra_volumes: Option<&[String]>,
) -> String {
    let mut lines: Vec<String> = base_compose.lines().map(|s| s.to_string()).collect();

    // 0. Assainissement proactif des volumes localtime défectueux (ex: ./data/localtime -> /etc/localtime:ro)
    for line in &mut lines {
        if line.contains("localtime") && (line.contains("./data/localtime") || line.contains("data/localtime")) {
            if line.contains("/etc/localtime") {
                *line = line.replace("./data/localtime:/etc/localtime:ro", "/etc/localtime:/etc/localtime:ro")
                            .replace("./data/localtime:/etc/localtime", "/etc/localtime:/etc/localtime:ro")
                            .replace("data/localtime:/etc/localtime:ro", "/etc/localtime:/etc/localtime:ro")
                            .replace("data/localtime:/etc/localtime", "/etc/localtime:/etc/localtime:ro");
            }
        }
    }

    // 1. Remplacer le port d'hôte Web si spécifié (sans écraser les ports DNS 53, DHCP ou DoT)
    if let Some(p) = new_port {
        let mut port_replaced = false;
        let p_str = p.to_string();
        let web_indicators = [p_str.as_str(), "2283", "3000", "80", "8080", "443", "8096", "9000", "8443", "5000"];
        for line in &mut lines {
            if !port_replaced && line.trim().starts_with("- ") && line.contains(':') {
                let trimmed = line.trim().trim_start_matches("- ").trim_matches('"').trim_matches('\'');
                if let Some(colon) = trimmed.find(':') {
                    let cont_chunk = &trimmed[colon + 1..];
                    let cont_port = cont_chunk.split('/').next().unwrap_or("").trim();
                    if web_indicators.iter().any(|&ind| ind == cont_port) {
                        let indent = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
                        *line = format!("{}- \"{}:{}\"", indent, p, cont_chunk);
                        port_replaced = true;
                    }
                }
            }
        }

        if !port_replaced {
            for line in &mut lines {
                if !port_replaced && line.trim().starts_with("- ") && line.contains(':') {
                    let trimmed = line.trim().trim_start_matches("- ").trim_matches('"').trim_matches('\'');
                    if let Some(colon) = trimmed.find(':') {
                        let host_chunk = &trimmed[..colon];
                        let cont_chunk = &trimmed[colon + 1..];
                        let cont_port = cont_chunk.split('/').next().unwrap_or("").trim();
                        if cont_port != "53" && cont_port != "67" && cont_port != "68" && cont_port != "853" && host_chunk.chars().all(|c| c.is_ascii_digit()) {
                            let indent = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
                            *line = format!("{}- \"{}:{}\"", indent, p, cont_chunk);
                            port_replaced = true;
                        }
                    }
                }
            }
        }
    }

    // 2. Mettre à jour les variables d'environnement avec conservation stricte de l'indentation YAML
    if let Some(envs) = env_vars {
        if !envs.is_empty() {
            let mut env_idx = None;
            for (idx, line) in lines.iter().enumerate() {
                if line.trim() == "environment:" {
                    env_idx = Some(idx);
                    break;
                }
            }

            if let Some(e_idx) = env_idx {
                let header_indent = lines[e_idx].chars().take_while(|c| c.is_whitespace()).collect::<String>();
                let mut existing_keys = HashMap::new();
                for i in (e_idx + 1)..lines.len() {
                    let l = &lines[i];
                    if !l.trim().starts_with("- ") {
                        break;
                    }
                    let item = l.trim().trim_start_matches("- ").trim();
                    if let Some(eq) = item.find('=') {
                        let k = item[..eq].trim();
                        existing_keys.insert(k.to_string(), i);
                    }
                }

                for (k, v) in envs {
                    let clean_k = k.trim();
                    let clean_v = v.trim();
                    if let Some(&line_num) = existing_keys.get(clean_k) {
                        let orig_indent = lines[line_num].chars().take_while(|c| c.is_whitespace()).collect::<String>();
                        let indent = if orig_indent.is_empty() { format!("{}  ", header_indent) } else { orig_indent };
                        lines[line_num] = format!("{}- {}={}", indent, clean_k, clean_v);
                    } else {
                        let item_indent = format!("{}  ", header_indent);
                        lines.insert(e_idx + 1, format!("{}- {}={}", item_indent, clean_k, clean_v));
                    }
                }
            }
        }
    }

    // 3. Remplacement des variables génériques non résolues du template (ex: ${PORT})
    if let Some(p) = new_port {
        for line in &mut lines {
            if line.contains("${PORT") {
                *line = line.replace("${PORT}", &p.to_string())
                            .replace("${PORT:-3001}", &p.to_string())
                            .replace("${PORT:-2283}", &p.to_string())
                            .replace("${PORT:-8080}", &p.to_string());
            }
        }
    }

    // 4. Injection conditionnelle de l'accélération matérielle GPU si demandée
    if let Some(gpu) = gpu_device {
        let clean_gpu = gpu.trim();
        if !clean_gpu.is_empty() && clean_gpu != "none" {
            // Identifier le service cible pour injecter le GPU
            let mut target_service_idx = None;
            let mut in_services = false;
            let mut current_service_idx = None;

            for (idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed == "services:" {
                    in_services = true;
                    continue;
                }
                if in_services && line.starts_with("  ") && !line.starts_with("    ") && trimmed.ends_with(':') {
                    let s_name = trimmed.trim_end_matches(':').trim();
                    current_service_idx = Some(idx);
                    if s_name == "immich-server" || s_name == "jellyfin" || s_name == "plex" || s_name == "emby" || s_name == "ollama" {
                        target_service_idx = Some(idx);
                        break;
                    }
                    if target_service_idx.is_none() {
                        target_service_idx = Some(idx);
                    }
                }
            }

            if let Some(s_idx) = target_service_idx.or(current_service_idx) {
                let mut insert_pos = lines.len();
                for i in (s_idx + 1)..lines.len() {
                    let l = &lines[i];
                    if l.starts_with("  ") && !l.starts_with("    ") && l.trim().ends_with(':') {
                        insert_pos = i;
                        break;
                    }
                }

                let already_has_gpu = lines[s_idx..insert_pos].iter().any(|l| {
                    l.contains("/dev/dri") || l.contains("driver: nvidia")
                });

                if !already_has_gpu {
                    if clean_gpu == "--gpus all" || clean_gpu == "nvidia" {
                        lines.insert(insert_pos, "    deploy:".to_string());
                        lines.insert(insert_pos + 1, "      resources:".to_string());
                        lines.insert(insert_pos + 2, "        reservations:".to_string());
                        lines.insert(insert_pos + 3, "          devices:".to_string());
                        lines.insert(insert_pos + 4, "            - driver: nvidia".to_string());
                        lines.insert(insert_pos + 5, "              count: all".to_string());
                        lines.insert(insert_pos + 6, "              capabilities: [gpu, video]".to_string());
                    } else {
                        let dev_path = if clean_gpu.contains("/dev/dri") { clean_gpu } else { "/dev/dri:/dev/dri" };
                        lines.insert(insert_pos, "    devices:".to_string());
                        lines.insert(insert_pos + 1, format!("      - {}", dev_path));
                    }
                }
            }
        }
    }

    // 5. Injection des volumes supplémentaires choisis par l'utilisateur
    if let Some(vols) = extra_volumes {
        if !vols.is_empty() {
            let mut target_service_idx = None;
            let mut in_services = false;
            let mut current_service_idx = None;

            for (idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed == "services:" {
                    in_services = true;
                    continue;
                }
                if in_services && line.starts_with("  ") && !line.starts_with("    ") && trimmed.ends_with(':') {
                    let s_name = trimmed.trim_end_matches(':').trim();
                    current_service_idx = Some(idx);
                    if s_name == "immich-server" || s_name == "jellyfin" || s_name == "plex" || s_name == "emby" || s_name == "nextcloud" {
                        target_service_idx = Some(idx);
                        break;
                    }
                    if target_service_idx.is_none() {
                        target_service_idx = Some(idx);
                    }
                }
            }

            if let Some(s_idx) = target_service_idx.or(current_service_idx) {
                let mut vol_header_idx = None;
                let mut service_end_idx = lines.len();

                for i in (s_idx + 1)..lines.len() {
                    let l = &lines[i];
                    if l.starts_with("  ") && !l.starts_with("    ") && l.trim().ends_with(':') {
                        service_end_idx = i;
                        break;
                    }
                    if l.trim() == "volumes:" {
                        vol_header_idx = Some(i);
                    }
                }

                if let Some(v_idx) = vol_header_idx {
                    for vol in vols {
                        let clean_v = vol.trim();
                        if !clean_v.is_empty() && !lines[s_idx..service_end_idx].iter().any(|l| l.contains(clean_v)) {
                            lines.insert(v_idx + 1, format!("      - {}", clean_v));
                        }
                    }
                } else {
                    lines.insert(service_end_idx, "    volumes:".to_string());
                    for (offset, vol) in vols.iter().enumerate() {
                        let clean_v = vol.trim();
                        if !clean_v.is_empty() {
                            lines.insert(service_end_idx + 1 + offset, format!("      - {}", clean_v));
                        }
                    }
                }
            }
        }
    }

    lines.join("\n") + "\n"
}

fn validate_safe_data_dir(d: &str, user: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(d);
    for comp in p.components() {
        if comp.as_os_str() == ".." {
            return Err("Le chemin spécifié ne peut pas contenir de composant relatif ('..').".into());
        }
    }
    let s = p.to_string_lossy();
    if !p.is_absolute() {
        return Err("Le chemin doit être absolu (commencer par /).".into());
    }
    let allowed_prefixes = ["/home/", "/mnt/"];
    let is_allowed = allowed_prefixes.iter().any(|prefix| s.starts_with(prefix));
    if !is_allowed {
        return Err(format!(
            "Le dossier '{}' n'est pas autorisé. Les applications Docker doivent être stockées dans /home/ ou /mnt/.",
            s
        ));
    }
    let forbidden_exact = [
        "/", "/etc", "/var", "/usr", "/nix", "/boot", "/root", "/sys", "/proc", "/dev", "/run", "/tmp",
        "/home", "/mnt", &format!("/home/{}", user),
    ];
    let trimmed = s.trim_end_matches('/');
    if forbidden_exact.iter().any(|&f| trimmed == f) {
        return Err("Le dossier de données ne peut pas être un répertoire système ou la racine du compte utilisateur.".into());
    }
    Ok(p)
}

pub fn install_store_app(req: InstallAppRequest) -> Result<String, String> {
    let clean_id = req.app_id.trim().to_lowercase();
    if clean_id.is_empty() || !clean_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Identifiant d'application invalide".to_string());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // 0. Enregistrer le début du déploiement dans le tracker
    if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
        tracker.insert(
            clean_id.clone(),
            DockerStoreDeployStatus {
                app_id: clean_id.clone(),
                status: "installing".into(),
                step: 1,
                progress_percent: 20,
                message: "Préparation du dossier persistant (~/docker)...".into(),
                started_at: now,
                is_running: false,
                error: None,
            },
        );
    }

    let user = get_target_user();
    let app_dir = if let Some(ref d) = req.data_dir {
        match validate_safe_data_dir(d, &user) {
            Ok(p) => p,
            Err(e) => {
                if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
                    if let Some(entry) = tracker.get_mut(&clean_id) {
                        entry.status = "failed".into();
                        entry.error = Some(e.clone());
                    }
                }
                return Err(e);
            }
        }
    } else {
        PathBuf::from(format!("/home/{}/docker/{}", user, clean_id))
    };

    let data_dir = app_dir.join("data");
    let compose_file = app_dir.join("compose.yaml");

    // 1. Assurer la création des dossiers persistants avec permissions saines
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        let err_msg = format!("Impossible de créer le dossier {} : {}", data_dir.display(), e);
        if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
            if let Some(entry) = tracker.get_mut(&clean_id) {
                entry.status = "failed".into();
                entry.error = Some(err_msg.clone());
            }
        }
        return Err(err_msg);
    }

    // Nettoyage préventif des répertoires erronés créés par de mauvaises tentatives antérieures
    let bad_lt1 = data_dir.join("localtime");
    if bad_lt1.is_dir() {
        let _ = std::fs::remove_dir_all(&bad_lt1);
    }
    let bad_lt2 = app_dir.join("localtime");
    if bad_lt2.is_dir() {
        let _ = std::fs::remove_dir_all(&bad_lt2);
    }

    let _ = Command::new("chown").args(["-R", &format!("{}:users", user), &app_dir.display().to_string()]).status();
    let _ = Command::new("chmod").args(["-R", "0775", &app_dir.display().to_string()]).status();

    // Étape 2 : Configuration des variables et compose.yaml
    if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
        if let Some(entry) = tracker.get_mut(&clean_id) {
            entry.step = 2;
            entry.progress_percent = 45;
            entry.message = "Configuration des variables & ports réseau...".into();
        }
    }

    // 2. Récupérer le manifest de l'application pour connaître les variables et leurs types
    let manifest_url = format!("https://raw.githubusercontent.com/Chomiam/noos_nas_store/main/apps/{}/manifest.json", clean_id);
    let mut fetched_manifest: Option<StoreApp> = None;
    if let Ok(out) = Command::new("curl")
        .args(["-s", "-L", "--connect-timeout", "4", "--max-time", "10", &manifest_url])
        .output()
    {
        let txt = String::from_utf8_lossy(&out.stdout);
        if txt.trim().starts_with('{') {
            fetched_manifest = serde_json::from_str(&txt).ok();
        }
    }
    if fetched_manifest.is_none() {
        let catalog = get_store_catalog();
        fetched_manifest = catalog.apps.into_iter().find(|a| a.id.to_lowercase() == clean_id);
    }

    // 2.b Résoudre et auto-générer les variables d'environnement (.env)
    let mut final_envs: HashMap<String, String> = HashMap::new();
    let mut env_types: HashMap<String, String> = HashMap::new();

    if let Some(ref m) = fetched_manifest {
        for e in &m.env {
            if let Some(ref t) = e.r#type {
                env_types.insert(e.name.clone(), t.clone());
            }
            if !e.default.is_empty() {
                final_envs.insert(e.name.clone(), e.default.clone());
            }
        }
    }

    if let Some(ref user_envs) = req.env_vars {
        for (k, v) in user_envs {
            final_envs.insert(k.clone(), v.clone());
        }
    }

    // Auto-génération des clés selon leur type ou détection heuristique
    for (k, v) in final_envs.iter_mut() {
        let t = env_types.get(k).map(|s| s.as_str());
        if let Some(gen) = should_generate_secret(k, t, v) {
            *v = gen;
        }
    }

    // Pour les variables déclarées dans le manifest mais absentes ou vides dans final_envs
    if let Some(ref m) = fetched_manifest {
        for e in &m.env {
            if !final_envs.contains_key(&e.name) || final_envs.get(&e.name).map(|s| s.trim().is_empty()).unwrap_or(true) {
                if let Some(gen) = should_generate_secret(&e.name, e.r#type.as_deref(), &e.default) {
                    final_envs.insert(e.name.clone(), gen);
                } else if !e.default.is_empty() {
                    final_envs.insert(e.name.clone(), e.default.clone());
                }
            }
        }
    }

    // Harmonisation des variables de bases de données relationnelles (ex: Immich DB_PASSWORD <-> POSTGRES_PASSWORD)
    if let Some(db_pass) = final_envs.get("DB_PASSWORD").cloned() {
        if !db_pass.is_empty() {
            final_envs.entry("POSTGRES_PASSWORD".to_string()).or_insert(db_pass);
        }
    }
    if let Some(db_user) = final_envs.get("DB_USERNAME").cloned() {
        final_envs.entry("POSTGRES_USER".to_string()).or_insert(db_user);
    }
    if let Some(db_name) = final_envs.get("DB_DATABASE_NAME").cloned() {
        final_envs.entry("POSTGRES_DB".to_string()).or_insert(db_name);
    }

    // Variables système Noos NAS standard
    final_envs.entry("TZ".to_string()).or_insert_with(|| "Europe/Paris".to_string());
    final_envs.entry("PUID".to_string()).or_insert_with(|| "1000".to_string());
    final_envs.entry("PGID".to_string()).or_insert_with(|| "100".to_string());
    if let Some(p) = req.port {
        final_envs.insert("PORT".to_string(), p.to_string());
    }

    // 2.c Écrire le fichier .env sécurisé pour ce conteneur
    let env_file = app_dir.join(".env");
    let mut env_content = format!(
        "# =========================================================================\n\
         # ⚙️ Noos NAS Edition — Fichier d'environnement pour {}\n\
         # Généré automatiquement le {}\n\
         # =========================================================================\n\n",
        clean_id,
        now
    );
    let mut sorted_keys: Vec<_> = final_envs.keys().cloned().collect();
    sorted_keys.sort();
    for k in sorted_keys {
        if let Some(v) = final_envs.get(&k) {
            env_content.push_str(&format!("{}={}\n", k, v));
        }
    }
    if let Err(e) = std::fs::write(&env_file, &env_content) {
        eprintln!("Avertissement: Impossible d'écrire {} : {}", env_file.display(), e);
    } else {
        let _ = Command::new("chown").args([&format!("{}:users", user), &env_file.display().to_string()]).status();
        let _ = Command::new("chmod").args(["0600", &env_file.display().to_string()]).status();
    }

    // 2.d Récupérer le compose.yaml depuis noos_nas_store
    let compose_url = format!("https://raw.githubusercontent.com/Chomiam/noos_nas_store/main/apps/{}/compose.yaml", clean_id);

    let mut fetched_compose = None;
    if let Ok(out) = Command::new("curl")
        .args(["-s", "-L", "--connect-timeout", "5", "--max-time", "15", &compose_url])
        .output()
    {
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        if s.contains("services:") {
            fetched_compose = Some(s);
        }
    }

    let base_compose = fetched_compose.unwrap_or_else(|| generate_default_compose(&clean_id, req.port.unwrap_or(8080)));

    // 3. Personnaliser le compose.yaml (port, variables d'environnement, GPU et volumes)
    let customized = customize_compose_yaml(
        &base_compose,
        req.port,
        Some(&final_envs),
        req.gpu_device.as_deref(),
        req.extra_volumes.as_deref(),
    );

    // 4. Écrire le fichier compose.yaml
    if let Err(e) = std::fs::write(&compose_file, &customized) {
        let err_msg = format!("Impossible d'écrire {} : {}", compose_file.display(), e);
        if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
            if let Some(entry) = tracker.get_mut(&clean_id) {
                entry.status = "failed".into();
                entry.error = Some(err_msg.clone());
            }
        }
        return Err(err_msg);
    }

    // 4.b Libération proactive du port 53 si l'application expose le service DNS (AdGuard Home / Pi-hole)
    if clean_id == "adguard" || clean_id == "pihole" || customized.contains(":53") {
        let _ = Command::new("sudo").args(["systemctl", "stop", "systemd-resolved"]).output();
        let _ = Command::new("sudo").args(["mkdir", "-p", "/etc/systemd/resolved.conf.d"]).output();
        let dropin = "[Resolve]\nDNSStubListener=no\n";
        let tmp_dropin = "/tmp/noos-resolved-stub.conf";
        if std::fs::write(tmp_dropin, dropin).is_ok() {
            let _ = Command::new("sudo").args(["cp", tmp_dropin, "/etc/systemd/resolved.conf.d/noos-dns.conf"]).output();
            let _ = std::fs::remove_file(tmp_dropin);
        }
        let _ = Command::new("sudo").args(["systemctl", "restart", "systemd-resolved"]).output();
    }

    // Étape 3 : Démarrage du conteneur (docker compose up -d)
    if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
        if let Some(entry) = tracker.get_mut(&clean_id) {
            entry.step = 3;
            entry.progress_percent = 75;
            entry.message = "Lancement Docker Compose (docker compose up -d)...".into();
        }
    }

    // 5. Déployer instantanément via Docker Compose
    let compose_cmd = Command::new("docker")
        .args(["compose", "-f", &compose_file.display().to_string(), "up", "-d"])
        .output();

    match compose_cmd {
        Ok(out) => {
            if out.status.success() {
                if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
                    if let Some(entry) = tracker.get_mut(&clean_id) {
                        entry.status = "ready".into();
                        entry.step = 4;
                        entry.progress_percent = 100;
                        entry.message = "Application installée et prête".into();
                        entry.is_running = true;
                    }
                }
                Ok(format!("Application '{}' installée et lancée avec succès en 1 clic !", clean_id))
            } else {
                let err = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                let full_log = if err.trim().is_empty() {
                    stdout.to_string()
                } else if stdout.trim().is_empty() {
                    err.to_string()
                } else {
                    format!("{}\n{}", stdout.trim(), err.trim())
                };
                let err_msg = format!("Erreur lors du démarrage Docker Compose :\n{}", full_log.trim());
                if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
                    if let Some(entry) = tracker.get_mut(&clean_id) {
                        entry.status = "failed".into();
                        entry.step = 3;
                        entry.error = Some(err_msg.clone());
                    }
                }
                Err(err_msg)
            }
        }
        Err(e) => {
            let err_msg = format!("Échec d'exécution de docker compose : {}", e);
            if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
                if let Some(entry) = tracker.get_mut(&clean_id) {
                    entry.status = "failed".into();
                    entry.step = 3;
                    entry.error = Some(err_msg.clone());
                }
            }
            Err(err_msg)
        }
    }
}

pub fn uninstall_store_app(app_id: &str, delete_data: bool) -> Result<String, String> {
    let clean_id = app_id.trim().to_lowercase();
    if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
        tracker.remove(&clean_id);
    }
    let user = get_target_user();
    let app_dir = PathBuf::from(format!("/home/{}/docker/{}", user, clean_id));
    let compose_file_yaml = app_dir.join("compose.yaml");
    let compose_file_yml = app_dir.join("docker-compose.yml");

    // 1. Arrêter le conteneur via docker compose down
    if compose_file_yaml.exists() {
        let _ = Command::new("docker")
            .args(["compose", "-f", &compose_file_yaml.display().to_string(), "down", "--remove-orphans"])
            .output();
        let _ = std::fs::remove_file(&compose_file_yaml);
    } else if compose_file_yml.exists() {
        let _ = Command::new("docker")
            .args(["compose", "-f", &compose_file_yml.display().to_string(), "down", "--remove-orphans"])
            .output();
        let _ = std::fs::remove_file(&compose_file_yml);
    } else {
        let _ = Command::new("docker").args(["stop", &clean_id]).output();
        let _ = Command::new("docker").args(["rm", "-f", &clean_id]).output();
    }

    // 2. Nettoyage de l'ancien module NixOS si présent
    let config_dir = get_config_dir();
    let docker_dir = config_dir.join("docker");
    let nix_file = docker_dir.join(format!("{}.nix", clean_id));
    if nix_file.exists() {
        let _ = std::fs::remove_file(&nix_file);
        let _ = Command::new("git")
            .args(["-C", &config_dir.display().to_string(), "rm", "-f", &format!("docker/{}.nix", clean_id)])
            .output();
    }

    // 3. Supprimer les données si demandé
    if delete_data && app_dir.exists() {
        let _ = std::fs::remove_dir_all(&app_dir);
    }

    Ok(format!("Application '{}' désinstallée avec succès.", clean_id))
}

pub fn control_docker_container(name_or_id: &str, action: &str) -> Result<String, String> {
    let allowed = ["start", "stop", "restart", "pause", "unpause"];
    if !allowed.contains(&action) {
        return Err(format!("Action non autorisée : {}", action));
    }

    let status = Command::new("docker")
        .args([action, name_or_id])
        .status()
        .map_err(|e| format!("Échec d'exécution docker : {}", e))?;

    if status.success() {
        Ok(format!("Action '{}' exécutée avec succès sur '{}'", action, name_or_id))
    } else {
        Err(format!("Docker a retourné une erreur lors de l'action '{}'", action))
    }
}

pub fn get_docker_logs(name_or_id: &str, lines: usize) -> Result<String, String> {
    let max_lines = lines.min(500).to_string();
    let output = Command::new("docker")
        .args(["logs", "--tail", &max_lines, name_or_id])
        .output()
        .map_err(|e| format!("Impossible de lire les logs docker : {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined.is_empty() { combined.push('\n'); }
        combined.push_str(&stderr);
    }

    Ok(combined)
}

fn generate_default_compose(app_id: &str, port: u32) -> String {
    format!(r#"services:
  {id}:
    container_name: {id}
    image: {id}:latest
    restart: unless-stopped
    ports:
      - "{port}:{port}"
    volumes:
      - ./data:/data
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=100
"#, id = app_id, port = port)
}

fn get_default_catalog() -> StoreCatalog {
    StoreCatalog {
        version: "2.0.0".to_string(),
        updated_at: "2026-10-01T02:00:00Z".to_string(),
        repository: "https://github.com/Chomiam/noos_nas_store".to_string(),
        total_apps: 0,
        categories: vec![
            "Tous".into(),
            "Multimédia".into(),
            "Sécurité & Réseau".into(),
            "Administration & Monitoring".into(),
            "Téléchargement".into(),
            "Domotique & IoT".into(),
            "Outils & Utilitaires".into(),
            "Développement".into(),
            "Finance & Organisation".into(),
            "Jeux & Divertissement".into(),
        ],
        apps: Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerImageInfo {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
    pub created_at: String,
    pub is_used: bool,
    pub used_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerImagesOverview {
    pub total_images: usize,
    pub unused_images: usize,
    pub total_size: String,
    pub reclaimable_size: String,
    pub images: Vec<DockerImageInfo>,
}

pub fn list_docker_images() -> DockerImagesOverview {
    let mut container_image_map: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(output) = Command::new("docker").args(["ps", "-a", "--format", "{{.ID}}"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for cid in stdout.lines().filter(|s| !s.trim().is_empty()) {
            if let Ok(insp) = Command::new("docker").args(["inspect", "--format", "{{.Image}}|{{.Name}}", cid.trim()]).output() {
                let insp_txt = String::from_utf8_lossy(&insp.stdout);
                if let Some((img_id, cname)) = insp_txt.trim().split_once('|') {
                    let clean_name = cname.trim().trim_start_matches('/').to_string();
                    let raw_id = img_id.trim();
                    let short_id = raw_id.trim_start_matches("sha256:").chars().take(12).collect::<String>();
                    container_image_map.entry(short_id).or_default().push(clean_name.clone());
                    container_image_map.entry(raw_id.to_string()).or_default().push(clean_name);
                }
            }
        }
    }

    let mut images: Vec<DockerImageInfo> = Vec::new();
    if let Ok(output) = Command::new("docker")
        .args(["images", "-a", "--format", "{{.ID}}|{{.Repository}}|{{.Tag}}|{{.Size}}|{{.CreatedAt}}"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 5 {
                let id = parts[0].trim().to_string();
                let repo = parts[1].trim().to_string();
                let tag = parts[2].trim().to_string();
                let size = parts[3].trim().to_string();
                let created = parts[4].trim().to_string();

                let repo_tag = format!("{}:{}", repo, tag);
                let mut used_by: Vec<String> = Vec::new();
                if let Some(c) = container_image_map.get(&id) {
                    used_by.extend(c.clone());
                }
                if let Some(c) = container_image_map.get(&repo_tag) {
                    used_by.extend(c.clone());
                }
                if let Some(c) = container_image_map.get(&repo) {
                    used_by.extend(c.clone());
                }
                used_by.sort();
                used_by.dedup();

                let is_used = !used_by.is_empty();
                images.push(DockerImageInfo {
                    id,
                    repository: repo,
                    tag,
                    size,
                    created_at: created,
                    is_used,
                    used_by,
                });
            }
        }
    }

    let mut total_size = "0 B".to_string();
    let mut reclaimable_size = "0 B".to_string();

    if let Ok(output) = Command::new("docker").args(["system", "df"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("Images") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 5 {
                    total_size = parts[3].to_string();
                    reclaimable_size = parts[4..].join(" ");
                }
            }
        }
    }

    let total_images = images.len();
    let unused_images = images.iter().filter(|i| !i.is_used).count();

    DockerImagesOverview {
        total_images,
        unused_images,
        total_size,
        reclaimable_size,
        images,
    }
}

pub fn delete_docker_image(id: &str) -> Result<String, String> {
    let output = Command::new("docker")
        .args(["rmi", id])
        .output()
        .map_err(|e| format!("Erreur lors de l'exécution de docker rmi : {}", e))?;

    if output.status.success() {
        Ok(format!("Image {} supprimée avec succès.", id))
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Impossible de supprimer l'image : {}", err.trim()))
    }
}

pub fn prune_docker_images(all: bool) -> Result<String, String> {
    let mut args = vec!["image", "prune", "-f"];
    if all {
        args.push("-a");
    }
    let output = Command::new("docker")
        .args(&args)
        .output()
        .map_err(|e| format!("Erreur lors de la purge docker image prune : {}", e))?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        let reclaimed = text.lines()
            .find(|l| l.contains("Total reclaimed space:"))
            .unwrap_or("Nettoyage des images terminé avec succès.");
        Ok(reclaimed.to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Échec du nettoyage : {}", err.trim()))
    }
}

pub async fn remove_docker_container(
    name_or_id: &str,
    delete_image: bool,
    delete_data: bool,
) -> Result<String, String> {
    let clean_name = name_or_id.trim().trim_start_matches('/');
    if clean_name.is_empty() {
        return Err("Nom de conteneur invalide.".into());
    }

    // 1. Récupérer l'image et les labels du conteneur avant suppression
    let mut image_to_delete = None;
    let mut compose_project: Option<String> = None;
    let mut compose_workdir: Option<PathBuf> = None;

    if let Ok(out) = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{.Config.Image}}|{{index .Config.Labels \"com.docker.compose.project\"}}|{{index .Config.Labels \"com.docker.compose.working_dir\"}}",
            clean_name,
        ])
        .output()
    {
        if out.status.success() {
            let inspect_out = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parts: Vec<&str> = inspect_out.split('|').collect();
            if !parts.is_empty() && !parts[0].is_empty() {
                image_to_delete = Some(parts[0].to_string());
            }
            if parts.len() > 1 && !parts[1].is_empty() && parts[1] != "<no value>" {
                compose_project = Some(parts[1].to_string());
            }
            if parts.len() > 2 && !parts[2].is_empty() && parts[2] != "<no value>" {
                compose_workdir = Some(PathBuf::from(parts[2]));
            }
        }
    }

    // 2. Déterminer le projet et les dossiers associés
    let user = get_target_user();
    let stripped_name = clean_name.strip_prefix("docker-").unwrap_or(clean_name);
    let target_app_id = compose_project
        .clone()
        .unwrap_or_else(|| stripped_name.to_string());

    let default_app_dir = PathBuf::from(format!("/home/{}/docker/{}", user, target_app_id));
    let resolved_app_dir = compose_workdir.unwrap_or(default_app_dir);

    let compose_file_yaml = resolved_app_dir.join("compose.yaml");
    let compose_file_yml = resolved_app_dir.join("docker-compose.yml");
    let compose_path = if compose_file_yaml.exists() {
        Some(compose_file_yaml)
    } else if compose_file_yml.exists() {
        Some(compose_file_yml)
    } else {
        None
    };

    // 3. Exécuter l'arrêt et la suppression
    if let Some(ref cp) = compose_path {
        let _ = Command::new("docker")
            .args(["compose", "-f", &cp.display().to_string(), "down", "--remove-orphans"])
            .output();

        // Supprimer le fichier compose pour réinitialiser le statut Store
        let _ = std::fs::remove_file(cp);
    }

    // Arrêt forcé et suppression directe par Docker CLI au cas où
    let _ = Command::new("docker").args(["stop", clean_name]).output();
    let _ = Command::new("docker").args(["rm", "-f", clean_name]).output();

    // 4. Nettoyage du suivi de déploiement en mémoire
    if let Ok(mut tracker) = STORE_DEPLOY_TRACKER.lock() {
        tracker.remove(&target_app_id.to_lowercase());
        tracker.remove(&clean_name.to_lowercase());
    }

    // 5. Suppression des données persistantes si demandé
    let mut data_msg = String::new();
    if delete_data && resolved_app_dir.exists() {
        if let Ok(_) = std::fs::remove_dir_all(&resolved_app_dir) {
            data_msg = format!(" (données de '{}' supprimées)", resolved_app_dir.display());
        }
    }

    // 6. Suppression de l'image Docker si demandée
    let mut img_msg = String::new();
    if delete_image {
        if let Some(img) = image_to_delete {
            let rmi_out = Command::new("docker").args(["rmi", "-f", &img]).output();
            if let Ok(o) = rmi_out {
                if o.status.success() {
                    img_msg = format!(" (image '{}' supprimée)", img);
                }
            }
        }
    }

    // 7. Nettoyage de l'éventuel fichier NixOS si présent
    let config_dir = get_config_dir();
    let docker_dir = config_dir.join("docker");
    let nix_file = docker_dir.join(format!("{}.nix", target_app_id));
    if nix_file.exists() {
        let _ = std::fs::remove_file(&nix_file);
        let _ = Command::new("git")
            .args(["-C", &config_dir.display().to_string(), "rm", "-f", &format!("docker/{}.nix", target_app_id)])
            .output();
    }

    Ok(format!(
        "Conteneur '{}' supprimé avec succès{}{}.",
        clean_name, img_msg, data_msg
    ))
}

// ============================================================================
// VÉRIFICATION ET APPLICATION DES MISES À JOUR D'IMAGES DOCKER / COMPOSE
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerUpdateInfo {
    pub container_name: String,
    pub container_id: String,
    pub image: String,
    pub compose_project: Option<String>,
    pub compose_service: Option<String>,
    pub compose_file_path: Option<String>,
    pub has_update: bool,
    pub local_digest: Option<String>,
    pub remote_digest: Option<String>,
    pub checked_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerUpdatesCheckResult {
    pub checked_at: String,
    pub total_checked: usize,
    pub updates_available: usize,
    pub stacks_with_updates: usize,
    pub containers: Vec<ContainerUpdateInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyDockerUpdateRequest {
    /// "container" ou "stack"
    #[serde(default = "default_update_target_type")]
    pub target_type: String,
    /// Nom du conteneur ou nom du projet/stack Compose
    pub target: String,
    #[serde(default = "default_true_flag")]
    pub prune_old_images: bool,
}

fn default_update_target_type() -> String {
    "container".to_string()
}

fn default_true_flag() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct ApplyAllDockerUpdatesRequest {
    #[serde(default = "default_true_flag")]
    pub prune_old_images: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDockerUpdateResult {
    pub updated_targets: Vec<String>,
    pub logs: Vec<String>,
    pub summary: String,
}

static DOCKER_IMAGE_UPDATES_CACHE: LazyLock<Mutex<HashMap<String, ContainerUpdateInfo>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static DOCKER_LAST_CHECK_TIME: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

/// Retourne le statut de mise à jour mis en cache pour un conteneur donné.
pub fn get_cached_container_update(container_name: &str) -> Option<ContainerUpdateInfo> {
    if let Ok(cache) = DOCKER_IMAGE_UPDATES_CACHE.lock() {
        cache.get(container_name).cloned()
    } else {
        None
    }
}

/// Retourne l'état actuel du cache des mises à jour Docker sans interroger les registres distants.
pub fn get_docker_updates_status() -> DockerUpdatesCheckResult {
    let checked_at = DOCKER_LAST_CHECK_TIME
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default();

    let mut containers: Vec<ContainerUpdateInfo> = Vec::new();
    if let Ok(cache) = DOCKER_IMAGE_UPDATES_CACHE.lock() {
        containers = cache.values().cloned().collect();
    }
    containers.sort_by(|a, b| a.container_name.cmp(&b.container_name));

    let updates_available = containers.iter().filter(|c| c.has_update).count();
    let mut updated_stacks = std::collections::HashSet::new();
    for c in containers.iter().filter(|c| c.has_update) {
        let key = c
            .compose_project
            .clone()
            .unwrap_or_else(|| c.container_name.clone());
        updated_stacks.insert(key);
    }

    DockerUpdatesCheckResult {
        checked_at,
        total_checked: containers.len(),
        updates_available,
        stacks_with_updates: updated_stacks.len(),
        containers,
    }
}

fn shorten_sha256(digest: &str) -> String {
    let clean = digest.trim();
    if let Some(pos) = clean.find("sha256:") {
        let hex = &clean[pos + 7..];
        let short_hex: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).take(12).collect();
        if !short_hex.is_empty() {
            return format!("sha256:{}", short_hex);
        }
    }
    clean.chars().take(19).collect()
}

fn extract_remote_short_digest(manifest_out: &str) -> Option<String> {
    // Cherche d'abord un digest proche de "amd64" ou le premier digest sha256 valide dans le manifeste
    if let Some(amd64_pos) = manifest_out.find("\"amd64\"") {
        let window_start = amd64_pos.saturating_sub(600);
        let window_end = (amd64_pos + 600).min(manifest_out.len());
        let slice = &manifest_out[window_start..window_end];
        if let Some(sha_pos) = slice.find("sha256:") {
            let rest = &slice[sha_pos..];
            let hex: String = rest[7..].chars().take_while(|c| c.is_ascii_hexdigit()).take(12).collect();
            if hex.len() >= 12 {
                return Some(format!("sha256:{}", hex));
            }
        }
    }
    if let Some(sha_pos) = manifest_out.find("sha256:") {
        let rest = &manifest_out[sha_pos..];
        let hex: String = rest[7..].chars().take_while(|c| c.is_ascii_hexdigit()).take(12).collect();
        if hex.len() >= 12 {
            return Some(format!("sha256:{}", hex));
        }
    }
    None
}

/// Vérifie si le conteneur dispose d'une image plus récente sur le registre distant.
fn check_single_container_image(
    container_name: &str,
    image: &str,
) -> (bool, Option<String>, Option<String>, Option<String>) {
    // 1. Récupérer l'ID d'image actuellement exécuté par le conteneur
    let container_img_id = match Command::new("docker")
        .args(["inspect", "--format", "{{.Image}}", container_name])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    };

    if container_img_id.is_empty() {
        return (
            false,
            None,
            None,
            Some("Impossible d'inspecter l'image du conteneur.".into()),
        );
    }

    // 2. Récupérer les RepoDigests de l'image actuelle du conteneur
    let mut local_hashes: Vec<String> = vec![container_img_id.clone()];
    let mut primary_local_digest = shorten_sha256(&container_img_id);

    if let Ok(out) = Command::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{range .RepoDigests}}{{.}};{{end}}",
            &container_img_id,
        ])
        .output()
    {
        if out.status.success() {
            let digests_raw = String::from_utf8_lossy(&out.stdout);
            for part in digests_raw.split(';') {
                let p = part.trim();
                if let Some(at_pos) = p.find("@sha256:") {
                    let sha = p[at_pos + 1..].trim().to_string();
                    if !sha.is_empty() {
                        if primary_local_digest == shorten_sha256(&container_img_id) {
                            primary_local_digest = shorten_sha256(&sha);
                        }
                        local_hashes.push(sha);
                    }
                }
            }
        }
    }

    // 3. Vérifier si le tag local pointe déjà vers une image plus récente que celle du conteneur
    if let Ok(out) = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", image])
        .output()
    {
        if out.status.success() {
            let tag_img_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !tag_img_id.is_empty() && tag_img_id != container_img_id {
                return (
                    true,
                    Some(primary_local_digest),
                    Some(shorten_sha256(&tag_img_id)),
                    None,
                );
            }
        }
    }

    // 4. Interroger le registre distant via `docker manifest inspect -v <image>` (sans télécharger les couches)
    if let Ok(out) = Command::new("docker")
        .args(["manifest", "inspect", "-v", image])
        .output()
    {
        if out.status.success() {
            let manifest_stdout = String::from_utf8_lossy(&out.stdout);
            if manifest_stdout.contains("sha256:") {
                let matches_local = local_hashes
                    .iter()
                    .any(|h| !h.is_empty() && manifest_stdout.contains(h));

                let remote_short = extract_remote_short_digest(&manifest_stdout);
                if matches_local {
                    return (
                        false,
                        Some(primary_local_digest.clone()),
                        remote_short.or(Some(primary_local_digest)),
                        None,
                    );
                } else {
                    return (
                        true,
                        Some(primary_local_digest),
                        remote_short,
                        None,
                    );
                }
            }
        }
    }

    // 5. Fallback : `docker pull -q <image>` puis comparaison de l'ID d'image
    if let Ok(pull_out) = Command::new("docker").args(["pull", "-q", image]).output() {
        if pull_out.status.success() {
            if let Ok(inspect_out) = Command::new("docker")
                .args(["image", "inspect", "--format", "{{.Id}}", image])
                .output()
            {
                if inspect_out.status.success() {
                    let new_img_id = String::from_utf8_lossy(&inspect_out.stdout).trim().to_string();
                    let has_upd = !new_img_id.is_empty() && new_img_id != container_img_id;
                    return (
                        has_upd,
                        Some(primary_local_digest),
                        Some(shorten_sha256(&new_img_id)),
                        None,
                    );
                }
            }
        }
    }

    (
        false,
        Some(primary_local_digest),
        None,
        Some("Impossible de vérifier le registre distant pour cette image.".into()),
    )
}

/// Vérifie en parallèle les mises à jour disponibles pour tous les conteneurs Docker (hors serveurs de jeux).
pub fn check_docker_updates() -> Result<DockerUpdatesCheckResult, String> {
    let all_containers: Vec<crate::services::DockerContainer> = crate::services::get_docker_containers()
        .into_iter()
        .filter(|c| !c.name.starts_with("noos-game"))
        .collect();

    let now_str = Command::new("date")
        .arg("+%d/%m/%Y %H:%M:%S")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Récent".to_string());

    let results: Vec<ContainerUpdateInfo> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for c in &all_containers {
            handles.push(s.spawn({
                let now_ref = &now_str;
                move || {
                    let (has_update, local_digest, remote_digest, error) =
                        check_single_container_image(&c.name, &c.image);
                    ContainerUpdateInfo {
                        container_name: c.name.clone(),
                        container_id: c.id.clone(),
                        image: c.image.clone(),
                        compose_project: c.compose_project.clone(),
                        compose_service: c.compose_service.clone(),
                        compose_file_path: c.compose_file_path.clone(),
                        has_update,
                        local_digest,
                        remote_digest,
                        checked_at: now_ref.clone(),
                        error,
                    }
                }
            }));
        }

        let mut collected = Vec::with_capacity(handles.len());
        for h in handles {
            if let Ok(info) = h.join() {
                collected.push(info);
            }
        }
        collected
    });

    if let Ok(mut cache) = DOCKER_IMAGE_UPDATES_CACHE.lock() {
        cache.clear();
        for info in &results {
            cache.insert(info.container_name.clone(), info.clone());
        }
    }
    if let Ok(mut last_check) = DOCKER_LAST_CHECK_TIME.lock() {
        *last_check = Some(now_str.clone());
    }

    Ok(get_docker_updates_status())
}

/// Applique la mise à jour d'un conteneur individuel ou d'une pile Docker Compose complète.
pub fn apply_docker_update(req: &ApplyDockerUpdateRequest) -> Result<ApplyDockerUpdateResult, String> {
    let target = req.target.trim();
    if target.is_empty() {
        return Err("Cible de mise à jour invalide.".into());
    }

    let all_containers: Vec<crate::services::DockerContainer> = crate::services::get_docker_containers()
        .into_iter()
        .filter(|c| !c.name.starts_with("noos-game"))
        .collect();

    let matching_containers: Vec<crate::services::DockerContainer> = if req.target_type == "stack" {
        all_containers
            .into_iter()
            .filter(|c| {
                let stack_key = c
                    .compose_project
                    .as_deref()
                    .or(c.store_app_id.as_deref())
                    .unwrap_or(&c.name);
                stack_key.eq_ignore_ascii_case(target) || c.name.eq_ignore_ascii_case(target)
            })
            .collect()
    } else {
        all_containers
            .into_iter()
            .filter(|c| c.name.eq_ignore_ascii_case(target) || c.id == target)
            .collect()
    };

    if matching_containers.is_empty() {
        return Err(format!("Aucun conteneur ou projet trouvé pour '{}'.", target));
    }

    let mut logs = Vec::new();
    let mut updated_targets = Vec::new();

    // Déterminer si un fichier Docker Compose est disponible pour ce groupe
    let compose_file = matching_containers
        .iter()
        .find_map(|c| c.compose_file_path.clone());

    if let Some(cp) = compose_file {
        let cp_path = PathBuf::from(&cp);
        let work_dir = cp_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/tmp"));

        // Si on met à jour un seul conteneur au sein d'une stack multi-services et qu'il a un compose_service
        let single_service = if req.target_type == "container" && matching_containers.len() == 1 {
            matching_containers[0].compose_service.clone()
        } else {
            None
        };

        logs.push(format!("📦 Fichier Compose détecté : {}", cp));

        // 1. docker compose pull
        let mut pull_cmd = Command::new("docker");
        pull_cmd.current_dir(&work_dir).args(["compose", "-f", &cp, "pull"]);
        if let Some(ref svc) = single_service {
            pull_cmd.arg(svc);
        }

        let pull_out = pull_cmd
            .output()
            .map_err(|e| format!("Erreur lors de l'exécution de docker compose pull : {}", e))?;

        if !pull_out.status.success() {
            let err_txt = String::from_utf8_lossy(&pull_out.stderr).trim().to_string();
            return Err(format!("Échec du téléchargement de la nouvelle image (docker compose pull) : {}", err_txt));
        }
        logs.push("⬇️ Téléchargement des nouvelles images terminé avec succès.".into());

        // 2. docker compose up -d
        let mut up_cmd = Command::new("docker");
        up_cmd.current_dir(&work_dir).args(["compose", "-f", &cp, "up", "-d", "--remove-orphans"]);
        if let Some(ref svc) = single_service {
            up_cmd.arg(svc);
        }

        let up_out = up_cmd
            .output()
            .map_err(|e| format!("Erreur lors de la recréation du conteneur (docker compose up -d) : {}", e))?;

        if !up_out.status.success() {
            let err_txt = String::from_utf8_lossy(&up_out.stderr).trim().to_string();
            return Err(format!("Échec de l'application de la mise à jour (docker compose up -d) : {}", err_txt));
        }
        logs.push("🚀 Conteneur(s) recréé(s) et redémarré(s) avec la nouvelle version (volumes et .env préservés).".into());

        for c in &matching_containers {
            updated_targets.push(c.name.clone());
        }
    } else {
        // Conteneur autonome sans fichier compose.yaml détecté
        for c in &matching_containers {
            logs.push(format!("⬇️ Téléchargement de l'image '{}' pour '{}'...", c.image, c.name));
            let pull_out = Command::new("docker")
                .args(["pull", &c.image])
                .output()
                .map_err(|e| format!("Erreur lors de docker pull {} : {}", c.image, e))?;

            if !pull_out.status.success() {
                let err_txt = String::from_utf8_lossy(&pull_out.stderr).trim().to_string();
                return Err(format!("Échec du téléchargement de '{}' : {}", c.image, err_txt));
            }

            // Vérifier si géré par un service systemd NixOS (ex: docker-<name>.service)
            let clean_name = c.name.strip_prefix("docker-").unwrap_or(&c.name);
            let unit_name = format!("docker-{}.service", clean_name);
            let unit_exists = Command::new("systemctl")
                .args(["cat", &unit_name])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if unit_exists {
                let _ = Command::new("systemctl").args(["restart", &unit_name]).output();
                logs.push(format!("🔄 Service systemd '{}' redémarré avec la nouvelle image.", unit_name));
            } else {
                let _ = Command::new("docker").args(["restart", &c.name]).output();
                logs.push(format!("🔄 Image téléchargée et conteneur '{}' redémarré.", c.name));
            }
            updated_targets.push(c.name.clone());
        }
    }

    // 3. Nettoyage optionnel des anciennes couches d'images (dangling images)
    if req.prune_old_images {
        if let Ok(reclaimed) = prune_docker_images(false) {
            logs.push(format!("🧹 Nettoyage des anciennes images : {}", reclaimed));
        }
    }

    // 4. Mettre à jour le cache pour retirer la pastille orange sur les conteneurs mis à jour
    if let Ok(mut cache) = DOCKER_IMAGE_UPDATES_CACHE.lock() {
        for name in &updated_targets {
            if let Some(entry) = cache.get_mut(name) {
                entry.has_update = false;
                if let Some(ref rem) = entry.remote_digest {
                    entry.local_digest = Some(rem.clone());
                }
            }
        }
    }

    let summary = if updated_targets.len() == 1 {
        format!("Mise à jour de '{}' appliquée avec succès.", updated_targets[0])
    } else {
        format!("{} conteneurs mis à jour avec succès.", updated_targets.len())
    };

    Ok(ApplyDockerUpdateResult {
        updated_targets,
        logs,
        summary,
    })
}

/// Met à jour globalement toutes les piles Docker Compose et conteneurs ayant une mise à jour disponible.
pub fn apply_all_docker_updates(prune_old_images: bool) -> Result<ApplyDockerUpdateResult, String> {
    let all_containers: Vec<crate::services::DockerContainer> = crate::services::get_docker_containers()
        .into_iter()
        .filter(|c| !c.name.starts_with("noos-game") && c.has_update)
        .collect();

    if all_containers.is_empty() {
        return Err("Aucune mise à jour en attente n'a été trouvée.".into());
    }

    // Regrouper par pile Compose (ou nom de conteneur si autonome) pour éviter de relancer plusieurs fois le même compose.yaml
    let mut targets_to_update: Vec<(String, String)> = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    for c in &all_containers {
        if let Some(ref proj) = c.compose_project {
            let key = format!("stack:{}", proj.to_lowercase());
            if seen_keys.insert(key) {
                targets_to_update.push(("stack".to_string(), proj.clone()));
            }
        } else {
            let key = format!("container:{}", c.name.to_lowercase());
            if seen_keys.insert(key) {
                targets_to_update.push(("container".to_string(), c.name.clone()));
            }
        }
    }

    let mut all_updated = Vec::new();
    let mut all_logs = Vec::new();
    let mut errors = Vec::new();

    for (target_type, target) in targets_to_update {
        let req = ApplyDockerUpdateRequest {
            target_type,
            target: target.clone(),
            prune_old_images: false, // On fait le prune une seule fois à la fin
        };
        match apply_docker_update(&req) {
            Ok(res) => {
                for u in res.updated_targets {
                    if !all_updated.contains(&u) {
                        all_updated.push(u);
                    }
                }
                all_logs.extend(res.logs);
            }
            Err(e) => {
                errors.push(format!("{} : {}", target, e));
                all_logs.push(format!("❌ Erreur sur '{}' : {}", target, e));
            }
        }
    }

    if prune_old_images {
        if let Ok(reclaimed) = prune_docker_images(false) {
            all_logs.push(format!("🧹 Nettoyage global des anciennes images : {}", reclaimed));
        }
    }

    if all_updated.is_empty() && !errors.is_empty() {
        return Err(format!("Échec de la mise à jour globale : {}", errors.join(" | ")));
    }

    let summary = if errors.is_empty() {
        format!(
            "Mise à jour globale terminée avec succès ({} conteneur(s) mis à jour).",
            all_updated.len()
        )
    } else {
        format!(
            "{} conteneur(s) mis à jour, {} erreur(s) rencontrée(s).",
            all_updated.len(),
            errors.len()
        )
    };

    Ok(ApplyDockerUpdateResult {
        updated_targets: all_updated,
        logs: all_logs,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_secret_key_types() {
        let pass = generate_secret_key("password");
        assert_eq!(pass.len(), 24);

        let hex = generate_secret_key("hex32");
        assert_eq!(hex.len(), 32);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));

        let b64 = generate_secret_key("base64");
        assert!(!b64.is_empty());

        let uuid = generate_secret_key("uuid");
        assert_eq!(uuid.len(), 36);
        assert_eq!(uuid.chars().nth(8), Some('-'));
        assert_eq!(uuid.chars().nth(13), Some('-'));
        assert_eq!(uuid.chars().nth(18), Some('-'));
        assert_eq!(uuid.chars().nth(23), Some('-'));
    }

    #[test]
    fn test_should_generate_secret_templates_and_heuristics() {
        let gen1 = should_generate_secret("DB_PASSWORD", Some("password"), "{{GENERATE_PASSWORD}}");
        assert!(gen1.is_some());
        assert_eq!(gen1.unwrap().len(), 24);

        let gen2 = should_generate_secret("JWT_SECRET", Some("base64"), "{{GENERATE_BASE64}}");
        assert!(gen2.is_some());

        let gen3 = should_generate_secret("ADMIN_PASSWORD", None, "");
        assert!(gen3.is_some());

        let gen4 = should_generate_secret("DB_NAME", None, "immich");
        assert!(gen4.is_none());
    }

    #[test]
    fn test_customize_compose_yaml_preserves_indentation() {
        let original_yaml = r#"services:
  immich-server:
    container_name: immich_server
    image: ghcr.io/immich-app/immich-server:release
    environment:
      - NODE_ENV=production
      - DB_HOSTNAME=immich_postgres
      - DB_USERNAME=postgres
      - DB_PASSWORD=postgres
      - DB_DATABASE_NAME=immich
"#;

        let mut envs = HashMap::new();
        envs.insert("DB_PASSWORD".to_string(), "SuperSecretKey123!".to_string());
        envs.insert("CUSTOM_KEY".to_string(), "CustomVal".to_string());

        let output = customize_compose_yaml(original_yaml, None, Some(&envs), None, None);

        // Verify that existing keys retain their 6-space indentation
        assert!(output.contains("      - DB_PASSWORD=SuperSecretKey123!"));
        // Verify that newly inserted keys use the correct 6-space indentation under environment:
        assert!(output.contains("      - CUSTOM_KEY=CustomVal"));
        // Verify no invalid 4-space un-indentation occurred that would break YAML parser
        assert!(!output.contains("\n    - DB_PASSWORD="));
        assert!(!output.contains("\n    - CUSTOM_KEY="));
    }

    #[test]
    fn test_check_compose_has_device_section() {
        let with_devices = r#"services:
  immich:
    image: immich:release
    # devices:
    #   - /dev/dri:/dev/dri
"#;
        assert!(check_compose_has_device_section(with_devices));

        let with_active_devices = r#"services:
  jellyfin:
    image: jellyfin:latest
    devices:
      - /dev/dri:/dev/dri
"#;
        assert!(check_compose_has_device_section(with_active_devices));

        let without_devices = r#"services:
  adguard:
    image: adguard/adguardhome:latest
    ports:
      - 53:53/udp
      - 3000:3000
    environment:
      - TZ=Europe/Paris
"#;
        assert!(!check_compose_has_device_section(without_devices));
    }
}


