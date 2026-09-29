use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireguardClient {
    pub id: String,
    pub username: String,
    pub client_ip: String,
    pub private_key: String,
    pub public_key: String,
    pub allowed_ips: String,
    pub dns: String,
    pub endpoint: String,
    pub created_at: u64,
    pub config_text: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub username: String,
    pub client_ip: Option<String>,
    pub allowed_ips: Option<String>,
    pub dns: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireguardServerInfo {
    pub is_active: bool,
    pub interface: String,
    pub port: u16,
    pub subnet: String,
    pub host_ip: String,
    pub public_key: String,
    pub endpoint: String,
    pub total_clients: usize,
    pub next_client_ip: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn get_clients_file_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("STEVEOS_WG_CLIENTS_FILE") {
        return PathBuf::from(env_path);
    }
    let var_lib = Path::new("/var/lib/steveos");
    if var_lib.exists() || fs::create_dir_all(var_lib).is_ok() {
        return var_lib.join("wireguard_clients.json");
    }
    PathBuf::from("/run/steveos-wireguard-clients.json")
}

pub fn load_wireguard_clients() -> Vec<WireguardClient> {
    let path = get_clients_file_path();
    if !path.exists() {
        return Vec::new();
    }
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(clients) = serde_json::from_str::<Vec<WireguardClient>>(&content) {
            return clients;
        }
    }
    Vec::new()
}

pub fn save_wireguard_clients(clients: &[WireguardClient]) -> Result<(), String> {
    let path = get_clients_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let json_bytes = serde_json::to_vec_pretty(clients)
        .map_err(|e| format!("Erreur sérialisation JSON : {}", e))?;

    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, json_bytes)
        .map_err(|e| format!("Erreur écriture fichier temporaire : {}", e))?;

    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Erreur renommage fichier : {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

fn get_primary_lan_ip() -> String {
    if let Ok(output) = Command::new("ip").args(["-4", "route", "get", "1.1.1.1"]).output() {
        let text = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = text.split_whitespace().collect();
        for i in 0..parts.len() {
            if parts[i] == "src" && i + 1 < parts.len() {
                let ip = parts[i + 1];
                if !ip.starts_with("127.") && !ip.starts_with("172.17.") && !ip.starts_with("169.254.") {
                    return ip.to_string();
                }
            }
        }
    }

    if let Ok(output) = Command::new("hostname").arg("-I").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for ip in text.split_whitespace() {
            if !ip.starts_with("127.") && !ip.starts_with("172.17.") && !ip.starts_with("169.254.") {
                return ip.to_string();
            }
        }
    }

    "127.0.0.1".to_string()
}

pub fn get_server_public_key() -> String {
    // 1. Tenter via `wg show wg0 public-key`
    if let Ok(output) = Command::new("wg").args(["show", "wg0", "public-key"]).output() {
        if output.status.success() {
            let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !key.is_empty() {
                return key;
            }
        }
    }

    // 2. Tenter de lire depuis un fichier de clé serveur persistant
    let key_path = Path::new("/var/lib/steveos/server_public.key");
    if let Ok(k) = fs::read_to_string(key_path) {
        let trimmed = k.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    // 3. Tenter d'extraire la clé publique depuis une clé privée serveur existante
    let priv_path = Path::new("/var/lib/steveos/server_private.key");
    if let Ok(priv_k) = fs::read_to_string(priv_path) {
        if let Ok(mut child) = Command::new("wg").arg("pubkey").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(priv_k.trim().as_bytes());
            }
            if let Ok(out) = child.wait_with_output() {
                let pub_k = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !pub_k.is_empty() {
                    let _ = fs::write(key_path, &pub_k);
                    return pub_k;
                }
            }
        }
    }

    // 4. Générer automatiquement une paire de clés serveur persistante si nécessaire
    if let Ok(output) = Command::new("wg").arg("genkey").output() {
        if output.status.success() {
            let priv_k = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(mut child) = Command::new("wg").arg("pubkey").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(priv_k.as_bytes());
                }
                if let Ok(out) = child.wait_with_output() {
                    let pub_k = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    let _ = fs::create_dir_all("/var/lib/steveos");
                    let _ = fs::write("/var/lib/steveos/server_private.key", &priv_k);
                    let _ = fs::write(key_path, &pub_k);
                    return pub_k;
                }
            }
        }
    }

    "Non_definie_serveur_wireguard".to_string()
}

pub fn sync_clients_to_kernel() {
    let clients = load_wireguard_clients();
    for c in &clients {
        let _ = Command::new("wg")
            .args(["set", "wg0", "peer", &c.public_key, "allowed-ips", &format!("{}/32", c.client_ip)])
            .output();
    }
}

