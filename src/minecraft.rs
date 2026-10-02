use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
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

// ============================================================================
// INTÉGRATION MODRINTH & GESTIONNAIRE D'ADDONS (MODS & PLUGINS MINECRAFT)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthSearchHit {
    pub project_id: String,
    pub project_type: String, // "mod", "plugin", "datapack"
    pub slug: String,
    pub author: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub display_categories: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub date_created: Option<String>,
    #[serde(default)]
    pub date_modified: Option<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthSearchResponse {
    pub hits: Vec<ModrinthSearchHit>,
    pub offset: usize,
    pub limit: usize,
    pub total_hits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthVersionItem {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub version_type: String, // "release", "beta", "alpha"
    pub files: Vec<ModrinthFile>,
    #[serde(default)]
    pub date_published: Option<String>,
    #[serde(default)]
    pub downloads: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthInstallRequest {
    pub project_id: String,
    pub project_title: Option<String>,
    pub version_id: Option<String>,
    pub file_url: String,
    pub filename: String,
    pub target_dir_type: String, // "mods" ou "plugins"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAddonItem {
    pub filename: String,
    pub display_name: String,
    pub addon_type: String, // "mods" ou "plugins"
    pub size_bytes: u64,
    pub size_human: String,
    pub is_enabled: bool,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToggleAddonRequest {
    pub filename: String,
    pub addon_type: String, // "mods" ou "plugins"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftServerProfile {
    pub server_id: String,
    pub server_name: String,
    pub is_minecraft: bool,
    pub detected_loader: String,
    pub detected_version: String,
    pub preferred_type: String, // "plugin" ou "mod"
    pub supports_plugins: bool,
    pub supports_mods: bool,
    pub data_dir: String,
    pub installed_mods_count: usize,
    pub installed_plugins_count: usize,
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} Go", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} Mo", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} Ko", bytes as f64 / KB as f64)
    } else {
        format!("{} o", bytes)
    }
}

/// Recherche des mods, plugins ou datapacks via l'API v2 publique de Modrinth
pub fn search_modrinth(
    query: &str,
    project_type: Option<&str>,
    loader: Option<&str>,
    version: Option<&str>,
    sort: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<ModrinthSearchResponse, String> {
    let clean_query = query.trim();
    let limit = if limit == 0 { 20 } else { limit.min(60) };

    // Construction des facettes Modrinth JSON
    // Exemple : [["project_type:mod"],["categories:fabric"],["versions:1.21.1"]]
    let mut facet_groups: Vec<String> = Vec::new();

    if let Some(pt) = project_type {
        let clean_pt = pt.trim().to_lowercase();
        if !clean_pt.is_empty() && clean_pt != "all" {
            facet_groups.push(format!("[\"project_type:{}\"]", clean_pt));
        }
    }

    if let Some(ld) = loader {
        let clean_ld = ld.trim().to_lowercase();
        if !clean_ld.is_empty() && clean_ld != "all" {
            facet_groups.push(format!("[\"categories:{}\"]", clean_ld));
        }
    }

    if let Some(v) = version {
        let clean_v = v.trim();
        if !clean_v.is_empty() && clean_v != "all" {
            facet_groups.push(format!("[\"versions:{}\"]", clean_v));
        }
    }

    let facets_param = if !facet_groups.is_empty() {
        format!("[{}]", facet_groups.join(","))
    } else {
        String::new()
    };

    let index_sort = match sort.unwrap_or("") {
        "newest" => "newest",
        "updated" => "updated",
        "follows" => "follows",
        "downloads" => "downloads",
        _ => if clean_query.is_empty() { "downloads" } else { "relevance" },
    };

    let mut url = format!(
        "https://api.modrinth.com/v2/search?index={}&offset={}&limit={}",
        index_sort, offset, limit
    );

    if !clean_query.is_empty() {
        url.push_str(&format!("&query={}", url_encode(clean_query)));
    }
    if !facets_param.is_empty() {
        url.push_str(&format!("&facets={}", url_encode(&facets_param)));
    }

    let output = Command::new("curl")
        .args([
            "-s", "-L",
            "-A", "NoosNAS/0.3.10 (github.com/Chomiam/noos-nas-dashboard)",
            "--connect-timeout", "6",
            "--max-time", "12",
            &url,
        ])
        .output()
        .map_err(|e| format!("Impossible de joindre Modrinth : {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Erreur API Modrinth : {}", err_str));
    }

    serde_json::from_slice::<ModrinthSearchResponse>(&output.stdout)
        .map_err(|e| format!("Réponse Modrinth invalide : {}", e))
}

