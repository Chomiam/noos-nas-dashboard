use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftLoader {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub badge: String,
    pub default_version: String,
    pub recommended_ram_mb: u32,
    pub min_ram_mb: u32,
    pub supports_plugins: bool,
    pub supports_mods: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveMinecraftRequest {
    pub loader: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedMinecraftServer {
    pub loader: String,
    pub version: String,
    pub download_url: String,
    pub is_installer: bool,
    pub docker_image: String,
    pub startup_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MinecraftServerProperties {
    pub motd: Option<String>,
    pub gamemode: Option<String>,
    pub difficulty: Option<String>,
    pub max_players: Option<u32>,
    pub online_mode: Option<bool>,
    pub pvp: Option<bool>,
    pub hardcore: Option<bool>,
    pub white_list: Option<bool>,
    pub enable_command_block: Option<bool>,
    pub view_distance: Option<u32>,
    pub simulation_distance: Option<u32>,
    pub level_name: Option<String>,
    pub level_seed: Option<String>,
    pub allow_flight: Option<bool>,
    pub spawn_protection: Option<u32>,
}

static VERSIONS_CACHE: LazyLock<Mutex<HashMap<String, (Instant, Vec<String>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn get_available_loaders() -> Vec<MinecraftLoader> {
    vec![
        MinecraftLoader {
            id: "paper".into(),
            name: "PaperMC".into(),
            description: "Serveur haute performance optimisé pour l'anti-lag avec support complet des plugins Bukkit/Spigot/Paper.".into(),
            category: "Plugins & Performance (Recommandé)".into(),
            icon: "📄".into(),
            badge: "RECOMMANDÉ".into(),
            default_version: "1.21.1".into(),
            recommended_ram_mb: 4096,
            min_ram_mb: 2048,
            supports_plugins: true,
            supports_mods: false,
        },
        MinecraftLoader {
            id: "purpur".into(),
            name: "Purpur".into(),
            description: "Fork ultra-optimisé et hautement personnalisable de Paper, idéal pour le gameplay survie fluide.".into(),
            category: "Plugins & Survie Optimisée".into(),
            icon: "🟣".into(),
            badge: "PERFORMANCE".into(),
            default_version: "1.21.1".into(),
            recommended_ram_mb: 4096,
            min_ram_mb: 2048,
            supports_plugins: true,
            supports_mods: false,
        },
        MinecraftLoader {
            id: "fabric".into(),
            name: "Fabric".into(),
            description: "Loader de mods ultra-léger et moderne, plébiscité pour les optimisations techniques (Sodium, Lithium).".into(),
            category: "Mods Légers & Technique".into(),
            icon: "🧵".into(),
            badge: "MODS RAPIDES".into(),
            default_version: "1.21.1".into(),
            recommended_ram_mb: 4096,
            min_ram_mb: 2048,
            supports_plugins: false,
            supports_mods: true,
        },
        MinecraftLoader {
            id: "neoforge".into(),
            name: "NeoForge".into(),
            description: "Le nouveau standard officiel de l'équipe de Forge pour les versions récentes (1.20.2 et supérieures).".into(),
            category: "Gros Modpacks Récents".into(),
            icon: "⚡".into(),
            badge: "MODPACKS MODERNES".into(),
            default_version: "1.21.1".into(),
            recommended_ram_mb: 6144,
            min_ram_mb: 4096,
            supports_plugins: false,
            supports_mods: true,
        },
        MinecraftLoader {
            id: "forge".into(),
            name: "Forge".into(),
            description: "La plateforme historique indispensable pour les modpacks classiques et rétro (1.12.2, 1.16.5, 1.20.1).".into(),
            category: "Modpacks Historiques".into(),
            icon: "🔨".into(),
            badge: "MODPACKS CLASSIQUES".into(),
            default_version: "1.20.1".into(),
            recommended_ram_mb: 6144,
            min_ram_mb: 4096,
            supports_plugins: false,
            supports_mods: true,
        },
        MinecraftLoader {
            id: "vanilla".into(),
            name: "Vanilla Mojang".into(),
            description: "Le serveur officiel certifié par Mojang Studios, pour une expérience 100% pure sans modification.".into(),
            category: "Officiel Pur".into(),
            icon: "☕".into(),
            badge: "OFFICIEL".into(),
            default_version: "1.21.1".into(),
            recommended_ram_mb: 3072,
            min_ram_mb: 2048,
            supports_plugins: false,
            supports_mods: false,
        },
    ]
}

fn fetch_curl_json(url: &str) -> Result<serde_json::Value, String> {
    let output = Command::new("curl")
        .args(["-s", "-L", "-A", "Noos/1.0", "--max-time", "6", url])
        .output()
        .map_err(|e| format!("Erreur d'exécution curl : {}", e))?;

    if !output.status.success() {
        return Err("Échec HTTP lors du scraping".into());
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Format JSON invalide : {}", e))
}

pub fn get_versions_for_loader(loader: &str) -> Vec<String> {
    if let Ok(cache) = VERSIONS_CACHE.lock() {
        if let Some((instant, list)) = cache.get(loader) {
            if instant.elapsed() < Duration::from_secs(900) && !list.is_empty() {
                return list.clone();
            }
        }
    }

    let versions = match loader {
        "paper" => fetch_paper_versions(),
        "purpur" => fetch_purpur_versions(),
        "fabric" => fetch_fabric_versions(),
        "neoforge" => fetch_neoforge_versions(),
        "forge" => fetch_forge_versions(),
        "vanilla" => fetch_vanilla_versions(),
        _ => fetch_vanilla_versions(),
    };

    let result = if versions.is_empty() {
        fallback_versions(loader)
    } else {
        versions
    };

    if let Ok(mut cache) = VERSIONS_CACHE.lock() {
        cache.insert(loader.to_string(), (Instant::now(), result.clone()));
    }

    result
}

fn fallback_versions(loader: &str) -> Vec<String> {
    match loader {
        "forge" => vec!["1.20.1".into(), "1.19.4".into(), "1.19.2".into(), "1.18.2".into(), "1.16.5".into(), "1.12.2".into(), "1.7.10".into()],
        "neoforge" => vec!["1.21.1".into(), "1.21".into(), "1.20.6".into(), "1.20.4".into()],
        _ => vec!["1.21.1".into(), "1.21".into(), "1.20.4".into(), "1.20.1".into(), "1.19.4".into(), "1.18.2".into(), "1.16.5".into()],
    }
}

fn fetch_paper_versions() -> Vec<String> {
    if let Ok(json) = fetch_curl_json("https://fill.papermc.io/v3/projects/paper") {
        if let Some(obj) = json.get("versions").and_then(|v| v.as_object()) {
            let mut list = Vec::new();
            for sublist in obj.values() {
                if let Some(arr) = sublist.as_array() {
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            if !s.contains("pre") && !s.contains("rc") && !s.contains("snapshot") {
                                list.push(s.to_string());
                            }
                        }
                    }
                }
            }
            sort_versions(&mut list);
            return list;
        }
    }
    Vec::new()
}

fn fetch_purpur_versions() -> Vec<String> {
    if let Ok(json) = fetch_curl_json("https://api.purpurmc.org/v2/purpur") {
        if let Some(arr) = json.get("versions").and_then(|v| v.as_array()) {
            let mut list: Vec<String> = arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.contains("pre") && !s.contains("rc"))
                .map(|s| s.to_string())
                .collect();
            sort_versions(&mut list);
            return list;
        }
    }
    Vec::new()
}

fn fetch_fabric_versions() -> Vec<String> {
    if let Ok(json) = fetch_curl_json("https://meta.fabricmc.net/v2/versions/game") {
        if let Some(arr) = json.as_array() {
            let mut list: Vec<String> = arr.iter()
                .filter(|item| item.get("stable").and_then(|s| s.as_bool()).unwrap_or(false))
                .filter_map(|item| item.get("version").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect();
            sort_versions(&mut list);
            return list;
        }
    }
    Vec::new()
}

fn fetch_vanilla_versions() -> Vec<String> {
    if let Ok(json) = fetch_curl_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json") {
        if let Some(arr) = json.get("versions").and_then(|v| v.as_array()) {
            let mut list: Vec<String> = arr.iter()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("release"))
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
                .map(|s| s.to_string())
                .collect();
            sort_versions(&mut list);
            return list;
        }
    }
    Vec::new()
}

fn fetch_forge_versions() -> Vec<String> {
    if let Ok(json) = fetch_curl_json("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json") {
        if let Some(promos) = json.get("promos").and_then(|p| p.as_object()) {
            let mut seen = std::collections::HashSet::new();
            let mut list = Vec::new();
            for key in promos.keys() {
                if let Some(ver) = key.split('-').next() {
                    if seen.insert(ver.to_string()) {
                        list.push(ver.to_string());
                    }
                }
            }
            sort_versions(&mut list);
            return list;
        }
    }
    Vec::new()
}

fn fetch_neoforge_versions() -> Vec<String> {
    let mut list = vec![
        "1.21.1".to_string(),
        "1.21".to_string(),
        "1.20.6".to_string(),
        "1.20.4".to_string(),
        "1.20.2".to_string(),
    ];
    sort_versions(&mut list);
    list
}

fn sort_versions(list: &mut [String]) {
    list.sort_by(|a, b| {
        let parse_parts = |s: &str| -> Vec<u32> {
            s.split('.').filter_map(|p| p.parse::<u32>().ok()).collect()
        };
        let parts_b = parse_parts(b);
        let parts_a = parse_parts(a);
        parts_b.cmp(&parts_a)
    });
}

pub fn resolve_java_image(mc_version: &str) -> String {
    let parts: Vec<u32> = mc_version.split('.').filter_map(|p| p.parse::<u32>().ok()).collect();
    if parts.len() >= 2 {
        let major = parts[0];
        let minor = parts[1];
        let patch = parts.get(2).copied().unwrap_or(0);

        if major > 1 || (major == 1 && minor > 20) || (major == 1 && minor == 20 && patch >= 5) {
            return "ghcr.io/pterodactyl/yolks:java_21".into();
        }
        if major == 1 && minor >= 17 {
            return "ghcr.io/pterodactyl/yolks:java_17".into();
        }
    }
    "ghcr.io/pterodactyl/yolks:java_17".into()
}

pub fn resolve_minecraft_server(loader: &str, version: &str) -> Result<ResolvedMinecraftServer, String> {
    let docker_image = resolve_java_image(version);

    match loader {
        "paper" => {
            let url = format!("https://fill.papermc.io/v3/projects/paper/versions/{}/builds/latest", version);
            let dl_url = if let Ok(json) = fetch_curl_json(&url) {
                json.get("downloads")
                    .and_then(|d| d.get("server:default"))
                    .and_then(|s| s.get("url"))
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("https://api.purpurmc.org/v2/purpur/{}/latest/download", version))
            } else {
                format!("https://api.purpurmc.org/v2/purpur/{}/latest/download", version)
            };

            Ok(ResolvedMinecraftServer {
                loader: "paper".into(),
                version: version.into(),
                download_url: dl_url,
                is_installer: false,
                docker_image,
                startup_command: "java -Xms128M -Xmx${SERVER_MEMORY}M -XX:+AlwaysPreTouch -XX:+UseG1GC -jar server.jar nogui".into(),
            })
        }
        "purpur" => {
            let dl_url = format!("https://api.purpurmc.org/v2/purpur/{}/latest/download", version);
            Ok(ResolvedMinecraftServer {
                loader: "purpur".into(),
                version: version.into(),
                download_url: dl_url,
                is_installer: false,
                docker_image,
                startup_command: "java -Xms128M -Xmx${SERVER_MEMORY}M -XX:+AlwaysPreTouch -XX:+UseG1GC -jar server.jar nogui".into(),
            })
        }
        "fabric" => {
            let loader_ver = "0.16.5";
            let dl_url = format!("https://meta.fabricmc.net/v2/versions/loader/{}/{}/1.0.1/server/jar", version, loader_ver);
            Ok(ResolvedMinecraftServer {
                loader: "fabric".into(),
                version: version.into(),
                download_url: dl_url,
                is_installer: false,
                docker_image,
                startup_command: "java -Xms128M -Xmx${SERVER_MEMORY}M -XX:+AlwaysPreTouch -XX:+UseG1GC -jar server.jar nogui".into(),
            })
        }
        "vanilla" => {
            let mut dl_url = String::new();
            if let Ok(manifest) = fetch_curl_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json") {
                if let Some(versions_arr) = manifest.get("versions").and_then(|v| v.as_array()) {
                    if let Some(v_obj) = versions_arr.iter().find(|item| item.get("id").and_then(|id| id.as_str()) == Some(version)) {
                        if let Some(pkg_url) = v_obj.get("url").and_then(|u| u.as_str()) {
                            if let Ok(pkg) = fetch_curl_json(pkg_url) {
                                if let Some(u) = pkg.get("downloads").and_then(|d| d.get("server")).and_then(|s| s.get("url")).and_then(|u| u.as_str()) {
                                    dl_url = u.to_string();
                                }
                            }
                        }
                    }
                }
            }

            if dl_url.is_empty() {
                return Err(format!("Version Vanilla {} introuvable sur le manifest Mojang", version));
            }

            Ok(ResolvedMinecraftServer {
                loader: "vanilla".into(),
                version: version.into(),
                download_url: dl_url,
                is_installer: false,
                docker_image,
                startup_command: "java -Xms128M -Xmx${SERVER_MEMORY}M -jar server.jar nogui".into(),
            })
        }
        "forge" => {
            let mut forge_ver = String::new();
            if let Ok(promos_json) = fetch_curl_json("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json") {
                if let Some(promos) = promos_json.get("promos").and_then(|p| p.as_object()) {
                    let rec_key = format!("{}-recommended", version);
                    let lat_key = format!("{}-latest", version);
                    if let Some(v) = promos.get(&rec_key).or_else(|| promos.get(&lat_key)).and_then(|v| v.as_str()) {
                        forge_ver = v.to_string();
                    }
                }
            }

            if forge_ver.is_empty() {
                forge_ver = match version {
                    "1.20.1" => "47.3.0".into(),
                    "1.19.4" => "45.2.0".into(),
                    "1.19.2" => "43.3.0".into(),
                    "1.18.2" => "40.2.0".into(),
                    "1.16.5" => "36.2.39".into(),
                    "1.12.2" => "14.23.5.2860".into(),
                    _ => "47.3.0".into(),
                };
            }

            let dl_url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{}-{}/forge-{}-{}-installer.jar", version, forge_ver, version, forge_ver);

            Ok(ResolvedMinecraftServer {
                loader: "forge".into(),
                version: version.into(),
                download_url: dl_url,
                is_installer: true,
                docker_image,
                startup_command: "./run.sh nogui".into(),
            })
        }
        "neoforge" => {
            let neoforge_ver = match version {
                "1.21.1" => "21.1.88",
                "1.21" => "21.0.167",
                "1.20.6" => "20.6.119",
                "1.20.4" => "20.4.237",
                _ => "21.1.88",
            };

            let dl_url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar", neoforge_ver, neoforge_ver);

            Ok(ResolvedMinecraftServer {
                loader: "neoforge".into(),
                version: version.into(),
                download_url: dl_url,
                is_installer: true,
                docker_image,
                startup_command: "./run.sh nogui".into(),
            })
        }
        _ => Err(format!("Loader inconnu : {}", loader)),
    }
}

