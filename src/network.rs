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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireguardStatus {
    pub is_active: bool,
    pub unit: String,
    pub interface: String,
    pub port: u16,
    pub subnet: String,
    pub public_key: Option<String>,
    pub endpoint: Option<String>,
    pub transfer_rx: Option<String>,
    pub transfer_tx: Option<String>,
    pub peers_count: usize,
    pub status_text: String,
    pub peers: Vec<WireguardPeerItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireguardPeerItem {
    pub public_key: String,
    pub allowed_ips: String,
    pub endpoint: Option<String>,
    pub latest_handshake: Option<String>,
    pub transfer_rx: Option<String>,
    pub transfer_tx: Option<String>,
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
        },
        firewall: get_firewall_overview(),
        samba: get_samba_section(),
        sftp: get_sftp_section(),
    }
}

pub fn get_hostname() -> String {
    Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "noos-nas".to_string())
}

pub fn get_primary_lan_ip() -> String {
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
    let mut peers: Vec<WireguardPeerItem> = Vec::new();
    let mut current_peer: Option<WireguardPeerItem> = None;

    if let Ok(output) = Command::new(crate::wireguard::get_wg_bin()).arg("show").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("public key:") && current_peer.is_none() {
                public_key = trimmed.split_once(':').map(|(_, v)| v.trim().to_string());
            } else if trimmed.starts_with("peer:") {
                if let Some(p) = current_peer.take() {
                    peers.push(p);
                }
                let peer_key = trimmed.split_once(':').map(|(_, v)| v.trim().to_string()).unwrap_or_default();
                current_peer = Some(WireguardPeerItem {
                    public_key: peer_key,
                    allowed_ips: "10.100.0.x/32".into(),
                    endpoint: None,
                    latest_handshake: None,
                    transfer_rx: None,
                    transfer_tx: None,
                });
            } else if let Some(ref mut p) = current_peer {
                if trimmed.starts_with("endpoint:") {
                    p.endpoint = trimmed.split_once(':').map(|(_, v)| v.trim().to_string());
                } else if trimmed.starts_with("allowed ips:") {
                    p.allowed_ips = trimmed.split_once(':').map(|(_, v)| v.trim().to_string()).unwrap_or_default();
                } else if trimmed.starts_with("latest handshake:") {
                    p.latest_handshake = trimmed.split_once(':').map(|(_, v)| v.trim().to_string());
                } else if trimmed.starts_with("transfer:") {
                    if let Some((_, v)) = trimmed.split_once(':') {
                        let parts: Vec<&str> = v.split(',').collect();
                        if parts.len() >= 2 {
                            p.transfer_rx = Some(parts[0].trim().to_string());
                            p.transfer_tx = Some(parts[1].trim().to_string());
                        }
                    }
                }
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
        if let Some(p) = current_peer {
            peers.push(p);
        }
    }

    let peers_count = peers.len();

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
        subnet: "10.100.0.1/24".into(),
        public_key,
        endpoint,
        transfer_rx,
        transfer_tx,
        peers_count,
        status_text,
        peers,
    }
}

