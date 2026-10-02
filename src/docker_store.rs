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

    let cache_file = Path::new("/var/cache/steveos-nas-dashboard/store_cache.json");
    let catalog: StoreCatalog = if let Ok(txt) = std::fs::read_to_string(cache_file) {
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
    let cache_dir = Path::new("/var/cache/steveos-nas-dashboard");
    let cache_file = cache_dir.join("store_cache.json");

    // 1. Tenter la récupération depuis GitHub (store.json)
    let fetched = Command::new("curl")
        .args([
            "-s",
            "--connect-timeout",
            "4",
            "--max-time",
            "8",
            "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/store.json",
        ])
        .output();

    let json_text = if let Ok(out) = fetched {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        if text.trim().starts_with('{') {
            let _ = std::fs::create_dir_all(cache_dir);
            let _ = std::fs::write(&cache_file, &text);
            Some(text)
        } else {
            None
        }
    } else {
        None
    };

    // 2. Repli sur le cache local
    let json_text = json_text.or_else(|| std::fs::read_to_string(&cache_file).ok());

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

fn customize_compose_yaml(
    base_compose: &str,
    new_port: Option<u32>,
    env_vars: Option<&HashMap<String, String>>,
) -> String {
    let mut lines: Vec<String> = base_compose.lines().map(|s| s.to_string()).collect();

    // 1. Remplacer le port d'hôte Web si spécifié (sans écraser les ports DNS 53, DHCP ou DoT)
    if let Some(p) = new_port {
        let mut port_replaced = false;
        let p_str = p.to_string();
        let web_indicators = [p_str.as_str(), "3000", "80", "8080", "443", "8096", "9000", "8443", "5000"];
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

    // 2. Mettre à jour les variables d'environnement
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
                        lines[line_num] = format!("    - {}={}", clean_k, clean_v);
                    } else {
                        lines.insert(e_idx + 1, format!("    - {}={}", clean_k, clean_v));
                    }
                }
            }
        }
    }

    lines.join("\n") + "\n"
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
        PathBuf::from(d)
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

    // 2. Récupérer le compose.yaml depuis steveos_nas_store
    let url = format!(
        "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/{}/compose.yaml",
        clean_id
    );
    let curl_res = Command::new("curl")
        .args(["-s", "--connect-timeout", "5", "--max-time", "15", &url])
        .output();

    let base_compose = match curl_res {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if s.contains("services:") {
                s
            } else {
                generate_default_compose(&clean_id, req.port.unwrap_or(8080))
            }
        }
        Err(_) => generate_default_compose(&clean_id, req.port.unwrap_or(8080)),
    };

    // 3. Personnaliser le compose.yaml (port et variables d'environnement)
    let customized = customize_compose_yaml(&base_compose, req.port, req.env_vars.as_ref());

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
        let tmp_dropin = "/tmp/steveos-resolved-stub.conf";
        if std::fs::write(tmp_dropin, dropin).is_ok() {
            let _ = Command::new("sudo").args(["cp", tmp_dropin, "/etc/systemd/resolved.conf.d/steveos-dns.conf"]).output();
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
        repository: "https://github.com/Chomiam/steveos_nas_store".to_string(),
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
