use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path as StdPath, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualMachine {
    pub id: String,
    pub name: String,
    pub state: String,
    pub vcpus: u32,
    pub memory_mb: u64,
    pub disk_size_gb: f64,
    pub disk_path: String,
    pub os_type: String,
    pub network_type: String,
    pub vnc_port: Option<u16>,
    pub autostart: bool,
    pub gpu_passthrough: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVmRequest {
    pub name: String,
    pub vcpus: u32,
    pub memory_mb: u64,
    pub disk_size_gb: u64,
    pub os_type: String,
    pub iso_path: Option<String>,
    pub network_type: String,
    pub gpu_pci: Option<String>,
    pub enable_tpm: Option<bool>,
    pub enable_uefi: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct VmActionRequest {
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub modified: u64,
}

#[derive(Debug, Deserialize)]
pub struct IsoDownloadRequest {
    pub url: String,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoDownloadJob {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub progress: f64,
    pub speed_mbps: f64,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDeviceInfo {
    pub pci_address: String,
    pub name: String,
    pub vendor: String,
    pub driver: String,
    pub iommu_group: Option<u32>,
    pub is_in_use_by_jellyfin: bool,
    pub is_in_use_by_docker: bool,
    pub is_primary_display: bool,
    pub conflict_warning: Option<String>,
}

type IsoJobStore = Arc<RwLock<HashMap<String, IsoDownloadJob>>>;



// Global in-memory storage for ISO download jobs
static ISO_JOBS: std::sync::OnceLock<IsoJobStore> = std::sync::OnceLock::new();

pub fn get_iso_job_store() -> IsoJobStore {
    ISO_JOBS.get_or_init(|| Arc::new(RwLock::new(HashMap::new()))).clone()
}


pub fn get_libvirt_uri() -> String {
    for sock in &["/run/libvirt/libvirt-sock", "/var/run/libvirt/libvirt-sock"] {
        if StdPath::new(sock).exists() {
            return format!("qemu:///system?socket={}", sock);
        }
    }
    "qemu:///system".to_string()
}

pub fn get_all_devices_in_iommu_group(pci_address: &str) -> Vec<String> {
    let pci_path = StdPath::new("/sys/bus/pci/devices").join(pci_address);
    let iommu_group_link = pci_path.join("iommu_group");
    if let Ok(target) = fs::read_link(&iommu_group_link) {
        if let Some(group_name) = target.file_name() {
            let group_devices_dir = StdPath::new("/sys/kernel/iommu_groups").join(group_name).join("devices");
            if let Ok(entries) = fs::read_dir(group_devices_dir) {
                let mut dev_list = Vec::new();
                for entry in entries.flatten() {
                    let dev_name = entry.file_name().to_string_lossy().to_string();
                    dev_list.push(dev_name);
                }
                dev_list.sort();
                return dev_list;
            }
        }
    }
    vec![pci_address.to_string()]
}

pub fn get_virsh_bin() -> String {
    for p in &["/run/current-system/sw/bin/virsh", "/usr/bin/virsh", "virsh"] {
        if StdPath::new(p).exists() {
            return p.to_string();
        }
    }
    "virsh".to_string()
}

pub fn get_virt_install_bin() -> String {
    for p in &["/run/current-system/sw/bin/virt-install", "/usr/bin/virt-install", "virt-install"] {
        if StdPath::new(p).exists() {
            return p.to_string();
        }
    }
    "virt-install".to_string()
}

pub fn get_qemu_img_bin() -> String {
    for p in &["/run/current-system/sw/bin/qemu-img", "/usr/bin/qemu-img", "qemu-img"] {
        if StdPath::new(p).exists() {
            return p.to_string();
        }
    }
    "qemu-img".to_string()
}

pub fn get_vms_pool_dir() -> PathBuf {
    for p in &["/mnt/storage/vms", "/storage/vms", "/var/lib/noos/vms", "/var/lib/steveos/vms"] {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            return pb;
        }
    }
    let default_path = PathBuf::from("/mnt/storage/vms");
    let _ = fs::create_dir_all(&default_path);
    default_path
}

pub fn get_isos_dir() -> PathBuf {
    for p in &["/mnt/storage/isos", "/storage/isos", "/var/lib/noos/isos", "/var/lib/steveos/isos"] {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            return pb;
        }
    }
    let default_path = PathBuf::from("/mnt/storage/isos");
    let _ = fs::create_dir_all(&default_path);
    default_path
}

pub fn ensure_default_nat_network() {
    let virsh = get_virsh_bin();
    let check = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "net-info", "default"]).output();
    if let Ok(out) = check {
        if out.status.success() {
            let txt = String::from_utf8_lossy(&out.stdout);
            if txt.contains("Active:         no") {
                let _ = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "net-start", "default"]).output();
            }
            return;
        }
    }

    let xml = r#"<network>
  <name>default</name>
  <forward mode='nat'/>
  <bridge name='virbr0' stp='on' delay='0'/>
  <ip address='192.168.122.1' netmask='255.255.255.0'>
    <dhcp>
      <range start='192.168.122.2' end='192.168.122.254'/>
    </dhcp>
  </ip>
