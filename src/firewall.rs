use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallOverview {
    pub is_enabled: bool,
    pub status_text: String,
    pub rules: Vec<UnifiedPortRule>,
    pub tcp_ports: Vec<PortRule>,
    pub udp_ports: Vec<PortRule>,
    pub banned_ips: Vec<String>,
    pub total_open_ports: usize,
    pub custom_rules_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRule {
    pub port: u16,
    pub protocol: String,
    pub service_name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPortRule {
    pub id: String,
    pub port: u16,
    pub protocol: String, // "TCP", "UDP", "BOTH"
    pub label: String,
    pub category: String, // "Jeux", "Web", "Multimédia", "Partage", "VPN", "Autre"
    pub enabled: bool,
    #[serde(default = "default_created_at")]
    pub created_at: String,
}

fn default_created_at() -> String {
    "Aujourd'hui".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedPortRule {
    pub id: String,
    pub port: u16,
    pub protocol: String,
    pub label: String,
    pub category: String,
    pub is_system: bool,
    pub enabled: bool,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePortRuleRequest {
    pub port: u16,
    pub protocol: String,
    pub label: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePortRuleRequest {
    pub port: Option<u16>,
    pub protocol: Option<String>,
    pub label: Option<String>,
    pub category: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToggleFirewallRequest {
    pub enable: bool,
}

pub fn get_firewall_rules_file_path() -> PathBuf {
    let candidates = [
        PathBuf::from("/etc/nixos/firewall-rules.json"),
        PathBuf::from("/etc/nixos/steveos-nas/firewall-rules.json"),
        PathBuf::from("/home/chomiam/Projects/steveos-nas/firewall-rules.json"),
        PathBuf::from("./firewall-rules.json"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    if Path::new("/etc/nixos").exists() {
        PathBuf::from("/etc/nixos/firewall-rules.json")
    } else {
        PathBuf::from("/home/chomiam/Projects/steveos-nas/firewall-rules.json")
    }
}

pub fn load_custom_rules() -> Vec<CustomPortRule> {
    let path = get_firewall_rules_file_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(rules) = serde_json::from_str::<Vec<CustomPortRule>>(&content) {
            return rules;
        }
    }
    Vec::new()
}

pub fn save_custom_rules(rules: &[CustomPortRule]) -> Result<(), String> {
    let path = get_firewall_rules_file_path();
    let json = serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Impossible d'écrire dans {} : {}", path.display(), e))?;
    Ok(())
}

fn apply_runtime_port(port: u16, protocol: &str, open: bool) {
    let action = if open { "-I" } else { "-D" };
    let protos: Vec<&str> = match protocol.to_uppercase().as_str() {
        "BOTH" => vec!["tcp", "udp"],
        "UDP" => vec!["udp"],
        _ => vec!["tcp"],
    };

    for proto in protos {
        let p_str = port.to_string();
        let _ = Command::new("iptables")
            .args([action, "nixos-fw", "1", "-p", proto, "--dport", &p_str, "-j", "nixos-fw-accept"])
            .output();
        let _ = Command::new("iptables")
            .args([action, "INPUT", "1", "-p", proto, "--dport", &p_str, "-j", "ACCEPT"])
            .output();
    }
}

pub fn create_custom_rule(req: CreatePortRuleRequest) -> Result<CustomPortRule, String> {
    if req.port == 0 {
        return Err("Le numéro de port doit être compris entre 1 et 65535".into());
    }
    let label = req.label.trim();
    if label.is_empty() {
        return Err("Le libellé de la règle ne peut pas être vide".into());
    }

    let protocol = match req.protocol.trim().to_uppercase().as_str() {
        "UDP" => "UDP".to_string(),
        "BOTH" => "BOTH".to_string(),
        _ => "TCP".to_string(),
    };

    let category = req.category.unwrap_or_else(|| "Autre".to_string());
    let mut rules = load_custom_rules();

    // Vérifier doublon
    if rules.iter().any(|r| r.port == req.port && (r.protocol == protocol || r.protocol == "BOTH" || protocol == "BOTH")) {
        return Err(format!("Une règle pour le port {} ({}) existe déjà", req.port, protocol));
    }

    let id = format!("rule-{}", chrono_simple_id());
    let new_rule = CustomPortRule {
        id: id.clone(),
        port: req.port,
        protocol: protocol.clone(),
        label: label.to_string(),
        category,
        enabled: true,
        created_at: "Maintenant".to_string(),
    };

    rules.push(new_rule.clone());
    save_custom_rules(&rules)?;

    // Application immédiate en mémoire
    apply_runtime_port(req.port, &protocol, true);

    Ok(new_rule)
}

pub fn update_custom_rule(id: &str, req: UpdatePortRuleRequest) -> Result<CustomPortRule, String> {
    let mut rules = load_custom_rules();
    let idx = rules.iter().position(|r| r.id == id).ok_or_else(|| "Règle introuvable".to_string())?;

    let old_rule = rules[idx].clone();

    if let Some(p) = req.port {
        if p == 0 {
            return Err("Port invalide".into());
        }
        rules[idx].port = p;
    }
    if let Some(proto) = req.protocol {
        let clean = match proto.trim().to_uppercase().as_str() {
            "UDP" => "UDP".to_string(),
            "BOTH" => "BOTH".to_string(),
            _ => "TCP".to_string(),
        };
        rules[idx].protocol = clean;
    }
    if let Some(lbl) = req.label {
        let trimmed = lbl.trim();
        if !trimmed.is_empty() {
            rules[idx].label = trimmed.to_string();
        }
    }
    if let Some(cat) = req.category {
        rules[idx].category = cat;
    }
    if let Some(en) = req.enabled {
        rules[idx].enabled = en;
    }

    let updated = rules[idx].clone();
    save_custom_rules(&rules)?;

    // Si le port ou le protocole ou l'état a changé, ajuster iptables
    if old_rule.port != updated.port || old_rule.protocol != updated.protocol || old_rule.enabled != updated.enabled {
        apply_runtime_port(old_rule.port, &old_rule.protocol, false);
        if updated.enabled {
            apply_runtime_port(updated.port, &updated.protocol, true);
        }
    }

    Ok(updated)
}

pub fn delete_custom_rule(id: &str) -> Result<String, String> {
    let mut rules = load_custom_rules();
    let idx = rules.iter().position(|r| r.id == id).ok_or_else(|| "Règle introuvable".to_string())?;

    let removed = rules.remove(idx);
    save_custom_rules(&rules)?;

    apply_runtime_port(removed.port, &removed.protocol, false);

    Ok(format!("Règle pour le port {} ({}) supprimée avec succès", removed.port, removed.label))
}

fn chrono_simple_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn toggle_firewall(enable: bool) -> Result<String, String> {
    let action = if enable { "start" } else { "stop" };
    let output = Command::new("systemctl")
        .args([action, "firewall"])
        .output()
        .map_err(|e| format!("Erreur d'exécution de systemctl : {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        eprintln!("Avertissement bascule pare-feu : {}", err);
    }

    let msg = if enable {
        "Pare-feu NixOS activé avec succès (Protection maximale en vigueur)".to_string()
    } else {
        "Pare-feu NixOS désactivé (Tous les flux réseau sont temporairement autorisés)".to_string()
    };
    Ok(msg)
}

pub fn get_firewall_overview() -> FirewallOverview {
    // Vérifier si le service firewall NixOS est actif
    let is_enabled = Command::new("systemctl")
        .args(["is-active", "firewall"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(true);

    let status_text = if is_enabled {
        "Protection Active (Filtrage strict NixOS)".to_string()
    } else {
        "Protection Désactivée (Tous flux autorisés)".to_string()
    };

    let system_tcp_rules = vec![
        PortRule { port: 22, protocol: "TCP".into(), service_name: "SSH / sFTP (Administration & Transferts)".into(), status: "Autorisé".into() },
        PortRule { port: 139, protocol: "TCP".into(), service_name: "Samba NetBIOS-SSN".into(), status: "Autorisé".into() },
        PortRule { port: 445, protocol: "TCP".into(), service_name: "Samba SMB (Partage de fichiers Windows/Mac)".into(), status: "Autorisé".into() },
        PortRule { port: 2049, protocol: "TCP".into(), service_name: "NFS (Partages Réseau Linux)".into(), status: "Autorisé".into() },
        PortRule { port: 3552, protocol: "TCP".into(), service_name: "Arcane Management".into(), status: "Autorisé".into() },
        PortRule { port: 5357, protocol: "TCP".into(), service_name: "WSDD (Découverte Web Services Windows)".into(), status: "Autorisé".into() },
        PortRule { port: 8096, protocol: "TCP".into(), service_name: "Jellyfin HTTP (Streaming Multimédia)".into(), status: "Autorisé".into() },
        PortRule { port: 8920, protocol: "TCP".into(), service_name: "Jellyfin HTTPS".into(), status: "Autorisé".into() },
        PortRule { port: 9339, protocol: "TCP".into(), service_name: "STEvE_OS NAS Dashboard (Ce Tableau de bord)".into(), status: "Autorisé".into() },
    ];

    let system_udp_rules = vec![
        PortRule { port: 137, protocol: "UDP".into(), service_name: "Samba NetBIOS-NS".into(), status: "Autorisé".into() },
        PortRule { port: 138, protocol: "UDP".into(), service_name: "Samba NetBIOS-DGM".into(), status: "Autorisé".into() },
        PortRule { port: 1900, protocol: "UDP".into(), service_name: "Jellyfin DLNA Discovery".into(), status: "Autorisé".into() },
        PortRule { port: 3702, protocol: "UDP".into(), service_name: "WS-Discovery Multicast (Windows)".into(), status: "Autorisé".into() },
        PortRule { port: 7359, protocol: "UDP".into(), service_name: "Jellyfin Client Autodiscovery".into(), status: "Autorisé".into() },
        PortRule { port: 51820, protocol: "UDP".into(), service_name: "WireGuard VPN (Réseau privé)".into(), status: "Autorisé".into() },
    ];

    let custom_rules = load_custom_rules();

    let mut unified_rules = Vec::new();

    // 1. Ajouter les règles système
    for r in &system_tcp_rules {
        unified_rules.push(UnifiedPortRule {
            id: format!("sys-tcp-{}", r.port),
            port: r.port,
            protocol: "TCP".into(),
            label: r.service_name.clone(),
            category: "Système".into(),
            is_system: true,
            enabled: is_enabled,
            status: "Autorisé".into(),
        });
    }

    for r in &system_udp_rules {
        unified_rules.push(UnifiedPortRule {
            id: format!("sys-udp-{}", r.port),
            port: r.port,
            protocol: "UDP".into(),
            label: r.service_name.clone(),
            category: "Système".into(),
            is_system: true,
            enabled: is_enabled,
            status: "Autorisé".into(),
        });
    }

    // 2. Ajouter les règles personnalisées
    for c in &custom_rules {
        unified_rules.push(UnifiedPortRule {
            id: c.id.clone(),
            port: c.port,
            protocol: c.protocol.clone(),
            label: c.label.clone(),
            category: c.category.clone(),
            is_system: false,
            enabled: c.enabled && is_enabled,
            status: if c.enabled && is_enabled { "Autorisé".into() } else if !c.enabled { "Désactivé".into() } else { "Pare-feu inactif".into() },
        });
    }

    // Récupérer les IPs bannies par Fail2ban si actif
    let mut banned_ips = Vec::new();
    if let Ok(output) = Command::new("fail2ban-client").args(["status", "sshd"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("Banned IP list:") {
                if let Some((_, list)) = line.split_once(':') {
                    for ip in list.split_whitespace() {
                        banned_ips.push(ip.to_string());
                    }
                }
            }
        }
    }

    let total_open_ports = unified_rules.iter().filter(|r| r.enabled).count();
    let custom_rules_count = custom_rules.len();

    FirewallOverview {
        is_enabled,
        status_text,
        rules: unified_rules,
        tcp_ports: system_tcp_rules,
        udp_ports: system_udp_rules,
        banned_ips,
        total_open_ports,
        custom_rules_count,
    }
}

pub fn unban_ip(ip: &str) -> Result<String, String> {
    let clean_ip = ip.trim();
    if !clean_ip.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ':') {
        return Err("Adresse IP invalide".into());
    }
    let output = Command::new("fail2ban-client")
        .args(["set", "sshd", "unbanip", clean_ip])
        .output()
        .map_err(|e| format!("Erreur fail2ban-client : {}", e))?;
    if output.status.success() {
        Ok(format!("Adresse IP {} débannie avec succès !", clean_ip))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