/// Récupère la liste des versions et fichiers d'un projet Modrinth
pub fn get_modrinth_project_versions(
    project_id_or_slug: &str,
    loader: Option<&str>,
    game_version: Option<&str>,
) -> Result<Vec<ModrinthVersionItem>, String> {
    let clean_id = project_id_or_slug.trim();
    if clean_id.is_empty() {
        return Err("Identifiant de projet manquant".into());
    }

    let mut url = format!("https://api.modrinth.com/v2/project/{}/version", clean_id);
    let mut params = Vec::new();

    if let Some(l) = loader {
        let clean_l = l.trim().to_lowercase();
        if !clean_l.is_empty() && clean_l != "all" {
            params.push(format!("loaders={}", url_encode(&format!("[\"{}\"]", clean_l))));
        }
    }
    if let Some(v) = game_version {
        let clean_v = v.trim();
        if !clean_v.is_empty() && clean_v != "all" {
            params.push(format!("game_versions={}", url_encode(&format!("[\"{}\"]", clean_v))));
        }
    }

    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let output = Command::new("curl")
        .args([
            "-s", "-L",
            "-A", "NoosNAS/0.3.10 (github.com/Chomiam/noos-nas-dashboard)",
            "--connect-timeout", "6",
            "--max-time", "12",
            &url,
        ])
        .output()
        .map_err(|e| format!("Impossible de contacter Modrinth : {}", e))?;

    if !output.status.success() {
        return Err("Échec de récupération des versions Modrinth".into());
    }

    serde_json::from_slice::<Vec<ModrinthVersionItem>>(&output.stdout)
        .map_err(|e| format!("Format de versions Modrinth invalide : {}", e))
}

/// Détermine de manière stricte si un serveur est un serveur Minecraft (Java ou Bedrock)
pub fn is_minecraft_server(server: &crate::games::GameServer) -> bool {
    let egg_id = server.egg_id.to_lowercase();
    let game_name = server.game_name.to_lowercase();
    let name = server.name.to_lowercase();

    if egg_id.contains("minecraft") || game_name.contains("minecraft") || name.contains("minecraft") {
        return true;
    }
    if server.env.contains_key("MINECRAFT_VERSION") || server.env.contains_key("MINECRAFT_LOADER") {
        return true;
    }
    let data_path = PathBuf::from(&server.data_dir);
    if data_path.join("server.properties").exists()
        || data_path.join("paper.yml").exists()
        || data_path.join("paper.toml").exists()
        || data_path.join("purpur.yml").exists()
        || data_path.join("fabric-server-launch.jar").exists()
        || data_path.join("eula.txt").exists()
    {
        return true;
    }
    false
}