</network>"#;
    let tmp_path = "/tmp/libvirt_default_net.xml";
    if fs::write(tmp_path, xml).is_ok() {
        let _ = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "net-define", tmp_path]).output();
        let _ = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "net-autostart", "default"]).output();
        let _ = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "net-start", "default"]).output();
        let _ = fs::remove_file(tmp_path);
    }
}

pub fn list_vms() -> Vec<VirtualMachine> {
    ensure_default_nat_network();
    let virsh = get_virsh_bin();
    let mut vms = Vec::new();

    let output = match Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "list", "--all", "--name"]).output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
        Err(_) => return vms,
    };

    for line in output.lines() {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }

        let mut vm = VirtualMachine {
            id: name.to_string(),
            name: name.to_string(),
            state: "shut off".to_string(),
            vcpus: 1,
            memory_mb: 1024,
            disk_size_gb: 0.0,
            disk_path: String::new(),
            os_type: "linux".to_string(),
            network_type: "nat".to_string(),
            vnc_port: None,
            autostart: false,
            gpu_passthrough: None,
        };

        if let Ok(info_out) = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "dominfo", name]).output() {
            let info_txt = String::from_utf8_lossy(&info_out.stdout);
            for info_line in info_txt.lines() {
                let parts: Vec<&str> = info_line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let val = parts[1].trim();
                    match key {
                        "State" => vm.state = val.to_string(),
                        "CPU(s)" => vm.vcpus = val.parse().unwrap_or(1),
                        "Max memory" => {
                            let kib: u64 = val.split_whitespace().next().unwrap_or("0").parse().unwrap_or(0);
                            vm.memory_mb = kib / 1024;
                        }
                        "Autostart" => vm.autostart = val == "enable",
                        _ => {}
                    }
                }
            }
        }

        if vm.state == "running" {
            vm.vnc_port = get_vm_vnc_port(name);
        }

        if let Ok(blk_out) = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "domblklist", name, "--details"]).output() {
            let blk_txt = String::from_utf8_lossy(&blk_out.stdout);
            for b_line in blk_txt.lines().skip(2) {
                let cols: Vec<&str> = b_line.split_whitespace().collect();
                if cols.len() >= 4 && cols[1] == "disk" {
                    let path = cols[3];
                    vm.disk_path = path.to_string();
                    if let Ok(meta) = fs::metadata(path) {
                        vm.disk_size_gb = (meta.len() as f64) / (1024.0 * 1024.0 * 1024.0);
                    }
                    break;
                }
            }
        }

        if let Ok(xml_out) = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "dumpxml", name]).output() {
            let xml = String::from_utf8_lossy(&xml_out.stdout);
            if xml.contains("bridge='br0'") || xml.contains("type='direct'") || xml.contains("mode='bridge'") {
                vm.network_type = "bridge".to_string();
            } else if xml.contains("network='default'") {
                vm.network_type = "nat".to_string();
            } else {
                vm.network_type = "isolated".to_string();
            }

            if xml.contains("windows") || name.to_lowercase().contains("win") {
                vm.os_type = "windows".to_string();
            }

            if xml.contains("<hostdev mode='subsystem' type='pci'") {
                vm.gpu_passthrough = Some("Attribué".to_string());
            }
        }

        vms.push(vm);
    }

    vms
}

