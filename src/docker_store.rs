use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreVolume {
    pub host: String,
    pub container: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub category: String,
    pub tagline: String,
    pub description: String,
    pub website: String,
    pub icon: String,
    pub default_port: u16,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub media_support: bool,
    #[serde(default)]
    pub volumes: Vec<StoreVolume>,
    #[serde(default)]
    pub nix_file: String,

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
    pub categories: Vec<String>,
    pub apps: Vec<StoreApp>,
}

#[derive(Debug, Deserialize)]
pub struct InstallAppRequest {
    pub app_id: String,
    pub port: Option<u16>,
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

fn get_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("STEVEOS_CONFIG_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return p;
        }
    }
    if Path::new("/etc/nixos").exists() {
        PathBuf::from("/etc/nixos")
    } else {
        PathBuf::from("/home/chomiam/Projects/steveos-nas")
    }
}

fn get_target_user() -> String {
    std::env::var("STEVEOS_USER").unwrap_or_else(|_| "chomiam".to_string())
}

pub fn get_running_containers_map() -> HashMap<String, (String, String, bool)> {
    let mut map = HashMap::new();
    if let Ok(output) = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.ID}}	{{.Names}}	{{.Status}}	{{.State}}"])
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
                map.insert(clean_name.clone(), (id.clone(), status.clone(), is_running));
                map.insert(name, (id, status, is_running));
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

    // 3. Repli sur catalogue intégré par défaut
    let mut catalog: StoreCatalog = if let Some(txt) = json_text {
        serde_json::from_str(&txt).unwrap_or_else(|_| get_embedded_catalog())
    } else {
        get_embedded_catalog()
    };

    // Filtrer d'éventuelles entrées supprimées
    catalog.apps.retain(|a| a.id != "homepage" && a.id != "filebrowser");

    // 4. Enrichir avec l'état du système NixOS et de Docker
    let config_dir = get_config_dir();
    let docker_dir = config_dir.join("docker");
    let containers_map = get_running_containers_map();

    for app in &mut catalog.apps {
        let nix_path = docker_dir.join(format!("{}.nix", app.id));
        app.is_installed = nix_path.exists();

        if let Some((cid, status, is_running)) = containers_map.get(&app.id) {
            app.is_running = *is_running;
            app.container_id = Some(cid.clone());
            app.container_status = Some(status.clone());
        } else {
            app.is_running = false;
            app.container_id = None;
            app.container_status = None;
        }
    }

    catalog
}

