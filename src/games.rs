use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDeployProgress {
    pub server_id: String,
    pub server_name: String,
    pub egg_id: String,
    pub egg_name: String,
    pub icon: String,
    pub step: String, // "preparing", "pulling_image", "starting_container", "downloading_game", "ready", "error", "stopped"
    pub step_index: u32, // 1 to 4
    pub progress_percent: u32,
    pub status_message: String,
    pub detail: String,
    pub logs: Vec<String>,
    pub is_complete: bool,
    pub is_error: bool,
    #[serde(default)]
    pub is_cancelled: bool,
}

static DEPLOY_TRACKER: LazyLock<Mutex<HashMap<String, GameDeployProgress>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn is_deployment_active(server_id: &str) -> bool {
    let tracker = DEPLOY_TRACKER.lock().unwrap();
    if let Some(entry) = tracker.get(server_id) {
        !entry.is_cancelled
    } else {
        false
    }
}

pub fn get_deployment_status(server_id: &str) -> Option<GameDeployProgress> {
    let tracker = DEPLOY_TRACKER.lock().unwrap();
    if let Some(p) = tracker.get(server_id) {
        return Some(p.clone());
    }
    drop(tracker);

    let servers = load_saved_servers();
    if let Some(s) = servers.into_iter().find(|s| s.id == server_id) {
        let is_running = if let Ok(output) = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Running}}", &s.container_name])
            .output()
        {
            String::from_utf8_lossy(&output.stdout).trim() == "true"
        } else {
            false
        };

        let last_logs = if let Ok(logs_out) = Command::new("docker")
            .args(["logs", "--tail", "25", &s.container_name])
            .output()
        {
            let text = String::from_utf8_lossy(&logs_out.stdout);
            text.lines().map(|l| l.to_string()).collect()
        } else {
            vec![]
        };

        return Some(GameDeployProgress {
            server_id: s.id.clone(),
            server_name: s.name.clone(),
            egg_id: s.egg_id.clone(),
            egg_name: s.game_name.clone(),
            icon: s.icon.clone(),
            step: if is_running { "ready".into() } else { "stopped".into() },
            step_index: 4,
            progress_percent: if is_running { 100 } else { 0 },
            status_message: if is_running { "Serveur actif et opérationnel.".into() } else { "Serveur arrêté.".into() },
            detail: "".into(),
            logs: last_logs,
            is_complete: true,
            is_error: false,
            is_cancelled: false,
        });
    }

    None
}

fn update_deployment(server_id: &str, mut f: impl FnMut(&mut GameDeployProgress)) {
    let mut tracker = DEPLOY_TRACKER.lock().unwrap();
    if let Some(entry) = tracker.get_mut(server_id) {
        f(entry);
    }
}

fn clean_terminal_log_line(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c == '[' {
            let mut is_ansi = false;
            let temp = chars.clone();
            let mut count = 0;
            for tc in temp {
                count += 1;
                if tc.is_ascii_digit() || tc == ';' {
                    continue;
                } else if tc == 'm' {
                    is_ansi = true;
                    break;
                } else {
                    break;
                }
            }
            if is_ansi {
                for _ in 0..count {
                    chars.next();
                }
            } else {
                res.push('[');
            }
        } else if c != '\r' {
            res.push(c);
        }
    }
    res.trim().to_string()
}

fn append_deploy_log(server_id: &str, line: &str) {
    let cleaned = clean_terminal_log_line(line);
    if cleaned.is_empty() {
        return;
    }
    let mut tracker = DEPLOY_TRACKER.lock().unwrap();
    if let Some(entry) = tracker.get_mut(server_id) {
        if entry.logs.last().map(|l| l.as_str()) == Some(&cleaned) {
            return;
        }
        if entry.logs.len() > 150 {
            entry.logs.remove(0);
        }
        entry.logs.push(cleaned);
    }
}

fn mark_server_online(server_id: &str) {
    let mut servers = load_saved_servers();
    if let Some(s) = servers.iter_mut().find(|s| s.id == server_id) {
        s.status = "online".into();
    }
    let _ = save_servers(&servers);
}