pub fn get_vm_vnc_port(name: &str) -> Option<u16> {
    let virsh = get_virsh_bin();
    let out = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), "domdisplay", name]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();

    if text.contains(":") {
        let part = text.rsplit(':').next()?;
        if let Ok(p) = part.parse::<u16>() {
            if p >= 5900 {
                return Some(p);
            } else {
                return Some(5900 + p);
            }
        }
    }
    None
}

pub fn control_vm(name: &str, action: &str) -> Result<String, String> {
    let virsh = get_virsh_bin();
    let valid_name = name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    if !valid_name {
        return Err("Nom de machine virtuelle invalide.".to_string());
    }

    let cmd_arg = match action {
        "start" => "start",
        "shutdown" => "shutdown",
        "reset" => "reset",
        "destroy" => "destroy",
        "pause" => "suspend",
        "resume" => "resume",
        "delete" => {
            let uri = get_libvirt_uri();
            // 1. Forcer l'arrêt de la machine virtuelle si elle est en cours d'exécution
            let _ = Command::new(&virsh).args(["-c", &uri, "destroy", name]).output();

            // 2. Tenter la suppression complète déclarative (NVRAM/varstore + stockage + snapshots + TPM + managed-save)
            let mut out = Command::new(&virsh)
                .args(["-c", &uri, "undefine", name, "--remove-all-storage", "--nvram", "--snapshots-metadata", "--managed-save", "--tpm"])
                .output();

            // Repli sans l'option --tpm si non supportée par cette version de libvirt
            let mut success = out.as_ref().map(|o| o.status.success()).unwrap_or(false);
            if !success {
                out = Command::new(&virsh)
                    .args(["-c", &uri, "undefine", name, "--remove-all-storage", "--nvram", "--snapshots-metadata", "--managed-save"])
                    .output();
                success = out.as_ref().map(|o| o.status.success()).unwrap_or(false);
            }

            // Repli sans --remove-all-storage (si un lecteur CD-ROM/ISO attaché empêche la suppression du stockage global)
            if !success {
                out = Command::new(&virsh)
                    .args(["-c", &uri, "undefine", name, "--nvram", "--snapshots-metadata", "--managed-save"])
                    .output();
                success = out.as_ref().map(|o| o.status.success()).unwrap_or(false);
            }

            // Repli minimal ultime avec --nvram seul
            if !success {
                out = Command::new(&virsh)
                    .args(["-c", &uri, "undefine", name, "--nvram"])
                    .output();
                success = out.as_ref().map(|o| o.status.success()).unwrap_or(false);
            }

            // Nettoyage manuel du fichier disque principal .qcow2 si nécessaire
            let vms_dir = get_vms_pool_dir();
            let disk_path = vms_dir.join(format!("{}.qcow2", name));
            if disk_path.exists() {
                let _ = std::fs::remove_file(&disk_path);
            }

            match out {
                Ok(_) if success => {
                    return Ok(format!("Machine virtuelle '{}', NVRAM/varstore et disques associés supprimés avec succès.", name));
                }
                Ok(o) => {
                    let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    return Err(format!("Échec de la suppression de la VM '{}' : {}", name, err));
                }
                Err(e) => {
                    return Err(format!("Erreur lors de l'exécution de virsh : {}", e));
                }
            }
        }
        _ => return Err("Action inconnue (utilisez start, shutdown, reset, destroy, pause, resume, delete).".to_string()),
    };

    let out = Command::new(&virsh).args(&["-c", &get_libvirt_uri(), cmd_arg, name]).output()
        .map_err(|e| format!("Impossible d'exécuter l'action {} : {}", action, e))?;

    if out.status.success() {
        Ok(format!("Action '{}' exécutée avec succès sur la machine virtuelle '{}'.", action, name))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn get_bridge_network_target() -> String {
    // 1. Si un pont Linux br0 physique existe déjà sur l'hôte
    if StdPath::new("/sys/class/net/br0").exists() {
        return "bridge=br0,model=virtio".to_string();
    }

    // 2. Détecter l'interface physique active (ex: enp8s0) et utiliser un pont direct macvtap
    if let Ok(out) = Command::new("ip").args(["route", "show", "default"]).output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let words: Vec<&str> = text.split_whitespace().collect();
            for i in 0..words.len() {
                if words[i] == "dev" && i + 1 < words.len() {
                    let iface = words[i + 1];
                    if !iface.starts_with("virbr") && !iface.starts_with("docker") && !iface.starts_with("wg") {
                        return format!("type=direct,source={},source_mode=bridge,model=virtio", iface);
                    }
                }
            }
        }
    }

    // 3. Repli automatique sur le réseau NAT par défaut si aucune carte physique n'est trouvée
    "network=default,model=virtio".to_string()
}

