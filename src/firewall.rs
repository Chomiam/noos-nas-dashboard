use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallOverview {
    pub is_enabled: bool,
    pub tcp_ports: Vec<PortRule>,
    pub udp_ports: Vec<PortRule>,
    pub banned_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRule {
    pub port: u16,
    pub protocol: String,
    pub service_name: String,
    pub status: String,
}

pub fn get_firewall_overview() -> FirewallOverview {
    // Vérifier si iptables / nftables a des règles ou si le service firewall NixOS est actif
    let is_enabled = Command::new("systemctl")
        .args(["is-active", "firewall"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(true);

    let tcp_rules = vec![
        PortRule { port: 22, protocol: "TCP".into(), service_name: "SSH / sFTP (Administration & Transferts)".into(), status: "Autorisé".into() },
        PortRule { port: 139, protocol: "TCP".into(), service_name: "Samba NetBIOS-SSN".into(), status: "Autorisé".into() },
        PortRule { port: 445, protocol: "TCP".into(), service_name: "Samba SMB (Partage de fichiers Windows/Mac)".into(), status: "Autorisé".into() },
        PortRule { port: 2049, protocol: "TCP".into(), service_name: "NFS (Partages Réseau Linux)".into(), status: "Autorisé".into() },
        PortRule { port: 5357, protocol: "TCP".into(), service_name: "WSDD (Découverte Web Services Windows)".into(), status: "Autorisé".into() },
        PortRule { port: 8096, protocol: "TCP".into(), service_name: "Jellyfin HTTP (Streaming Multimédia)".into(), status: "Autorisé".into() },
        PortRule { port: 8920, protocol: "TCP".into(), service_name: "Jellyfin HTTPS".into(), status: "Autorisé".into() },
        PortRule { port: 9339, protocol: "TCP".into(), service_name: "STEvE_OS NAS Dashboard (Ce Tableau de bord)".into(), status: "Autorisé".into() },
    ];

    let udp_rules = vec![
        PortRule { port: 137, protocol: "UDP".into(), service_name: "Samba NetBIOS-NS".into(), status: "Autorisé".into() },
        PortRule { port: 138, protocol: "UDP".into(), service_name: "Samba NetBIOS-DGM".into(), status: "Autorisé".into() },
        PortRule { port: 1900, protocol: "UDP".into(), service_name: "Jellyfin DLNA Discovery".into(), status: "Autorisé".into() },
        PortRule { port: 3702, protocol: "UDP".into(), service_name: "WS-Discovery Multicast (Windows)".into(), status: "Autorisé".into() },
        PortRule { port: 7359, protocol: "UDP".into(), service_name: "Jellyfin Client Autodiscovery".into(), status: "Autorisé".into() },
        PortRule { port: 51820, protocol: "UDP".into(), service_name: "WireGuard VPN (Réseau privé)".into(), status: "Autorisé".into() },
    ];

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

    FirewallOverview {
        is_enabled,
        tcp_ports: tcp_rules,
        udp_ports: udp_rules,
        banned_ips,
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
