use serde::{Deserialize, Serialize};
use std::process::Command;
use crate::firewall::{get_firewall_overview, FirewallOverview};
use crate::services::{get_smb_sessions, get_ssh_sessions, ActiveSession};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkOverview {
    pub hostname: String,
    pub primary_lan_ip: String,
    pub vpn: VpnSection,
    pub firewall: FirewallOverview,
    pub samba: SambaSection,
    pub sftp: SftpSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnSection {
    pub wireguard: WireguardStatus,
    pub tailscale: TailscaleStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireguardStatus {
    pub is_active: bool,
    pub unit: String,
    pub interface: String,
    pub port: u16,
    pub public_key: Option<String>,
    pub endpoint: Option<String>,
    pub transfer_rx: Option<String>,
    pub transfer_tx: Option<String>,
    pub peers_count: usize,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleStatus {
    pub is_active: bool,
    pub unit: String,
    pub tailscale_ip: Option<String>,
    pub node_name: Option<String>,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaSection {
    pub is_active: bool,
    pub unit: String,
    pub status_text: String,
    pub workgroup: String,
    pub server_string: String,
    pub shares: Vec<SambaShareItem>,
    pub active_sessions: Vec<ActiveSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaShareItem {
    pub name: String,
    pub path: String,
    pub description: String,
    pub read_only: bool,
    pub guest_ok: bool,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpSection {
    pub is_active: bool,
    pub unit: String,
    pub status_text: String,
    pub port: u16,
    pub chroot_path: String,
    pub active_sessions: Vec<ActiveSession>,
    pub fail2ban_protected: bool,
}

pub fn get_network_overview() -> NetworkOverview {
    NetworkOverview {
        hostname: get_hostname(),
        primary_lan_ip: get_primary_lan_ip(),
        vpn: VpnSection {
            wireguard: get_wireguard_status(),
            tailscale: get_tailscale_status(),
        },
        firewall: get_firewall_overview(),
        samba: get_samba_section(),
        sftp: get_sftp_section(),
    }
}

fn get_hostname() -> String {
    Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "steveos-nas".to_string())
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

fn get_wireguard_status() -> WireguardStatus {
    let is_active = Command::new("systemctl")
        .args(["is-active", "wireguard-wg0"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let mut public_key = None;
    let mut endpoint = None;
    let mut transfer_rx = None;
    let mut transfer_tx = None;
    let mut peers_count = 0;

    if let Ok(output) = Command::new("wg").arg("show").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("public key:") {
                public_key = trimmed.split_once(':').map(|(_, v)| v.trim().to_string());
            } else if trimmed.starts_with("peer:") {
                peers_count += 1;
            } else if trimmed.starts_with("endpoint:") {
                endpoint = trimmed.split_once(':').map(|(_, v)| v.trim().to_string());
            } else if trimmed.starts_with("transfer:") {
                if let Some((_, v)) = trimmed.split_once(':') {
                    let parts: Vec<&str> = v.split(',').collect();
                    if parts.len() >= 2 {
                        transfer_rx = Some(parts[0].trim().to_string());
                        transfer_tx = Some(parts[1].trim().to_string());
                    }
                }
            }
        }
    }

    let status_text = if is_active {
        "Actif (Interface wg0 en ligne)".into()
    } else {
        "Inactif / Arrêté".into()
    };

    WireguardStatus {
        is_active,
        unit: "wireguard-wg0".into(),
        interface: "wg0".into(),
        port: 51820,
        public_key,
        endpoint,
        transfer_rx,
        transfer_tx,
        peers_count,
        status_text,
    }
}

fn get_tailscale_status() -> TailscaleStatus {
    let is_active = Command::new("systemctl")
        .args(["is-active", "tailscaled"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let tailscale_ip = if is_active {
        Command::new("tailscale")
            .args(["ip", "-4"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    let status_text = if is_active {
        if tailscale_ip.is_some() {
            "Connecté au réseau Tailnet".into()
        } else {
            "Service actif (En attente d'authentification)".into()
        }
    } else {
        "Inactif / Arrêté".into()
    };

    TailscaleStatus {
        is_active,
        unit: "tailscaled".into(),
        tailscale_ip,
        node_name: Some("steveos-nas".into()),
        status_text,
    }
}

fn get_samba_section() -> SambaSection {
    let is_active = Command::new("systemctl")
        .args(["is-active", "samba-smbd"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let shares = vec![
        SambaShareItem {
            name: "data".into(),
            path: "/storage/data".into(),
            description: "Partage Général NAS (Lecture & Écriture)".into(),
            read_only: false,
            guest_ok: false,
            exists: std::path::Path::new("/storage/data").exists(),
        },
        SambaShareItem {
            name: "media".into(),
            path: "/storage/media".into(),
            description: "Médiathèque (Films, Séries, Musique - Jellyfin)".into(),
            read_only: false,
            guest_ok: true,
            exists: std::path::Path::new("/storage/media").exists(),
        },
        SambaShareItem {
            name: "backups".into(),
            path: "/storage/backups".into(),
            description: "Dépôt Sauvegardes & Snapshots (Accès Restreint)".into(),
            read_only: false,
            guest_ok: false,
            exists: std::path::Path::new("/storage/backups").exists(),
        },
        SambaShareItem {
            name: "shares".into(),
            path: "/mnt/storage/shares".into(),
            description: "Racine des Partages Réseau STEvE_OS".into(),
            read_only: false,
            guest_ok: false,
            exists: std::path::Path::new("/mnt/storage/shares").exists(),
        },
    ];

    let active_sessions = get_smb_sessions();

    SambaSection {
        is_active,
        unit: "samba-smbd".into(),
        status_text: if is_active { "Actif (Partages SMB en ligne)".into() } else { "Inactif / Arrêté".into() },
        workgroup: "WORKGROUP".into(),
        server_string: "STEvE_OS NAS".into(),
        shares,
        active_sessions,
    }
}

fn get_sftp_section() -> SftpSection {
    let is_active = Command::new("systemctl")
        .args(["is-active", "sshd"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let fail2ban_protected = Command::new("systemctl")
        .args(["is-active", "fail2ban"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let active_sessions = get_ssh_sessions();

    SftpSection {
        is_active,
        unit: "sshd".into(),
        status_text: if is_active { "Actif (Serveur SSH/sFTP en écoute)".into() } else { "Inactif / Arrêté".into() },
        port: 22,
        chroot_path: "/mnt/storage/sftp".into(),
        active_sessions,
        fail2ban_protected,
    }
}