pub fn create_vm(req: CreateVmRequest) -> Result<String, String> {
    ensure_default_nat_network();
    let name = req.name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Le nom de la machine virtuelle ne doit contenir que des lettres, chiffres, tirets ou underscores.".to_string());
    }

    let virt_install = get_virt_install_bin();
    let qemu_img = get_qemu_img_bin();
    let vms_dir = get_vms_pool_dir();
    let disk_path = vms_dir.join(format!("{}.qcow2", name));

    if disk_path.exists() {
        return Err(format!("Le disque virtuel {} existe déjà.", disk_path.display()));
    }

    let disk_size = if req.disk_size_gb < 5 { 20 } else { req.disk_size_gb };
    let img_create = Command::new(&qemu_img)
        .args(&["create", "-f", "qcow2", disk_path.to_str().unwrap(), &format!("{}G", disk_size)])
        .output()
        .map_err(|e| format!("Impossible de créer l'image QCOW2 : {}", e))?;

    if !img_create.status.success() {
        return Err(format!("Erreur qemu-img : {}", String::from_utf8_lossy(&img_create.stderr)));
    }

    let uri = get_libvirt_uri();
    let mut args = vec![
        "--connect".to_string(), uri,
        "--name".to_string(), name.to_string(),
        "--vcpus".to_string(), req.vcpus.max(1).to_string(),
        "--memory".to_string(), req.memory_mb.max(512).to_string(),
        "--disk".to_string(), format!("path={},format=qcow2,bus=virtio", disk_path.display()),
        "--graphics".to_string(), "vnc,listen=127.0.0.1".to_string(),
        "--noautoconsole".to_string(),
        "--osinfo".to_string(), "detect=on,require=off".to_string(),
    ];

    if req.os_type == "windows" {
        args.push("--os-variant".to_string());
        args.push("win11".to_string());
    } else if req.os_type == "linux" {
        args.push("--os-variant".to_string());
        args.push("linux2022".to_string());
    }

    if req.enable_uefi.unwrap_or(true) {
        args.push("--boot".to_string());
        args.push("uefi".to_string());
    }

    if req.enable_tpm.unwrap_or(false) || req.os_type == "windows" {
        args.push("--tpm".to_string());
        args.push("backend.type=emulator,backend.version=2.0,model=tpm-crb".to_string());
    }

    if req.network_type == "bridge" {
        args.push("--network".to_string());
        args.push(get_bridge_network_target());
    } else {
        args.push("--network".to_string());
        args.push("network=default,model=virtio".to_string());
    }

    if let Some(ref iso) = req.iso_path {
        if !iso.trim().is_empty() && StdPath::new(iso).exists() {
            args.push("--cdrom".to_string());
            args.push(iso.clone());
        } else {
            args.push("--import".to_string());
        }
    } else {
        args.push("--import".to_string());
    }

    let mut is_nvidia_gpu = false;
    if let Some(ref pci) = req.gpu_pci {
        let trimmed_pci = pci.trim();
        if !trimmed_pci.is_empty() {
            let vendor_path = format!("/sys/bus/pci/devices/{}/vendor", trimmed_pci);
            if fs::read_to_string(vendor_path).unwrap_or_default().trim() == "0x10de" {
                is_nvidia_gpu = true;
            }

            // Attacher tous les périphériques du même groupe IOMMU (ex: GPU + contrôleur Audio HDMI Nvidia)
            let group_devs = get_all_devices_in_iommu_group(trimmed_pci);
            for dev in group_devs {
                args.push("--hostdev".to_string());
                args.push(dev);
            }
        }
    }

    // Contournement Erreur 43 Nvidia & compatibilité maximale (Nvidia moderne & Nvidia Legacy)
    if is_nvidia_gpu || req.os_type == "windows" {
        args.push("--features".to_string());
        args.push("kvm.hidden=on,hyperv.relaxed=on,hyperv.vapic=on,hyperv.spinlocks=on,hyperv.vendor_id=1234567890ab".to_string());
    }

    let output = Command::new(&virt_install).args(&args).output()
        .map_err(|e| format!("Impossible d'exécuter virt-install : {}", e))?;

    if output.status.success() {
        Ok(format!("Machine virtuelle '{}' créée avec succès avec {} vCPU, {} Mo de RAM et {} Go de stockage.", name, req.vcpus, req.memory_mb, disk_size))
    } else {
        let err_msg = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = fs::remove_file(&disk_path);
        Err(format!("Échec virt-install : {}", err_msg))
    }
}

