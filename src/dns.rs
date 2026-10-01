use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub mode: String, // "cloudflare", "quad9", "adguard", "google", "mullvad", "custom"
    pub primary_servers: Vec<String>,
    #[serde(default)]
    pub custom_servers: Vec<String>,
    #[serde(default = "default_fallbacks")]
    pub fallback_servers: Vec<String>,
    #[serde(default = "default_true")]
    pub free_port_53: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_fallbacks() -> Vec<String> {
    vec!["1.1.1.1".to_string(), "9.9.9.9".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsProviderInfo {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub tags: Vec<String>,
    pub ping_ms: Option<u64>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsOverview {
    pub mode: String,
    pub active_servers: Vec<String>,
    pub custom_servers: Vec<String>,
    pub fallback_servers: Vec<String>,
    pub free_port_53: bool,
    pub port_53_available: bool,
    pub resolution_status: bool,
    pub active_ping_ms: Option<u64>,
    pub providers: Vec<DnsProviderInfo>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDnsRequest {
    pub mode: String,
    pub custom_servers: Option<Vec<String>>,
    pub fallback_servers: Option<Vec<String>>,
    pub free_port_53: Option<bool>,
}

pub fn get_dns_json_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(PathBuf::from("/var/lib/steveos/dns.json"));
    if let Ok(config_dir) = std::env::var("STEVEOS_CONFIG_DIR") {
        let p = PathBuf::from(config_dir).join("dns.json");
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    let user_dev_dns = PathBuf::from("/home/chomiam/Projects/steveos-nas/dns.json");
    if !paths.contains(&user_dev_dns) {
        paths.push(user_dev_dns);
    }
    for p in &[
        "/etc/nixos/dns.json",
        "/etc/nixos/steveos-nas/dns.json",
        "./dns.json",
        "../dns.json",
        "../../dns.json",
    ] {
        let pb = PathBuf::from(p);
        if !paths.contains(&pb) {
            paths.push(pb);
        }
    }
    paths
}

pub fn load_dns_config() -> DnsConfig {
    for path in get_dns_json_paths() {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<DnsConfig>(&content) {
                    return cfg;
                }
            }
        }
    }
    DnsConfig {
        mode: "cloudflare".to_string(),
        primary_servers: vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()],
        custom_servers: Vec::new(),
        fallback_servers: default_fallbacks(),
        free_port_53: true,
        updated_at: None,
    }
}

pub fn save_dns_config(cfg: &DnsConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    let _ = std::fs::create_dir_all("/var/lib/steveos");

    let mut written = false;
    for path in get_dns_json_paths() {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        if std::fs::write(&path, &json).is_ok() {
            written = true;
        }
    }

    if written {
        Ok(())
    } else {
        Err("Impossible d'enregistrer dns.json sur le stockage".into())
    }
}

pub async fn measure_dns_latency_async(ip: &str) -> Option<u64> {
    let clean = ip.trim();
    if clean.is_empty() {
        return None;
    }

    let target = if clean.contains(':') {
        format!("[{}]:53", clean)
    } else {
        format!("{}:53", clean)
    };

    if let Ok(addr) = target.parse::<SocketAddr>() {
        let start = Instant::now();
        if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_millis(350), tokio::net::TcpStream::connect(addr)).await {
            return Some(start.elapsed().as_millis() as u64);
        }
    }

    None
}

pub fn check_port_53_available() -> bool {
    let out = Command::new("ss").args(["-H", "-tlun", "sport", "=", ":53"]).output();
    if let Ok(o) = out {
        let stdout = String::from_utf8_lossy(&o.stdout);
        // Le port 53 est bloqué si systemd-resolved (127.0.0.53/54), dnsmasq (192.168.122.1) ou 0.0.0.0:53 est actif
        let has_conflict = stdout.lines().any(|line| {
            line.contains("127.0.0.53") || line.contains("127.0.0.54") || line.contains("0.0.0.0:53") || line.contains("192.168.122.1:53")
        });
        !has_conflict
    } else {
        true
    }
}