pub fn generate_server_properties(props: &MinecraftServerProperties, port: u16) -> String {
    let motd = props.motd.as_deref().unwrap_or("Serveur Minecraft propulsé par Noos NAS");
    let gamemode = props.gamemode.as_deref().unwrap_or("survival");
    let difficulty = props.difficulty.as_deref().unwrap_or("normal");
    let max_players = props.max_players.unwrap_or(20);
    let online_mode = props.online_mode.unwrap_or(true);
    let pvp = props.pvp.unwrap_or(true);
    let hardcore = props.hardcore.unwrap_or(false);
    let white_list = props.white_list.unwrap_or(false);
    let enable_cmd_block = props.enable_command_block.unwrap_or(true);
    let view_dist = props.view_distance.unwrap_or(10);
    let sim_dist = props.simulation_distance.unwrap_or(8);
    let level_name = props.level_name.as_deref().unwrap_or("world");
    let level_seed = props.level_seed.as_deref().unwrap_or("");
    let allow_flight = props.allow_flight.unwrap_or(false);
    let spawn_prot = props.spawn_protection.unwrap_or(16);

    format!(
"# Minecraft server properties generated by Noos NAS Edition
server-port={port}
query.port={port}
rcon.port={rcon_port}
enable-rcon=false
motd={motd}
gamemode={gamemode}
difficulty={difficulty}
max-players={max_players}
online-mode={online_mode}
pvp={pvp}
hardcore={hardcore}
white-list={white_list}
enable-command-block={enable_cmd_block}
view-distance={view_dist}
simulation-distance={sim_dist}
level-name={level_name}
level-seed={level_seed}
allow-flight={allow_flight}
spawn-protection={spawn_prot}
server-ip=0.0.0.0
network-compression-threshold=256
enable-status=true
sync-chunk-writes=true
enforce-whitelist=false
",
        port = port,
        rcon_port = port + 10,
        motd = motd,
        gamemode = gamemode,
        difficulty = difficulty,
        max_players = max_players,
        online_mode = online_mode,
        pvp = pvp,
        hardcore = hardcore,
        white_list = white_list,
        enable_cmd_block = enable_cmd_block,
        view_dist = view_dist,
        sim_dist = sim_dist,
        level_name = level_name,
        level_seed = level_seed,
        allow_flight = allow_flight,
        spawn_prot = spawn_prot,
    )
}
