use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareOverview {
    pub cpu: CpuHardwareInfo,
    pub motherboard: MotherboardInfo,
    pub memory: MemoryHardwareInfo,
    pub gpu: GpuHardwareInfo,
    pub network_adapters: Vec<NetworkAdapterInfo>,
    pub storage_controllers: Vec<StorageControllerInfo>,
    pub system_summary: SystemSummaryInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuHardwareInfo {
    pub model: String,
    pub architecture: String,
    pub sockets: u32,
    pub cores_per_socket: u32,
    pub total_cores: u32,
    pub total_threads: u32,
    pub base_frequency_ghz: String,
    pub cache: String,
    pub virtualization: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotherboardInfo {
    pub vendor: String,
    pub product_name: String,
    pub board_name: String,
    pub chipset: String,
    pub bios_version: String,
    pub bios_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHardwareInfo {
    pub total_gb: f64,
    pub free_gb: f64,
    pub available_gb: f64,
    pub mem_type: String,
    pub channels: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuHardwareInfo {
    pub model: String,
    pub vendor: String,
    pub driver: String,
    pub vram: String,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAdapterInfo {
    pub interface_name: String,
    pub controller_model: String,
    pub mac_address: String,
    pub ipv4: Option<String>,
    pub speed_mbps: i32,
    pub is_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageControllerInfo {
    pub name: String,
    pub controller_type: String, // "SATA", "NVMe"
    pub pci_slot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSummaryInfo {
    pub os_name: String,
    pub version: String,
    pub kernel_version: String,
    pub uptime: String,
    pub hostname: String,
}

fn read_dmi_field(field: &str) -> String {
    let p = format!("/sys/class/dmi/id/{}", field);
    fs::read_to_string(&p)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Inconnu".to_string())
}

pub fn get_hardware_overview() -> HardwareOverview {
    // 1. CPU Info
    let mut cpu_model = "Intel(R) Xeon(R) CPU E5-2650 v4 @ 2.20GHz".to_string();
    let mut total_threads = 48;
    let mut sockets = 2;
    let mut cores_per_socket = 12;

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if line.starts_with("model name") {
                if let Some(m) = line.split(':').nth(1) {
                    cpu_model = m.trim().to_string();
                    break;
                }
            }
        }
        let processor_count = cpuinfo.lines().filter(|l| l.starts_with("processor")).count();
        if processor_count > 0 {
            total_threads = processor_count as u32;
        }
    }

    if let Ok(out) = Command::new("lscpu").output() {
        if out.status.success() {
            let lscpu_str = String::from_utf8_lossy(&out.stdout);
            for line in lscpu_str.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    let k = parts[0].trim();
                    let v = parts[1].trim();
                    if k == "Socket(s)" {
                        if let Ok(s) = v.parse::<u32>() { sockets = s; }
                    } else if k == "Cœur(s) par socket" || k == "Core(s) per socket" {
                        if let Ok(c) = v.parse::<u32>() { cores_per_socket = c; }
                    }
                }
            }
        }
    }

    let cpu = CpuHardwareInfo {
        model: format!("{}x {}", sockets, cpu_model),
        architecture: "x86_64 (64-bit)".to_string(),
        sockets,
        cores_per_socket,
        total_cores: sockets * cores_per_socket,
        total_threads,
        base_frequency_ghz: "2.20 GHz (Turbo 2.90 GHz)".to_string(),
        cache: "60 Mo Intel® Smart Cache (30 Mo / socket)".to_string(),
        virtualization: "Intel VT-x / VT-d (Activé)".to_string(),
    };

    // 2. Motherboard & BIOS
    let vendor = read_dmi_field("sys_vendor");
    let product_name = read_dmi_field("product_name");
    let board_name = read_dmi_field("board_name");
    let bios_version = read_dmi_field("bios_version");
    let bios_date = read_dmi_field("bios_date");

    let motherboard = MotherboardInfo {
        vendor: if vendor.is_empty() || vendor == "Inconnu" { "HUANANZHI".to_string() } else { vendor },
        product_name: if product_name.is_empty() || product_name == "Inconnu" { "X99-F8D PLUS".to_string() } else { product_name },
        board_name: if board_name.is_empty() || board_name == "Inconnu" { "X99-F8D PLUS (Dual Socket LGA 2011-v3)".to_string() } else { board_name },
        chipset: "Intel C610 / X99 Express Server Chipset".to_string(),
        bios_version: if bios_version.is_empty() { "American Megatrends 5.11".to_string() } else { bios_version },
        bios_date: if bios_date.is_empty() { "2023".to_string() } else { bios_date },
    };

    // 3. Memory Info
    let mut total_kb: f64 = 65000000.0;
    let mut free_kb: f64 = 50000000.0;
    let mut avail_kb: f64 = 60000000.0;

    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[0] {
                    "MemTotal:" => total_kb = parts[1].parse().unwrap_or(total_kb),
                    "MemFree:" => free_kb = parts[1].parse().unwrap_or(free_kb),
                    "MemAvailable:" => avail_kb = parts[1].parse().unwrap_or(avail_kb),
                    _ => {}
                }
            }
        }
    }

    let memory = MemoryHardwareInfo {
        total_gb: (total_kb / 1024.0 / 1024.0 * 10.0).round() / 10.0,
        free_gb: (free_kb / 1024.0 / 1024.0 * 10.0).round() / 10.0,
        available_gb: (avail_kb / 1024.0 / 1024.0 * 10.0).round() / 10.0,
        mem_type: "DDR4 ECC Registered Server Memory".to_string(),
        channels: "Quad-Channel par Socket (8 slots DIMM)".to_string(),
    };

    // 4. GPU Info
    let gpu = GpuHardwareInfo {
        model: "Intel Corporation DG2 [Arc A380]".to_string(),
        vendor: "Intel Corporation".to_string(),
        driver: "i915 (Kernel Direct Rendering Manager)".to_string(),
        vram: "6 Go GDDR6 96-bit".to_string(),
        features: vec![
            "Encodage & Décodage Matériel AV1".to_string(),
            "Décodage HEVC / H.265 10-bit".to_string(),
            "Transcodage matériel Jellyfin / QSV".to_string(),
            "PCIe 4.0 x8".to_string(),
        ],
    };

    // 5. Network Adapters
    let mut network_adapters = Vec::new();
    let net_path = Path::new("/sys/class/net");
    if let Ok(entries) = fs::read_dir(net_path) {
        for entry in entries.flatten() {
            let iface = entry.file_name().to_string_lossy().to_string();
            if iface == "lo" {
                continue;
            }

            let operstate = fs::read_to_string(entry.path().join("operstate"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            let is_up = operstate == "up";

            let speed: i32 = fs::read_to_string(entry.path().join("speed"))
                .map(|s| s.trim().parse::<i32>().unwrap_or(-1))
                .unwrap_or(-1);

            let mac = fs::read_to_string(entry.path().join("address"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "--".to_string());

            let mut ipv4 = None;
            if let Ok(out) = Command::new("ip").args(["-j", "-4", "addr", "show", &iface]).output() {
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                    if let Some(arr) = val.as_array() {
                        if let Some(first) = arr.first() {
                            if let Some(addrs) = first.get("addr_info").and_then(|a| a.as_array()) {
                                if let Some(a0) = addrs.first() {
                                    ipv4 = a0.get("local").and_then(|l| l.as_str()).map(|s| s.to_string());
                                }
                            }
                        }
                    }
                }
            }

            let controller_model = if iface.starts_with("enp7") || iface.starts_with("enp8") {
                "Realtek Semiconductor RTL8125 2.5GbE Controller".to_string()
            } else if iface.starts_with("docker") {
                "Pont Réseau Virtuel Docker".to_string()
            } else {
                "Contrôleur Réseau Ethernet".to_string()
            };

            network_adapters.push(NetworkAdapterInfo {
                interface_name: iface,
                controller_model,
                mac_address: mac,
                ipv4,
                speed_mbps: speed,
                is_up,
            });
        }
    }

    // 6. Storage Controllers
    let storage_controllers = vec![
        StorageControllerInfo {
            name: "Intel C610/X99 Series Chipset 6-Port SATA Controller [AHCI]".to_string(),
            controller_type: "SATA 6Gb/s (AHCI)".to_string(),
            pci_slot: "00:1f.2".to_string(),
        },
        StorageControllerInfo {
            name: "MAXIO Technology MAP1202 NVMe SSD Controller".to_string(),
            controller_type: "NVMe PCIe Gen3 x4".to_string(),
            pci_slot: "01:00.0".to_string(),
        },
        StorageControllerInfo {
            name: "MAXIO Technology MAP1202 NVMe SSD Controller".to_string(),
            controller_type: "NVMe PCIe Gen3 x4".to_string(),
            pci_slot: "02:00.0".to_string(),
        },
    ];

    // 7. System Summary
    let kernel = fs::read_to_string("/proc/version")
        .map(|s| {
            let p: Vec<&str> = s.split_whitespace().collect();
            if p.len() >= 3 { p[2].to_string() } else { "Linux 7.2.5".to_string() }
        })
        .unwrap_or_else(|_| "Linux 7.2.5".to_string());

    let hostname = fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "steveos-nas".to_string());

    let uptime = format_uptime();

    let system_summary = SystemSummaryInfo {
        os_name: "STEvE_OS NAS Edition".to_string(),
        version: "26.05 (Ailurus)".to_string(),
        kernel_version: kernel,
        uptime,
        hostname,
    };

    HardwareOverview {
        cpu,
        motherboard,
        memory,
        gpu,
        network_adapters,
        storage_controllers,
        system_summary,
    }
}

fn format_uptime() -> String {
    if let Ok(contents) = fs::read_to_string("/proc/uptime") {
        if let Some(first) = contents.split_whitespace().next() {
            if let Ok(total_secs) = first.parse::<f64>() {
                let s = total_secs as u64;
                let days = s / 86400;
                let hours = (s % 86400) / 3600;
                let minutes = (s % 3600) / 60;
                if days > 0 {
                    return format!("{}j {}h {}m", days, hours, minutes);
                } else if hours > 0 {
                    return format!("{}h {}m", hours, minutes);
                } else {
                    return format!("{}m", minutes);
                }
            }
        }
    }
    "Actif".to_string()
}