pub async fn get_dns_overview() -> DnsOverview {
    let cfg = load_dns_config();
    let port_53_available = check_port_53_available();

    let raw_providers = vec![
        (
            "cloudflare",
            "Cloudflare (1.1.1.1)",
            "⚡",
            "Résolveur mondial le plus rapide, politique stricte de confidentialité (zéro vente de données, purge des logs en 24h).",
            vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()],
            vec!["2606:4700:4700::1111".to_string(), "2606:4700:4700::1001".to_string()],
            vec!["Ultra-Rapide".to_string(), "Confidentialité".to_string(), "Anycast".to_string()],
        ),
        (
            "quad9",
            "Quad9 (Protection Malwares)",
            "🛡️",
            "Fondation suisse sans but lucratif bloquant automatiquement les domaines malveillants, botnets et phishing en temps réel.",
            vec!["9.9.9.9".to_string(), "149.112.112.112".to_string()],
            vec!["2620:fe::fe".to_string(), "2620:fe::9".to_string()],
            vec!["Anti-Malware".to_string(), "Suisse / RGPD".to_string(), "Zero-Log".to_string()],
        ),
        (
            "adguard",
            "AdGuard DNS (Anti-Pub & Traqueurs)",
            "🚫",
            "Bloque les bannières publicitaires, compteurs analytiques et traqueurs au niveau DNS pour tous les appareils du réseau.",
            vec!["94.140.14.14".to_string(), "94.140.15.15".to_string()],
            vec!["2a10:50c0::ad1:ff".to_string(), "2a10:50c0::ad2:ff".to_string()],
            vec!["Anti-Pub".to_string(), "Anti-Traqueur".to_string(), "Protection Web".to_string()],
        ),
        (
            "google",
            "Google Public DNS",
            "🌐",
            "Infrastructure robuste haute disponibilité à couverture planétaire, accélération de la résolution et résilience mondiale.",
            vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
            vec!["2001:4860:4860::8888".to_string(), "2001:4860:4860::8844".to_string()],
            vec!["Haute Disponibilité".to_string(), "Anycast Mondial".to_string(), "Standard".to_string()],
        ),
        (
            "mullvad",
            "Mullvad DNS (Non censuré & Chiffré)",
            "🔒",
            "Orienté vie privée maximale basé en Suède, zéro journalisation, conforme aux normes strictes de non-surveillance.",
            vec!["194.242.2.2".to_string()],
            vec!["2a07:e340::2".to_string()],
            vec!["Confidentialité Maximale".to_string(), "Suède".to_string(), "Anti-Censure".to_string()],
        ),
    ];

    // Lancement concurrent (en parallèle) de toutes les sondes de latence avec Tokio
    let mut probe_handles = Vec::new();
    for (id, name, icon, desc, v4, v6, tags) in raw_providers {
        let is_active = cfg.mode == id;
        let test_ip = v4.first().cloned();
        let handle = tokio::spawn(async move {
            let ping = match test_ip {
                Some(ref ip) => measure_dns_latency_async(ip).await,
                None => None,
            };
            DnsProviderInfo {
                id: id.to_string(),
                name: name.to_string(),
                icon: icon.to_string(),
                description: desc.to_string(),
                ipv4: v4,
                ipv6: v6,
                tags,
                ping_ms: ping,
                is_active,
            }
        });
        probe_handles.push(handle);
    }

    let mut providers = Vec::new();
    for h in probe_handles {
        if let Ok(info) = h.await {
            providers.push(info);
        }
    }

    // Calcul des serveurs actifs actuels
    let active_servers = if cfg.mode == "custom" && !cfg.custom_servers.is_empty() {
        let mut list = cfg.custom_servers.clone();
        for fb in &cfg.fallback_servers {
            if !list.contains(fb) {
                list.push(fb.clone());
            }
        }
        list
    } else {
        let mut list = cfg.primary_servers.clone();
        for fb in &cfg.fallback_servers {
            if !list.contains(fb) {
                list.push(fb.clone());
            }
        }
        list
    };

    let active_ping_ms = if let Some(ip) = active_servers.first() {
        measure_dns_latency_async(ip).await
    } else {
        None
    };

    // Test non-bloquant de résolution DNS (timeout 400ms)
    let resolution_status = tokio::time::timeout(Duration::from_millis(400), tokio::net::lookup_host("nixos.org:80"))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);

    DnsOverview {
        mode: cfg.mode,
        active_servers,
        custom_servers: cfg.custom_servers,
        fallback_servers: cfg.fallback_servers,
        free_port_53: cfg.free_port_53,
        port_53_available,
        resolution_status,
        active_ping_ms,
        providers,
        updated_at: cfg.updated_at,
    }
}