fn customize_nix_content(
    base_nix: &str,
    app_id: &str,
    port: Option<u16>,
    data_dir: Option<&str>,
    media_dir: Option<&str>,
    gpu_device: Option<&str>,
    env_vars: Option<&HashMap<String, String>>,
) -> String {
    let mut res = base_nix.to_string();

    // 1. Personnalisation du dossier de données
    if let Some(dir) = data_dir {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            let default_pattern = format!("dataDir = \"/home/${{user}}/docker/{}\";", app_id);
            let custom_pattern = format!("dataDir = \"{}\";", trimmed);
            if res.contains(&default_pattern) {
                res = res.replace(&default_pattern, &custom_pattern);
            }
        }
    }

    // 2. Personnalisation du dossier média (ex: Jellyfin)
    if let Some(m_dir) = media_dir {
        let trimmed = m_dir.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            let default_media = "mediaDir = \"/home/${user}/video\";";
            let custom_media = format!("mediaDir = \"{}\";", trimmed);
            if res.contains(default_media) {
                res = res.replace(default_media, &custom_media);
            } else if let Some(idx) = res.find("mediaDir = \"") {
                if let Some(end_idx) = res[idx..].find("\";") {
                    let old_val = &res[idx..idx + end_idx + 2];
                    res = res.replace(old_val, &custom_media);
                }
            }
        }
    }

    // 3. Personnalisation du périphérique GPU (ex: Jellyfin)
    if let Some(gpu) = gpu_device {
        let trimmed = gpu.trim();
        if trimmed == "none" || trimmed.is_empty() {
            res = res.replace("\"--device=/dev/dri:/dev/dri\"", "");
            res = res.replace("extraOptions = [\n      \"--device=/dev/dri:/dev/dri\"\n    ];", "extraOptions = [ ];");
        } else if trimmed == "--gpus all" || trimmed == "--gpus=all" || trimmed == "nvidia" {
            res = res.replace("\"--device=/dev/dri:/dev/dri\"", "\"--gpus=all\"");
        } else if trimmed.contains(':') && !res.contains(&format!("\"--device={}\"", trimmed)) {
            res = res.replace("\"--device=/dev/dri:/dev/dri\"", &format!("\"--device={}\"", trimmed));
        }
    }

    // 2. Personnalisation du port
    if let Some(new_port) = port {
        // Remplacement dans allowedTCPPorts
        if let Some(tcp_idx) = res.find("networking.firewall.allowedTCPPorts = [") {
            if let Some(end_bracket) = res[tcp_idx..].find(']') {
                let full_end = tcp_idx + end_bracket;
                let prefix = &res[..tcp_idx + "networking.firewall.allowedTCPPorts = [".len()];
                let suffix = &res[full_end..];
                res = format!("{} {} {}", prefix, new_port, suffix);
            }
        }

        // Remplacement dans ports = [ "XXXX:
        if let Some(p_idx) = res.find("ports = [") {
            if let Some(p_end) = res[p_idx..].find(']') {
                let ports_block = &res[p_idx..p_idx + p_end];
                let mut new_block = ports_block.to_string();
                for chunk in ports_block.split('"') {
                    if let Some(colon) = chunk.find(':') {
                        let host_p = &chunk[..colon];
                        let cont_p = &chunk[colon + 1..];
                        if host_p.chars().all(|c| c.is_ascii_digit()) {
                            let old_str = format!("\"{}:{}\"", host_p, cont_p);
                            let new_str = format!("\"{}:{}\"", new_port, cont_p);
                            new_block = new_block.replace(&old_str, &new_str);
                            break;
                        }
                    }
                }
                res = res[..p_idx].to_string() + &new_block + &res[p_idx + p_end..];
            }
        }
    }

    // 3. Personnalisation des variables d'environnement
    if let Some(envs) = env_vars {
        if !envs.is_empty() {
            if !res.contains("environment = {") {
                if let Some(auto_idx) = res.find("autoStart = true;") {
                    let insert_pt = auto_idx + "autoStart = true;".len();
                    res.insert_str(insert_pt, "
    environment = {
    };");
                }
            }

            if let Some(env_idx) = res.find("environment = {") {
                if let Some(env_end) = res[env_idx..].find("};") {
                    let mut env_block = res[env_idx + "environment = {".len()..env_idx + env_end].to_string();
                    for (k, v) in envs {
                        let clean_k = k.trim().replace('"', "");
                        let clean_v = v.trim().replace('"', "");
                        if !clean_k.is_empty() {
                            let key_match = format!("{} =", clean_k);
                            let key_match_space = format!("{} =", clean_k);
                            let mut found = false;
                            for line in env_block.lines() {
                                let trimmed_line = line.trim();
                                if trimmed_line.starts_with(&key_match) || trimmed_line.starts_with(&key_match_space) {
                                    env_block = env_block.replace(line, &format!("      {} = \"{}\";", clean_k, clean_v));
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                env_block.push_str(&format!("\n      {} = \"{}\";", clean_k, clean_v));
                            }
                        }
                    }
                    res = res[..env_idx + "environment = {".len()].to_string() + &env_block + &res[env_idx + env_end..];
                }
            }
        }
    }

    res
}

pub async fn install_store_app(req: InstallAppRequest) -> Result<String, String> {
    let clean_id = req.app_id.trim().to_lowercase();
    if clean_id.is_empty() || !clean_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Identifiant d'application invalide".to_string());
    }

    let config_dir = get_config_dir();
    let docker_dir = config_dir.join("docker");
    let target_nix_file = docker_dir.join(format!("{}.nix", clean_id));

    // Récupérer le contenu du .nix de base
    let url = format!(
        "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/{}/{}.nix",
        clean_id, clean_id
    );
    let curl_res = Command::new("curl")
        .args(["-s", "--connect-timeout", "5", "--max-time", "15", &url])
        .output();

    let base_nix = match curl_res {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if s.contains("virtualisation.oci-containers") {
                s
            } else {
                get_embedded_app_nix(&clean_id)?
            }
        }
        Err(_) => get_embedded_app_nix(&clean_id)?,
    };

    // Appliquer les personnalisations (port, data_dir, media_dir, env_vars)
    let user = get_target_user();
    let customized_nix = customize_nix_content(
        &base_nix,
        &clean_id,
        req.port,
        req.data_dir.as_deref(),
        req.media_dir.as_deref(),
        req.gpu_device.as_deref(),
        req.env_vars.as_ref(),
    );

    // Assurer le dossier docker/ dans NixOS
    if let Err(e) = std::fs::create_dir_all(&docker_dir) {
        return Err(format!("Impossible de créer le dossier docker dans NixOS : {}", e));
    }

    // Assurer le dossier persistant dans /home/<user>/docker/<app_id>
    let app_data_dir = req.data_dir.clone().unwrap_or_else(|| format!("/home/{}/docker/{}", user, clean_id));
    let _ = std::fs::create_dir_all(&app_data_dir);
    let _ = Command::new("chown").args(["-R", &format!("{}:users", user), &app_data_dir]).status();
    let _ = Command::new("chmod").args(["-R", "0775", &app_data_dir]).status();

    // Gestion spécifique des dossiers médias pour Jellyfin (ou apps multimédias)
    if clean_id == "jellyfin" || req.media_dir.is_some() {
        let media_path = req.media_dir.clone().unwrap_or_else(|| format!("/home/{}/video", user));
        let m_path = std::path::PathBuf::from(&media_path);

        let movies_dir = m_path.join("movies");
        let tv_shows_dir = m_path.join("tv_shows");
        let anims_dir = m_path.join("anims");

        let _ = std::fs::create_dir_all(&m_path);
        let _ = std::fs::create_dir_all(&movies_dir);
        let _ = std::fs::create_dir_all(&tv_shows_dir);
        let _ = std::fs::create_dir_all(&anims_dir);

        let _ = Command::new("chown").args(["-R", &format!("{}:users", user), &media_path]).status();
        let _ = Command::new("chmod").args(["-R", "0775", &media_path]).status();

        if media_path.starts_with("/mnt/storage") {
            let _ = Command::new("chmod").args(["-R", "2775", &media_path]).status();
        }
    }

    // Écrire le fichier .nix
    if let Err(e) = std::fs::write(&target_nix_file, customized_nix) {
        return Err(format!("Impossible d'écrire le module Nix : {}", e));
    }

    // Indexer le fichier dans git pour que Nix flake le reconnaisse
    let _ = Command::new("git")
        .args(["-C", &config_dir.display().to_string(), "add", &format!("docker/{}.nix", clean_id)])
        .status();

    // Lancer le déploiement en arrière-plan
    let cfg_clone = config_dir.clone();
    tokio::spawn(async move {
        let _ = Command::new("nh")
            .args(["os", "switch", "--no-nom", &cfg_clone.display().to_string()])
            .status();
    });

    Ok(format!(
        "Application '{}' configurée et déploiement NixOS initié avec succès !",
        clean_id
    ))
}