pub fn get_wireguard_server_info() -> WireguardServerInfo {
    let is_active = Command::new("systemctl")
        .args(["is-active", "wireguard-wg0"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    if is_active {
        sync_clients_to_kernel();
    }

    let lan_ip = get_primary_lan_ip();
    let endpoint = format!("{}:51820", lan_ip);
    let public_key = get_server_public_key();
    let clients = load_wireguard_clients();
    let next_client_ip = get_next_available_ip(&clients);

    WireguardServerInfo {
        is_active,
        interface: "wg0".into(),
        port: 51820,
        subnet: "10.100.0.1/24".into(),
        host_ip: "10.100.0.1".into(),
        public_key,
        endpoint,
        total_clients: clients.len(),
        next_client_ip,
    }
}

fn generate_keypair() -> Result<(String, String), String> {
    let gen_out = Command::new("wg")
        .arg("genkey")
        .output()
        .map_err(|e| format!("Impossible d'exécuter wg genkey : {}", e))?;

    if !gen_out.status.success() {
        return Err("Échec de génération de la clé privée avec wg genkey".into());
    }

    let priv_key = String::from_utf8_lossy(&gen_out.stdout).trim().to_string();

    let mut pub_child = Command::new("wg")
        .arg("pubkey")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Impossible d'exécuter wg pubkey : {}", e))?;

    if let Some(mut stdin) = pub_child.stdin.take() {
        stdin.write_all(priv_key.as_bytes())
            .map_err(|e| format!("Erreur écriture stdin wg pubkey : {}", e))?;
    }

    let pub_out = pub_child.wait_with_output()
        .map_err(|e| format!("Erreur attente sortie wg pubkey : {}", e))?;

    let pub_key = String::from_utf8_lossy(&pub_out.stdout).trim().to_string();

    Ok((priv_key, pub_key))
}

pub fn get_next_available_ip(existing_clients: &[WireguardClient]) -> String {
    let used_ips: HashSet<String> = existing_clients
        .iter()
        .map(|c| c.client_ip.clone())
        .collect();

    for i in 2..255 {
        let candidate = format!("10.100.0.{}", i);
        if !used_ips.contains(&candidate) {
            return candidate;
        }
    }

    "10.100.0.2".to_string()
}

pub fn build_client_config_text(
    client_private_key: &str,
    client_ip: &str,
    dns: &str,
    server_public_key: &str,
    server_endpoint: &str,
    allowed_ips: &str,
) -> String {
    format!(
r#"[Interface]
# Profil Client WireGuard - STEvE_OS NAS Edition
# Attention : Cette clé privée est strictement confidentielle
PrivateKey = {}
Address = {}/32
DNS = {}

[Peer]
# Serveur STEvE_OS NAS
# Règle de sécurité : Accès exclusif à la machine hôte ({})
PublicKey = {}
Endpoint = {}
AllowedIPs = {}
PersistentKeepalive = 25
"#,
        client_private_key, client_ip, dns, allowed_ips, server_public_key, server_endpoint, allowed_ips
    )
}

pub fn create_client(req: CreateClientRequest) -> Result<WireguardClient, String> {
    let username = req.username.trim();
    if username.is_empty() {
        return Err("Le nom d'utilisateur ou d'appareil ne peut pas être vide.".into());
    }

    // Filtrer les caractères autorisés pour le nom
    if !username.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err("Le nom ne doit contenir que des lettres, chiffres, tirets et underscores.".into());
    }

    let mut clients = load_wireguard_clients();

    // Vérifier si un client avec ce nom existe déjà
    if clients.iter().any(|c| c.username.eq_ignore_ascii_case(username)) {
        return Err(format!("Un client nommé '{}' existe déjà.", username));
    }

    let client_ip = req.client_ip
        .map(|ip| ip.trim().to_string())
        .filter(|ip| !ip.is_empty())
        .unwrap_or_else(|| get_next_available_ip(&clients));

    let dns = req.dns
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "1.1.1.1, 8.8.8.8".to_string());

    // Sécurité stricte : Par défaut, AllowedIPs restreint UNIQUEMENT l'accès à la machine hôte (10.100.0.1/32)
    // et ne donne AUCUN accès au reste du réseau local ni à Internet via le NAS.
    let allowed_ips = req.allowed_ips
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "10.100.0.1/32".to_string());

    let server_pub_key = get_server_public_key();
    let lan_ip = get_primary_lan_ip();
    let default_endpoint = format!("{}:51820", lan_ip);

    let endpoint = req.endpoint
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .unwrap_or(default_endpoint);

    // Générer la paire de clés
    let (priv_key, pub_key) = generate_keypair()?;

    let config_text = build_client_config_text(
        &priv_key,
        &client_ip,
        &dns,
        &server_pub_key,
        &endpoint,
        &allowed_ips,
    );

    let id = format!("{}_{}", username, now_secs());

    let client = WireguardClient {
        id: id.clone(),
        username: username.to_string(),
        client_ip: client_ip.clone(),
        private_key: priv_key,
        public_key: pub_key.clone(),
        allowed_ips,
        dns,
        endpoint,
        created_at: now_secs(),
        config_text,
    };

    clients.push(client.clone());
    save_wireguard_clients(&clients)?;

    // Enregistrer le pair immédiatement dans l'interface WireGuard du noyau Linux si elle est active
    let _ = Command::new("wg")
        .args(["set", "wg0", "peer", &pub_key, "allowed-ips", &format!("{}/32", client_ip)])
        .output();

    Ok(client)
}

pub fn delete_client(id: &str) -> Result<(), String> {
    let mut clients = load_wireguard_clients();
    let initial_len = clients.len();

    let mut removed_pub_key = None;
    clients.retain(|c| {
        if c.id == id || c.username == id {
            removed_pub_key = Some(c.public_key.clone());
            false
        } else {
            true
        }
    });

    if clients.len() == initial_len {
        return Err(format!("Aucun client trouvé avec l'identifiant '{}'", id));
    }

    save_wireguard_clients(&clients)?;

    // Retirer le pair du noyau Linux
    if let Some(pub_key) = removed_pub_key {
        let _ = Command::new("wg")
            .args(["set", "wg0", "peer", &pub_key, "remove"])
            .output();
    }

    Ok(())
}