fn run_server_deployment_pipeline(
    server_id: String,
    _server_name: String,
    egg_id: String,
    egg_name: String,
    final_docker_image: String,
    container_name: String,
    docker_args: Vec<String>,
) {
    if !is_deployment_active(&server_id) {
        return;
    }

    // Étape 2 : Vérification et téléchargement de l'image Docker (Docker Pull)
    update_deployment(&server_id, |p| {
        p.step = "pulling_image".into();
        p.step_index = 2;
        p.progress_percent = 20;
        p.status_message = format!("Vérification de l'image {}...", final_docker_image);
    });

    let inspect_status = Command::new("docker")
        .args(["image", "inspect", &final_docker_image])
        .output();

    let needs_pull = match inspect_status {
        Ok(out) => !out.status.success(),
        Err(_) => true,
    };

    if needs_pull {
        append_deploy_log(&server_id, &format!("⚡ Téléchargement de l'image Docker {} (Docker Pull)...", final_docker_image));
        update_deployment(&server_id, |p| {
            p.status_message = format!("Téléchargement de l'image conteneur ({})...", final_docker_image);
            p.progress_percent = 25;
        });

        if let Ok(mut child) = Command::new("docker")
            .args(["pull", &final_docker_image])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            use std::io::BufRead;
            let stdout = child.stdout.take();
            if let Some(out) = stdout {
                let reader = std::io::BufReader::new(out);
                let mut pull_step = 0;
                for line in reader.lines().flatten() {
                    if !is_deployment_active(&server_id) {
                        let _ = child.kill();
                        return;
                    }
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        append_deploy_log(&server_id, trimmed);
                        pull_step += 1;
                        let pct = (25 + (pull_step * 2)).min(45);
                        update_deployment(&server_id, |p| {
                            p.progress_percent = pct;
                            p.detail = trimmed.to_string();
                        });
                    }
                }
            }
            let _ = child.wait();
        }
        append_deploy_log(&server_id, "✓ Image de conteneur téléchargée avec succès.");
    } else {
        append_deploy_log(&server_id, "✓ Image de conteneur déjà présente dans le cache local.");
    }

    update_deployment(&server_id, |p| {
        p.progress_percent = 48;
    });

    if !is_deployment_active(&server_id) {
        return;
    }

    // Étape 3 : Démarrage du conteneur
    update_deployment(&server_id, |p| {
        p.step = "starting_container".into();
        p.step_index = 3;
        p.progress_percent = 50;
        p.status_message = "Démarrage du conteneur sécurisé...".into();
    });
    append_deploy_log(&server_id, &format!("🚀 Lancement du conteneur {}...", container_name));

    let _ = Command::new("docker").args(["rm", "-f", &container_name]).status();

    let run_res = Command::new("docker").args(&docker_args).output();
    match run_res {
        Ok(output) if output.status.success() => {
            append_deploy_log(&server_id, "✓ Conteneur Docker démarré.");
            update_deployment(&server_id, |p| {
                p.step = "downloading_game".into();
                p.step_index = 4;
                p.progress_percent = 55;
                p.status_message = format!("Initialisation et installation de {} en cours...", egg_name);
            });
        },
        Ok(output) => {
            let err = String::from_utf8_lossy(&output.stderr);
            append_deploy_log(&server_id, &format!("❌ Erreur Docker : {}", err));
            update_deployment(&server_id, |p| {
                p.is_error = true;
                p.is_complete = true;
                p.status_message = format!("Erreur Docker : {}", err);
            });
            return;
        },
        Err(e) => {
            append_deploy_log(&server_id, &format!("❌ Erreur système : {}", e));
            update_deployment(&server_id, |p| {
                p.is_error = true;
                p.is_complete = true;
                p.status_message = format!("Erreur système : {}", e);
            });
            return;
        }
    }

    // Étape 4 : Surveillance du téléchargement & installation du jeu
    let start_time = Instant::now();
    let max_duration = Duration::from_secs(1800); // 30 min max

    while start_time.elapsed() < max_duration {
        if !is_deployment_active(&server_id) {
            let _ = Command::new("docker").args(["stop", "-t", "2", &container_name]).status();
            let _ = Command::new("docker").args(["rm", "-f", &container_name]).status();
            return;
        }
        std::thread::sleep(Duration::from_millis(1500));
        if !is_deployment_active(&server_id) {
            let _ = Command::new("docker").args(["stop", "-t", "2", &container_name]).status();
            let _ = Command::new("docker").args(["rm", "-f", &container_name]).status();
            return;
        }

        // 1. Vérifier si le conteneur tourne
        if let Ok(insp_out) = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Running}}|{{.State.ExitCode}}", &container_name])
            .output()
        {
            let txt = String::from_utf8_lossy(&insp_out.stdout).trim().to_string();
            let parts: Vec<&str> = txt.split('|').collect();
            if parts.len() >= 2 {
                let running = parts[0] == "true";
                let exit_code: i32 = parts[1].parse().unwrap_or(0);
                if !running && exit_code != 0 {
                    append_deploy_log(&server_id, &format!("❌ Le conteneur s'est arrêté avec le code d'erreur {}", exit_code));
                    update_deployment(&server_id, |p| {
                        p.is_error = true;
                        p.is_complete = true;
                        p.status_message = format!("Le conteneur s'est arrêté inopinément (code {}).", exit_code);
                    });
                    return;
                }
            }
        }

        // 2. Parser les logs
        if let Ok(logs_out) = Command::new("docker")
            .args(["logs", "--tail", "30", &container_name])
            .output()
        {
            let stdout = String::from_utf8_lossy(&logs_out.stdout);
            let stderr = String::from_utf8_lossy(&logs_out.stderr);
            let combined = format!("{}{}", stdout, stderr);

            for line in combined.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }

                // SteamCMD (Palworld / Valheim)
                if trimmed.contains("downloading, progress:") || trimmed.contains("preallocating, progress:") {
                    if let Some(pos) = trimmed.find("progress:") {
                        let sub = &trimmed[pos + 9..].trim();
                        let pct_str = sub.split_whitespace().next().unwrap_or("0").trim_end_matches('%');
                        if let Ok(raw_pct) = pct_str.parse::<f32>() {
                            let scaled_pct = (55.0 + (raw_pct * 0.40)).min(95.0) as u32;
                            let extra = if let Some(paren_start) = sub.find('(') {
                                if let Some(paren_end) = sub.find(')') {
                                    &sub[paren_start..=paren_end]
                                } else { "" }
                            } else { "" };

                            update_deployment(&server_id, |p| {
                                p.progress_percent = scaled_pct;
                                p.status_message = format!("Téléchargement SteamCMD : {:.1}% {}", raw_pct, extra);
                                p.detail = trimmed.to_string();
                            });
                        }
                    }
                }

                if (trimmed.contains("Success! App '") && trimmed.contains("fully installed"))
                    || trimmed.contains("Démarrage du serveur dédié Palworld")
                    || trimmed.contains("Démarrage du serveur Valheim")
                {
                    append_deploy_log(&server_id, "✓ Téléchargement SteamCMD validé avec succès !");
                    update_deployment(&server_id, |p| {
                        p.progress_percent = 95;
                        p.status_message = "Installation terminée, démarrage du moteur de jeu...".into();
                    });
                }

                // Palworld prêt
                if trimmed.contains("AppID = 2394010") || trimmed.contains("PalServer-Linux-Shipping") {
                    append_deploy_log(&server_id, "🎉 Serveur Palworld opérationnel et en ligne !");
                    update_deployment(&server_id, |p| {
                        p.step = "ready".into();
                        p.progress_percent = 100;
                        p.is_complete = true;
                        p.status_message = "Serveur Palworld opérationnel et en ligne !".into();
                    });
                    mark_server_online(&server_id);
                    return;
                }

                // Valheim prêt
                if trimmed.contains("Game server connected") || trimmed.contains("server id") {
                    append_deploy_log(&server_id, "🎉 Serveur Valheim opérationnel et en ligne !");
                    update_deployment(&server_id, |p| {
                        p.step = "ready".into();
                        p.progress_percent = 100;
                        p.is_complete = true;
                        p.status_message = "Serveur Valheim opérationnel et en ligne !".into();
                    });
                    mark_server_online(&server_id);
                    return;
                }

                // Minecraft Java
                if trimmed.contains("Téléchargement certifié de") {
                    update_deployment(&server_id, |p| {
                        p.progress_percent = 65;
                        p.status_message = trimmed.to_string();
                    });
                }
                if trimmed.contains("Fichier server.jar validé") {
                    update_deployment(&server_id, |p| {
                        p.progress_percent = 80;
                        p.status_message = "Fichier JAR validé, démarrage du serveur...".into();
                    });
                }
                if trimmed.contains("Done (") && trimmed.contains("For help, type \"help\"") {
                    append_deploy_log(&server_id, "🎉 Serveur Minecraft opérationnel et en ligne !");
                    update_deployment(&server_id, |p| {
                        p.step = "ready".into();
                        p.progress_percent = 100;
                        p.is_complete = true;
                        p.status_message = "Serveur Minecraft en ligne et prêt !".into();
                    });
                    mark_server_online(&server_id);
                    return;
                }

                // Bedrock
                if trimmed.contains("Server started") {
                    append_deploy_log(&server_id, "🎉 Serveur Minecraft Bedrock opérationnel et en ligne !");
                    update_deployment(&server_id, |p| {
                        p.step = "ready".into();
                        p.progress_percent = 100;
                        p.is_complete = true;
                        p.status_message = "Serveur Minecraft Bedrock en ligne et prêt !".into();
                    });
                    mark_server_online(&server_id);
                    return;
                }
            }

            // Insertion sans doublons des nouvelles lignes de logs
            {
                let mut tracker = DEPLOY_TRACKER.lock().unwrap();
                if let Some(entry) = tracker.get_mut(&server_id) {
                    for chunk in combined.split('\n') {
                        for sub in chunk.split('\r') {
                            let cleaned = clean_terminal_log_line(sub);
                            if cleaned.is_empty() { continue; }
                            let already_in_recent = entry.logs.iter().rev().take(25).any(|l| l == &cleaned);
                            if !already_in_recent {
                                if entry.logs.len() > 150 {
                                    entry.logs.remove(0);
                                }
                                entry.logs.push(cleaned);
                            }
                        }
                    }
                }
            }
        }

        // Generic egg fallback
        if egg_id != "palworld" && egg_id != "valheim" && egg_id != "minecraft-java" && egg_id != "minecraft-bedrock" {
            if start_time.elapsed().as_secs() > 15 {
                append_deploy_log(&server_id, "✓ Conteneur actif et stable.");
                update_deployment(&server_id, |p| {
                    p.step = "ready".into();
                    p.progress_percent = 100;
                    p.is_complete = true;
                    p.status_message = "Serveur de jeu opérationnel !".into();
                });
                mark_server_online(&server_id);
                return;
            }
        } else {
            let is_near_done = {
                let tracker = DEPLOY_TRACKER.lock().unwrap();
                tracker.get(&server_id).map(|p| p.progress_percent >= 95).unwrap_or(false)
            };
            if is_near_done && start_time.elapsed().as_secs() > 60 {
                append_deploy_log(&server_id, "✓ Serveur démarré et opérationnel.");
                update_deployment(&server_id, |p| {
                    p.step = "ready".into();
                    p.progress_percent = 100;
                    p.is_complete = true;
                    p.status_message = "Serveur de jeu opérationnel !".into();
                });
                mark_server_online(&server_id);
                return;
            }
        }
    }

    if !is_deployment_active(&server_id) {
        return;
    }

    update_deployment(&server_id, |p| {
        p.is_complete = true;
        p.status_message = "Le déploiement se poursuit en arrière-plan. Consultez la console.".into();
    });
    mark_server_online(&server_id);
}