fn get_samba_section() -> SambaSection {
    let is_active = Command::new("systemctl")
        .args(["is-active", "samba-smbd"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    let dynamic_shares = crate::samba::load_samba_shares();
    let shares = dynamic_shares.into_iter().map(|s| SambaShareItem {
        name: s.name,
        path: s.path.clone(),
        description: s.comment,
        read_only: s.read_only,
        guest_ok: s.guest_ok,
        exists: std::path::Path::new(&s.path).exists(),
    }).collect();

    let active_sessions = get_smb_sessions();

    SambaSection {
        is_active,
        unit: "samba-smbd".into(),
        status_text: if is_active { "Actif (Partages SMB en ligne)".into() } else { "Inactif / Arrêté".into() },
        workgroup: "WORKGROUP".into(),
        server_string: "Noos NAS".into(),
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

// =========================================================================
// 🚀 SURVEILLANCE DU TRAFIC RÉSEAU EN TEMPS RÉEL & HISTORIQUE
// =========================================================================

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

static LAST_TRAFFIC_SAMPLE: Mutex<Option<(Instant, HashMap<String, (u64, u64)>)>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTrafficOverview {
    pub total_rx_sec: u64,
    pub total_tx_sec: u64,
    pub total_rx_speed_human: String,
    pub total_tx_speed_human: String,
    pub interfaces: Vec<InterfaceTraffic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceTraffic {
    pub name: String,
    pub is_up: bool,
    pub rx_bytes_sec: u64,
    pub tx_bytes_sec: u64,
    pub rx_speed_human: String,
    pub tx_speed_human: String,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub total_rx_human: String,
    pub total_tx_human: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficHistoryOverview {
    pub today_rx_human: String,
    pub today_tx_human: String,
    pub today_total_human: String,
    pub month_rx_human: String,
    pub month_tx_human: String,
    pub month_total_human: String,
    pub source: String,
    pub has_vnstat: bool,
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} To", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} Go", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} Mo", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} Ko", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_speed(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

fn read_proc_net_dev() -> HashMap<String, (u64, u64)> {
    let mut map = HashMap::new();
    if let Ok(content) = std::fs::read_to_string("/proc/net/dev") {
        for line in content.lines().skip(2) {
            if let Some((iface, data)) = line.split_once(':') {
                let iface_name = iface.trim().to_string();
                let parts: Vec<&str> = data.split_whitespace().collect();
                if parts.len() >= 9 {
                    let rx_bytes = parts[0].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[8].parse::<u64>().unwrap_or(0);
                    map.insert(iface_name, (rx_bytes, tx_bytes));
                }
            }
        }
    }
    map
}

pub fn get_live_traffic() -> LiveTrafficOverview {
    let now = Instant::now();
    let current_map = read_proc_net_dev();

    let mut last_guard = LAST_TRAFFIC_SAMPLE.lock().unwrap();
    let (delta_secs, prev_map) = match last_guard.take() {
        Some((last_instant, prev)) => {
            let secs = now.duration_since(last_instant).as_secs_f64();
            (if secs > 0.05 { secs } else { 1.0 }, prev)
        }
        None => (1.0, HashMap::new()),
    };

    *last_guard = Some((now, current_map.clone()));

    let mut interfaces = Vec::new();
    let mut sum_rx_sec = 0u64;
    let mut sum_tx_sec = 0u64;

    for (iface, (rx_tot, tx_tot)) in &current_map {
        if iface == "lo" {
            continue;
        }

        let is_up = std::fs::read_to_string(format!("/sys/class/net/{}/operstate", iface))
            .map(|s| s.trim() != "down")
            .unwrap_or(true);

        let (prev_rx, prev_tx) = prev_map.get(iface).copied().unwrap_or((*rx_tot, *tx_tot));

        let rx_delta = rx_tot.saturating_sub(prev_rx);
        let tx_delta = tx_tot.saturating_sub(prev_tx);

        let rx_rate = (rx_delta as f64 / delta_secs) as u64;
        let tx_rate = (tx_delta as f64 / delta_secs) as u64;

        sum_rx_sec += rx_rate;
        sum_tx_sec += tx_rate;

        interfaces.push(InterfaceTraffic {
            name: iface.clone(),
            is_up,
            rx_bytes_sec: rx_rate,
            tx_bytes_sec: tx_rate,
            rx_speed_human: format_speed(rx_rate),
            tx_speed_human: format_speed(tx_rate),
            total_rx_bytes: *rx_tot,
            total_tx_bytes: *tx_tot,
            total_rx_human: format_bytes(*rx_tot),
            total_tx_human: format_bytes(*tx_tot),
        });
    }

    interfaces.sort_by(|a, b| {
        let a_score = if a.name.starts_with("en") || a.name.starts_with("eth") { 0 }
            else if a.name.starts_with("wl") { 1 }
            else if a.name.starts_with("wg") { 2 }
            else { 3 };
        let b_score = if b.name.starts_with("en") || b.name.starts_with("eth") { 0 }
            else if b.name.starts_with("wl") { 1 }
            else if b.name.starts_with("wg") { 2 }
            else { 3 };
        a_score.cmp(&b_score).then_with(|| a.name.cmp(&b.name))
    });

    LiveTrafficOverview {
        total_rx_sec: sum_rx_sec,
        total_tx_sec: sum_tx_sec,
        total_rx_speed_human: format_speed(sum_rx_sec),
        total_tx_speed_human: format_speed(sum_tx_sec),
        interfaces,
    }
}

pub fn get_traffic_history() -> TrafficHistoryOverview {
    if let Ok(output) = Command::new("vnstat").args(["--json", "d", "1"]).output() {
        if output.status.success() {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(interfaces) = val.get("interfaces").and_then(|i| i.as_array()) {
                    let mut today_rx = 0u64;
                    let mut today_tx = 0u64;
                    let mut month_rx = 0u64;
                    let mut month_tx = 0u64;

                    for iface in interfaces {
                        if let Some(traffic) = iface.get("traffic") {
                            if let Some(days) = traffic.get("day").and_then(|d| d.as_array()) {
                                if let Some(last_day) = days.last() {
                                    today_rx += last_day.get("rx").and_then(|v| v.as_u64()).unwrap_or(0);
                                    today_tx += last_day.get("tx").and_then(|v| v.as_u64()).unwrap_or(0);
                                }
                            }
                            if let Some(months) = traffic.get("month").and_then(|m| m.as_array()) {
                                if let Some(last_month) = months.last() {
                                    month_rx += last_month.get("rx").and_then(|v| v.as_u64()).unwrap_or(0);
                                    month_tx += last_month.get("tx").and_then(|v| v.as_u64()).unwrap_or(0);
                                }
                            }
                        }
                    }

                    if today_rx > 0 || today_tx > 0 || month_rx > 0 || month_tx > 0 {
                        return TrafficHistoryOverview {
                            today_rx_human: format_bytes(today_rx),
                            today_tx_human: format_bytes(today_tx),
                            today_total_human: format_bytes(today_rx + today_tx),
                            month_rx_human: format_bytes(month_rx),
                            month_tx_human: format_bytes(month_tx),
                            month_total_human: format_bytes(month_rx + month_tx),
                            source: "vnStat".to_string(),
                            has_vnstat: true,
                        };
                    }
                }
            }
        }
    }

    // Fallback: cumul total depuis le boot (/proc/net/dev)
    let current_map = read_proc_net_dev();
    let mut total_rx = 0u64;
    let mut total_tx = 0u64;
    for (iface, (rx, tx)) in &current_map {
        if iface != "lo" {
            total_rx += rx;
            total_tx += tx;
        }
    }

    TrafficHistoryOverview {
        today_rx_human: format_bytes(total_rx),
        today_tx_human: format_bytes(total_tx),
        today_total_human: format_bytes(total_rx + total_tx),
        month_rx_human: format_bytes(total_rx),
        month_tx_human: format_bytes(total_tx),
        month_total_human: format_bytes(total_rx + total_tx),
        source: "Noyau Linux (Total session)".to_string(),
        has_vnstat: false,
    }
}