/// Détecte le profil Minecraft d'un serveur (loader, version, mods/plugins)
pub fn detect_minecraft_profile(server_id: &str) -> Result<MinecraftServerProfile, String> {
    let servers = crate::games::load_saved_servers();
    let server = servers
        .into_iter()
        .find(|s| s.id == server_id)
        .ok_or_else(|| format!("Serveur de jeu '{}' introuvable", server_id))?;

    if !is_minecraft_server(&server) {
        return Ok(MinecraftServerProfile {
            server_id: server.id,
            server_name: server.name,
            is_minecraft: false,
            detected_loader: String::new(),
            detected_version: String::new(),
            preferred_type: String::new(),
            supports_plugins: false,
            supports_mods: false,
            data_dir: server.data_dir,
            installed_mods_count: 0,
            installed_plugins_count: 0,
        });
    }

    let data_path = PathBuf::from(&server.data_dir);

    // Détection de loader & version
    let mut detected_loader = server
        .env
        .get("MINECRAFT_LOADER")
        .cloned()
        .unwrap_or_default();
    let detected_version = server
        .env
        .get("MINECRAFT_VERSION")
        .cloned()
        .unwrap_or_else(|| "1.21.1".to_string());

    if detected_loader.is_empty() {
        if data_path.join("paper.yml").exists() || data_path.join("paper.toml").exists() {
            detected_loader = "paper".to_string();
        } else if data_path.join("purpur.yml").exists() {
            detected_loader = "purpur".to_string();
        } else if data_path.join("fabric-server-launch.jar").exists() || data_path.join(".fabric").exists() {
            detected_loader = "fabric".to_string();
        } else if data_path.join("mods").exists() {
            detected_loader = "fabric".to_string();
        } else {
            detected_loader = "paper".to_string();
        }
    }

    let supports_plugins = matches!(detected_loader.as_str(), "paper" | "purpur" | "spigot" | "bukkit");
    let supports_mods = matches!(detected_loader.as_str(), "fabric" | "neoforge" | "forge" | "quilt");
    let preferred_type = if supports_mods { "mod".to_string() } else { "plugin".to_string() };

    let installed_mods_count = if data_path.join("mods").is_dir() {
        fs::read_dir(data_path.join("mods"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.ends_with(".jar") || name.ends_with(".jar.disabled")
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let installed_plugins_count = if data_path.join("plugins").is_dir() {
        fs::read_dir(data_path.join("plugins"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.ends_with(".jar") || name.ends_with(".jar.disabled")
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    Ok(MinecraftServerProfile {
        server_id: server.id,
        server_name: server.name,
        is_minecraft: true,
        detected_loader,
        detected_version,
        preferred_type,
        supports_plugins,
        supports_mods,
        data_dir: server.data_dir,
        installed_mods_count,
        installed_plugins_count,
    })
}

/// Télécharge et installe un addon Modrinth (.jar) directement sur le serveur
pub fn install_modrinth_addon(
    server_id: &str,
    req: &ModrinthInstallRequest,
) -> Result<String, String> {
    let servers = crate::games::load_saved_servers();
    let server = servers
        .into_iter()
        .find(|s| s.id == server_id)
        .ok_or_else(|| format!("Serveur de jeu '{}' introuvable", server_id))?;

    if !is_minecraft_server(&server) {
        return Err("Ce serveur n'est pas un serveur Minecraft (Modrinth est réservé aux serveurs Minecraft).".into());
    }

    // 1. Validation stricte du nom de fichier
    let safe_filename = req.filename.trim();
    if safe_filename.is_empty()
        || !safe_filename.ends_with(".jar")
        || safe_filename.contains('/')
        || safe_filename.contains('\\')
        || safe_filename.contains("..")
    {
        return Err("Nom de fichier d'addon invalide (doit être un fichier .jar sans chemin relatif)".into());
    }

    // 2. Validation stricte de l'URL Modrinth
    let file_url = req.file_url.trim();
    if !file_url.starts_with("https://cdn.modrinth.com/") && !file_url.starts_with("https://api.modrinth.com/") {
        return Err("L'URL du fichier doit impérativement provenir du CDN officiel de Modrinth (https://cdn.modrinth.com/)".into());
    }

    // 3. Détermination du sous-dossier cible ("mods" ou "plugins")
    let target_folder = match req.target_dir_type.to_lowercase().as_str() {
        "mods" => "mods",
        "plugins" => "plugins",
        _ => "mods",
    };

    let server_data_dir = PathBuf::from(&server.data_dir);
    if !server_data_dir.is_dir() {
        return Err(format!("Le dossier du serveur n'existe pas : {}", server.data_dir));
    }

    let dest_dir = server_data_dir.join(target_folder);
    if let Err(e) = fs::create_dir_all(&dest_dir) {
        return Err(format!("Impossible de préparer le dossier {}: {}", target_folder, e));
    }

    let target_file_path = dest_dir.join(safe_filename);

    // 4. Téléchargement via curl
    let output = Command::new("curl")
        .args([
            "-s", "-L", "--fail",
            "-A", "NoosNAS/0.3.10 (github.com/Chomiam/noos-nas-dashboard)",
            "--connect-timeout", "10",
            "--max-time", "120",
            "-o", target_file_path.to_str().unwrap(),
            "--", file_url,
        ])
        .output()
        .map_err(|e| format!("Échec d'exécution du téléchargement : {}", e))?;

    if !output.status.success() {
        let _ = fs::remove_file(&target_file_path);
        return Err("Le téléchargement de l'addon depuis Modrinth a échoué.".into());
    }

    // 5. Vérifier la taille du fichier
    if let Ok(meta) = fs::metadata(&target_file_path) {
        if meta.len() < 100 {
            let _ = fs::remove_file(&target_file_path);
            return Err("Le fichier téléchargé est corrompu ou vide.".into());
        }
    }

    // 6. Permissions saines
    let target_user = crate::updates::target_user();
    let _ = Command::new("chown")
        .args([&format!("{}:users", target_user), target_file_path.to_str().unwrap()])
        .status();
    let _ = Command::new("chmod")
        .args(["0664", target_file_path.to_str().unwrap()])
        .status();

    let title = req.project_title.as_deref().unwrap_or(safe_filename);
    Ok(format!(
        "L'addon '{}' a été téléchargé et installé avec succès dans '{}/{}' !",
        title, target_folder, safe_filename
    ))
}

/// Liste tous les mods et plugins installés dans le serveur Minecraft
pub fn list_installed_addons(server_id: &str) -> Result<Vec<InstalledAddonItem>, String> {
    let servers = crate::games::load_saved_servers();
    let server = servers
        .into_iter()
        .find(|s| s.id == server_id)
        .ok_or_else(|| format!("Serveur de jeu '{}' introuvable", server_id))?;

    if !is_minecraft_server(&server) {
        return Ok(Vec::new());
    }

    let data_path = PathBuf::from(&server.data_dir);
    let mut items = Vec::new();

    for folder_name in &["mods", "plugins"] {
        let dir = data_path.join(folder_name);
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_jar = name.ends_with(".jar");
                let is_disabled = name.ends_with(".jar.disabled");

                if !is_jar && !is_disabled {
                    continue;
                }

                let is_enabled = is_jar;
                let display_name = if is_disabled {
                    name.trim_end_matches(".disabled").trim_end_matches(".jar").to_string()
                } else {
                    name.trim_end_matches(".jar").to_string()
                };

                let meta = entry.metadata().ok();
                let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let size_human = format_file_size(size_bytes);

                let modified_at = meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        let secs = d.as_secs();
                        format!("{}", secs)
                    });

                items.push(InstalledAddonItem {
                    filename: name,
                    display_name,
                    addon_type: folder_name.to_string(),
                    size_bytes,
                    size_human,
                    is_enabled,
                    modified_at,
                });
            }
        }
    }

    items.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    Ok(items)
}

/// Active ou désactive un addon installé (.jar <-> .jar.disabled)
pub fn toggle_installed_addon(server_id: &str, req: &ToggleAddonRequest) -> Result<bool, String> {
    let servers = crate::games::load_saved_servers();
    let server = servers
        .into_iter()
        .find(|s| s.id == server_id)
        .ok_or_else(|| format!("Serveur de jeu '{}' introuvable", server_id))?;

    let safe_folder = match req.addon_type.as_str() {
        "mods" => "mods",
        "plugins" => "plugins",
        _ => return Err("Type d'addon invalide".into()),
    };

    let filename = req.filename.trim();
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("Nom de fichier invalide".into());
    }

    let folder_path = PathBuf::from(&server.data_dir).join(safe_folder);
    let src_path = folder_path.join(filename);

    if !src_path.exists() {
        return Err(format!("Le fichier '{}' n'existe pas.", filename));
    }

    let (new_path, now_enabled) = if filename.ends_with(".jar.disabled") {
        let new_name = filename.trim_end_matches(".disabled");
        (folder_path.join(new_name), true)
    } else if filename.ends_with(".jar") {
        let new_name = format!("{}.disabled", filename);
        (folder_path.join(new_name), false)
    } else {
        return Err("Extension de fichier non supportée".into());
    };

    fs::rename(&src_path, &new_path)
        .map_err(|e| format!("Impossible de renommer le fichier d'addon : {}", e))?;

    Ok(now_enabled)
}

/// Supprime définitivement un addon (.jar) du serveur
pub fn delete_installed_addon(server_id: &str, addon_type: &str, filename: &str) -> Result<(), String> {
    let servers = crate::games::load_saved_servers();
    let server = servers
        .into_iter()
        .find(|s| s.id == server_id)
        .ok_or_else(|| format!("Serveur de jeu '{}' introuvable", server_id))?;

    let safe_folder = match addon_type {
        "mods" => "mods",
        "plugins" => "plugins",
        _ => return Err("Type d'addon invalide".into()),
    };

    let clean_filename = filename.trim();
    if clean_filename.contains('/') || clean_filename.contains('\\') || clean_filename.contains("..") {
        return Err("Nom de fichier invalide".into());
    }

    let target_path = PathBuf::from(&server.data_dir).join(safe_folder).join(clean_filename);
    if !target_path.exists() {
        return Err("Addon introuvable".into());
    }

    fs::remove_file(&target_path)
        .map_err(|e| format!("Impossible de supprimer l'addon : {}", e))?;

    Ok(())
}

