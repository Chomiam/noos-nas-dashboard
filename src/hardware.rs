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
    #[serde(default)]
    pub render_node: String,
    #[serde(default)]
    pub device_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAdapterInfo {
    pub interface_name: String,
    pub controller_model: String,
    pub mac_address: String,
    pub ipv4: Option<String>,
    pub speed_mbps: i32,
    pub is_up: bool,
    #[serde(default)]
    pub is_physical: bool,
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
    // 1. CPU Info dynamique
    let mut cpu_model = "Processeur x86_64".to_string();
    let mut total_threads: u32 = 1;
    let mut sockets: u32 = 1;
    let mut cores_per_socket: u32 = 1;
    let mut has_vmx = false;
    let mut has_svm = false;

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if line.starts_with("model name") {
                if let Some(m) = line.split(':').nth(1) {
                    cpu_model = m.trim().to_string();
                }
            } else if line.starts_with("flags") {
                if line.contains(" vmx ") { has_vmx = true; }
                if line.contains(" svm ") { has_svm = true; }
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

    if cores_per_socket == 1 && total_threads > 1 {
        cores_per_socket = total_threads / sockets.max(1);
    }

    let virt_str = if has_vmx {
        "Intel VT-x / VT-d (Matériel Activé)".to_string()
    } else if has_svm {
        "AMD-V / SVM (Matériel Activé)".to_string()
    } else {
        "Virtualisation Standard".to_string()
    };

    let base_freq = if let Ok(khz_str) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq") {
        if let Ok(khz) = khz_str.trim().parse::<f32>() {
            format!("{:.2} GHz Max", khz / 1_000_000.0)
        } else {
            "Fréquence dynamique".to_string()
        }
    } else {
        "Fréquence adaptative".to_string()
    };

    let cpu = CpuHardwareInfo {
        model: if sockets > 1 { format!("{}x {}", sockets, cpu_model) } else { cpu_model.clone() },
        architecture: "x86_64 (64-bit)".to_string(),
        sockets,
        cores_per_socket,
        total_cores: (sockets * cores_per_socket).max(1),
        total_threads,
        base_frequency_ghz: base_freq,
        cache: "Cache CPU Hiérarchique L1/L2/L3".to_string(),
        virtualization: virt_str,
    };

    // 2. Motherboard & BIOS dynamique
    let mut vendor = read_dmi_field("sys_vendor");
    if vendor.is_empty() || vendor == "Inconnu" {
        vendor = read_dmi_field("board_vendor");
    }
    let mut product_name = read_dmi_field("product_name");
    if product_name.is_empty() || product_name == "Inconnu" {
        product_name = read_dmi_field("board_name");
    }
    let board_name = read_dmi_field("board_name");
    let bios_version = read_dmi_field("bios_version");
    let bios_date = read_dmi_field("bios_date");

    let motherboard = MotherboardInfo {
        vendor: if vendor.is_empty() || vendor == "Inconnu" { "Carte Mère Hôte".to_string() } else { vendor },
        product_name: if product_name.is_empty() || product_name == "Inconnu" { "Plateforme Serveur / NAS".to_string() } else { product_name },
        board_name: if board_name.is_empty() || board_name == "Inconnu" { "Format standard".to_string() } else { board_name },
        chipset: "Contrôleur Système Hôte".to_string(),
        bios_version: if bios_version.is_empty() { "UEFI / BIOS".to_string() } else { bios_version },
        bios_date: if bios_date.is_empty() { "Inconnu".to_string() } else { bios_date },
    };

    // 3. Memory Info dynamique
    let mut total_kb: f64 = 8000000.0;
    let mut free_kb: f64 = 4000000.0;
    let mut avail_kb: f64 = 6000000.0;

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
        mem_type: "Mémoire Vive Système (RAM)".to_string(),
        channels: "Canal Mémoire Détecté".to_string(),
    };

    // 4. GPU Info & Render Node Detection dynamique
    let mut render_node = String::new();
    if Path::new("/dev/dri/renderD128").exists() {
        render_node = "/dev/dri/renderD128".to_string();
    } else if Path::new("/dev/dri").exists() {
        if let Ok(entries) = fs::read_dir("/dev/dri") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("renderD") {
                    render_node = format!("/dev/dri/{}", name);
                    break;
                }
            }
        }
    }

    let device_path = if !render_node.is_empty() {
        "/dev/dri:/dev/dri".to_string()
    } else if Path::new("/dev/nvidia0").exists() {
        "--gpus all".to_string()
    } else {
        "none".to_string()
    };

    // Détection réelle du GPU via PCI sysfs
    let mut gpu_model = "Contrôleur Graphique Intégré".to_string();
    let mut gpu_vendor = "Générique".to_string();
    let mut gpu_driver = "Direct Rendering Manager (DRM)".to_string();

    let pci_dir = Path::new("/sys/bus/pci/devices");
    if let Ok(entries) = fs::read_dir(pci_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let class = fs::read_to_string(p.join("class")).unwrap_or_default();
            if class.trim().starts_with("0x03") {
                let vendor_hex = fs::read_to_string(p.join("vendor")).unwrap_or_default();
                let device_hex = fs::read_to_string(p.join("device")).unwrap_or_default();

                let drv = p.join("driver")
                    .canonicalize()
                    .ok()
                    .and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "drm".to_string());

                gpu_driver = format!("{} (Noyau Linux)", drv);

                if vendor_hex.trim() == "0x1002" {
                    gpu_vendor = "Advanced Micro Devices (AMD)".to_string();
                    gpu_model = format!("AMD Radeon Graphics [{}]", device_hex.trim());
                    break;
                } else if vendor_hex.trim() == "0x8086" {
                    gpu_vendor = "Intel Corporation".to_string();
                    gpu_model = format!("Intel Graphics / Arc [{}]", device_hex.trim());
                    break;
                } else if vendor_hex.trim() == "0x10de" {
                    gpu_vendor = "NVIDIA Corporation".to_string();
                    gpu_model = format!("NVIDIA GeForce / RTX [{}]", device_hex.trim());
                    break;
                }
            }
        }
    }

    let gpu = GpuHardwareInfo {
        model: gpu_model,
        vendor: gpu_vendor,
        driver: gpu_driver,
        vram: "Allouée dynamiquement / Dédiée".to_string(),
        features: vec![
            "Accélération Matérielle DRM / DRI".to_string(),
            "Transcodage matériel / VA-API".to_string(),
        ],
        render_node,
        device_path,
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

            let is_physical = entry.path().join("device").exists();

            let controller_model = if iface.starts_with("enp7") || iface.starts_with("enp8") {
                "Realtek Semiconductor RTL8125 2.5GbE Controller".to_string()
            } else if iface.starts_with("docker") {
                "Pont Réseau Virtuel Docker".to_string()
            } else if iface.starts_with("eno") || iface.starts_with("eth") {
                "Contrôleur Réseau Ethernet Intel / Gigabit".to_string()
            } else if !is_physical {
                "Interface Réseau Virtuelle".to_string()
            } else {
                "Interface Réseau Ethernet".to_string()
            };

            network_adapters.push(NetworkAdapterInfo {
                interface_name: iface,
                controller_model,
                mac_address: mac,
                ipv4,
                speed_mbps: speed,
                is_up,
                is_physical,
            });
        }
    }

    // 6. Storage Controllers dynamiques
    let mut storage_controllers = Vec::new();
    if let Ok(entries) = fs::read_dir(pci_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let class = fs::read_to_string(p.join("class")).unwrap_or_default();
            if class.trim().starts_with("0x01") {
                let slot = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                let c_type = if class.trim().starts_with("0x0108") {
                    "NVMe PCIe Gen3/Gen4 Controller".to_string()
                } else if class.trim().starts_with("0x0106") {
                    "SATA AHCI Controller".to_string()
                } else {
                    "Mass Storage Controller".to_string()
                };

                storage_controllers.push(StorageControllerInfo {
                    name: format!("Contrôleur {}", c_type),
                    controller_type: c_type,
                    pci_slot: slot,
                });
            }
        }
    }
    if storage_controllers.is_empty() {
        storage_controllers.push(StorageControllerInfo {
            name: "Contrôleur de Stockage Système".to_string(),
            controller_type: "NVMe / SATA".to_string(),
            pci_slot: "00:00.0".to_string(),
        });
    }

    // 7. System Summary
    let kernel = fs::read_to_string("/proc/version")
        .map(|s| {
            let p: Vec<&str> = s.split_whitespace().collect();
            if p.len() >= 3 { p[2].to_string() } else { "Linux 7.2.5".to_string() }
        })
        .unwrap_or_else(|_| "Linux 7.2.5".to_string());

    let hostname = fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "noos-nas".to_string());

    let uptime = format_uptime();

    let system_summary = SystemSummaryInfo {
        os_name: "Noos NAS Edition".to_string(),
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
