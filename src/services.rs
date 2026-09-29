use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesOverview {
    pub samba: ServiceItem,
    pub sshd: ServiceItem,
    pub nfs: ServiceItem,
    pub jellyfin: ServiceItem,
    pub docker: ServiceItem,
    pub cockpit: ServiceItem,
    pub fail2ban: ServiceItem,
    pub tailscale: ServiceItem,
    pub wireguard: ServiceItem,
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
}

pub fn get_services_overview() -> ServicesOverview {
    let samba = check_unit("samba-smbd", "Partages Windows / Mac (Samba SMB)");
    let sshd = check_unit("sshd", "Serveur SSH & Partage sFTP Chiffré");
    let nfs = check_unit("nfs-server", "Serveur de Partages NFS Linux");
    let jellyfin = check_unit("jellyfin", "Serveur Multimédia & Transcodage");
    let docker = check_unit("docker", "Moteur de Conteneurs Docker");
    let cockpit = check_unit("cockpit", "Console d'Administration Web (Port 9090)");
    let fail2ban = check_unit("fail2ban", "Système Anti-Bruteforce & Sécurité");
    let tailscale = check_unit("tailscaled", "Tunnel Réseau Privé Tailscale");
    let wireguard = check_unit("wireguard-wg0", "Réseau Privé WireGuard");

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
        cockpit,
        fail2ban,
        tailscale,
        wireguard,
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

fn get_ssh_sessions() -> Vec<ActiveSession> {
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

fn get_smb_sessions() -> Vec<ActiveSession> {
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

fn get_docker_containers() -> Vec<DockerContainer> {
    let mut containers = Vec::new();
    if let Ok(output) = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.State}}"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let state = parts[4].to_lowercase();
                containers.push(DockerContainer {
                    id: parts[0].to_string(),
                    name: parts[1].to_string(),
                    image: parts[2].to_string(),
                    status: parts[3].to_string(),
                    is_running: state == "running",
                });
            }
        }
    }
    containers
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