pub fn list_isos() -> Vec<IsoInfo> {
    let isos_dir = get_isos_dir();
    let mut isos = Vec::new();

    let search_paths = vec![isos_dir, PathBuf::from("/storage/isos"), PathBuf::from("/mnt/storage/isos")];
    let mut seen = std::collections::HashSet::new();

    for dir in search_paths {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("iso")).unwrap_or(false) {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    if seen.contains(&filename) {
                        continue;
                    }
                    seen.insert(filename.clone());

                    let meta = fs::metadata(&path).ok();
                    let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let size_human = format_bytes_human(size_bytes);
                    let modified = meta.and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    isos.push(IsoInfo {
                        name: filename,
                        path: path.to_string_lossy().to_string(),
                        size_bytes,
                        size_human,
                        modified,
                    });
                }
            }
        }
    }

    isos.sort_by(|a, b| b.modified.cmp(&a.modified));
    isos
}

pub async fn start_iso_download(req: IsoDownloadRequest) -> Result<String, String> {
    let url = req.url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL invalide (doit commencer par http:// ou https://)".to_string());
    }

    let default_name = url.split('/').last().unwrap_or("image.iso").split('?').next().unwrap_or("image.iso").to_string();
    let mut filename = req.filename.unwrap_or(default_name);
    if !filename.ends_with(".iso") {
        filename.push_str(".iso");
    }

    let isos_dir = get_isos_dir();
    let part_path = isos_dir.join(format!("{}.part", filename));
    let final_path = isos_dir.join(&filename);

    let job_id = format!("iso_{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());

    let job = IsoDownloadJob {
        id: job_id.clone(),
        url: url.clone(),
        filename: filename.clone(),
        downloaded_bytes: 0,
        total_bytes: 0,
        progress: 0.0,
        speed_mbps: 0.0,
        status: "downloading".to_string(),
        error: None,
    };

    let store = get_iso_job_store();
    store.write().await.insert(job_id.clone(), job);

    let jid = job_id.clone();
    let store_clone = store.clone();

    tokio::spawn(async move {
        let total_size = get_remote_file_size(&url).await.unwrap_or(0);
        {
            let mut w = store_clone.write().await;
            if let Some(j) = w.get_mut(&jid) {
                j.total_bytes = total_size;
            }
        }

        let mut child = match tokio::process::Command::new("curl")
            .args(&["-sSL", "--fail", "-C", "-", "-o", part_path.to_str().unwrap(), &url])
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let mut w = store_clone.write().await;
                if let Some(j) = w.get_mut(&jid) {
                    j.status = "failed".to_string();
                    j.error = Some(format!("Impossible de lancer curl : {}", e));
                }
                return;
            }
        };

        let mut last_size = 0u64;
        let mut last_time = Instant::now();

        loop {
            tokio::select! {
                res = child.wait() => {
                    let mut w = store_clone.write().await;
                    if let Some(j) = w.get_mut(&jid) {
                        match res {
                            Ok(st) if st.success() => {
                                let _ = fs::rename(&part_path, &final_path);
                                j.status = "completed".to_string();
                                j.progress = 100.0;
                                if let Ok(meta) = fs::metadata(&final_path) {
                                    j.downloaded_bytes = meta.len();
                                    j.total_bytes = meta.len();
                                }
                            }
                            Ok(st) => {
                                j.status = "failed".to_string();
                                j.error = Some(format!("curl s'est arrêté avec le code : {}", st));
                            }
                            Err(e) => {
                                j.status = "failed".to_string();
                                j.error = Some(format!("Erreur curl : {}", e));
                            }
                        }
                    }
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    if let Ok(meta) = fs::metadata(&part_path) {
                        let cur_size = meta.len();
                        let elapsed = last_time.elapsed().as_secs_f64();
                        let speed_mbps = if elapsed > 0.0 {
                            ((cur_size.saturating_sub(last_size)) as f64 * 8.0) / (elapsed * 1_000_000.0)
                        } else {
                            0.0
                        };

                        last_size = cur_size;
                        last_time = Instant::now();

                        let mut w = store_clone.write().await;
                        if let Some(j) = w.get_mut(&jid) {
                            j.downloaded_bytes = cur_size;
                            j.speed_mbps = (speed_mbps * 10.0).round() / 10.0;
                            if j.total_bytes > 0 {
                                j.progress = ((cur_size as f64 / j.total_bytes as f64) * 1000.0).round() / 10.0;
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(job_id)
}

async fn get_remote_file_size(url: &str) -> Option<u64> {
    let out = tokio::process::Command::new("curl")
        .args(&["-sIL", url])
        .output()
        .await
        .ok()?;

    let txt = String::from_utf8_lossy(&out.stdout);
    for line in txt.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            let val = line.split(':').nth(1)?.trim();
            if let Ok(len) = val.parse::<u64>() {
                return Some(len);
            }
        }
    }
    None
}

pub fn detect_gpus() -> Vec<GpuDeviceInfo> {
    let mut gpus = Vec::new();
    let pci_path = StdPath::new("/sys/bus/pci/devices");
    if !pci_path.exists() {
        return gpus;
    }

    let is_jellyfin_active = Command::new("systemctl")
        .args(&["is-active", "jellyfin"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    if let Ok(entries) = fs::read_dir(pci_path) {
        for entry in entries.flatten() {
            let dev_path = entry.path();
            let pci_addr = dev_path.file_name().unwrap_or_default().to_string_lossy().to_string();

            let class_file = dev_path.join("class");
            if let Ok(cls) = fs::read_to_string(&class_file) {
                let cls_trim = cls.trim();
                // 0x030000 = VGA, 0x030200 = 3D Controller, 0x038000 = Display Controller
                if cls_trim.starts_with("0x03") {
                    let vendor_code = fs::read_to_string(dev_path.join("vendor")).unwrap_or_default().trim().to_string();
                    let device_code = fs::read_to_string(dev_path.join("device")).unwrap_or_default().trim().to_string();

                    let driver = if dev_path.join("driver").exists() {
                        fs::read_link(dev_path.join("driver"))
                            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                            .unwrap_or_else(|_| "inconnu".to_string())
                    } else {
                        "aucun".to_string()
                    };

                    let iommu_group = if dev_path.join("iommu_group").exists() {
                        fs::read_link(dev_path.join("iommu_group"))
                            .ok()
                            .and_then(|p| p.file_name().and_then(|f| f.to_str().and_then(|s| s.parse::<u32>().ok())))
                    } else {
                        None
                    };

                    let vendor_name = match vendor_code.as_str() {
                        "0x1002" => "Advanced Micro Devices (AMD)",
                        "0x10de" => "NVIDIA Corporation",
                        "0x8086" => "Intel Corporation",
                        _ => "Contrôleur Graphique PCI",
                    }.to_string();

                    let device_name = format!("{} [{}:{}]", vendor_name, vendor_code.trim_start_matches("0x"), device_code.trim_start_matches("0x"));

                    let mut conflict_warning = None;
                    let has_drm = StdPath::new("/dev/dri/renderD128").exists() || StdPath::new("/dev/dri/card0").exists();
                    let has_nvidia_dev = StdPath::new("/dev/nvidiactl").exists() || StdPath::new("/dev/nvidia0").exists();

                    if vendor_code == "0x10de" {
                        // Détection Nvidia / Nvidia Legacy
                        if has_nvidia_dev || driver.contains("nvidia") {
                            if is_jellyfin_active {
                                conflict_warning = Some("Ce GPU NVIDIA est actuellement configuré pour le transcodage matériel Jellyfin / Docker (NVENC/NVDEC). L'assigner à une machine virtuelle suspendra l'accélération Jellyfin pendant l'exécution de la VM.".to_string());
                            } else {
                                conflict_warning = Some("Pilote propriétaire Nvidia actif sur l'hôte. Lors du passthrough VFIO, le pilote sera détaché du noyau hôte au démarrage de la VM.".to_string());
                            }
                        }
                    } else if is_jellyfin_active && has_drm {
                        conflict_warning = Some("Ce GPU est actuellement exploité par le service Jellyfin pour le transcodage matériel 4K. L'assigner à une VM désactivera l'accélération matérielle Jellyfin pendant l'exécution de la VM.".to_string());
                    }

                    gpus.push(GpuDeviceInfo {
                        pci_address: pci_addr,
                        name: device_name,
                        vendor: vendor_name,
                        driver,
                        iommu_group,
                        is_in_use_by_jellyfin: is_jellyfin_active,
                        is_in_use_by_docker: false,
                        is_primary_display: true,
                        conflict_warning,
                    });
                }
            }
        }
    }

    gpus
}

pub async fn handle_vm_vnc_ws(
    Path(vm_name): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_vnc_socket(vm_name, socket))
}

async fn handle_vnc_socket(vm_name: String, mut socket: WebSocket) {
    let port = match get_vm_vnc_port(&vm_name) {
        Some(p) => p,
        None => return,
    };

    let tcp_stream = match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
        Ok(s) => s,
        Err(_) => return,
    };

    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);
    let (tx_bytes, mut rx_bytes) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

    let tcp_to_mpsc = tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match tcp_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx_bytes.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    loop {
        tokio::select! {
            Some(bytes) = rx_bytes.recv() => {
                if socket.send(Message::Binary(bytes)).await.is_err() {
                    break;
                }
            }
            res = socket.recv() => {
                match res {
                    Some(Ok(Message::Binary(bin))) => {
                        if tcp_write.write_all(&bin).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    tcp_to_mpsc.abort();
}

fn format_bytes_human(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;

    if bytes >= GIB {
        format!("{:.2} Go", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} Mo", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{} Ko", bytes / KIB)
    } else {
        format!("{} octets", bytes)
    }
}
