use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// =========================================================================
// STRUCTURES DE DONNÉES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurvePoint {
    pub temp_c: f32,
    pub pwm_percent: u8, // 0 - 100 %
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FanProfile {
    Silent,
    Balanced,
    Performance,
    Freeze,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanDevice {
    pub id: String,                  // Empreinte unique: "nct6775_0000:00:1f.0_pwm1"
    pub chip_name: String,           // ex: "nct6775", "amdgpu", "it8728"
    pub device_path: String,         // Chemin canonique sysfs
    pub fan_index: u32,              // index 1, 2, ...
    pub label: String,               // ex: "Ventilateur CPU", "fan1"
    pub rpm: u32,                    // Vitesse actuelle en tours/minute
    pub pwm_percent: u8,             // 0 à 100 %
    pub pwm_raw: u8,                 // 0 à 255
    pub pwm_controllable: bool,      // pwmX accessible en écriture
    pub active_profile: FanProfile,
    pub assigned_sensor_id: String,  // ID de la sonde de référence
    pub custom_curve: Vec<CurvePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempSensorInfo {
    pub id: String,                  // Empreinte unique: "k10temp_0000:00:18.3_temp1"
    pub chip_name: String,           // ex: "k10temp", "coretemp", "amdgpu"
    pub temp_index: u32,             // index 1, 2...
    pub label: String,               // ex: "Tctl", "Package id 0", "GPU Edge"
    pub temp_c: f32,                 // Température actuelle
    pub crit_c: Option<f32>,         // Seuil critique si exposé par le pilote
    pub hardware_type: String,       // "CPU", "GPU", "Motherboard", "Storage", "Memory", "Unknown"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCoreMetric {
    pub core_id: usize,
    pub freq_ghz: f32,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuEnergyStatus {
    pub governor: String,
    pub available_governors: Vec<String>,
    pub epp: Option<String>,
    pub available_epp: Vec<String>,
    pub boost_enabled: Option<bool>,
    pub package_power_watts: Option<f32>,
    pub total_usage_percent: f32,
    pub cores: Vec<CpuCoreMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareMonitoringState {
    pub fans: Vec<FanDevice>,
    pub temperatures: Vec<TempSensorInfo>,
    pub cpu: CpuEnergyStatus,
    pub last_updated_secs: u64,
}

// =========================================================================
// CONFIGURATION PERSISTANTE
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardwarePersistentConfig {
    pub fan_configs: HashMap<String, FanSavedConfig>,
    pub preferred_cpu_governor: Option<String>,
    pub preferred_cpu_epp: Option<String>,
    pub preferred_cpu_boost: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanSavedConfig {
    pub custom_label: Option<String>,
    pub profile: FanProfile,
    pub assigned_sensor_id: String,
    pub custom_curve: Vec<CurvePoint>,
}

// =========================================================================
// COURBES PRÉDÉFINIES
// =========================================================================

pub fn get_preset_curve(profile: &FanProfile) -> Vec<CurvePoint> {
    match profile {
        FanProfile::Silent => vec![
            CurvePoint { temp_c: 35.0, pwm_percent: 20 },
            CurvePoint { temp_c: 50.0, pwm_percent: 30 },
            CurvePoint { temp_c: 65.0, pwm_percent: 50 },
            CurvePoint { temp_c: 75.0, pwm_percent: 75 },
            CurvePoint { temp_c: 82.0, pwm_percent: 100 },
        ],
        FanProfile::Balanced => vec![
            CurvePoint { temp_c: 30.0, pwm_percent: 30 },
            CurvePoint { temp_c: 50.0, pwm_percent: 50 },
            CurvePoint { temp_c: 65.0, pwm_percent: 70 },
            CurvePoint { temp_c: 75.0, pwm_percent: 85 },
            CurvePoint { temp_c: 80.0, pwm_percent: 100 },
        ],
        FanProfile::Performance => vec![
            CurvePoint { temp_c: 30.0, pwm_percent: 50 },
            CurvePoint { temp_c: 45.0, pwm_percent: 70 },
            CurvePoint { temp_c: 60.0, pwm_percent: 85 },
            CurvePoint { temp_c: 70.0, pwm_percent: 100 },
        ],
        FanProfile::Freeze => vec![
            CurvePoint { temp_c: 0.0, pwm_percent: 100 },
            CurvePoint { temp_c: 100.0, pwm_percent: 100 },
        ],
        FanProfile::Custom => vec![
            CurvePoint { temp_c: 35.0, pwm_percent: 25 },
            CurvePoint { temp_c: 55.0, pwm_percent: 45 },
            CurvePoint { temp_c: 70.0, pwm_percent: 75 },
            CurvePoint { temp_c: 80.0, pwm_percent: 100 },
        ],
    }
}

// Calcule le % PWM par interpolation linéaire
pub fn interpolate_pwm(curve: &[CurvePoint], current_temp: f32) -> u8 {
    if curve.is_empty() {
        return 50;
    }

    // Failsafe critique universel STEvE_OS : à 80°C minimum 85%, à 85°C 100%
    if current_temp >= 85.0 {
        return 100;
    }

    if current_temp <= curve[0].temp_c {
        return curve[0].pwm_percent;
    }
    if current_temp >= curve[curve.len() - 1].temp_c {
        return curve[curve.len() - 1].pwm_percent;
    }

    for i in 0..curve.len() - 1 {
        let p1 = &curve[i];
        let p2 = &curve[i + 1];
        if current_temp >= p1.temp_c && current_temp <= p2.temp_c {
            let temp_diff = p2.temp_c - p1.temp_c;
            if temp_diff <= 0.001 {
                return p1.pwm_percent;
            }
            let factor = (current_temp - p1.temp_c) / temp_diff;
            let pwm_diff = (p2.pwm_percent as f32) - (p1.pwm_percent as f32);
            let calculated = (p1.pwm_percent as f32) + (factor * pwm_diff);
            let res = calculated.round().clamp(0.0, 100.0) as u8;
            if current_temp >= 80.0 && res < 85 {
                return 85;
            }
            return res;
        }
    }

    50
}

// =========================================================================
// SCANNER MATÉRIEL HWMON & CPUFREQ
// =========================================================================

pub struct HardwareScanner;

impl HardwareScanner {
    pub fn scan_temperatures_and_fans() -> (Vec<TempSensorInfo>, Vec<FanDevice>) {
        let mut temperatures = Vec::new();
        let mut fans = Vec::new();

        let hwmon_root = Path::new("/sys/class/hwmon");
        if !hwmon_root.exists() {
            return (temperatures, fans);
        }

        let entries = match fs::read_dir(hwmon_root) {
            Ok(e) => e,
            Err(_) => return (temperatures, fans),
        };

        for entry in entries.flatten() {
            let hwmon_dir = entry.path();
            let chip_name = fs::read_to_string(hwmon_dir.join("name"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "hwmon".to_string());

            // Empreinte matérielle stable via le lien device ou le chemin sysfs
            let device_fingerprint = hwmon_dir.join("device")
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| hwmon_dir.to_string_lossy().to_string());

            let short_pci = device_fingerprint.split('/').last().unwrap_or(&chip_name).to_string();

            // 1. Détecter les capteurs de température
            for t_idx in 1..=16 {
                let temp_input_file = hwmon_dir.join(format!("temp{}_input", t_idx));
                if temp_input_file.exists() {
                    if let Ok(raw_str) = fs::read_to_string(&temp_input_file) {
                        if let Ok(raw_val) = raw_str.trim().parse::<f32>() {
                            let temp_c = (raw_val / 1000.0 * 10.0).round() / 10.0;
                            let label_file = hwmon_dir.join(format!("temp{}_label", t_idx));
                            let default_label = format!("{}_{}", chip_name, t_idx);
                            let label = fs::read_to_string(label_file)
                                .map(|s| s.trim().to_string())
                                .unwrap_or(default_label);

                            let crit_c = fs::read_to_string(hwmon_dir.join(format!("temp{}_crit", t_idx)))
                                .ok()
                                .and_then(|s| s.trim().parse::<f32>().ok())
                                .map(|v| v / 1000.0);

                            let hardware_type = match chip_name.as_str() {
                                "coretemp" | "k10temp" | "zenpower" => "CPU",
                                "amdgpu" | "nouveau" | "i915" | "xe" => "GPU",
                                "nvme" | "drivetemp" => "Storage",
                                "spd5118" | "jc42" => "Memory",
                                _ if chip_name.starts_with("nct") || chip_name.starts_with("it8") => "Motherboard",
                                _ => "Unknown",
                            }.to_string();

                            let sensor_id = format!("{}_{}_temp{}", chip_name, short_pci, t_idx);

                            temperatures.push(TempSensorInfo {
                                id: sensor_id,
                                chip_name: chip_name.clone(),
                                temp_index: t_idx,
                                label,
                                temp_c,
                                crit_c,
                                hardware_type,
                            });
                        }
                    }
                }
            }

            // 2. Détecter les ventilateurs et canaux PWM
            for f_idx in 1..=8 {
                let fan_input_file = hwmon_dir.join(format!("fan{}_input", f_idx));
                let pwm_file = hwmon_dir.join(format!("pwm{}", f_idx));

                if fan_input_file.exists() || pwm_file.exists() {
                    let rpm = fs::read_to_string(&fan_input_file)
                        .ok()
                        .and_then(|s| s.trim().parse::<u32>().ok())
                        .unwrap_or(0);

                    let (pwm_raw, pwm_percent, controllable) = if pwm_file.exists() {
                        let raw = fs::read_to_string(&pwm_file)
                            .ok()
                            .and_then(|s| s.trim().parse::<u8>().ok())
                            .unwrap_or(0);
                        let pct = ((raw as f32 / 255.0) * 100.0).round() as u8;
                        (raw, pct, true)
                    } else {
                        (0, 0, false)
                    };

                    let default_label = format!("Ventilateur {} ({})", f_idx, chip_name);
                    let label = fs::read_to_string(hwmon_dir.join(format!("fan{}_label", f_idx)))
                        .map(|s| s.trim().to_string())
                        .unwrap_or(default_label);

                    let fan_id = format!("{}_{}_fan{}", chip_name, short_pci, f_idx);

                    fans.push(FanDevice {
                        id: fan_id,
                        chip_name: chip_name.clone(),
                        device_path: hwmon_dir.to_string_lossy().to_string(),
                        fan_index: f_idx,
                        label,
                        rpm,
                        pwm_percent,
                        pwm_raw,
                        pwm_controllable: controllable,
                        active_profile: FanProfile::Silent,
                        assigned_sensor_id: String::new(),
                        custom_curve: get_preset_curve(&FanProfile::Silent),
                    });
                }
            }
        }

        // Tri ergonomique : CPU d'abord, GPU ensuite, puis carte mère et disques
        temperatures.sort_by_key(|t| match t.hardware_type.as_str() {
            "CPU" => 0,
            "GPU" => 1,
            "Motherboard" => 2,
            "Storage" => 3,
            "Memory" => 4,
            _ => 5,
        });

        (temperatures, fans)
    }

    pub fn scan_cpu_energy() -> CpuEnergyStatus {
        let cpu0_cpufreq = Path::new("/sys/devices/system/cpu/cpu0/cpufreq");

        // 1. Gouverneur actuel et disponibles
        let governor = fs::read_to_string(cpu0_cpufreq.join("scaling_governor"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "ondemand".to_string());

        let available_governors = fs::read_to_string(cpu0_cpufreq.join("scaling_available_governors"))
            .map(|s| s.split_whitespace().map(|g| g.to_string()).collect())
            .unwrap_or_else(|_| vec!["powersave".into(), "performance".into(), "schedutil".into()]);

        // 2. EPP (Energy Performance Preference)
        let epp = fs::read_to_string(cpu0_cpufreq.join("energy_performance_preference"))
            .ok()
            .map(|s| s.trim().to_string());

        let available_epp = fs::read_to_string(cpu0_cpufreq.join("energy_performance_available_preferences"))
            .map(|s| s.split_whitespace().map(|p| p.to_string()).collect())
            .unwrap_or_default();

        // 3. Boost Turbo
        let boost_enabled = fs::read_to_string("/sys/devices/system/cpu/cpufreq/boost")
            .or_else(|_| fs::read_to_string(cpu0_cpufreq.join("boost")))
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .map(|v| v == 1);

        // 4. Fréquences par cœur
        let mut cores = Vec::new();
        let cpu_dir = Path::new("/sys/devices/system/cpu");
        if let Ok(entries) = fs::read_dir(cpu_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(core_id) = name[3..].parse::<usize>() {
                        let cur_freq_file = entry.path().join("cpufreq/scaling_cur_freq");
                        let freq_ghz = fs::read_to_string(cur_freq_file)
                            .ok()
                            .and_then(|s| s.trim().parse::<f32>().ok())
                            .map(|khz| (khz / 1_000_000.0 * 100.0).round() / 100.0)
                            .unwrap_or(0.0);

                        cores.push(CpuCoreMetric {
                            core_id,
                            freq_ghz,
                            usage_percent: 0.0, // calculé par delta si besoin
                        });
                    }
                }
            }
        }
        cores.sort_by_key(|c| c.core_id);

        // 5. RAPL Package Power (Watts)
        let package_power_watts = Self::read_rapl_watts();

        CpuEnergyStatus {
            governor,
            available_governors,
            epp,
            available_epp,
            boost_enabled,
            package_power_watts,
            total_usage_percent: 0.0,
            cores,
        }
    }

    fn read_rapl_watts() -> Option<f32> {
        let rapl_file = Path::new("/sys/class/powercap/intel-rapl:0/energy_uj");
        if !rapl_file.exists() {
            return None;
        }

        let e1 = fs::read_to_string(rapl_file).ok()?.trim().parse::<u64>().ok()?;
        std::thread::sleep(Duration::from_millis(150));
        let e2 = fs::read_to_string(rapl_file).ok()?.trim().parse::<u64>().ok()?;

        if e2 > e1 {
            let delta_uj = e2 - e1;
            // Joules par seconde (Watts) sur 150ms: delta_uj / 150_000
            let watts = (delta_uj as f32) / 150_000.0;
            Some((watts * 10.0).round() / 10.0)
        } else {
            None
        }
    }
}

// =========================================================================
// GESTIONNAIRE THERMIQUE & DÉMON DE RÉGULATION
// =========================================================================

pub struct ThermalManager {
    config_file_path: PathBuf,
    config: RwLock<HardwarePersistentConfig>,
    is_pulsing: AtomicBool,
}

static INSTANCE: tokio::sync::OnceCell<Arc<ThermalManager>> = tokio::sync::OnceCell::const_new();

impl ThermalManager {
    pub async fn global() -> Arc<Self> {
        INSTANCE.get_or_init(|| async {
            Arc::new(Self::init())
        }).await.clone()
    }

    fn init() -> Self {
        let config_file_path = if Path::new("/etc/nixos").is_dir() {
            PathBuf::from("/etc/nixos/steveos-hardware.json")
        } else {
            PathBuf::from("steveos-hardware.json")
        };

        let config = if let Ok(data) = fs::read_to_string(&config_file_path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HardwarePersistentConfig::default()
        };

        Self {
            config_file_path,
            config: RwLock::new(config),
            is_pulsing: AtomicBool::new(false),
        }
    }

    pub async fn save_config(&self) {
        let cfg = self.config.read().await;
        if let Ok(json) = serde_json::to_string_pretty(&*cfg) {
            let _ = fs::write(&self.config_file_path, json);
        }
    }

    // Récupère l'état complet actuel (sondes, ventilateurs, CPU)
    pub async fn get_monitoring_state(&self) -> HardwareMonitoringState {
        let (temperatures, mut fans) = HardwareScanner::scan_temperatures_and_fans();
        let cpu = HardwareScanner::scan_cpu_energy();

        let cfg = self.config.read().await;

        // Fusionner avec la configuration sauvegardée
        for fan in &mut fans {
            if let Some(saved) = cfg.fan_configs.get(&fan.id) {
                if let Some(lbl) = &saved.custom_label {
                    fan.label = lbl.clone();
                }
                fan.active_profile = saved.profile.clone();
                fan.assigned_sensor_id = saved.assigned_sensor_id.clone();
                fan.custom_curve = saved.custom_curve.clone();
            } else {
                // Affectation automatique par défaut : chercher le CPU ou le GPU
                if fan.assigned_sensor_id.is_empty() {
                    if let Some(cpu_sensor) = temperatures.iter().find(|t| t.hardware_type == "CPU") {
                        fan.assigned_sensor_id = cpu_sensor.id.clone();
                    } else if let Some(first) = temperatures.first() {
                        fan.assigned_sensor_id = first.id.clone();
                    }
                }
            }
        }

        HardwareMonitoringState {
            fans,
            temperatures,
            cpu,
            last_updated_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    // Définir un profil de ventilation
    pub async fn set_fan_profile(&self, fan_id: &str, profile: FanProfile) -> Result<(), String> {
        let mut cfg = self.config.write().await;
        let entry = cfg.fan_configs.entry(fan_id.to_string()).or_insert_with(|| FanSavedConfig {
            custom_label: None,
            profile: FanProfile::Silent,
            assigned_sensor_id: String::new(),
            custom_curve: get_preset_curve(&FanProfile::Silent),
        });

        entry.profile = profile.clone();
        if profile != FanProfile::Custom {
            entry.custom_curve = get_preset_curve(&profile);
        }

        drop(cfg);
        self.save_config().await;

        // Si profil Freeze, appliquer immédiatement 100%
        if profile == FanProfile::Freeze {
            self.apply_fan_direct_percent(fan_id, 100).await?;
        }

        Ok(())
    }

    // Définir une courbe personnalisée
    pub async fn set_fan_curve(&self, fan_id: &str, curve: Vec<CurvePoint>, sensor_id: Option<String>) -> Result<(), String> {
        let mut cfg = self.config.write().await;
        let entry = cfg.fan_configs.entry(fan_id.to_string()).or_insert_with(|| FanSavedConfig {
            custom_label: None,
            profile: FanProfile::Custom,
            assigned_sensor_id: String::new(),
            custom_curve: curve.clone(),
        });

        entry.profile = FanProfile::Custom;
        entry.custom_curve = curve;
        if let Some(sid) = sensor_id {
            entry.assigned_sensor_id = sid;
        }

        drop(cfg);
        self.save_config().await;
        Ok(())
    }

    // Tester un ventilateur par impulsion (2.5 secondes à 100%)
    pub async fn test_fan_pulse(&self, fan_id: &str) -> Result<u32, String> {
        if self.is_pulsing.swap(true, Ordering::SeqCst) {
            return Err("Un test est déjà en cours".to_string());
        }

        let state = self.get_monitoring_state().await;
        let fan = state.fans.iter().find(|f| f.id == fan_id)
            .ok_or_else(|| "Ventilateur introuvable".to_string())?;

        let initial_rpm = fan.rpm;
        let _ = self.apply_fan_direct_percent(fan_id, 100).await;

        tokio::time::sleep(Duration::from_millis(2500)).await;

        let state_after = self.get_monitoring_state().await;
        let max_rpm = state_after.fans.iter().find(|f| f.id == fan_id).map(|f| f.rpm).unwrap_or(initial_rpm);

        self.is_pulsing.store(false, Ordering::SeqCst);
        let _ = self.tick_regulation_cycle().await;

        Ok(max_rpm)
    }

    // Appliquer directement un pourcentage PWM sur le matériel
    pub async fn apply_fan_direct_percent(&self, fan_id: &str, percent: u8) -> Result<(), String> {
        let state = self.get_monitoring_state().await;
        let fan = state.fans.iter().find(|f| f.id == fan_id)
            .ok_or_else(|| "Ventilateur introuvable".to_string())?;

        let hwmon_dir = Path::new(&fan.device_path);
        let pwm_file = hwmon_dir.join(format!("pwm{}", fan.fan_index));
        let pwm_enable_file = hwmon_dir.join(format!("pwm{}_enable", fan.fan_index));

        if !pwm_file.exists() {
            return Err("Ce ventilateur n'est pas régulable par PWM".to_string());
        }

        // 1. Activer le mode manuel si disponible (1 = Manuel)
        if pwm_enable_file.exists() {
            let _ = fs::write(&pwm_enable_file, "1\n");
        }

        // 2. Écrire la consigne 0 - 255
        let raw_val = ((percent.clamp(0, 100) as f32 / 100.0) * 255.0).round() as u8;
        fs::write(&pwm_file, format!("{}\n", raw_val))
            .map_err(|e| format!("Erreur d'écriture PWM sysfs : {}", e))?;

        Ok(())
    }

    // Cycle d'asservissement thermique exécuté toutes les 2 secondes
    pub async fn tick_regulation_cycle(&self) -> Result<(), String> {
        if self.is_pulsing.load(Ordering::SeqCst) {
            return Ok(()); // Ne pas interférer pendant un test
        }

        let state = self.get_monitoring_state().await;
        let temp_map: HashMap<String, f32> = state.temperatures.iter()
            .map(|t| (t.id.clone(), t.temp_c))
            .collect();

        for fan in state.fans {
            if !fan.pwm_controllable {
                continue;
            }

            // Récupérer la température de référence assignée
            let source_temp = temp_map.get(&fan.assigned_sensor_id)
                .cloned()
                .or_else(|| {
                    // Fallback sur la température maximale de la machine
                    state.temperatures.iter().map(|t| t.temp_c).max_by(|a, b| a.partial_cmp(b).unwrap())
                })
                .unwrap_or(45.0);

            let target_pwm = match fan.active_profile {
                FanProfile::Freeze => 100,
                FanProfile::Silent => interpolate_pwm(&get_preset_curve(&FanProfile::Silent), source_temp),
                FanProfile::Balanced => interpolate_pwm(&get_preset_curve(&FanProfile::Balanced), source_temp),
                FanProfile::Performance => interpolate_pwm(&get_preset_curve(&FanProfile::Performance), source_temp),
                FanProfile::Custom => interpolate_pwm(&fan.custom_curve, source_temp),
            };

            let _ = self.apply_fan_direct_percent(&fan.id, target_pwm).await;
        }

        Ok(())
    }

    // Gestion du profil CPU (gouverneur, EPP, boost)
    pub async fn apply_cpu_profile(&self, governor: Option<String>, epp: Option<String>, boost: Option<bool>) -> Result<(), String> {
        let cpu_dir = Path::new("/sys/devices/system/cpu");

        if let Some(gov) = governor {
            if let Ok(entries) = fs::read_dir(cpu_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit()) {
                        let gov_file = entry.path().join("cpufreq/scaling_governor");
                        if gov_file.exists() {
                            let _ = fs::write(gov_file, format!("{}\n", gov));
                        }
                    }
                }
            }
        }

        if let Some(epp_val) = epp {
            if let Ok(entries) = fs::read_dir(cpu_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit()) {
                        let epp_file = entry.path().join("cpufreq/energy_performance_preference");
                        if epp_file.exists() {
                            let _ = fs::write(epp_file, format!("{}\n", epp_val));
                        }
                    }
                }
            }
        }

        if let Some(b) = boost {
            let val = if b { "1\n" } else { "0\n" };
            let _ = fs::write("/sys/devices/system/cpu/cpufreq/boost", val)
                .or_else(|_| fs::write("/sys/devices/system/cpu/cpu0/cpufreq/boost", val));
        }

        Ok(())
    }
}

// Démon de régulation thermique en tâche de fond (2 secondes)
pub fn start_thermal_daemon() {
    tokio::spawn(async move {
        // Laisser 5 secondes au démarrage du système
        tokio::time::sleep(Duration::from_secs(5)).await;
        let manager = ThermalManager::global().await;
        let mut interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            interval.tick().await;
            let _ = manager.tick_regulation_cycle().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolation() {
        let curve = get_preset_curve(&FanProfile::Silent);
        let pwm_low = interpolate_pwm(&curve, 30.0);
        assert_eq!(pwm_low, 20);
        let pwm_high = interpolate_pwm(&curve, 86.0);
        assert_eq!(pwm_high, 100);
    }
}
