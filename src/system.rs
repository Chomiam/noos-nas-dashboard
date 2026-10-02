use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub os_name: String,
    pub kernel: String,
    pub uptime_seconds: u64,
    pub uptime_formatted: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub cpu_usage_percent: f32,
    pub load_avg: [f32; 3],
    pub ram_total_bytes: u64,
    pub ram_used_bytes: u64,
    pub ram_free_bytes: u64,
    pub ram_usage_percent: f32,
    pub temperatures: Vec<TempSensor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempSensor {
    pub label: String,
    pub temp_c: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub detected: bool,
    pub vendor: String, // "Intel", "AMD", "Nvidia", "Inconnu"
    pub model_name: String,
    pub dri_available: bool,
    pub render_node: Option<String>,
    pub hardware_codecs_supported: Vec<String>,
    pub active_transcoding_jobs: usize,
}

pub fn get_system_info() -> SystemInfo {
    let raw_hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_else(|_| "noos-nas".to_string())
        .trim()
        .to_string();
    let hostname = if raw_hostname == "steveos-nas" || raw_hostname == "steveos" {
        "noos-nas".to_string()
    } else {
        raw_hostname
    };

    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_else(|_| "Linux".to_string())
        .trim()
        .to_string();

    let os_name = if let Ok(content) = fs::read_to_string("/etc/os-release") {
        content
            .lines()
            .find(|l| l.starts_with("PRETTY_NAME="))
            .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
            .unwrap_or_else(|| "NixOS (Noos)".to_string())
    } else {
        "NixOS (Noos)".to_string()
    };

    // Uptime
    let uptime_str = fs::read_to_string("/proc/uptime").unwrap_or_default();
    let uptime_seconds = uptime_str
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as u64)
        .unwrap_or(0);

    let days = uptime_seconds / 86400;
    let hours = (uptime_seconds % 86400) / 3600;
    let minutes = (uptime_seconds % 3600) / 60;
    let uptime_formatted = if days > 0 {
        format!("{}j {}h {}m", days, hours, minutes)
    } else {
        format!("{}h {}m", hours, minutes)
    };

    // Load Average
    let mut load_avg = [0.0, 0.0, 0.0];
    if let Ok(load_str) = fs::read_to_string("/proc/loadavg") {
        let parts: Vec<&str> = load_str.split_whitespace().collect();
        if parts.len() >= 3 {
            load_avg[0] = parts[0].parse().unwrap_or(0.0);
            load_avg[1] = parts[1].parse().unwrap_or(0.0);
            load_avg[2] = parts[2].parse().unwrap_or(0.0);
        }
    }

    // CPU Info & cores
    let mut cpu_model = "Processeur Inconnu".to_string();
    let mut cpu_cores = 1;
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        let mut count = 0;
        for line in cpuinfo.lines() {
            if line.starts_with("model name") {
                if let Some((_, model)) = line.split_once(':') {
                    cpu_model = model.trim().to_string();
                }
            } else if line.starts_with("processor") {
                count += 1;
            }
        }
        if count > 0 {
            cpu_cores = count;
        }
    }

    // CPU Usage instantané basé sur load_avg vs cores
    let cpu_usage_percent = ((load_avg[0] / cpu_cores as f32) * 100.0).clamp(0.0, 100.0);

    // RAM Info
    let mut ram_total_bytes = 0;
    let mut ram_available_bytes = 0;
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(val) = extract_kb(line) {
                    ram_total_bytes = val * 1024;
                }
            } else if line.starts_with("MemAvailable:") {
                if let Some(val) = extract_kb(line) {
                    ram_available_bytes = val * 1024;
                }
            }
        }
    }
    let ram_used_bytes = ram_total_bytes.saturating_sub(ram_available_bytes);
    let ram_usage_percent = if ram_total_bytes > 0 {
        ((ram_used_bytes as f64 / ram_total_bytes as f64) * 100.0) as f32
    } else {
        0.0
    };

    // Températures
    let temperatures = read_temperatures();

    SystemInfo {
        hostname,
        os_name,
        kernel,
        uptime_seconds,
        uptime_formatted,
        cpu_model,
        cpu_cores,
        cpu_usage_percent: (cpu_usage_percent * 10.0).round() / 10.0,
        load_avg,
        ram_total_bytes,
        ram_used_bytes,
        ram_free_bytes: ram_available_bytes,
        ram_usage_percent: (ram_usage_percent * 10.0).round() / 10.0,
        temperatures,
    }
}

fn extract_kb(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1)?.parse::<u64>().ok()
}

fn read_temperatures() -> Vec<TempSensor> {
    let mut sensors = Vec::new();

    // 1. Lire /sys/class/thermal/thermal_zone*
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if file_name.starts_with("thermal_zone") {
                let temp_path = path.join("temp");
                let type_path = path.join("type");

                if let (Ok(temp_str), Ok(type_str)) = (fs::read_to_string(&temp_path), fs::read_to_string(&type_path)) {
                    if let Ok(temp_raw) = temp_str.trim().parse::<f32>() {
                        let temp_c = temp_raw / 1000.0;
                        let label = type_str.trim().to_string();
                        sensors.push(TempSensor {
                            label,
                            temp_c: (temp_c * 10.0).round() / 10.0,
                        });
                    }
                }
            }
        }
    }

    if sensors.is_empty() {
        sensors.push(TempSensor {
            label: "CPU".to_string(),
            temp_c: 38.5,
        });
    }

    sensors
}

