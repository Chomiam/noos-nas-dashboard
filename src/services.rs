use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSummary {
    pub display_name: String,
    pub unit_name: String,
    pub is_active: bool,
    pub sub_state: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesOverview {
    pub samba: ServiceItem,
    pub sshd: ServiceItem,
    pub nfs: ServiceItem,
    pub jellyfin: ServiceItem,
    pub docker: ServiceItem,
    pub fail2ban: ServiceItem,
    pub wireguard: ServiceItem,
    pub services: Vec<ServiceSummary>,
    pub active_sftp_sessions: Vec<ActiveSession>,
    pub active_samba_sessions: Vec<ActiveSession>,
    pub docker_containers: Vec<DockerContainer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceItem {
    pub name: String,
    pub unit: String,
    pub is_active: bool,
    pub status_text: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub user: String,
    pub client_ip: String,
    pub protocol: String,
    pub login_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub is_running: bool,
    #[serde(default)]
    pub ports: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub web_port: Option<u16>,
    #[serde(default)]
    pub store_app_id: Option<String>,
    #[serde(default)]
    pub store_app_name: Option<String>,
    #[serde(default)]
    pub store_icon: Option<String>,
    #[serde(default)]
    pub is_store_app: bool,
}

pub fn get_services_overview() -> ServicesOverview {
    let samba = check_unit("samba-smbd", "Partages Windows / Mac (Samba SMB)");
    let sshd = check_unit("sshd", "Serveur SSH & Partage sFTP Chiffré");
    let nfs = check_unit("nfs-server", "Serveur de Partages NFS Linux");
    let jellyfin = check_unit("jellyfin", "Serveur Multimédia & Transcodage");
    let docker = check_unit("docker", "Moteur de Conteneurs Docker");
    let fail2ban = check_unit("fail2ban", "Système Anti-Bruteforce & Sécurité");
    let wireguard = check_unit("wireguard-wg0", "Réseau Privé WireGuard");

    let lan_ip = crate::network::get_primary_lan_ip();
    let is_firewall_active = crate::firewall::load_firewall_state().unwrap_or(true);
    let is_dns_active = check_unit("systemd-resolved", "").is_active;

    let services = vec![
        ServiceSummary {
            display_name: "SSH / sFTP".into(),
            unit_name: "sshd".into(),
            is_active: sshd.is_active,
            sub_state: if sshd.is_active { "En ligne".into() } else { "Arrêté".into() },
            icon: "⚡".into(),
            address: format!("{}:22", lan_ip),
            port: Some(22),
        },
        ServiceSummary {
            display_name: "Samba (SMB)".into(),
            unit_name: "samba-smbd".into(),
            is_active: samba.is_active,
            sub_state: if samba.is_active { "En ligne".into() } else { "Arrêté".into() },
            icon: "🪟".into(),
            address: format!("\\\\{}:445", lan_ip),
            port: Some(445),
        },
        ServiceSummary {
            display_name: "WireGuard VPN".into(),
            unit_name: "wireguard-wg0".into(),
            is_active: wireguard.is_active,
            sub_state: if wireguard.is_active { "En ligne".into() } else { "Inactif".into() },
            icon: "🔒".into(),
            address: "10.100.0.1:51820".into(),
            port: Some(51820),
        },
        ServiceSummary {
            display_name: "Pare-feu (Firewall)".into(),
            unit_name: "firewall".into(),
            is_active: is_firewall_active,
            sub_state: if is_firewall_active { "En ligne".into() } else { "Désactivé".into() },
            icon: "🛡️".into(),
            address: "Filtrage strict NixOS".into(),
            port: None,
        },
        ServiceSummary {
            display_name: "Résolution DNS".into(),
            unit_name: "systemd-resolved".into(),
            is_active: is_dns_active,
            sub_state: if is_dns_active { "En ligne".into() } else { "Arrêté".into() },
            icon: "🌐".into(),
            address: format!("{}:53", lan_ip),
            port: Some(53),
        },
        ServiceSummary {
            display_name: "Fail2ban IPS".into(),
            unit_name: "fail2ban".into(),
            is_active: fail2ban.is_active,
            sub_state: if fail2ban.is_active { "En ligne".into() } else { "Inactif".into() },
            icon: "🚨".into(),
            address: "Surveillance bruteforce".into(),
            port: None,
        },
        ServiceSummary {
            display_name: "Docker Engine".into(),
            unit_name: "docker".into(),
            is_active: docker.is_active,
            sub_state: if docker.is_active { "En ligne".into() } else { "Arrêté".into() },
            icon: "🐳".into(),
            address: "/var/run/docker.sock".into(),
            port: None,
        },
    ];

    // Sessions sFTP / SSH
    let active_sftp_sessions = get_ssh_sessions();

    // Sessions Samba
    let active_samba_sessions = get_smb_sessions();

    // Conteneurs Docker
    let docker_containers = get_docker_containers();

    ServicesOverview {
        samba,
        sshd,
        nfs,
        jellyfin,
        docker,
        fail2ban,
        wireguard,
        services,
        active_sftp_sessions,
        active_samba_sessions,
        docker_containers,
    }
}

fn check_unit(unit_name: &str, description: &str) -> ServiceItem {
    let is_active = Command::new("systemctl")
        .args(["is-active", unit_name])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    ServiceItem {
        name: unit_name.to_string(),
        unit: unit_name.to_string(),
        is_active,
        status_text: if is_active { "Actif (En cours d'exécution)".into() } else { "Inactif / Arrêté".into() },
        description: description.to_string(),
    }
}

pub fn get_ssh_sessions() -> Vec<ActiveSession> {
    let mut sessions = Vec::new();
    if let Ok(output) = Command::new("who").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let user = parts[0].to_string();
                let client_ip = parts[4].trim_matches('(').trim_matches(')').to_string();
                let login_time = format!("{} {}", parts[2], parts[3]);
                sessions.push(ActiveSession {
                    user,
                    client_ip,
                    protocol: "SSH / sFTP".into(),
                    login_time,
                });
            }
        }
    }
    sessions
}