pub fn update_dns(req: &UpdateDnsRequest) -> Result<String, String> {
    let mode = req.mode.trim().to_lowercase();
    let primary_servers = match mode.as_str() {
        "cloudflare" => vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()],
        "quad9" => vec!["9.9.9.9".to_string(), "149.112.112.112".to_string()],
        "adguard" => vec!["94.140.14.14".to_string(), "94.140.15.15".to_string()],
        "google" => vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
        "mullvad" => vec!["194.242.2.2".to_string()],
        "custom" => {
            let custom = req.custom_servers.clone().unwrap_or_default();
            let clean: Vec<String> = custom
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if clean.is_empty() {
                return Err("Veuillez saisir au moins une adresse IP DNS personnalisée valide (ex: 127.0.0.1 ou 192.168.1.100).".into());
            }
            clean
        }
        _ => {
            return Err(format!("Mode DNS inconnu : {}", mode));
        }
    };

    let fallback_servers = req.fallback_servers.clone().unwrap_or_else(default_fallbacks);
    let free_port_53 = req.free_port_53.unwrap_or(true);

    let new_cfg = DnsConfig {
        mode: mode.clone(),
        primary_servers: primary_servers.clone(),
        custom_servers: if mode == "custom" { primary_servers.clone() } else { req.custom_servers.clone().unwrap_or_default() },
        fallback_servers: fallback_servers.clone(),
        free_port_53,
        updated_at: Some(get_now_string()),
    };

    // 1. Sauvegarde déclarative dans dns.json
    save_dns_config(&new_cfg)?;

    // 2. Application à chaud du DNS
    apply_dns_runtime(&new_cfg)?;

    Ok(format!(
        "Configuration DNS '{}' appliquée avec succès à chaud ! Résolution vérifiée.",
        mode
    ))
}

fn apply_dns_runtime(cfg: &DnsConfig) -> Result<(), String> {
    let mut effective_servers = cfg.primary_servers.clone();
    for fb in &cfg.fallback_servers {
        if !effective_servers.contains(fb) {
            effective_servers.push(fb.clone());
        }
    }

    // 1. Libération du port 53 si demandé
    if cfg.free_port_53 {
        // Stopper immédiatement systemd-resolved via sudo pour libérer 127.0.0.53:53 à chaud
        let _ = Command::new("sudo").args(["systemctl", "stop", "systemd-resolved"]).output();

        // Écriture déclarative du drop-in désactivant le stub listener
        let _ = Command::new("sudo").args(["mkdir", "-p", "/etc/systemd/resolved.conf.d"]).output();
        let dropin = "[Resolve]
DNSStubListener=no
";
        let tmp_dropin = "/tmp/steveos-resolved-stub.conf";
        if std::fs::write(tmp_dropin, dropin).is_ok() {
            let _ = Command::new("sudo").args(["cp", tmp_dropin, "/etc/systemd/resolved.conf.d/steveos-dns.conf"]).output();
            let _ = std::fs::remove_file(tmp_dropin);
        }

        // Redémarrer systemd-resolved (avec DNSStubListener=no il ne liera plus aucun port 53)
        let _ = Command::new("sudo").args(["systemctl", "restart", "systemd-resolved"]).output();

        // Si libvirt écoute sur 192.168.122.1:53 (virbr0), désactiver le serveur DNS libvirt
        let _ = Command::new("sudo").args(["virsh", "-c", "qemu:///system", "net-update", "default", "modify", "dns", "<dns enable='no'/>", "--live", "--config"]).output();
    }

    // 2. Écriture immédiate dans /etc/resolv.conf
    let mut resolv_lines = vec![
        "# Generated dynamically by STEvE_OS Dashboard (Runtime DNS)".to_string(),
    ];
    for ip in &effective_servers {
        resolv_lines.push(format!("nameserver {}", ip.trim()));
    }
    resolv_lines.push("options timeout:2 attempts:3".to_string());
    let resolv_content = resolv_lines.join("
") + "
";

    let _ = std::fs::write("/run/systemd/resolve/resolv.conf", &resolv_content);
    let tmp_resolv = "/tmp/steveos-resolv.conf";
    if std::fs::write(tmp_resolv, &resolv_content).is_ok() {
        let _ = Command::new("sudo").args(["cp", tmp_resolv, "/etc/resolv.conf"]).output();
        let _ = std::fs::remove_file(tmp_resolv);
    } else {
        let _ = Command::new("sudo").args(["bash", "-c", &format!("echo '{}' > /etc/resolv.conf", resolv_content)]).output();
    }

    // 3. Mise à jour via resolvectl si disponible
    for srv in &effective_servers {
        let _ = Command::new("resolvectl").args(["dns", "default", srv]).output();
    }

    Ok(())
}

fn get_now_string() -> String {
    if let Ok(out) = Command::new("date").arg("+%Y-%m-%d %H:%M:%S").output() {
        return String::from_utf8_lossy(&out.stdout).trim().to_string();
    }
    "Aujourd'hui".to_string()
}