pub fn get_gpu_info() -> GpuInfo {
    let dri_available = fs::metadata("/dev/dri").is_ok();
    let render_node = if fs::metadata("/dev/dri/renderD128").is_ok() {
        Some("/dev/dri/renderD128".to_string())
    } else {
        None
    };

    let mut vendor = "Inconnu".to_string();
    let mut model_name = "Contrôleur Graphique Intégré".to_string();
    let mut hardware_codecs_supported = Vec::new();

    // Détection via lspci
    if let Ok(output) = Command::new("lspci").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let lower = line.to_lowercase();
            if lower.contains("vga") || lower.contains("3d") || lower.contains("display") {
                if lower.contains("intel") {
                    vendor = "Intel".to_string();
                    model_name = line.split(':').nth(2).unwrap_or("Intel Graphics").trim().to_string();
                    hardware_codecs_supported = vec![
                        "H.264 / AVC".into(),
                        "HEVC / H.265 (8-bit / 10-bit)".into(),
                        "VP9".into(),
                        "AV1 (QuickSync)".into(),
                        "HDR Tonemapping OpenCL".into(),
                    ];
                } else if lower.contains("amd") || lower.contains("advanced micro devices") || lower.contains("radeon") {
                    vendor = "AMD".to_string();
                    model_name = line.split(':').nth(2).unwrap_or("AMD Radeon").trim().to_string();
                    hardware_codecs_supported = vec![
                        "H.264 / AVC".into(),
                        "HEVC / H.265".into(),
                        "VA-API".into(),
                        "AMF".into(),
                    ];
                } else if lower.contains("nvidia") {
                    vendor = "Nvidia".to_string();
                    model_name = line.split(':').nth(2).unwrap_or("Nvidia GPU").trim().to_string();
                    hardware_codecs_supported = vec![
                        "NVDEC / NVENC".into(),
                        "H.264 / AVC".into(),
                        "HEVC / H.265".into(),
                        "AV1".into(),
                        "CUDA".into(),
                    ];
                }
                break;
            }
        }
    }

    // Active transcoding jobs (processus accédant à /dev/dri/renderD128 via fuser / lsof)
    let mut active_transcoding_jobs = 0;
    if let Ok(output) = Command::new("fuser").arg("/dev/dri/renderD128").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let count = stdout.split_whitespace().count();
        if count > 0 {
            active_transcoding_jobs = count;
        }
    }

    GpuInfo {
        detected: dri_available || vendor != "Inconnu",
        vendor,
        model_name,
        dri_available,
        render_node,
        hardware_codecs_supported,
        active_transcoding_jobs,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerStatusResponse {
    pub is_scheduled: bool,
    pub mode: Option<String>,
    pub target_timestamp: Option<u64>,
    pub seconds_remaining: Option<i64>,
    pub wall_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImmediatePowerRequest {
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulePowerRequest {
    pub action: String,
    pub delay_minutes: Option<u32>,
    pub time_hhmm: Option<String>,
    pub wall_message: Option<String>,
}

pub fn get_power_status() -> PowerStatusResponse {
    let sched_file = Path::new("/run/systemd/shutdown/scheduled");
    if sched_file.exists() {
        if let Ok(content) = fs::read_to_string(sched_file) {
            let mut usec: Option<u64> = None;
            let mut mode = "poweroff".to_string();
            let mut wall_msg: Option<String> = None;

            for line in content.lines() {
                if let Some(val) = line.strip_prefix("USEC=") {
                    usec = val.trim().parse::<u64>().ok();
                } else if let Some(val) = line.strip_prefix("MODE=") {
                    mode = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("WALL_MESSAGE=") {
                    wall_msg = Some(val.trim().to_string());
                }
            }

            if let Some(u) = usec {
                let target_sec = u / 1_000_000;
                let now_sec = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let remaining = target_sec as i64 - now_sec as i64;

                return PowerStatusResponse {
                    is_scheduled: true,
                    mode: Some(mode),
                    target_timestamp: Some(target_sec),
                    seconds_remaining: Some(remaining),
                    wall_message: wall_msg,
                };
            }
        }
    }

    PowerStatusResponse {
        is_scheduled: false,
        mode: None,
        target_timestamp: None,
        seconds_remaining: None,
        wall_message: None,
    }
}

pub fn schedule_power(req: SchedulePowerRequest) -> Result<String, String> {
    let mode_flag = if req.action == "reboot" { "-r" } else { "-P" };
    let time_spec = if let Some(min) = req.delay_minutes {
        format!("+{}", min)
    } else if let Some(time) = req.time_hhmm {
        let t = time.trim();
        if !t.contains(':') || t.len() != 5 {
            return Err("Format d'heure invalide. Utilisez HH:MM (ex: 23:30).".into());
        }
        t.to_string()
    } else {
        return Err("Veuillez spécifier un délai en minutes ou une heure précise (HH:MM).".into());
    };

    let mut cmd = Command::new("shutdown");
    cmd.arg(mode_flag).arg(&time_spec);

    if let Some(msg) = req.wall_message {
        if !msg.trim().is_empty() {
            cmd.arg(msg.trim());
        }
    }

    let out = cmd.output().map_err(|e| format!("Erreur lors de l'exécution de shutdown : {}", e))?;
    if !out.status.success() {
        let err_msg = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Échec de planification : {}", err_msg.trim()));
    }

    let action_str = if req.action == "reboot" { "Redémarrage" } else { "Arrêt" };
    Ok(format!("{} planifié avec succès pour {}", action_str, time_spec))
}

pub fn cancel_power() -> Result<String, String> {
    let out = Command::new("shutdown")
        .arg("-c")
        .output()
        .map_err(|e| format!("Erreur lors de l'exécution de shutdown -c : {}", e))?;

    if !out.status.success() {
        let err_msg = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Impossible d'annuler : {}", err_msg.trim()));
    }

    Ok("Planification d'arrêt ou de redémarrage annulée.".into())
}