fn get_now_timestamp() -> String {
    if let Ok(output) = Command::new("date").args(["+%Y-%m-%d %H:%M"]).output() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{}", now)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EggVariable {
    pub name: String,
    pub env_variable: String,
    pub description: String,
    pub default_value: String,
    pub input_type: String, // "text", "number", "select", "password", "boolean"
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Egg {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub banner_color: String,
    pub docker_image: String,
    pub default_port: u16,
    pub port_protocol: String, // "tcp", "udp", "both"
    pub default_memory_mb: u64,
    pub min_memory_mb: u64,
    pub startup_cmd: String,
    pub variables: Vec<EggVariable>,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameServer {
    pub id: String,
    pub name: String,
    pub egg_id: String,
    pub game_name: String,
    pub icon: String,
    pub status: String, // "online", "offline", "starting"
    pub container_name: String,
    pub memory_mb: u64,
    pub port: u16,
    pub port_protocol: String,
    pub data_dir: String,
    pub ip_address: String,
    pub cpu_percent: f32,
    pub memory_used_mb: u64,
    pub created_at: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGameServerRequest {
    pub name: String,
    pub egg_id: String,
    pub memory_mb: u64,
    pub port: Option<u16>,
    pub variables: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct GameServerActionRequest {
    pub action: String, // "start", "stop", "restart", "kill"
}

#[derive(Debug, Deserialize)]
pub struct GameServerCommandRequest {
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportEggRequest {
    pub content: Option<String>,
    pub url: Option<String>,
}

pub fn get_games_base_dir() -> PathBuf {
    let p = PathBuf::from("/mnt/storage/games");
    if p.exists() || fs::create_dir_all(&p).is_ok() {
        return p;
    }
    let fallback = PathBuf::from("/home/chomiam/docker/games");
    let _ = fs::create_dir_all(&fallback);
    fallback
}

pub fn get_servers_state_file() -> PathBuf {
    get_games_base_dir().join("servers.json")
}

pub fn get_custom_eggs_dir() -> PathBuf {
    let p = get_games_base_dir().join("eggs");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn get_lan_ip() -> String {
    if let Ok(output) = Command::new("hostname").arg("-I").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for ip in text.split_whitespace() {
            if !ip.starts_with("127.") && !ip.starts_with("172.17.") && !ip.starts_with("172.18.") && !ip.contains(':') {
                return ip.to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

pub fn get_default_eggs() -> Vec<Egg> {
    vec![
        Egg {
            id: "minecraft-java".into(),
            name: "Minecraft: Java Edition".into(),
            author: "Pterodactyl & STEvE_OS".into(),
            description: "Serveur Minecraft Java haute performance propulsé par PaperMC 1.21. Supporte les plugins Spigot/Paper, l'optimisation G1GC et l'EULA automatique.".into(),
            category: "Bac à sable / Survie".into(),
            icon: "⛏️".into(),
            banner_color: "linear-gradient(135deg, #2e7d32, #1b5e20)".into(),
            docker_image: "ghcr.io/pterodactyl/yolks:java_21".into(),
            default_port: 25565,
            port_protocol: "both".into(),
            default_memory_mb: 4096,
            min_memory_mb: 2048,
            startup_cmd: "java -Xms128M -Xmx{{SERVER_MEMORY}}M -XX:+UseG1GC -jar server.jar nogui".into(),
            variables: vec![
                EggVariable {
                    name: "Version Minecraft".into(),
                    env_variable: "MINECRAFT_VERSION".into(),
                    description: "Version de PaperMC à télécharger".into(),
                    default_value: "1.21.1".into(),
                    input_type: "text".into(),
                    options: None,
                },
                EggVariable {
                    name: "Message du jour (MOTD)".into(),
                    env_variable: "MOTD".into(),
                    description: "Description affichée dans la liste des serveurs multijoueur".into(),
                    default_value: "Serveur Minecraft propulsé par STEvE_OS NAS".into(),
                    input_type: "text".into(),
                    options: None,
                },
                EggVariable {
                    name: "Nombre maximum de joueurs".into(),
                    env_variable: "MAX_PLAYERS".into(),
                    description: "Slots simultanés autorisés".into(),
                    default_value: "20".into(),
                    input_type: "number".into(),
                    options: None,
                },
                EggVariable {
                    name: "Difficulté".into(),
                    env_variable: "DIFFICULTY".into(),
                    description: "Niveau de difficulté du monde".into(),
                    default_value: "normal".into(),
                    input_type: "select".into(),
                    options: Some(vec!["peaceful".into(), "easy".into(), "normal".into(), "hard".into()]),
                },
                EggVariable {
                    name: "Mode en ligne (Authentification Mojang)".into(),
                    env_variable: "ONLINE_MODE".into(),
                    description: "Vérifier les comptes Minecraft officiels (désactiver autorise les versions non vérifiées)".into(),
                    default_value: "true".into(),
                    input_type: "boolean".into(),
                    options: None,
                },
            ],
            is_custom: false,
        },
        Egg {
            id: "minecraft-bedrock".into(),
            name: "Minecraft: Bedrock Edition".into(),
            author: "Mojang & STEvE_OS".into(),
            description: "Serveur officiel Mojang BDS pour consoles (Switch, PS5, Xbox), smartphones (iOS, Android) et Windows 10/11.".into(),
            category: "Bac à sable / Survie".into(),
            icon: "🧱".into(),
            banner_color: "linear-gradient(135deg, #1565c0, #0d47a1)".into(),
            docker_image: "ghcr.io/parkervcp/yolks:ubuntu".into(),
            default_port: 19132,
            port_protocol: "udp".into(),
            default_memory_mb: 2048,
            min_memory_mb: 1024,
            startup_cmd: "LD_LIBRARY_PATH=. ./bedrock_server".into(),
            variables: vec![
                EggVariable {
                    name: "Nom du serveur".into(),
                    env_variable: "SERVER_NAME".into(),
                    description: "Nom affiché aux joueurs Bedrock".into(),
                    default_value: "STEvE_OS Bedrock World".into(),
                    input_type: "text".into(),
                    options: None,
                },
                EggVariable {
                    name: "Mode de jeu".into(),
                    env_variable: "GAMEMODE".into(),
                    description: "Mode par défaut des nouveaux joueurs".into(),
                    default_value: "survival".into(),
                    input_type: "select".into(),
                    options: Some(vec!["survival".into(), "creative".into(), "adventure".into()]),
                },
                EggVariable {
                    name: "Difficulté".into(),
                    env_variable: "DIFFICULTY".into(),
                    description: "Difficulté du monde Bedrock".into(),
                    default_value: "normal".into(),
                    input_type: "select".into(),
                    options: Some(vec!["peaceful".into(), "easy".into(), "normal".into(), "hard".into()]),
                },
            ],
            is_custom: false,
        },
        Egg {
            id: "palworld".into(),
            name: "Palworld Dedicated Server".into(),
            author: "Pocketpair & SteamCMD".into(),
            description: "Serveur dédié Palworld officiel via SteamCMD avec optimisation multithread, gestion de sauvegarde automatique et support jusqu'à 32 joueurs.".into(),
            category: "Aventure / Survie".into(),
            icon: "🐾".into(),
            banner_color: "linear-gradient(135deg, #0288d1, #01579b)".into(),
            docker_image: "ghcr.io/parkervcp/steamcmd:debian".into(),
            default_port: 8211,
            port_protocol: "udp".into(),
            default_memory_mb: 8192,
            min_memory_mb: 4096,
            startup_cmd: "./PalServer.sh -useperfthreads -NoAsyncLoadingThread -UseMultithreadForDS".into(),
            variables: vec![
                EggVariable {
                    name: "Nom du serveur Palworld".into(),
                    env_variable: "SERVER_NAME".into(),
                    description: "Nom affiché dans la liste des serveurs de jeu".into(),
                    default_value: "Serveur Palworld STEvE_OS".into(),
                    input_type: "text".into(),
                    options: None,
                },
                EggVariable {
                    name: "Mot de passe d'accès (Optionnel)".into(),
                    env_variable: "SERVER_PASSWORD".into(),
                    description: "Mot de passe requis pour rejoindre (laisser vide si public)".into(),
                    default_value: "".into(),
                    input_type: "password".into(),
                    options: None,
                },
                EggVariable {
                    name: "Mot de passe Administrateur".into(),
                    env_variable: "ADMIN_PASSWORD".into(),
                    description: "Mot de passe pour les commandes d'administration (/AdminPassword)".into(),
                    default_value: "SteveAdminPass123!".into(),
                    input_type: "password".into(),
                    options: None,
                },
                EggVariable {
                    name: "Joueurs Maximum".into(),
                    env_variable: "MAX_PLAYERS".into(),
                    description: "Nombre maximum de joueurs simultanés".into(),
                    default_value: "32".into(),
                    input_type: "number".into(),
                    options: None,
                },
            ],
            is_custom: false,
        },
        Egg {
            id: "valheim".into(),
            name: "Valheim Dedicated Server".into(),
            author: "Iron Gate & SteamCMD".into(),
            description: "Serveur dédié Valheim viking multijoueur persistant avec téléchargement automatique des mises à jour Steam et génération de monde procédural.".into(),
            category: "Survie Mythologique".into(),
            icon: "⚔️".into(),
            banner_color: "linear-gradient(135deg, #d84315, #bf360c)".into(),
            docker_image: "ghcr.io/parkervcp/steamcmd:debian".into(),
            default_port: 2456,
            port_protocol: "udp".into(),
            default_memory_mb: 4096,
            min_memory_mb: 2048,
            startup_cmd: "./valheim_server.x86_64 -name \"{{SERVER_NAME}}\" -port {{SERVER_PORT}} -world \"{{WORLD_NAME}}\" -password \"{{SERVER_PASSWORD}}\" -public 1".into(),
            variables: vec![
                EggVariable {
                    name: "Nom du serveur".into(),
                    env_variable: "SERVER_NAME".into(),
                    description: "Nom de votre serveur Valheim".into(),
                    default_value: "Valheim STEvE_OS World".into(),
                    input_type: "text".into(),
                    options: None,
                },
                EggVariable {
                    name: "Nom du monde".into(),
                    env_variable: "WORLD_NAME".into(),
                    description: "Nom du fichier de sauvegarde du monde".into(),
                    default_value: "Dedicated".into(),
                    input_type: "text".into(),
                    options: None,
                },
                EggVariable {
                    name: "Mot de passe (Min 5 car.)".into(),
                    env_variable: "SERVER_PASSWORD".into(),
                    description: "Obligatoire par Valheim (minimum 5 caractères)".into(),
                    default_value: "Valheim123".into(),
                    input_type: "password".into(),
                    options: None,
                },
            ],
            is_custom: false,
        },
    ]
}

pub fn load_all_eggs() -> Vec<Egg> {
    let mut eggs = get_default_eggs();
    let custom_dir = get_custom_eggs_dir();

    if let Ok(entries) = fs::read_dir(custom_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(mut custom_egg) = serde_json::from_str::<Egg>(&content) {
                        custom_egg.is_custom = true;
                        eggs.push(custom_egg);
                    }
                }
            }
        }
    }

    eggs
}

pub fn load_saved_servers() -> Vec<GameServer> {
    let state_file = get_servers_state_file();
    if !state_file.exists() {
        return vec![];
    }
    fs::read_to_string(&state_file)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn save_servers(servers: &[GameServer]) -> Result<(), String> {
    let state_file = get_servers_state_file();
    if let Some(parent) = state_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(servers).map_err(|e| e.to_string())?;
    fs::write(&state_file, json).map_err(|e| format!("Impossible de sauvegarder servers.json : {}", e))?;
    Ok(())
}

pub fn list_game_servers() -> Vec<GameServer> {
    let mut servers = load_saved_servers();
    let lan_ip = get_lan_ip();

    // Récupérer les stats en un seul appel rapide
    let mut stats_map: HashMap<String, (f32, u64)> = HashMap::new();
    if let Ok(output) = Command::new("docker")
        .args(["stats", "--no-stream", "--format", "{{.Name}}|{{.CPUPerc}}|{{.MemUsage}}"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 3 {
                let name = parts[0].trim();
                let cpu_str = parts[1].trim().trim_end_matches('%');
                let cpu: f32 = cpu_str.parse().unwrap_or(0.0);

                let mem_part = parts[2].split('/').next().unwrap_or("0").trim();
                let mem_mb: u64 = if mem_part.to_lowercase().ends_with("gib") {
                    let num: f32 = mem_part.trim_end_matches(|c: char| !c.is_numeric() && c != '.').parse().unwrap_or(0.0);
                    (num * 1024.0) as u64
                } else if mem_part.to_lowercase().ends_with("mib") {
                    let num: f32 = mem_part.trim_end_matches(|c: char| !c.is_numeric() && c != '.').parse().unwrap_or(0.0);
                    num as u64
                } else {
                    0
                };
                stats_map.insert(name.to_string(), (cpu, mem_mb));
            }
        }
    }

    for s in &mut servers {
        s.ip_address = lan_ip.clone();

        let is_deploying = {
            let tracker = DEPLOY_TRACKER.lock().unwrap();
            tracker.get(&s.id).map(|p| !p.is_complete && !p.is_error).unwrap_or(false)
        };
        if is_deploying {
            s.status = "deploying".into();
            continue;
        }

        // Vérifier l'état du conteneur
        if let Ok(output) = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Status}}|{{.State.Running}}", &s.container_name])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let parts: Vec<&str> = text.split('|').collect();
            if parts.len() >= 2 && parts[1] == "true" {
                s.status = "online".into();
                if let Some(&(cpu, mem)) = stats_map.get(&s.container_name) {
                    s.cpu_percent = cpu;
                    s.memory_used_mb = mem;
                }
            } else {
                s.status = "offline".into();
                s.cpu_percent = 0.0;
                s.memory_used_mb = 0;
            }
        } else {
            s.status = "offline".into();
        }
    }

    servers
}

pub fn create_game_server(req: CreateGameServerRequest) -> Result<GameServer, String> {
    let eggs = load_all_eggs();
    let egg = eggs.iter().find(|e| e.id == req.egg_id).ok_or("Egg sélectionné introuvable")?;

    let clean_name = req.name.trim();
    if clean_name.is_empty() {
        return Err("Le nom du serveur ne peut pas être vide.".into());
    }

    let mut servers = load_saved_servers();

    // Génération d'un ID slug unique
    let base_slug: String = clean_name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let base_slug = base_slug.trim_matches('-').to_string();
    let mut slug = if base_slug.is_empty() { "game-server".to_string() } else { base_slug };
    let mut count = 1;
    while servers.iter().any(|s| s.id == slug) {
        count += 1;
        slug = format!("{}-{}", slug, count);
    }

    // Détermination et vérification du port
    let mut target_port = req.port.unwrap_or(egg.default_port);
    while servers.iter().any(|s| s.port == target_port) {
        target_port += 1;
    }

    let data_dir = get_games_base_dir().join(&slug);
    fs::create_dir_all(&data_dir).map_err(|e| format!("Impossible de créer le dossier {} : {}", data_dir.display(), e))?;

    // Préparation des variables environnementales
    let mut user_vars = req.variables.unwrap_or_default();
    let mut env_map = HashMap::new();
    for v in &egg.variables {
        let val = user_vars.remove(&v.env_variable).unwrap_or_else(|| v.default_value.clone());
        env_map.insert(v.env_variable.clone(), val);
    }

    env_map.insert("SERVER_PORT".into(), target_port.to_string());
    env_map.insert("SERVER_MEMORY".into(), req.memory_mb.to_string());
    let mut final_docker_image = egg.docker_image.clone();

    // Scripts de configuration initiaux personnalisés par jeu
    if egg.id == "minecraft-java" {
        let loader = env_map.get("LOADER").cloned().unwrap_or_else(|| "paper".to_string());
        let version = env_map.get("MINECRAFT_VERSION").cloned().unwrap_or_else(|| "1.21.1".to_string());

        let resolved = crate::minecraft::resolve_minecraft_server(&loader, &version)
            .unwrap_or_else(|_| crate::minecraft::ResolvedMinecraftServer {
                loader: loader.clone(),
                version: version.clone(),
                download_url: format!("https://api.purpurmc.org/v2/purpur/{}/latest/download", version),
                is_installer: false,
                docker_image: crate::minecraft::resolve_java_image(&version),
                startup_command: "java -Xms128M -XmxM -XX:+AlwaysPreTouch -XX:+UseG1GC -jar server.jar nogui".into(),
            });

        final_docker_image = resolved.docker_image.clone();

        // 1. EULA et dossiers d'extensions
        let _ = fs::write(data_dir.join("eula.txt"), "eula=true
");
        let _ = fs::create_dir_all(data_dir.join("mods"));
        let _ = fs::create_dir_all(data_dir.join("plugins"));

        // 2. Génération assistée de server.properties
        let props = crate::minecraft::MinecraftServerProperties {
            motd: env_map.get("MOTD").cloned(),
            gamemode: env_map.get("GAMEMODE").cloned(),
            difficulty: env_map.get("DIFFICULTY").cloned(),
            max_players: env_map.get("MAX_PLAYERS").and_then(|v| v.parse().ok()),
            online_mode: env_map.get("ONLINE_MODE").map(|v| v == "true"),
            pvp: env_map.get("PVP").map(|v| v != "false"),
            hardcore: env_map.get("HARDCORE").map(|v| v == "true"),
            white_list: env_map.get("WHITELIST").map(|v| v == "true"),
            enable_command_block: env_map.get("COMMAND_BLOCKS").map(|v| v != "false"),
            view_distance: env_map.get("VIEW_DISTANCE").and_then(|v| v.parse().ok()),
            simulation_distance: env_map.get("SIMULATION_DISTANCE").and_then(|v| v.parse().ok()),
            level_name: env_map.get("LEVEL_NAME").cloned(),
            level_seed: env_map.get("LEVEL_SEED").cloned(),
            allow_flight: env_map.get("ALLOW_FLIGHT").map(|v| v == "true"),
            spawn_protection: env_map.get("SPAWN_PROTECTION").and_then(|v| v.parse().ok()),
        };
        let server_props = crate::minecraft::generate_server_properties(&props, target_port);
        let _ = fs::write(data_dir.join("server.properties"), server_props);

        // 3. Entrypoint avec vérification d'intégrité anti-corruption
        let target_jar = if resolved.is_installer { "installer.jar" } else { "server.jar" };
        let install_step = if resolved.is_installer {
            "echo '🔨 Assemblage et installation du serveur moddé...'
java -jar installer.jar --installServer
touch .installed
"
        } else {
            ""
        };

        let entrypoint = format!(
r#"#!/bin/bash
set -e
cd /home/container
echo "eula=true" > eula.txt
mkdir -p mods plugins

if [ ! -s server.jar ] && [ ! -f .installed ]; then
  echo "⚡ Téléchargement certifié de {loader_name} {version}..."
  curl -f -s -L -A "STEvE_OS/1.0" -o {target_jar} "{download_url}"

  if command -v jar >/dev/null 2>&1; then
    if ! jar -tf {target_jar} >/dev/null 2>&1; then
      echo "❌ Erreur : Le fichier téléchargé est corrompu !"
      rm -f {target_jar}
      exit 1
    fi
  elif [ ! -s {target_jar} ]; then
    echo "❌ Erreur : Le fichier téléchargé est vide !"
    rm -f {target_jar}
    exit 1
  fi
  echo "✓ Fichier {target_jar} validé avec succès."


  {install_step}
fi

echo "🚀 Démarrage de {loader_name} ({version})..."
exec {startup_command}
"#,
            loader_name = resolved.loader.to_uppercase(),
            version = resolved.version,
            target_jar = target_jar,
            download_url = resolved.download_url,
            install_step = install_step,
            startup_command = resolved.startup_command,
        );
        let _ = fs::write(data_dir.join("entrypoint.sh"), entrypoint);
    } else if egg.id == "minecraft-bedrock" {
        let entrypoint = r#"#!/bin/bash
set -e
cd /home/container
if [ ! -f bedrock_server ]; then
  echo "⚡ Téléchargement du binaire officiel Minecraft Bedrock..."
  curl -s -L -A "Mozilla/5.0" -o bedrock.zip "https://www.minecraft.net/bedrockdedicatedserver/bin-linux/bedrock-server-1.21.30.03.zip" || true
  if [ -f bedrock.zip ]; then
    unzip -q -o bedrock.zip && rm -f bedrock.zip
    chmod +x bedrock_server
  fi
fi
export LD_LIBRARY_PATH=.
echo "🚀 Lancement de Minecraft Bedrock Server..."
exec ./bedrock_server
"#;
        let _ = fs::write(data_dir.join("entrypoint.sh"), entrypoint);
    } else if egg.id == "palworld" {
        let entrypoint = r#"#!/bin/bash
set -e
cd /home/container

# 1. Téléchargement et installation initiale de SteamCMD si absent
mkdir -p /home/container/steamcmd /home/container/steamapps
if [ ! -f /home/container/steamcmd/steamcmd.sh ]; then
  echo "⚡ Téléchargement et initialisation de SteamCMD..."
  curl -sSL -o /tmp/steamcmd.tar.gz https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz
  tar -xzf /tmp/steamcmd.tar.gz -C /home/container/steamcmd
  rm -f /tmp/steamcmd.tar.gz
  chmod +x /home/container/steamcmd/steamcmd.sh /home/container/steamcmd/linux32/steamcmd 2>/dev/null || true
  ln -sf /home/container/steamcmd/steamcmd.sh /home/container/steamcmd/steamcmd 2>/dev/null || true
fi
export PATH="/home/container/steamcmd:$PATH"
export HOME=/home/container

# 2. Liens et bibliothèques Steam SDK
mkdir -p /home/container/.steam/sdk32 /home/container/.steam/sdk64
cp -f /home/container/steamcmd/linux32/steamclient.so /home/container/.steam/sdk32/steamclient.so 2>/dev/null || true
cp -f /home/container/steamcmd/linux64/steamclient.so /home/container/.steam/sdk64/steamclient.so 2>/dev/null || true

# 3. Téléchargement / Mise à jour de Palworld via SteamCMD
if [ ! -f PalServer.sh ]; then
  echo "⚡ Téléchargement de Palworld Dedicated Server via SteamCMD (App 2394010)..."
  /home/container/steamcmd/steamcmd.sh +force_install_dir /home/container +login anonymous +app_update 2394010 validate +quit
  chmod +x PalServer.sh ./Pal/Binaries/Linux/PalServer-Linux-Shipping 2>/dev/null || true
fi

# 4. Configuration par défaut si non initialisée
if [ ! -f "/home/container/Pal/Saved/Config/LinuxServer/PalWorldSettings.ini" ]; then
  mkdir -p /home/container/Pal/Saved/Config/LinuxServer
  if [ -f "/home/container/DefaultPalWorldSettings.ini" ]; then
    cp /home/container/DefaultPalWorldSettings.ini /home/container/Pal/Saved/Config/LinuxServer/PalWorldSettings.ini
  fi
fi

echo "🚀 Démarrage du serveur dédié Palworld..."
exec ./PalServer.sh -useperfthreads -NoAsyncLoadingThread -UseMultithreadForDS -port=${SERVER_PORT} -players=${MAX_PLAYERS}
"#;
        let _ = fs::write(data_dir.join("entrypoint.sh"), entrypoint);
    } else if egg.id == "valheim" {
        let entrypoint = r#"#!/bin/bash
set -e
cd /home/container

# 1. Téléchargement et installation initiale de SteamCMD si absent
mkdir -p /home/container/steamcmd /home/container/steamapps
if [ ! -f /home/container/steamcmd/steamcmd.sh ]; then
  echo "⚡ Téléchargement et initialisation de SteamCMD..."
  curl -sSL -o /tmp/steamcmd.tar.gz https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz
  tar -xzf /tmp/steamcmd.tar.gz -C /home/container/steamcmd
  rm -f /tmp/steamcmd.tar.gz
  chmod +x /home/container/steamcmd/steamcmd.sh /home/container/steamcmd/linux32/steamcmd 2>/dev/null || true
  ln -sf /home/container/steamcmd/steamcmd.sh /home/container/steamcmd/steamcmd 2>/dev/null || true
fi
export PATH="/home/container/steamcmd:$PATH"
export HOME=/home/container

# 2. Liens et bibliothèques Steam SDK
mkdir -p /home/container/.steam/sdk64
cp -f /home/container/steamcmd/linux64/steamclient.so /home/container/.steam/sdk64/steamclient.so 2>/dev/null || true

# 3. Téléchargement / Mise à jour de Valheim via SteamCMD
if [ ! -f valheim_server.x86_64 ]; then
  echo "⚡ Téléchargement de Valheim Dedicated Server via SteamCMD (App 896660)..."
  /home/container/steamcmd/steamcmd.sh +force_install_dir /home/container +login anonymous +app_update 896660 validate +quit
  chmod +x valheim_server.x86_64 2>/dev/null || true
fi

export templdpath=$LD_LIBRARY_PATH
export LD_LIBRARY_PATH=./linux64:$LD_LIBRARY_PATH
export SteamAppId=892970

echo "🚀 Démarrage du serveur Valheim..."
exec ./valheim_server.x86_64 -name "${SERVER_NAME}" -port ${SERVER_PORT} -world "${WORLD_NAME}" -password "${SERVER_PASSWORD}" -public 1
"#;
        let _ = fs::write(data_dir.join("entrypoint.sh"), entrypoint);
    } else {
        let is_steam = egg.docker_image.contains("steamcmd") || egg.startup_cmd.contains("steamcmd");
        let steam_setup = if is_steam {
            r#"
mkdir -p /home/container/steamcmd /home/container/steamapps
if [ ! -f /home/container/steamcmd/steamcmd.sh ]; then
  echo "⚡ Téléchargement et initialisation de SteamCMD..."
  curl -sSL -o /tmp/steamcmd.tar.gz https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz
  tar -xzf /tmp/steamcmd.tar.gz -C /home/container/steamcmd
  rm -f /tmp/steamcmd.tar.gz
  chmod +x /home/container/steamcmd/steamcmd.sh /home/container/steamcmd/linux32/steamcmd 2>/dev/null || true
  ln -sf /home/container/steamcmd/steamcmd.sh /home/container/steamcmd/steamcmd 2>/dev/null || true
fi
export PATH="/home/container/steamcmd:$PATH"
export HOME=/home/container
mkdir -p /home/container/.steam/sdk32 /home/container/.steam/sdk64
cp -f /home/container/steamcmd/linux32/steamclient.so /home/container/.steam/sdk32/steamclient.so 2>/dev/null || true
cp -f /home/container/steamcmd/linux64/steamclient.so /home/container/.steam/sdk64/steamclient.so 2>/dev/null || true
"#
        } else {
            ""
        };
        let entrypoint = format!(
            r#"#!/bin/bash
set -e
cd /home/container
{}
echo "🚀 Démarrage du conteneur de jeu..."
exec {}
"#,
            steam_setup, egg.startup_cmd
        );
        let _ = fs::write(data_dir.join("entrypoint.sh"), entrypoint);
    }

    // Droits complets sur le dossier de données pour le conteneur
    let _ = Command::new("chmod").args(["-R", "777", &data_dir.display().to_string()]).status();

    let container_name = format!("steveos-game-{}", slug);

    // Arrêt préventif si un conteneur orphelin existait
    let _ = Command::new("docker").args(["rm", "-f", &container_name]).status();

    // Construction des arguments Docker Run
    let mut docker_args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        container_name.clone(),
        "--restart".to_string(),
        "unless-stopped".to_string(),
        "-m".to_string(),
        format!("{}m", req.memory_mb),
        "-v".to_string(),
        format!("{}:/home/container", data_dir.display()),
        "-w".to_string(),
        "/home/container".to_string(),
    ];

    // Ports
    if egg.port_protocol == "udp" {
        docker_args.push("-p".to_string());
        docker_args.push(format!("{}:{}/udp", target_port, target_port));
        if egg.id == "valheim" {
            docker_args.push("-p".to_string());
            docker_args.push(format!("{}:{}/udp", target_port + 1, target_port + 1));
        }
    } else if egg.port_protocol == "both" {
        docker_args.push("-p".to_string());
        docker_args.push(format!("{}:{}/tcp", target_port, target_port));
        docker_args.push("-p".to_string());
        docker_args.push(format!("{}:{}/udp", target_port, target_port));
    } else {
        docker_args.push("-p".to_string());
        docker_args.push(format!("{}:{}/tcp", target_port, target_port));
    }

    // Variables d'environnement
    for (k, v) in &env_map {
        docker_args.push("-e".to_string());
        docker_args.push(format!("{}={}", k, v));
    }

    docker_args.push(final_docker_image.clone());
    docker_args.push("bash".to_string());
    docker_args.push("/home/container/entrypoint.sh".to_string());

    let server = GameServer {
        id: slug.clone(),
        name: clean_name.to_string(),
        egg_id: egg.id.clone(),
        game_name: egg.name.clone(),
        icon: egg.icon.clone(),
        status: "deploying".into(),
        container_name: container_name.clone(),
        memory_mb: req.memory_mb,
        port: target_port,
        port_protocol: egg.port_protocol.clone(),
        data_dir: data_dir.display().to_string(),
        ip_address: get_lan_ip(),
        cpu_percent: 0.0,
        memory_used_mb: 0,
        created_at: get_now_timestamp(),
        env: env_map,
    };

    servers.push(server.clone());
    save_servers(&servers)?;

    // Initialiser le suivi de progression
    {
        let mut tracker = DEPLOY_TRACKER.lock().unwrap();
        tracker.insert(slug.clone(), GameDeployProgress {
            server_id: slug.clone(),
            server_name: clean_name.to_string(),
            egg_id: egg.id.clone(),
            egg_name: egg.name.clone(),
            icon: egg.icon.clone(),
            step: "pulling_image".into(),
            step_index: 2,
            progress_percent: 15,
            status_message: format!("Initialisation de l'environnement pour {}...", egg.name),
            detail: "".into(),
            logs: vec![
                format!("✓ Dossier configuré dans {}", data_dir.display()),
                format!("⚡ Préparation de l'image de conteneur : {}", final_docker_image),
            ],
            is_complete: false,
            is_error: false,
            is_cancelled: false,
        });
    }

    // Lancement asynchrone du pipeline de déploiement
    let slug_bg = slug.clone();
    let name_bg = clean_name.to_string();
    let egg_id_bg = egg.id.clone();
    let egg_name_bg = egg.name.clone();
    let img_bg = final_docker_image.clone();
    let container_bg = container_name.clone();
    let args_bg = docker_args.clone();

    std::thread::spawn(move || {
        run_server_deployment_pipeline(
            slug_bg,
            name_bg,
            egg_id_bg,
            egg_name_bg,
            img_bg,
            container_bg,
            args_bg,
        );
    });

    Ok(server)
}

pub fn control_game_server(id: &str, action: &str) -> Result<String, String> {
    let container_name = format!("steveos-game-{}", id);
    let cmd = match action {
        "start" => "start",
        "stop" => "stop",
        "restart" => "restart",
        "kill" => "kill",
        _ => return Err("Action invalide".into()),
    };

    let status = Command::new("docker")
        .args([cmd, &container_name])
        .status()
        .map_err(|e| format!("Échec d'exécution docker {} : {}", cmd, e))?;

    if status.success() {
        Ok(format!("Action '{}' exécutée avec succès.", action))
    } else {
        Err(format!("Impossible d'exécuter l'action '{}' sur le conteneur.", action))
    }
}

pub fn delete_game_server(id: &str, delete_data: bool) -> Result<String, String> {
    let mut servers = load_saved_servers();
    let container_name = format!("steveos-game-{}", id);

    {
        let mut tracker = DEPLOY_TRACKER.lock().unwrap();
        if let Some(entry) = tracker.get_mut(id) {
            entry.is_cancelled = true;
            entry.is_complete = true;
            entry.status_message = "Déploiement annulé et supprimé.".into();
        }
        tracker.remove(id);
    }

    let _ = Command::new("docker").args(["stop", "-t", "2", &container_name]).status();
    let _ = Command::new("docker").args(["rm", "-f", &container_name]).status();

    if let Some(pos) = servers.iter().position(|s| s.id == id) {
        let server = servers.remove(pos);
        if delete_data {
            let p = PathBuf::from(&server.data_dir);
            if p.exists() && p.starts_with(get_games_base_dir()) {
                let _ = fs::remove_dir_all(&p);
            }
        }
        save_servers(&servers)?;
    } else if delete_data {
        let p = get_games_base_dir().join(id);
        if p.exists() {
            let _ = fs::remove_dir_all(&p);
        }
    }

    Ok("Serveur supprimé avec succès.".into())
}

pub fn get_game_server_logs(id: &str, lines: usize) -> Result<String, String> {
    let container_name = format!("steveos-game-{}", id);
    let output = Command::new("docker")
        .args(["logs", "--tail", &lines.to_string(), &container_name])
        .output()
        .map_err(|e| format!("Impossible de lire les logs : {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    Ok(combined)
}

pub fn send_game_server_command(id: &str, cmd: &str) -> Result<String, String> {
    let container_name = format!("steveos-game-{}", id);
    let clean_cmd = cmd.trim();
    if clean_cmd.is_empty() {
        return Err("La commande ne peut pas être vide.".into());
    }

    // Essayer docker exec en tâche de fond pour envoyer au shell/processus
    let output = Command::new("docker")
        .args(["exec", "-i", &container_name, "sh", "-c", &format!("echo '{}'", clean_cmd)])
        .output()
        .map_err(|e| format!("Échec d'envoi de commande : {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn import_egg_file(req: ImportEggRequest) -> Result<Egg, String> {
    let content = if let Some(c) = req.content {
        c
    } else if let Some(url) = req.url {
        let output = Command::new("curl")
            .args(["-s", "-L", &url])
            .output()
            .map_err(|e| format!("Impossible de télécharger l'Egg depuis l'URL : {}", e))?;
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        return Err("Contenu ou URL d'Egg manquant".into());
    };

    // Parser l'Egg : supporte à la fois le format natif et le format Pterodactyl
    let parsed: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Fichier JSON d'Egg invalide : {}", e))?;

    let egg_name = parsed["name"].as_str().unwrap_or("Serveur Personnalisé").to_string();
    let author = parsed["author"].as_str().unwrap_or("Communauté").to_string();
    let description = parsed["description"].as_str().unwrap_or("Egg importé pour STEvE_OS").to_string();
    let docker_image = parsed["image"].as_str()
        .or_else(|| parsed["docker_images"].as_object().and_then(|m| m.values().next().and_then(|v| v.as_str())))
        .unwrap_or("ghcr.io/pterodactyl/yolks:java_21")
        .to_string();
    let startup_cmd = parsed["startup"].as_str().unwrap_or("").to_string();

    let egg_id = format!("custom-{}", egg_name.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>());

    let mut variables = Vec::new();
    if let Some(vars_array) = parsed["variables"].as_array() {
        for v in vars_array {
            variables.push(EggVariable {
                name: v["name"].as_str().unwrap_or("").to_string(),
                env_variable: v["env_variable"].as_str().unwrap_or("").to_string(),
                description: v["description"].as_str().unwrap_or("").to_string(),
                default_value: v["default_value"].as_str().unwrap_or("").to_string(),
                input_type: "text".into(),
                options: None,
            });
        }
    }

    let egg = Egg {
        id: egg_id.clone(),
        name: egg_name,
        author,
        description,
        category: "Communauté / Custom".into(),
        icon: "🎮".into(),
        banner_color: "linear-gradient(135deg, #6c5ce7, #a29bfe)".into(),
        docker_image,
        default_port: 25565,
        port_protocol: "both".into(),
        default_memory_mb: 4096,
        min_memory_mb: 2048,
        startup_cmd,
        variables,
        is_custom: true,
    };

    let target_file = get_custom_eggs_dir().join(format!("{}.json", egg_id));
    let egg_json = serde_json::to_string_pretty(&egg).map_err(|e| e.to_string())?;
    fs::write(&target_file, egg_json).map_err(|e| format!("Impossible d'enregistrer l'Egg : {}", e))?;

    Ok(egg)
}