pub async fn uninstall_store_app(app_id: &str, delete_data: bool) -> Result<String, String> {
    let clean_id = app_id.trim().to_lowercase();
    let config_dir = get_config_dir();
    let docker_dir = config_dir.join("docker");
    let target_nix_file = docker_dir.join(format!("{}.nix", clean_id));

    if !target_nix_file.exists() {
        return Err(format!("L'application '{}' n'est pas installée.", clean_id));
    }

    // Arrêter le conteneur Docker immédiatement
    let _ = Command::new("docker").args(["stop", &clean_id]).status();
    let _ = Command::new("docker").args(["rm", "-f", &clean_id]).status();

    // Supprimer le fichier .nix
    let _ = std::fs::remove_file(&target_nix_file);

    // Mettre à jour git
    let _ = Command::new("git")
        .args(["-C", &config_dir.display().to_string(), "rm", "-f", &format!("docker/{}.nix", clean_id)])
        .status();
    let _ = Command::new("git")
        .args(["-C", &config_dir.display().to_string(), "add", "-u"])
        .status();

    // Supprimer les données si demandé
    if delete_data {
        let user = get_target_user();
        let app_data_dir = format!("/home/{}/docker/{}", user, clean_id);
        let _ = std::fs::remove_dir_all(&app_data_dir);
    }

    // Déclencher le switch NixOS pour mettre à jour les unités systemd et le pare-feu
    let cfg_clone = config_dir.clone();
    tokio::spawn(async move {
        let _ = Command::new("nh")
            .args(["os", "switch", "--no-nom", &cfg_clone.display().to_string()])
            .status();
    });

    Ok(format!(
        "Application '{}' désinstallée avec succès. Le système NixOS est mis à jour.",
        clean_id
    ))
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

fn get_embedded_catalog() -> StoreCatalog {
    StoreCatalog {
        version: "1.0.0".to_string(),
        updated_at: "2026-09-29T19:20:00Z".to_string(),
        repository: "https://github.com/Chomiam/steveos_nas_store".to_string(),
        categories: vec![
            "Tous".into(),
            "Administration".into(),
            "Multimédia".into(),
            "Téléchargement".into(),
            "Sécurité".into(),
            "Monitoring".into(),
        ],
        apps: vec![
            StoreApp {
                id: "arcane".into(),
                name: "Arcane".into(),
                version: "latest".into(),
                category: "Administration".into(),
                tagline: "Gestionnaire moderne et léger de conteneurs Docker".into(),
                description: "Arcane propose une interface utilisateur épurée et moderne pour superviser, déployer et administrer facilement vos conteneurs Docker et stacks Compose.".into(),
                website: "https://getarcane.app/".into(),
                icon: "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/arcane/icon.svg".into(),
                default_port: 3552,
                recommended: true,
                media_support: false,
                volumes: vec![
                    StoreVolume {
                        host: "/home/{USER}/docker/arcane/data".into(),
                        container: "/app/data".into(),
                        description: "Base de données et configuration".into(),
                    },
                    StoreVolume {
                        host: "/var/run/docker.sock".into(),
                        container: "/var/run/docker.sock".into(),
                        description: "Socket Docker".into(),
                    },
                ],
                nix_file: "arcane.nix".into(),
                is_installed: false,
                is_running: false,
                container_id: None,
                container_status: None,
            },
            StoreApp {
                id: "jellyfin".into(),
                name: "Jellyfin".into(),
                version: "latest".into(),
                category: "Multimédia".into(),
                tagline: "Système multimédia libre pour streamer vos films, séries et animés".into(),
                description: "Serveur multimédia open-source puissant sans abonnement ni pistage. Organisez et streamez vos bibliothèques (Films, Séries, Animés) sur tous vos écrans avec transcodage matériel.".into(),
                website: "https://jellyfin.org/".into(),
                icon: "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/jellyfin/icon.svg".into(),
                default_port: 8096,
                recommended: true,
                media_support: true,
                volumes: vec![
                    StoreVolume {
                        host: "/home/{USER}/docker/jellyfin/config".into(),
                        container: "/config".into(),
                        description: "Configuration Jellyfin et base de données".into(),
                    },
                    StoreVolume {
                        host: "/home/{USER}/docker/jellyfin/cache".into(),
                        container: "/cache".into(),
                        description: "Cache de transcodage et métadonnées".into(),
                    },
                    StoreVolume {
                        host: "{MEDIA_DIR}/movies".into(),
                        container: "/data/movies".into(),
                        description: "Dossier des films".into(),
                    },
                    StoreVolume {
                        host: "{MEDIA_DIR}/tv_shows".into(),
                        container: "/data/tv_shows".into(),
                        description: "Dossier des séries TV".into(),
                    },
                    StoreVolume {
                        host: "{MEDIA_DIR}/anims".into(),
                        container: "/data/anims".into(),
                        description: "Dossier des animés et animations".into(),
                    },
                ],
                nix_file: "jellyfin.nix".into(),
                is_installed: false,
                is_running: false,
                container_id: None,
                container_status: None,
            },
            StoreApp {
                id: "immich".into(),
                name: "Immich".into(),
                version: "latest".into(),
                category: "Multimédia".into(),
                tagline: "Solution d'hébergement de photos et vidéos type Google Photos".into(),
                description: "Sauvegarde automatique, détection des visages par IA, géolocalisation, albums partagés et lecture haute définition.".into(),
                website: "https://immich.app/".into(),
                icon: "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/immich/icon.svg".into(),
                default_port: 2283,
                recommended: true,
                media_support: false,
                volumes: vec![
                    StoreVolume {
                        host: "/home/{USER}/docker/immich/upload".into(),
                        container: "/usr/src/app/upload".into(),
                        description: "Stockage photos et vidéos".into(),
                    },
                ],
                nix_file: "immich.nix".into(),
                is_installed: false,
                is_running: false,
                container_id: None,
                container_status: None,
            },
            StoreApp {
                id: "jellyseerr".into(),
                name: "Jellyseerr".into(),
                version: "latest".into(),
                category: "Multimédia".into(),
                tagline: "Gestionnaire de demandes de films et séries pour Jellyfin".into(),
                description: "Permet aux utilisateurs de votre NAS de demander de nouveaux contenus vidéo avec découverte interactive et intégration Jellyfin.".into(),
                website: "https://github.com/Fallenbagel/jellyseerr".into(),
                icon: "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/jellyseerr/icon.svg".into(),
                default_port: 5055,
                recommended: true,
                media_support: false,
                volumes: vec![
                    StoreVolume {
                        host: "/home/{USER}/docker/jellyseerr/config".into(),
                        container: "/app/config".into(),
                        description: "Configuration et base SQLite".into(),
                    },
                ],
                nix_file: "jellyseerr.nix".into(),
                is_installed: false,
                is_running: false,
                container_id: None,
                container_status: None,
            },
            StoreApp {
                id: "qbittorrent".into(),
                name: "qBittorrent".into(),
                version: "latest".into(),
                category: "Téléchargement".into(),
                tagline: "Client BitTorrent rapide avec interface web complète".into(),
                description: "Client BitTorrent open-source complet doté d'une interface Web pour gérer vos téléchargements à distance.".into(),
                website: "https://www.qbittorrent.org/".into(),
                icon: "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/qbittorrent/icon.svg".into(),
                default_port: 8085,
                recommended: false,
                media_support: false,
                volumes: vec![
                    StoreVolume {
                        host: "/home/{USER}/docker/qbittorrent/config".into(),
                        container: "/config".into(),
                        description: "Configuration torrents".into(),
                    },
                    StoreVolume {
                        host: "/home/{USER}/docker/qbittorrent/downloads".into(),
                        container: "/downloads".into(),
                        description: "Fichiers téléchargés".into(),
                    },
                ],
                nix_file: "qbittorrent.nix".into(),
                is_installed: false,
                is_running: false,
                container_id: None,
                container_status: None,
            },
            StoreApp {
                id: "vaultwarden".into(),
                name: "Vaultwarden".into(),
                version: "latest".into(),
                category: "Sécurité".into(),
                tagline: "Serveur Bitwarden léger et ultra-rapide en Rust".into(),
                description: "Coffre-fort de mots de passe auto-hébergé, 100% compatible avec les applications mobiles et extensions officielles Bitwarden.".into(),
                website: "https://github.com/dani-garcia/vaultwarden".into(),
                icon: "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/vaultwarden/icon.svg".into(),
                default_port: 8222,
                recommended: true,
                media_support: false,
                volumes: vec![
                    StoreVolume {
                        host: "/home/{USER}/docker/vaultwarden/data".into(),
                        container: "/data".into(),
                        description: "Coffre chiffré".into(),
                    },
                ],
                nix_file: "vaultwarden.nix".into(),
                is_installed: false,
                is_running: false,
                container_id: None,
                container_status: None,
            },
            StoreApp {
                id: "uptime-kuma".into(),
                name: "Uptime Kuma".into(),
                version: "latest".into(),
                category: "Monitoring".into(),
                tagline: "Surveillance de disponibilité de vos services et sites web".into(),
                description: "Tableau de bord auto-hébergé surveillant le temps de disponibilité avec alertes instantanées (Discord, Telegram, Mail).".into(),
                website: "https://uptime.kuma.pet/".into(),
                icon: "https://raw.githubusercontent.com/Chomiam/steveos_nas_store/main/apps/uptime-kuma/icon.svg".into(),
                default_port: 3001,
                recommended: false,
                media_support: false,
                volumes: vec![
                    StoreVolume {
                        host: "/home/{USER}/docker/uptime-kuma/data".into(),
                        container: "/app/data".into(),
                        description: "Historiques et alertes".into(),
                    },
                ],
                nix_file: "uptime-kuma.nix".into(),
                is_installed: false,
                is_running: false,
                container_id: None,
                container_status: None,
            },
        ],
    }
}

fn get_embedded_app_nix(app_id: &str) -> Result<String, String> {
    match app_id {
        "jellyfin" => Ok(r#"{ config, lib, pkgs, ... }:

let
  user = config.steveos.user.username;
  dataDir = "/home/${user}/docker/jellyfin";
  mediaDir = "/home/${user}/video";
  gpuType = config.steveos.hardware.gpu or "intel";

  isNvidia = gpuType == "nvidia" || gpuType == "nvidia-legacy";
  hasDri = builtins.pathExists "/dev/dri" || gpuType == "intel" || gpuType == "amd";

  gpuOptions =
    if isNvidia then
      [ "--gpus=all" ]
    else if hasDri then
      [ "--device=/dev/dri:/dev/dri" ]
    else
      [ ];

  gpuEnv =
    if isNvidia then {
      NVIDIA_VISIBLE_DEVICES = "all";
      NVIDIA_DRIVER_CAPABILITIES = "all";
    } else { };
in
{
  systemd.tmpfiles.rules = [
    "d /home/${user}/docker 0775 ${user} users -"
    "d ${dataDir} 0775 ${user} users -"
    "d ${dataDir}/config 0775 ${user} users -"
    "d ${dataDir}/cache 0775 ${user} users -"
    "d ${mediaDir} 0775 ${user} users -"
    "d ${mediaDir}/movies 0775 ${user} users -"
    "d ${mediaDir}/tv_shows 0775 ${user} users -"
    "d ${mediaDir}/anims 0775 ${user} users -"
  ];

  virtualisation.oci-containers.backend = "docker";
  virtualisation.oci-containers.containers.jellyfin = {
    image = "lscr.io/linuxserver/jellyfin:latest";
    autoStart = true;
    ports = [
      "8096:8096"
      "8920:8920"
      "1900:1900/udp"
      "7359:7359/udp"
    ];
    volumes = [
      "${dataDir}/config:/config"
      "${dataDir}/cache:/cache"
      "${mediaDir}/movies:/data/movies"
      "${mediaDir}/tv_shows:/data/tv_shows"
      "${mediaDir}/anims:/data/anims"
      "${mediaDir}:/media"
    ];
    environment = {
      PUID = "1000";
      PGID = "100";
      TZ = config.steveos.timeZone or "Europe/Paris";
      UMASK = "002";
    } // gpuEnv;
    extraOptions = gpuOptions;
  };

  networking.firewall.allowedTCPPorts = [ 8096 8920 ];
  networking.firewall.allowedUDPPorts = [ 1900 7359 ];
}
"#.to_string()),
        "arcane" => Ok(r#"{ config, lib, pkgs, ... }:

let
  user = config.steveos.user.username;
  dataDir = "/home/${user}/docker/arcane";
in
{
  systemd.tmpfiles.rules = [
    "d /home/${user}/docker 0775 ${user} users -"
    "d ${dataDir} 0775 ${user} users -"
    "d ${dataDir}/data 0775 ${user} users -"
  ];

  virtualisation.oci-containers.backend = "docker";
  virtualisation.oci-containers.containers.arcane = {
    image = "ghcr.io/getarcaneapp/arcane:latest";
    autoStart = true;
    ports = [ "3552:3552" ];
    volumes = [
      "/var/run/docker.sock:/var/run/docker.sock"
      "${dataDir}/data:/app/data"
    ];
    environment = {
      PORT = "3552";
      ENCRYPTION_KEY = "0c8f24b63e073f21f04431b2bd81f6f65bbf5b2571ccaf9eda3dc5eab3486f85";
    };
  };

  networking.firewall.allowedTCPPorts = [ 3552 ];
}
"#.to_string()),
        "immich" => Ok(r#"{ config, lib, pkgs, ... }:

let
  user = config.steveos.user.username;
  dataDir = "/home/${user}/docker/immich";
in
{
  systemd.tmpfiles.rules = [
    "d /home/${user}/docker 0775 ${user} users -"
    "d ${dataDir} 0775 ${user} users -"
    "d ${dataDir}/upload 0775 ${user} users -"
    "d ${dataDir}/profile 0775 ${user} users -"
  ];

  virtualisation.oci-containers.backend = "docker";
  virtualisation.oci-containers.containers.immich = {
    image = "ghcr.io/immich-app/immich-server:release";
    autoStart = true;
    ports = [ "2283:2283" ];
    volumes = [
      "${dataDir}/upload:/usr/src/app/upload"
      "${dataDir}/profile:/usr/src/app/profile"
    ];
    environment = {
      IMMICH_ENV = "production";
      TZ = config.steveos.timeZone or "Europe/Paris";
    };
  };

  networking.firewall.allowedTCPPorts = [ 2283 ];
}
"#.to_string()),
        "jellyseerr" => Ok(r#"{ config, lib, pkgs, ... }:

let
  user = config.steveos.user.username;
  dataDir = "/home/${user}/docker/jellyseerr";
in
{
  systemd.tmpfiles.rules = [
    "d /home/${user}/docker 0775 ${user} users -"
    "d ${dataDir} 0775 ${user} users -"
    "d ${dataDir}/config 0775 ${user} users -"
  ];

  virtualisation.oci-containers.backend = "docker";
  virtualisation.oci-containers.containers.jellyseerr = {
    image = "fallenbagel/jellyseerr:latest";
    autoStart = true;
    ports = [ "5055:5055" ];
    volumes = [
      "${dataDir}/config:/app/config"
    ];
    environment = {
      PORT = "5055";
      TZ = config.steveos.timeZone or "Europe/Paris";
    };
  };

  networking.firewall.allowedTCPPorts = [ 5055 ];
}
"#.to_string()),
        "qbittorrent" => Ok(r#"{ config, lib, pkgs, ... }:

let
  user = config.steveos.user.username;
  dataDir = "/home/${user}/docker/qbittorrent";
in
{
  systemd.tmpfiles.rules = [
    "d /home/${user}/docker 0775 ${user} users -"
    "d ${dataDir} 0775 ${user} users -"
    "d ${dataDir}/config 0775 ${user} users -"
    "d ${dataDir}/downloads 0775 ${user} users -"
  ];

  virtualisation.oci-containers.backend = "docker";
  virtualisation.oci-containers.containers.qbittorrent = {
    image = "lscr.io/linuxserver/qbittorrent:latest";
    autoStart = true;
    ports = [
      "8085:8085"
      "6881:6881"
      "6881:6881/udp"
    ];
    volumes = [
      "${dataDir}/config:/config"
      "${dataDir}/downloads:/downloads"
    ];
    environment = {
      PUID = "1000";
      PGID = "100";
      TZ = config.steveos.timeZone or "Europe/Paris";
      WEBUI_PORT = "8085";
    };
  };

  networking.firewall.allowedTCPPorts = [ 8085 6881 ];
  networking.firewall.allowedUDPPorts = [ 6881 ];
}
"#.to_string()),
        "vaultwarden" => Ok(r#"{ config, lib, pkgs, ... }:

let
  user = config.steveos.user.username;
  dataDir = "/home/${user}/docker/vaultwarden";
in
{
  systemd.tmpfiles.rules = [
    "d /home/${user}/docker 0775 ${user} users -"
    "d ${dataDir} 0775 ${user} users -"
    "d ${dataDir}/data 0775 ${user} users -"
  ];

  virtualisation.oci-containers.backend = "docker";
  virtualisation.oci-containers.containers.vaultwarden = {
    image = "vaultwarden/server:latest";
    autoStart = true;
    ports = [ "8222:80" ];
    volumes = [
      "${dataDir}/data:/data"
    ];
    environment = {
      ROCKET_PORT = "80";
      TZ = config.steveos.timeZone or "Europe/Paris";
    };
  };

  networking.firewall.allowedTCPPorts = [ 8222 ];
}
"#.to_string()),
        "uptime-kuma" => Ok(r#"{ config, lib, pkgs, ... }:

let
  user = config.steveos.user.username;
  dataDir = "/home/${user}/docker/uptime-kuma";
in
{
  systemd.tmpfiles.rules = [
    "d /home/${user}/docker 0775 ${user} users -"
    "d ${dataDir} 0775 ${user} users -"
    "d ${dataDir}/data 0775 ${user} users -"
  ];

  virtualisation.oci-containers.backend = "docker";
  virtualisation.oci-containers.containers.uptime-kuma = {
    image = "louislam/uptime-kuma:latest";
    autoStart = true;
    ports = [ "3001:3001" ];
    volumes = [
      "${dataDir}/data:/app/data"
    ];
    environment = {
      TZ = config.steveos.timeZone or "Europe/Paris";
    };
  };

  networking.firewall.allowedTCPPorts = [ 3001 ];
}
"#.to_string()),
        _ => Err(format!("Module pour l'application '{}' non trouvé", app_id)),
    }
}