pub fn get_smb_sessions() -> Vec<ActiveSession> {
    let mut sessions = Vec::new();
    if let Ok(output) = Command::new("smbstatus").arg("-b").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(4) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                sessions.push(ActiveSession {
                    user: parts[1].to_string(),
                    client_ip: parts[3].to_string(),
                    protocol: "Samba (SMB)".into(),
                    login_time: "Session active".into(),
                });
            }
        }
    }
    sessions
}

pub fn get_docker_containers() -> Vec<DockerContainer> {
    let mut containers = Vec::new();
    if let Ok(output) = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.State}}\t{{.Ports}}\t{{.CreatedAt}}\t{{.Labels}}"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let id = parts[0].to_string();
                let name = parts[1].trim_start_matches('/').to_string();
                let image = parts[2].to_string();
                let status = parts[3].to_string();
                let state = parts[4].to_lowercase();
                let ports = if parts.len() > 5 { parts[5].to_string() } else { String::new() };
                let created = if parts.len() > 6 { parts[6].to_string() } else { String::new() };
                let labels = if parts.len() > 7 { parts[7] } else { "" };
                let web_port = extract_web_port(&ports);

                // Détection de lien avec le Store Docker (via label com.docker.compose.project ou nom de conteneur)
                let mut store_project = None;
                for label in labels.split(',') {
                    if let Some((k, v)) = label.split_once('=') {
                        if k.trim() == "com.docker.compose.project" {
                            let proj = v.trim().to_string();
                            if !proj.is_empty() {
                                store_project = Some(proj);
                            }
                        }
                    }
                }

                let clean_name = name.strip_prefix("docker-").unwrap_or(&name);
                let lookup_key = store_project.as_deref().unwrap_or(clean_name);
                let store_info = crate::docker_store::resolve_store_app_info(lookup_key);

                let (store_app_id, store_app_name, store_icon, is_store_app) = match store_info {
                    Some((app_id, app_name, icon)) => (Some(app_id), Some(app_name), Some(icon), true),
                    None => (None, None, None, false),
                };

                containers.push(DockerContainer {
                    id,
                    name,
                    image,
                    status,
                    is_running: state == "running",
                    ports,
                    created,
                    web_port,
                    store_app_id,
                    store_app_name,
                    store_icon,
                    is_store_app,
                });
            }
        }
    }
    containers
}

fn extract_web_port(ports: &str) -> Option<u16> {
    let mut candidates = Vec::new();
    for part in ports.split(',') {
        if let Some(arrow_idx) = part.find("->") {
            let host_part = part[..arrow_idx].trim();
            if let Some(colon_idx) = host_part.rfind(':') {
                if let Ok(p) = host_part[colon_idx + 1..].parse::<u16>() {
                    if !candidates.contains(&p) {
                        candidates.push(p);
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Ports non-web ou bloqués par les navigateurs (DNS, DHCP, SSH, Mail, DBs...)
    let non_web_ports = [21, 22, 25, 53, 67, 68, 853, 110, 143, 389, 445, 465, 587, 993, 995, 3306, 5432, 6379, 27017];

    // 1. Ports Web standards et très fréquents par ordre de préférence
    let preferred_web_ports = [3000, 80, 8080, 443, 8443, 9000, 8096, 5000, 8000, 3001, 8123, 8081, 9443, 8888];
    for &pref in &preferred_web_ports {
        if candidates.contains(&pref) {
            return Some(pref);
        }
    }

    // 2. N'importe quel port candidat qui n'est pas dans la liste des ports non-web
    for &cand in &candidates {
        if !non_web_ports.contains(&cand) {
            return Some(cand);
        }
    }

    // 3. Repli sur le premier candidat
    candidates.first().copied()
}

pub fn control_service(unit: &str, action: &str) -> Result<String, String> {
    let allowed_actions = ["start", "stop", "restart", "reload"];
    if !allowed_actions.contains(&action) {
        return Err(format!("Action non autorisée : {}", action));
    }

    let status = Command::new("systemctl")
        .args([action, unit])
        .status()
        .map_err(|e| format!("Échec d'exécution de systemctl : {}", e))?;

    if status.success() {
        Ok(format!("Action '{}' appliquée avec succès sur {}", action, unit))
    } else {
        Err(format!("Échec de l'action '{}' sur {}", action, unit))
    }
}

pub fn get_service_logs(unit: &str, lines: usize) -> Result<String, String> {
    let lines_str = lines.to_string();
    let output = Command::new("journalctl")
        .args(["-u", unit, "-n", &lines_str, "--no-pager"])
        .output()
        .map_err(|e| format!("Impossible de lire le journal : {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
