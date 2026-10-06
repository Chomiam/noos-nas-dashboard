use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitItem {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeInputStatus {
    pub name: String,
    pub locked_rev: String,
    pub remote_rev: Option<String>,
    pub has_update: bool,
    pub channel_or_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageUpdateItem {
    pub name: String,
    pub current_version: String,
    pub new_version: Option<String>,
    pub action: String, // "update", "add", "remove", "rebuild", "flake"
    pub size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsTelemetry {
    pub repo_name: String,
    pub local_commit: String,
    pub local_commit_full: String,
    pub remote_commit: Option<String>,
    pub remote_commit_full: Option<String>,
    pub commit_message: Option<String>,
    pub commits_behind: u32,
    pub pending_commits: Vec<GitCommitItem>,
    pub changed_files: Vec<String>,
    pub git_status: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardTelemetry {
    pub repo_name: String,
    pub running_version: String,
    pub running_commit: Option<String>,
    pub target_version: Option<String>,
    pub target_commit: Option<String>,
    pub remote_commit: Option<String>,
    pub update_available: bool,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckStatus {
    // Double Télémétrie
    pub os_telemetry: OsTelemetry,
    pub dashboard_telemetry: DashboardTelemetry,
    pub dashboard_update_available: bool,
    // Configuration Git
    pub config_update_available: bool,
    pub config_local_commit: String,
    pub config_local_commit_full: String,
    pub config_remote_commit: Option<String>,
    pub config_remote_commit_full: Option<String>,
    pub config_commit_message: Option<String>,
    pub config_commits_behind: u32,
    pub config_pending_commits: Vec<GitCommitItem>,
    pub config_changed_files: Vec<String>,
    pub config_git_status: String,

    // Paquets système & Flake
    pub package_updates_available: bool,
    pub package_updates_count: u32,
    pub package_details: Vec<String>,
    pub flake_inputs_status: Vec<FlakeInputStatus>,
    pub package_updates_list: Vec<PackageUpdateItem>,

    // Synthèse globale
    pub update_type: UpdateType, // "None", "ConfigOnly", "PackagesOnly", "Both"
    pub status_text: String,
    pub config_dir: String,
    pub last_checked: String,
    pub is_updating: bool,
    pub system_generation: Option<String>,
    #[serde(default)]
    pub system_generations_count: u32,
    #[serde(default = "default_channel")]
    pub channel: String,
}

fn default_channel() -> String {
    "stable".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgressState {
    pub is_running: bool,
    pub stage: String,
    pub step_index: u32,
    pub total_steps: u32,
    pub progress_percent: u32,
    pub status_title: String,
    pub status_detail: String,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    #[serde(default)]
    pub completed_timestamp: Option<u64>,
    pub target_commit: Option<String>,
    pub generation_before: Option<String>,
    pub generation_after: Option<String>,
    pub dashboard_restarting: bool,
    pub log_tail: String,
    #[serde(default)]
    pub total_derivations: Option<u32>,
    #[serde(default)]
    pub current_derivation_index: Option<u32>,
    #[serde(default)]
    pub current_package_name: Option<String>,
    #[serde(default)]
    pub failed_units: Vec<String>,
    #[serde(default)]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateType {
    None,
    ConfigOnly,
    PackagesOnly,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyUpdateResult {
    pub success: bool,
    pub steps_executed: Vec<String>,
    pub output_log: String,
    pub error: Option<String>,
}

static UPDATE_CACHE: Mutex<Option<(Instant, UpdateCheckStatus)>> = Mutex::new(None);
static IS_UPDATING: AtomicBool = AtomicBool::new(false);
static IS_CHECKING: AtomicBool = AtomicBool::new(false);
static LIVE_UPDATE_LOG: Mutex<String> = Mutex::new(String::new());

pub fn is_updating() -> bool {
    IS_UPDATING.load(Ordering::SeqCst)
}

pub fn clear_update_cache() {
    if let Ok(mut guard) = UPDATE_CACHE.lock() {
        *guard = None;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannelConfig {
    pub channel: String,
}

static ACTIVE_CHANNEL_CACHE: Mutex<Option<String>> = Mutex::new(None);

pub fn get_channel_candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(home) = env::var("HOME") {
        paths.push(PathBuf::from(home).join(".config/noos/update_channel"));
    }
    paths.push(PathBuf::from("/var/lib/noos/update_channel"));
    paths.push(PathBuf::from("/tmp/noos_update_channel"));
    paths
}

pub fn get_update_channel() -> String {
    if let Ok(guard) = ACTIVE_CHANNEL_CACHE.lock() {
        if let Some(ref ch) = *guard {
            return ch.clone();
        }
    }

    for path in get_channel_candidate_paths() {
        if let Ok(c) = fs::read_to_string(&path) {
            let ch = c.trim().to_lowercase();
            if ch == "testing" {
                if let Ok(mut guard) = ACTIVE_CHANNEL_CACHE.lock() {
                    *guard = Some("testing".to_string());
                }
                return "testing".to_string();
            } else if ch == "stable" {
                if let Ok(mut guard) = ACTIVE_CHANNEL_CACHE.lock() {
                    *guard = Some("stable".to_string());
                }
                return "stable".to_string();
            }
        }
    }

    "stable".to_string()
}

pub fn set_update_channel(channel: &str) -> Result<String, String> {
    let clean = channel.trim().to_lowercase();
    let valid_channel = if clean == "testing" { "testing" } else { "stable" };

    // 1. Mettre à jour immédiatement le cache en mémoire (effet immédiat garanti)
    if let Ok(mut guard) = ACTIVE_CHANNEL_CACHE.lock() {
        *guard = Some(valid_channel.to_string());
    }

    // 2. Tenter d'écrire sur les chemins candidats (user config ~/.config/noos, /var/lib/noos, /tmp)
    for path in get_channel_candidate_paths() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, valid_channel);
    }

    clear_update_cache();
    Ok(valid_channel.to_string())
}

fn get_update_log_path() -> std::path::PathBuf {
    let log_dir = std::path::Path::new("/var/log");
    if log_dir.exists() {
        return log_dir.join("noos-update.log");
    }
    std::path::PathBuf::from("/run/noos-update.log")
}

pub fn append_live_log(msg: &str) {
    if let Ok(mut log) = LIVE_UPDATE_LOG.lock() {
        log.push_str(msg);
    }
    use std::io::Write;
    let log_path = get_update_log_path();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = f.write_all(msg.as_bytes());
    }
}

pub fn get_live_log() -> (String, bool) {
    let mut log_str = LIVE_UPDATE_LOG.lock().map(|l| l.clone()).unwrap_or_default();
    if log_str.trim().is_empty() {
        let log_path = get_update_log_path();
        if let Ok(content) = std::fs::read_to_string(&log_path) {
            log_str = content;
        }
    }
    (log_str, is_updating())
}

pub fn get_current_system_generation() -> Option<String> {
    if let Ok(target) = fs::read_link("/nix/var/nix/profiles/system") {
        let name = target.file_name()?.to_string_lossy();
        if let Some(stripped) = name.strip_prefix("system-") {
            if let Some(gen) = stripped.strip_suffix("-link") {
                return Some(gen.to_string());
            }
        }
        return Some(name.to_string());
    }
    None
}

pub fn get_system_generations_count() -> u32 {
    if let Ok(entries) = fs::read_dir("/nix/var/nix/profiles") {
        entries.flatten().filter(|e| {
            let fname = e.file_name().to_string_lossy().to_string();
            fname.starts_with("system-") && fname.ends_with("-link")
        }).count() as u32
    } else {
        0
    }
}

pub fn get_update_state_file_path() -> PathBuf {
    PathBuf::from("/run/noos-update-state.json")
}

pub fn save_update_progress(state: &UpdateProgressState) {
    let path = get_update_state_file_path();
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(&path, &json);
    }
}

pub fn get_update_progress() -> UpdateProgressState {
    let path = get_update_state_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(state) = serde_json::from_str::<UpdateProgressState>(&content) {
            return state;
        }
    }
    UpdateProgressState {
        is_running: is_updating(),
        stage: if is_updating() { "running".to_string() } else { "idle".to_string() },
        step_index: if is_updating() { 2 } else { 0 },
        total_steps: 4,
        progress_percent: if is_updating() { 50 } else { 0 },
        status_title: if is_updating() { "Mise à jour en cours d'exécution...".to_string() } else { "Système prêt".to_string() },
        status_detail: "Aucune mise à jour en cours d'exécution.".to_string(),
        error: None,
        started_at: None,
        completed_at: None,
        completed_timestamp: None,
        target_commit: None,
        generation_before: get_current_system_generation(),
        generation_after: None,
        dashboard_restarting: false,
        log_tail: String::new(),
        total_derivations: None,
        current_derivation_index: None,
        current_package_name: None,
        failed_units: Vec::new(),
        warning: None,
    }
}

pub fn dismiss_update_progress() {
    let mut state = get_update_progress();
    state.is_running = false;
    state.stage = "idle".to_string();
    state.progress_percent = 0;
    state.error = None;
    state.completed_timestamp = None;
    state.total_derivations = None;
    state.current_derivation_index = None;
    state.current_package_name = None;
    save_update_progress(&state);
}

pub fn init_update_tracker() {
    let mut state = get_update_progress();
    if state.is_running {
        let cur_gen = get_current_system_generation();
        let changed = match (&state.generation_before, &cur_gen) {
            (Some(before), Some(current)) => before != current,
            _ => true,
        };

        if changed || state.dashboard_restarting || state.stage == "activating" {
            let now_ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            state.is_running = false;
            state.stage = "completed".to_string();
            state.step_index = 4;
            state.total_steps = 4;
            state.progress_percent = 100;
            state.status_title = "Mise à jour terminée avec succès !".to_string();
            state.status_detail = format!("Le système a basculé avec succès sur la génération {}.", cur_gen.clone().unwrap_or_else(|| "suivante".to_string()));
            state.generation_after = cur_gen;
            state.completed_at = Some(current_time_formatted());
            state.completed_timestamp = Some(now_ts);
            state.dashboard_restarting = false;
            save_update_progress(&state);
        }
    } else if state.stage == "completed" {
        // Purger automatiquement si la mise à jour s'est terminée il y a plus de 10 secondes
        if let Some(ts) = state.completed_timestamp {
            let now_ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            if now_ts.saturating_sub(ts) >= 10 {
                dismiss_update_progress();
            }
        }
    }
}

pub fn get_cached_status() -> Option<UpdateCheckStatus> {
    if let Ok(guard) = UPDATE_CACHE.lock() {
        if let Some((_, ref cached)) = *guard {
            let mut res = cached.clone();
            res.is_updating = is_updating();
            return Some(res);
        }
    }
    None
}

pub fn start_background_checker() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(3));
        let _ = check_updates(true);

        loop {
            std::thread::sleep(Duration::from_secs(15 * 60));
            if !is_updating() {
                let _ = check_updates(true);
            }
        }
    });
}

fn find_bin(candidates: &[&str]) -> String {
    for c in candidates {
        if c.starts_with('/') {
            if Path::new(c).exists() {
                return c.to_string();
            }
        } else if let Ok(out) = Command::new("which").arg(c).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return p;
                }
            }
        }
    }
    candidates[0].to_string()
}

pub fn git_binary() -> String {
    find_bin(&["/run/current-system/sw/bin/git", "git", "/nix/var/nix/profiles/default/bin/git", "/usr/bin/git"])
}

pub fn nixos_rebuild_binary() -> String {
    find_bin(&[
        "/run/current-system/sw/bin/nixos-rebuild",
        "nixos-rebuild",
        "/nix/var/nix/profiles/default/bin/nixos-rebuild",
        "/usr/bin/nixos-rebuild",
    ])
}

pub fn nix_binary() -> String {
    find_bin(&["/run/current-system/sw/bin/nix", "nix", "/nix/var/nix/profiles/default/bin/nix", "/usr/bin/nix"])
}

pub fn sudo_binary() -> String {
    find_bin(&[
        "/run/wrappers/bin/sudo",
        "sudo",
        "/run/current-system/sw/bin/sudo",
        "/usr/bin/sudo",
    ])
}

pub fn user_exists(username: &str) -> bool {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            if let Some(user) = line.split(':').next() {
                if user.trim() == username {
                    return true;
                }
            }
        }
    }
    false
}

pub fn find_first_human_user() -> Option<String> {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        let mut fallback: Option<String> = None;
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                let username = parts[0].trim();
                let uid: u32 = parts[2].trim().parse().unwrap_or(0);
                if uid >= 1000 && uid < 60000 && username != "nobody" && !username.starts_with("nixbld") {
                    // Priorité absolue aux utilisateurs humains réels différents de 'chomiam' et 'admin'
                    if username != "chomiam" && username != "admin" {
                    return Some(username.to_string());
                }
                    if fallback.is_none() {
                        fallback = Some(username.to_string());
                    }
                }
            }
        }
        return fallback;
    }
    None
}

pub fn get_user_home(username: &str) -> PathBuf {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 6 && parts[0].trim() == username {
                let home = parts[5].trim();
                if !home.is_empty() {
                    return PathBuf::from(home);
                }
            }
        }
    }
    PathBuf::from(format!("/home/{}", username))
}

pub fn target_user() -> String {
    // 1. Lire d'abord vars.local.nix / vars.nix si accessible pour avoir la vérité déclarative absolue
    let vars_paths = [
        PathBuf::from("/etc/nixos/vars.local.nix"),
        PathBuf::from("/etc/nixos/vars.nix"),
        PathBuf::from("/etc/nixos/.vars.nix.backup"),
    ];
    for vp in &vars_paths {
        if let Ok(c) = fs::read_to_string(vp) {
            for line in c.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("username") && trimmed.contains('=') {
                    if let Some(val) = trimmed.split('=').nth(1) {
                        let unquoted = val.trim().trim_matches(|c| c == '"' || c == ';' || c == ' ');
                        if !unquoted.is_empty() && unquoted != "admin" && user_exists(unquoted) {
                            return unquoted.to_string();
                        }
                    }
                }
            }
        }
    }

    // 2. Vérifier NOOS_USER depuis l'environnement
    if let Ok(u) = env::var("NOOS_USER") {
        let trimmed = u.trim();
        if !trimmed.is_empty() && user_exists(trimmed) {
            if trimmed != "chomiam" && trimmed != "admin" {
                return trimmed.to_string();
            }
            if let Some(human) = find_first_human_user() {
                if human != "chomiam" && human != "admin" {
                    return human;
                }
            }
            return trimmed.to_string();
        }
    }

    // 3. Premier utilisateur humain du système
    if let Some(human) = find_first_human_user() {
        return human;
    }
    "root".to_string()
}

pub fn is_root_process() -> bool {
    if let Ok(out) = Command::new("id").arg("-u").output() {
        String::from_utf8_lossy(&out.stdout).trim() == "0"
    } else {
        false
    }
}

pub fn create_user_command(bin: &str, args: &[&str]) -> Command {
    let user = target_user();
    let home_path = get_user_home(&user);
    let home_str = home_path.to_string_lossy().to_string();
    let is_root = is_root_process();
    let runuser_bin = "/run/current-system/sw/bin/runuser";
    let sudo_b = sudo_binary();
    let cur_path = env::var("PATH").unwrap_or_default();
    let complete_path = format!("/run/wrappers/bin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:{}", cur_path);

    if is_root && user != "root" && (Path::new(runuser_bin).exists() || Command::new("runuser").arg("--version").output().is_ok()) {
        let prog = if Path::new(runuser_bin).exists() { runuser_bin } else { "runuser" };
        let mut cmd = Command::new(prog);
        cmd.args(["-u", &user, "--", "env", &format!("PATH={}", complete_path), &format!("HOME={}", home_str), &format!("USER={}", user), &format!("NH_FLAKE=/etc/nixos"), &format!("NH_ELEVATION_STRATEGY={}", sudo_b), bin]);
        cmd.args(args);
        cmd.env("USER", &user);
        cmd.env("HOME", &home_str);
        cmd.env("NH_FLAKE", "/etc/nixos");
        cmd.env("NH_ELEVATION_STRATEGY", &sudo_b);
        cmd.env("PATH", &complete_path);
        cmd
    } else {
        let mut cmd = Command::new(bin);
        cmd.args(args);
        cmd.env("USER", &user);
        cmd.env("HOME", &home_str);
        cmd.env("NH_FLAKE", "/etc/nixos");
        cmd.env("NH_ELEVATION_STRATEGY", &sudo_b);
        cmd.env("PATH", &complete_path);
        cmd
    }
}

fn git_cmd(repo_dir: &str) -> Command {
    let git_b = git_binary();
    create_user_command(&git_b, &["-c", "safe.directory=*", "-c", "user.name=Noos", "-c", "user.email=noos@local", "-C", repo_dir])
}

pub fn resolve_config_dir() -> PathBuf {
    if let Ok(dir) = env::var("NOOS_CONFIG_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return p;
        }
    }

    let candidate1 = PathBuf::from("/etc/nixos");
    if candidate1.exists() {
        return candidate1;
    }

    let user = target_user();
    let candidate_noos = get_user_home(&user).join("Projects/noos-nas");
    if candidate_noos.exists() {
        return candidate_noos;
    }

    PathBuf::from(".")
}

fn get_local_commit_from_fs(config_dir: &Path) -> (String, String) {
    let git_dir = config_dir.join(".git");
    let head_path = git_dir.join("HEAD");
    if let Ok(head_content) = fs::read_to_string(&head_path) {
        let trimmed = head_content.trim();
        if trimmed.starts_with("ref: ") {
            let ref_rel = trimmed.trim_start_matches("ref: ").trim();
            let ref_file = git_dir.join(ref_rel);
            if let Ok(sha) = fs::read_to_string(&ref_file) {
                let full = sha.trim().to_string();
                let short = full[..7.min(full.len())].to_string();
                return (short, full);
            }
            // Check packed-refs
            let packed_path = git_dir.join("packed-refs");
            if let Ok(packed) = fs::read_to_string(&packed_path) {
                for line in packed.lines() {
                    let line = line.trim();
                    if !line.starts_with('#') && !line.starts_with('^') {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 && parts[1] == ref_rel {
                            let full = parts[0].to_string();
                            let short = full[..7.min(full.len())].to_string();
                            return (short, full);
                        }
                    }
                }
            }
        } else if trimmed.len() >= 40 {
            let full = trimmed.to_string();
            let short = full[..7.min(full.len())].to_string();
            return (short, full);
        }
    }
    ("inconnu".to_string(), String::new())
}

pub fn check_updates(force_refresh: bool) -> UpdateCheckStatus {
    if !force_refresh {
        if let Ok(guard) = UPDATE_CACHE.lock() {
            if let Some((instant, ref cached)) = *guard {
                if instant.elapsed() < Duration::from_secs(900) {
                    let mut res = cached.clone();
                    res.is_updating = is_updating();
                    return res;
                }
            }
        }
    }

    if IS_CHECKING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        if let Some(cached) = get_cached_status() {
            return cached;
        }
    }

    struct CheckGuard;
    impl Drop for CheckGuard {
        fn drop(&mut self) {
            IS_CHECKING.store(false, Ordering::SeqCst);
        }
    }
    let _check_guard = CheckGuard;

    let config_dir = resolve_config_dir();
    let config_dir_str = config_dir.display().to_string();

    // 1. Détection du commit local (lecture directe FS + fallback git rev-parse)
    let (fs_short, fs_full) = get_local_commit_from_fs(&config_dir);

    let (local_commit, full_local_commit) = if !fs_full.is_empty() {
        (fs_short, fs_full)
    } else {
        let rev = git_cmd(&config_dir_str)
            .args(["rev-parse", "HEAD"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if !rev.is_empty() {
            (rev[..7.min(rev.len())].to_string(), rev)
        } else {
            ("inconnu".to_string(), String::new())
        }
    };

    let mut config_update_available = false;
    let mut config_remote_commit = None;
    let mut config_remote_commit_full = None;
    let mut config_commit_message = None;
    let mut config_commits_behind = 0;
    let mut config_pending_commits = Vec::new();
    let mut config_changed_files = Vec::new();

    // 2. Détection du commit distant sur GitHub selon le canal (Stable=main, Testing=testing)
    let active_channel = get_update_channel();
    let preferred_branch = if active_channel == "testing" { "testing" } else { "main" };
    let candidate_remotes = [
        "https://github.com/Chomiam/noos-nas.git",
        "https://github.com/Chomiam/steve_os-nix.git",
    ];
    let mut _resolved_remote_url = candidate_remotes[0];

    for remote_url in &candidate_remotes {
        let mut target_ref = format!("refs/heads/{}", preferred_branch);
        let mut branch_to_fetch = preferred_branch;

        let mut check_out = git_cmd(&config_dir_str)
            .args(["ls-remote", remote_url, &target_ref])
            .output();

        // Si la branche testing n'existe pas encore sur le dépôt distant, basculer sur main
        if check_out.as_ref().map(|o| !o.status.success() || String::from_utf8_lossy(&o.stdout).trim().is_empty()).unwrap_or(true) && preferred_branch != "main" {
            target_ref = "refs/heads/main".to_string();
            branch_to_fetch = "main";
            check_out = git_cmd(&config_dir_str)
                .args(["ls-remote", remote_url, &target_ref])
                .output();
        }

        if let Ok(out) = check_out {
            if out.status.success() {
                _resolved_remote_url = remote_url;
                let text = String::from_utf8_lossy(&out.stdout);
                if let Some(token) = text.split_whitespace().next() {
                    let full_remote = token.to_string();
                    let short_remote = token[..7.min(token.len())].to_string();
                    config_remote_commit = Some(short_remote);
                    config_remote_commit_full = Some(full_remote.clone());

                    if !full_local_commit.is_empty() && full_remote != full_local_commit {
                        // Récupération sans toucher aux fichiers de travail
                        let _ = git_cmd(&config_dir_str)
                            .args(["fetch", remote_url, branch_to_fetch])
                            .output();

                        let is_ancestor = git_cmd(&config_dir_str)
                            .args(["merge-base", "--is-ancestor", &full_remote, "HEAD"])
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false);

                        if !is_ancestor {
                            config_update_available = true;

                            // Liste des commits en retard
                            if let Ok(log_out) = git_cmd(&config_dir_str)
                                .args(["log", "HEAD..FETCH_HEAD", "--pretty=format:%h|%an|%ad|%s", "--date=short"])
                                .output()
                            {
                                let log_str = String::from_utf8_lossy(&log_out.stdout);
                                for line in log_str.lines() {
                                    let parts: Vec<&str> = line.splitn(4, '|').collect();
                                    if parts.len() == 4 {
                                        config_pending_commits.push(GitCommitItem {
                                            hash: parts[0].to_string(),
                                            author: parts[1].to_string(),
                                            date: parts[2].to_string(),
                                            message: parts[3].to_string(),
                                        });
                                    }
                                }
                                config_commits_behind = config_pending_commits.len() as u32;
                            }

                            // Liste des fichiers modifiés
                            if let Ok(diff_out) = git_cmd(&config_dir_str)
                                .args(["diff", "--name-status", "HEAD", "FETCH_HEAD"])
                                .output()
                            {
                                let diff_str = String::from_utf8_lossy(&diff_out.stdout);
                                for line in diff_str.lines() {
                                    let l = line.trim();
                                    if !l.is_empty() {
                                        config_changed_files.push(l.to_string());
                                    }
                                }
                            }

                            config_commit_message = if !config_pending_commits.is_empty() {
                                Some(format!("{} nouvelle(s) révision(s) en attente : {}", config_commits_behind, config_pending_commits[0].message))
                            } else {
                                Some("Nouvelle révision disponible sur GitHub (noos-nas)".into())
                            };
                        }
                    }
                    break;
                }
            }
        }
    }

    // Statut local du repo Git (propre ou modifications en cours)
    let git_status_clean = git_cmd(&config_dir_str)
        .args(["status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(true);

    let config_git_status = if git_status_clean {
        "Arbre de travail propre".to_string()
    } else {
        "Modifications locales détectées (stash automatique)".to_string()
    };

    // 3. Télémétrie spécifique du Dashboard Noos (Double Télémétrie)
    let running_dashboard_version = env!("CARGO_PKG_VERSION").to_string();
    let mut dashboard_target_commit = None;
    let mut dashboard_target_commit_full = None;
    let mut dashboard_remote_commit = None;
    let mut dashboard_remote_commit_full = None;
    let mut dashboard_remote_version = None;
    let mut dashboard_update_available = false;

    let candidate_dashboard_urls = [
        "https://github.com/Chomiam/noos-nas-dashboard.git",
    ];

    let preferred_dashboard_branch = if active_channel == "testing" { "testing" } else { "stable" };
    let dashboard_target_ref = format!("refs/heads/{}", preferred_dashboard_branch);
    for dashboard_git_url in &candidate_dashboard_urls {
        let mut ls_out = Command::new(git_binary())
            .args(["-c", "safe.directory=*", "ls-remote", dashboard_git_url, &dashboard_target_ref])
            .output();

        if ls_out.as_ref().map(|o| !o.status.success() || String::from_utf8_lossy(&o.stdout).trim().is_empty()).unwrap_or(true) && preferred_dashboard_branch != "stable" {
            ls_out = Command::new(git_binary())
                .args(["-c", "safe.directory=*", "ls-remote", dashboard_git_url, "refs/heads/stable"])
                .output();
        }

        if let Ok(res) = ls_out {
            if res.status.success() {
                let text = String::from_utf8_lossy(&res.stdout);
                if let Some(sha) = text.split_whitespace().next() {
                    dashboard_remote_commit_full = Some(sha.to_string());
                    dashboard_remote_commit = Some(sha[..7.min(sha.len())].to_string());
                    break;
                }
            }
        }
    }

    // 1. Détection temps réel immédiate (0 seconde de cache) : tag git pointant sur le commit de preferred_branch
    if let Some(ref target_commit) = dashboard_remote_commit_full {
        for dashboard_git_url in &candidate_dashboard_urls {
            if let Ok(tags_out) = Command::new(git_binary())
                .args(["-c", "safe.directory=*", "ls-remote", "--tags", dashboard_git_url])
                .output()
            {
                if tags_out.status.success() {
                    let text = String::from_utf8_lossy(&tags_out.stdout);
                    for line in text.lines() {
                        let mut parts = line.split_whitespace();
                        if let (Some(sha), Some(ref_str)) = (parts.next(), parts.next()) {
                            if sha == target_commit {
                                if let Some(tag) = ref_str.strip_prefix("refs/tags/v") {
                                    let clean_tag = tag.trim_end_matches("^{}");
                                    if is_valid_semver(clean_tag) {
                                        dashboard_remote_version = Some(clean_tag.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if dashboard_remote_version.is_some() {
                        break;
                    }
                }
            }
        }
    }

    // 2. Requête directe sur Cargo.toml au commit exact de la branche (contourne tout cache CDN)
    if dashboard_remote_version.is_none() {
        let commit_ref = dashboard_remote_commit_full.as_deref().unwrap_or(preferred_branch);
        let raw_cargo_url = format!(
            "https://raw.githubusercontent.com/Chomiam/noos-nas-dashboard/{}/Cargo.toml",
            commit_ref
        );
        if let Ok(out) = Command::new("curl")
            .args(["-s", "-L", "--max-time", "4", &raw_cargo_url])
            .output()
        {
            if out.status.success() {
                let content = String::from_utf8_lossy(&out.stdout);
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("version = \"") {
                        if let Some(v) = trimmed.strip_prefix("version = \"").and_then(|s| s.strip_suffix("\"")) {
                            if is_valid_semver(v) {
                                dashboard_remote_version = Some(v.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Vérification des paquets Nixpkgs & Entrées Flake dans flake.lock
    let mut package_updates_available = false;
    let mut package_updates_count = 0;
    let mut package_details = Vec::new();
    let mut flake_inputs_status = Vec::new();

    let lock_path = config_dir.join("flake.lock");
    if let Ok(lock_str) = fs::read_to_string(&lock_path) {
        if let Ok(lock_json) = serde_json::from_str::<serde_json::Value>(&lock_str) {
            let root_inputs: Vec<String> = lock_json
                .get("nodes")
                .and_then(|n| n.get("root"))
                .and_then(|r| r.get("inputs"))
                .and_then(|i| i.as_object())
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();

            if let Some(nodes) = lock_json.get("nodes").and_then(|n| n.as_object()) {
                for (node_name, node_val) in nodes {
                    if node_name == "root" || !root_inputs.contains(node_name) {
                        continue;
                    }
                    let locked = node_val.get("locked");
                    let original = node_val.get("original");

                    let locked_rev = locked
                        .and_then(|l| l.get("rev"))
                        .and_then(|r| r.as_str())
                        .unwrap_or("");

                    if locked_rev.is_empty() {
                        continue;
                    }

                    if node_name == "noos-nas-dashboard" {
                        dashboard_target_commit_full = Some(locked_rev.to_string());
                        dashboard_target_commit = Some(locked_rev[..7.min(locked_rev.len())].to_string());
                        let dash_ref = preferred_dashboard_branch;
                        let mut dash_has_update = false;
                        if let Some(ref remote_sha) = dashboard_remote_commit_full {
                            if remote_sha != locked_rev {
                                dash_has_update = true;
                                package_updates_available = true;
                                package_updates_count += 1;
                                package_details.push(format!("noos-nas-dashboard ({}) : mise à niveau disponible", dash_ref));
                            }
                        }
                        flake_inputs_status.push(FlakeInputStatus {
                            name: node_name.clone(),
                            locked_rev: locked_rev[..7.min(locked_rev.len())].to_string(),
                            remote_rev: dashboard_remote_commit.clone(),
                            has_update: dash_has_update,
                            channel_or_ref: dash_ref.to_string(),
                        });
                        continue;
                    }

                    if node_name == "nixpkgs" {
                        let nixpkgs_ref = original
                            .and_then(|o| o.get("ref"))
                            .and_then(|r| r.as_str())
                            .unwrap_or("nixos-26.05");
                        let mut nixpkgs_remote_rev = None;
                        let mut nixpkgs_has_update = false;

                        if let Ok(ls_out) = Command::new(git_binary())
                            .args(["-c", "safe.directory=*", "ls-remote", "https://github.com/nixos/nixpkgs.git", &format!("refs/heads/{}", nixpkgs_ref)])
                            .output()
                        {
                            if ls_out.status.success() {
                                let ls_text = String::from_utf8_lossy(&ls_out.stdout);
                                if let Some(r_sha) = ls_text.split_whitespace().next() {
                                    let short_remote = r_sha[..7.min(r_sha.len())].to_string();
                                    nixpkgs_remote_rev = Some(short_remote);
                                    if r_sha != locked_rev {
                                        nixpkgs_has_update = true;
                                        package_updates_available = true;
                                        package_updates_count += 1;
                                        package_details.push(format!("Canal Nixpkgs ({}) : nouvelles versions de paquets disponibles", nixpkgs_ref));
                                    }
                                }
                            }
                        }

                        flake_inputs_status.push(FlakeInputStatus {
                            name: node_name.clone(),
                            locked_rev: locked_rev[..7.min(locked_rev.len())].to_string(),
                            remote_rev: nixpkgs_remote_rev,
                            has_update: nixpkgs_has_update,
                            channel_or_ref: nixpkgs_ref.to_string(),
                        });
                        continue;
                    }

                    let owner = original.and_then(|o| o.get("owner")).and_then(|o| o.as_str()).unwrap_or("");
                    let repo = original.and_then(|o| o.get("repo")).and_then(|r| r.as_str()).unwrap_or("");
                    let default_ref = if repo == "noos-nas-dashboard" { "stable" } else { "main" };
                    let ref_branch = original.and_then(|o| o.get("ref")).and_then(|r| r.as_str()).unwrap_or(default_ref);

                    if !owner.is_empty() && !repo.is_empty() {
                        let remote_git_url = format!("https://github.com/{}/{}", owner, repo);
                        let mut remote_rev = None;
                        let mut has_update = false;

                        if let Ok(ls_out) = Command::new(git_binary())
                            .args(["-c", "safe.directory=*", "ls-remote", &remote_git_url, &format!("refs/heads/{}", ref_branch)])
                            .output()
                        {
                            if ls_out.status.success() {
                                let ls_text = String::from_utf8_lossy(&ls_out.stdout);
                                if let Some(r_sha) = ls_text.split_whitespace().next() {
                                    let short_remote = r_sha[..7.min(r_sha.len())].to_string();
                                    remote_rev = Some(short_remote);
                                    if r_sha != locked_rev {
                                        has_update = true;
                                        package_updates_available = true;
                                        package_updates_count += 1;
                                        package_details.push(format!("{}/{} ({}) : mise à jour disponible", owner, repo, ref_branch));
                                    }
                                }
                            }
                        }

                        flake_inputs_status.push(FlakeInputStatus {
                            name: node_name.clone(),
                            locked_rev: locked_rev[..7.min(locked_rev.len())].to_string(),
                            remote_rev,
                            has_update,
                            channel_or_ref: ref_branch.to_string(),
                        });
                    }
                }
            }
        }
    }

    let target_dashboard_version = dashboard_remote_version.clone().unwrap_or_else(|| running_dashboard_version.clone());

    // Vérification de la disponibilité d'une mise à jour du Dashboard
    if compare_semver(&target_dashboard_version, &running_dashboard_version) > 0 {
        dashboard_update_available = true;
    }
    if let (Some(ref locked), Some(ref remote)) = (&dashboard_target_commit_full, &dashboard_remote_commit_full) {
        if locked != remote {
            dashboard_update_available = true;
        }
    }

    let os_telemetry = OsTelemetry {
        repo_name: "Chomiam/noos-nas".to_string(),
        local_commit: local_commit.clone(),
        local_commit_full: full_local_commit.clone(),
        remote_commit: config_remote_commit.clone(),
        remote_commit_full: config_remote_commit_full.clone(),
        commit_message: config_commit_message.clone(),
        commits_behind: config_commits_behind,
        pending_commits: config_pending_commits.clone(),
        changed_files: config_changed_files.clone(),
        git_status: config_git_status.clone(),
        update_available: config_update_available,
    };

    let dashboard_status_str = if dashboard_update_available {
        format!("Mise à niveau prête : v{} (actuel : v{})", target_dashboard_version, running_dashboard_version)
    } else {
        format!("À jour (version v{} active)", running_dashboard_version)
    };

    let dashboard_telemetry = DashboardTelemetry {
        repo_name: "Chomiam/noos-nas-dashboard".to_string(),
        running_version: running_dashboard_version.clone(),
        running_commit: None,
        target_version: Some(target_dashboard_version.clone()),
        target_commit: dashboard_target_commit.clone(),
        remote_commit: dashboard_remote_commit.clone(),
        update_available: dashboard_update_available,
        status_text: dashboard_status_str,
    };

    // 4. Liste détaillée des paquets qui seront mis à jour / modifiés
    let package_updates_list = detect_package_updates_list(
        &config_dir,
        package_updates_available,
        config_update_available,
        dashboard_update_available,
        &running_dashboard_version,
        dashboard_remote_version.as_deref(),
        &flake_inputs_status,
    );

    package_updates_count = package_updates_count.max(package_updates_list.len() as u32);
    let package_updates_available = package_updates_count > 0 || package_updates_available;

    // 5. Détermination du type d'action requise (Double Télémétrie)
    let has_pkg_or_dash = dashboard_update_available || package_updates_available || !package_updates_list.is_empty();
    let update_type = match (config_update_available, has_pkg_or_dash) {
        (true, true) => UpdateType::Both,
        (true, false) => UpdateType::ConfigOnly,
        (false, true) => UpdateType::PackagesOnly,
        (false, false) => UpdateType::None,
    };

    let status_text = match update_type {
        UpdateType::Both => {
            if dashboard_update_available && config_commits_behind > 0 {
                format!("⚡ Nouvelle configuration OS ({} commit(s)) ET Dashboard v{} disponibles !", config_commits_behind, target_dashboard_version)
            } else if dashboard_update_available {
                format!("⚡ Nouvelle configuration OS et Dashboard v{} disponibles !", target_dashboard_version)
            } else {
                "⚡ Nouvelle configuration OS et paquets disponibles !".to_string()
            }
        }
        UpdateType::ConfigOnly => {
            if config_commits_behind > 0 {
                format!("📥 Nouvelle configuration disponible sur GitHub ({} nouveau(x) commit(s))", config_commits_behind)
            } else {
                "📥 Nouvelle configuration disponible sur GitHub".to_string()
            }
        }
        UpdateType::PackagesOnly => {
            if dashboard_update_available {
                format!("⚡ Mise à jour du Dashboard Noos disponible (v{} → v{})", running_dashboard_version, target_dashboard_version)
            } else {
                "📦 Mises à jour de paquets système prêtes à être appliquées".to_string()
            }
        }
        UpdateType::None => "✨ Système d'exploitation et Dashboard Noos à jour".to_string(),
    };

    let status = UpdateCheckStatus {
        os_telemetry,
        dashboard_telemetry,
        dashboard_update_available,
        config_update_available,
        config_local_commit: local_commit,
        config_local_commit_full: full_local_commit,
        config_remote_commit,
        config_remote_commit_full,
        config_commit_message,
        config_commits_behind,
        config_pending_commits,
        config_changed_files,
        config_git_status,
        package_updates_available,
        package_updates_count,
        package_details,
        flake_inputs_status,
        package_updates_list,
        update_type,
        status_text,
        config_dir: config_dir_str,
        last_checked: current_time_formatted(),
        is_updating: is_updating(),
        system_generation: get_current_system_generation(),
        system_generations_count: get_system_generations_count(),
        channel: active_channel,
    };

    if let Ok(mut guard) = UPDATE_CACHE.lock() {
        *guard = Some((Instant::now(), status.clone()));
    }

    status
}

fn detect_package_updates_list(
    config_dir: &Path,
    inputs_have_updates: bool,
    config_has_updates: bool,
    dashboard_update_available: bool,
    running_dashboard_version: &str,
    target_dashboard_version: Option<&str>,
    flake_inputs: &[FlakeInputStatus],
) -> Vec<PackageUpdateItem> {
    let mut list = Vec::new();

    // 1. Toujours inclure le Dashboard Noos s'il possède une mise à niveau
    if dashboard_update_available {
        list.push(PackageUpdateItem {
            name: "noos-nas-dashboard".to_string(),
            current_version: format!("v{}", running_dashboard_version),
            new_version: target_dashboard_version.map(|v| format!("v{}", v)),
            action: "update".to_string(),
            size: None,
        });
    }

    // 2. Inclure les entrées Flake principales ayant des mises à jour détectées (nixpkgs, etc.)
    for input in flake_inputs {
        if input.has_update && input.name != "noos-nas-dashboard" {
            list.push(PackageUpdateItem {
                name: format!("Canal {}", input.name),
                current_version: input.locked_rev.clone(),
                new_version: input.remote_rev.clone(),
                action: "flake".to_string(),
                size: None,
            });
        }
    }

    // 3. Si aucun changement n'est détecté nulle part, retourner
    if !inputs_have_updates && !config_has_updates && !dashboard_update_available {
        return list;
    }

    let dir_str = config_dir.display().to_string();
    let target_attr = format!("path:{}#nixosConfigurations.nas.config.system.build.toplevel", dir_str);
    let args = vec![
        "build",
        "--extra-experimental-features",
        "nix-command flakes",
        &target_attr,
        "--dry-run",
        "--refresh",
    ];

    if let Ok(out) = Command::new(nix_binary()).args(&args).output() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let combined = format!("{}\n{}", stdout, stderr);
        let parsed = parse_nix_dry_run(&combined);
        for item in parsed {
            if !list.iter().any(|existing| existing.name == item.name) {
                list.push(item);
            }
        }
    }

    list
}

fn parse_nix_dry_run(output: &str) -> Vec<PackageUpdateItem> {
    let mut items = Vec::new();
    let mut current_action = "update".to_string();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("these ") && trimmed.contains("paths will be fetched") {
            current_action = "download".to_string();
            continue;
        } else if trimmed.starts_with("this path will be fetched") {
            current_action = "download".to_string();
            continue;
        } else if trimmed.starts_with("these ") && trimmed.contains("derivations will be built") {
            current_action = "build".to_string();
            continue;
        } else if trimmed.starts_with("this derivation will be built") {
            current_action = "build".to_string();
            continue;
        }

        if trimmed.starts_with("/nix/store/") {
            let path_clean = trimmed.trim_end_matches(".drv");
            let store_name = path_clean.strip_prefix("/nix/store/").unwrap_or(path_clean);
            if let Some(dash_idx) = store_name.find('-') {
                let pkg_and_ver = &store_name[dash_idx + 1..];
                let (name, ver) = split_pkg_name_and_version(pkg_and_ver);
                if !name.is_empty() 
                    && !name.starts_with("system-units") 
                    && !name.starts_with("etc") 
                    && !name.starts_with("unit-") 
                    && !name.starts_with("user-units")
                {
                    if !items.iter().any(|i: &PackageUpdateItem| i.name == name) {
                        items.push(PackageUpdateItem {
                            name: name.to_string(),
                            current_version: "Précédent / Installé".to_string(),
                            new_version: Some(ver.to_string()),
                            action: current_action.clone(),
                            size: None,
                        });
                    }
                }
            }
        }
    }
    items
}

fn split_pkg_name_and_version(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i - 1] == b'-' && bytes[i].is_ascii_digit() {
            return (&s[..i - 1], &s[i..]);
        }
    }
    (s, "dernière version")
}

pub fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() >= 3 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts[2].parse().ok()?;
        Some((major, minor, patch))
    } else if parts.len() == 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        Some((major, minor, 0))
    } else {
        None
    }
}

pub fn compare_semver(a: &str, b: &str) -> i32 {
    match (parse_semver(a), parse_semver(b)) {
        (Some(sa), Some(sb)) => {
            if sa.0 != sb.0 {
                sa.0.cmp(&sb.0) as i32
            } else if sa.1 != sb.1 {
                sa.1.cmp(&sb.1) as i32
            } else {
                sa.2.cmp(&sb.2) as i32
            }
        }
        _ => a.cmp(b) as i32,
    }
}

pub fn is_valid_semver(s: &str) -> bool {
    parse_semver(s).is_some()
}


pub fn execute_secure_git_pull(config_dir: &Path, log: &mut String) -> Result<(), String> {
    log.push_str("--- [Étape 1/3] Sécurisation de l'espace de travail local ---\n");

    let dir_str = config_dir.display().to_string();
    let vars_path = config_dir.join("vars.nix");
    let vars_backup_path = config_dir.join(".vars.nix.backup");
    let vars_local_path = config_dir.join("vars.local.nix");
    let vars_temp_backup = PathBuf::from("/tmp/noos-vars.backup");

    // 0. Sanctuarisation préalable et détection de la configuration utilisateur de l'hôte
    let mut saved_vars_content: Option<String> = None;
    if vars_path.exists() {
        if let Ok(c) = fs::read_to_string(&vars_path) {
            if !c.contains("<<<<<<<") && (c.contains("username") || c.contains("hostName")) {
                saved_vars_content = Some(c.clone());
                let _ = fs::write(&vars_backup_path, &c);
                let _ = fs::write(&vars_temp_backup, &c);
            }
        }
    }
    if saved_vars_content.is_none() || saved_vars_content.as_ref().map(|s| !s.contains("username")).unwrap_or(true) {
        if let Ok(c) = fs::read_to_string(&vars_local_path) {
            if c.contains("username") && !c.contains("<<<<<<<") {
                saved_vars_content = Some(c);
            }
        }
    }
    if saved_vars_content.is_none() || saved_vars_content.as_ref().map(|s| !s.contains("username")).unwrap_or(true) {
        if let Ok(c) = fs::read_to_string(&vars_backup_path) {
            if c.contains("username") && !c.contains("<<<<<<<") {
                saved_vars_content = Some(c);
            }
        }
    }
    if saved_vars_content.is_none() || saved_vars_content.as_ref().map(|s| !s.contains("username")).unwrap_or(true) {
        if let Ok(c) = fs::read_to_string("/tmp/noos-vars.backup") {
            if c.contains("username") && !c.contains("<<<<<<<") {
                saved_vars_content = Some(c);
            }
        }
    }
    if saved_vars_content.is_none() || saved_vars_content.as_ref().map(|s| !s.contains("username")).unwrap_or(true) {
        let stash_show = git_cmd(&dir_str).args(["show", "stash@{0}:vars.nix"]).output();
        if let Ok(o) = stash_show {
            let c = String::from_utf8_lossy(&o.stdout).to_string();
            if c.contains("username") && !c.contains("<<<<<<<") {
                saved_vars_content = Some(c);
            }
        }
    }

    // Si aucun bloc user n'a pu être retrouvé (ex: vars.nix déjà écrasé lors d'une précédente maj),
    // détecter automatiquement le compte personnel de l'utilisateur créé à l'installation (ex: 'steve')
    if saved_vars_content.is_none() || saved_vars_content.as_ref().map(|s| !s.contains("username")).unwrap_or(true) {
        if let Some(real_user) = find_first_human_user() {
            log.push_str(&format!("Détection automatique du compte administrateur créé à l'installation : '{}'...\n", real_user));
            let cur = if vars_path.exists() {
                fs::read_to_string(&vars_path).unwrap_or_default()
            } else {
                String::new()
            };
            let user_block = format!(
"  user = {{\n    username = \"{}\";\n    fullName = \"{}\";\n    homeDirectory = \"/home/{}\";\n    shell = \"bash\";\n  }};\n",
                real_user, real_user, real_user
            );
            let merged = if cur.contains('}') {
                cur.replacen('}', &format!("{}\n}}", user_block), 1)
            } else {
                format!("{{\n{}}}\n", user_block)
            };
            saved_vars_content = Some(merged);
        }
    }

    // Aligner préventivement flake.lock et les fichiers d'état déclaratifs avec le dépôt Git
    let _ = git_cmd(&dir_str).args(["checkout", "--", "flake.lock", "firewall-state.json", "firewall-rules.json"]).output();

    // 1. Sauvegarde automatique du commit courant
    let current_sha = git_cmd(&dir_str)
        .args(["rev-parse", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    log.push_str(&format!("Point de restauration courant : {}\n", &current_sha[..8.min(current_sha.len())]));

    // Sauvegarde en mémoire des fichiers d'état locaux déclaratifs et de la configuration matérielle s'ils existent
    let saved_firewall_state = fs::read_to_string(config_dir.join("firewall-state.json")).ok();
    let saved_firewall_rules = fs::read_to_string(config_dir.join("firewall-rules.json")).ok();
    let saved_hw_root = fs::read_to_string(config_dir.join("hardware-configuration.nix")).ok();
    let saved_hw_nas = fs::read_to_string(config_dir.join("hosts/nas/hardware-configuration.nix")).ok();
    let saved_hw_local = fs::read_to_string(config_dir.join("hosts/nas/hardware.local.nix")).ok();

    // 2. Détection de la branche locale courante
    let current_branch = git_cmd(&dir_str)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "main".to_string());

    // 3. Fetch et bascule sécurisée selon le canal actif
    let active_channel = get_update_channel();
    let preferred_pull_branch = if active_channel == "testing" { "testing" } else { "main" };
    log.push_str(&format!("\n--- [Étape 2/3] Récupération des nouveautés depuis GitHub (noos-nas, canal {}) ---\n", active_channel));

    let refspec = format!("+refs/heads/{}:refs/remotes/origin/{}", preferred_pull_branch, preferred_pull_branch);
    let mut fetch_res = git_cmd(&dir_str)
        .args(["fetch", "origin", &refspec])
        .output();

    let pull_branch = if fetch_res.as_ref().map(|o| !o.status.success()).unwrap_or(true) && preferred_pull_branch != "main" {
        log.push_str("Branche testing non trouvée sur origin, repli automatique sur la branche main...\n");
        let main_refspec = "+refs/heads/main:refs/remotes/origin/main";
        fetch_res = git_cmd(&dir_str)
            .args(["fetch", "origin", main_refspec])
            .output();
        "main"
    } else {
        preferred_pull_branch
    };

    let fetch_out = fetch_res.map_err(|e| format!("Échec git fetch : {}", e))?;

    if !fetch_out.status.success() {
        let err = String::from_utf8_lossy(&fetch_out.stderr);
        return Err(format!("Erreur lors de la récupération distante : {}", err));
    }

    let origin_target = "FETCH_HEAD";

    // Si on change de branche (ex: main -> testing ou testing -> main), basculer proprement
    if current_branch != pull_branch {
        log.push_str(&format!("Bascule de branche : alignement de '{}' vers '{}'...\n", current_branch, pull_branch));
        // Nettoyage préventif des fichiers d'état pour permettre le checkout sans blocage
        let _ = git_cmd(&dir_str).args(["checkout", "--", "."]).output();

        let switch_out = git_cmd(&dir_str)
            .args(["checkout", "-B", pull_branch, &origin_target])
            .output();

        match switch_out {
            Ok(o) if o.status.success() => {
                log.push_str(&format!("✔ Branche locale basculée avec succès sur {}.\n", pull_branch));
            }
            _ => {
                log.push_str(&format!("Forçage du basculement sur {}...\n", pull_branch));
                let _ = git_cmd(&dir_str).args(["checkout", "-f", "-B", pull_branch, &origin_target]).output();
            }
        }
    } else {
        // Même branche : tentative de fast-forward, et si divergence sur flake.lock/commits locaux, reset dur sur origin
        let merge_out = git_cmd(&dir_str)
            .args(["merge", "--ff-only", &origin_target])
            .output();

        let ff_success = merge_out.as_ref().map(|o| o.status.success()).unwrap_or(false);
        if ff_success {
            log.push_str("Mise à jour rapide appliquée avec succès (fast-forward).\n");
        } else {
            log.push_str(&format!("Divergence locale détectée. Alignement sécurisé de la branche {} sur {}...\n", pull_branch, origin_target));
            let _ = git_cmd(&dir_str).args(["reset", "--hard", &origin_target]).output();
        }
    }

    // Restauration des fichiers de règles de pare-feu sauvegardés s'ils existaient
    if let Some(rules) = saved_firewall_rules {
        let _ = fs::write(config_dir.join("firewall-rules.json"), rules);
    }
    if let Some(state) = saved_firewall_state {
        let _ = fs::write(config_dir.join("firewall-state.json"), state);
    }

    // 4 bis. 🛡️ SANCTUARISATION ET RESTAURATION INCONDITIONNELLE DE VARS.NIX
    if let Some(ref saved) = saved_vars_content {
        let mut final_content = saved.clone();
        if final_content.contains("hostName = \"steveos-nas\"") {
            final_content = final_content.replace("hostName = \"steveos-nas\"", "hostName = \"noos-nas\"");
        }
        let current = fs::read_to_string(&vars_path).unwrap_or_default();
        let needs_restore = current.contains("<<<<<<<")
            || current.contains(">>>>>>>")
            || (!current.contains("username") && final_content.contains("username"))
            || current.contains("hostName = \"steveos-nas\"");

        if needs_restore {
            log.push_str("Rétablissement garanti de vos paramètres hôte et utilisateur dans vars.nix...\n");
            let _ = fs::write(&vars_path, &final_content);
        }
        let _ = fs::write(&vars_local_path, &final_content);
        let _ = fs::write(&vars_backup_path, &final_content);
    }

    // 4 ter. 🛡️ SANCTUARISATION ET RESTAURATION INCONDITIONNELLE DU MATÉRIEL LOCAL
    if let Some(ref hw) = saved_hw_root {
        let _ = fs::write(config_dir.join("hardware-configuration.nix"), hw);
    }
    if let Some(ref hw) = saved_hw_local {
        let _ = fs::write(config_dir.join("hosts/nas/hardware.local.nix"), hw);
    } else if let Some(ref hw) = saved_hw_nas {
        // Si hardware.local.nix n'existait pas encore mais que le fichier local contenait des définitions spécifiques,
        // sanctuariser vers hardware.local.nix (protégé par .gitignore)
        if hw.contains("fileSystems") {
            let _ = fs::write(config_dir.join("hosts/nas/hardware.local.nix"), hw);
        }
    }

    // Nettoyer d'éventuels marqueurs de conflit résiduels sur vars.nix ou flake.lock
    let unmerged = git_cmd(&dir_str)
        .args(["diff", "--name-only", "--diff-filter=U"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if !unmerged.is_empty() {
        for file in unmerged.lines() {
            let f = file.trim();
            if f == "vars.nix" {
                if let Some(ref saved) = saved_vars_content {
                    let mut f_content = saved.clone();
                    if f_content.contains("hostName = \"steveos-nas\"") {
                        f_content = f_content.replace("hostName = \"steveos-nas\"", "hostName = \"noos-nas\"");
                    }
                    let _ = fs::write(&vars_path, &f_content);
                    let _ = git_cmd(&dir_str).args(["add", "vars.nix"]).output();
                }
            } else if f == "flake.lock" || f == "firewall-state.json" || f == "firewall-rules.json" {
                let _ = git_cmd(&dir_str).args(["checkout", "--theirs", f]).output();
                let _ = git_cmd(&dir_str).args(["add", f]).output();
            }
        }
    }

    // Indexer vars.nix pour que Nix Flake l'évalue sans conflit
    let _ = git_cmd(&dir_str).args(["add", "vars.nix"]).output();

    // 5. Validation de la syntaxe Nix (nix eval de sécurité sur la dérivation complète)
    log.push_str("\n--- [Étape 3/3] Validation de la syntaxe et de la configuration Nix ---\n");
    let nix_b = nix_binary();
    let eval_target = format!("path:{}#nixosConfigurations.nas.config.system.build.toplevel.drvPath", dir_str);
    let mut eval_cmd = create_switch_command(&nix_b, &["eval", "--extra-experimental-features", "nix-command flakes", &eval_target], config_dir);
    let eval_res = eval_cmd.output();

    match eval_res {
        Ok(out) if out.status.success() => {
            log.push_str("✔ Configuration système NixOS validée avec succès.\n");
            Ok(())
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            log.push_str(&format!("⚠ Erreur d'évaluation détectée :\n{}\nAnnulation du pull...\n", err));
            let _ = git_cmd(&dir_str).args(["reset", "--hard", &current_sha]).output();
            if let Some(ref saved) = saved_vars_content {
                let _ = fs::write(&vars_path, saved);
                let _ = git_cmd(&dir_str).args(["add", "vars.nix"]).output();
            }
            Err(format!("La nouvelle configuration contient une erreur d'évaluation Nix. Rollback de sécurité effectué : {}", err))
        }
        Err(e) => {
            Err(format!("Impossible de valider la syntaxe Nix : {}", e))
        }
    }
}

pub fn apply_intelligent_update(force_packages: bool) -> ApplyUpdateResult {
    if IS_UPDATING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return ApplyUpdateResult {
            success: false,
            steps_executed: vec![],
            output_log: "Une mise à jour est déjà en cours d'exécution. Veuillez patienter.".into(),
            error: Some("Mise à jour déjà en cours".into()),
        };
    }

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            IS_UPDATING.store(false, Ordering::SeqCst);
            if let Ok(mut g) = UPDATE_CACHE.lock() {
                *g = None;
            }
        }
    }
    let _guard = Guard;

    let mut steps_executed = Vec::new();
    let mut output_log = String::new();

    let cached = get_cached_status();
    let config_dir = resolve_config_dir();
    let dir_str = config_dir.display().to_string();

    let update_type = if force_packages {
        UpdateType::PackagesOnly
    } else if let Some(ref c) = cached {
        if c.update_type == UpdateType::None {
            UpdateType::PackagesOnly
        } else {
            c.update_type.clone()
        }
    } else {
        UpdateType::Both
    };

    let header = format!(
        "==========================================================\n🚀 Lancement de la mise à jour intelligente Noos\n   Mode détecté : {:?}\n   Répertoire   : {}\n==========================================================\n\n",
        update_type, dir_str
    );

    if let Ok(mut log) = LIVE_UPDATE_LOG.lock() {
        log.clear();
        log.push_str(&header);
    }
    let _ = std::fs::write(get_update_log_path(), &header);
    output_log.push_str(&header);

    match update_type {
        UpdateType::None => {
            output_log.push_str("✨ Le système et la configuration sont déjà parfaitement à jour.\n");
            return ApplyUpdateResult {
                success: true,
                steps_executed: vec!["Vérification effectuée : aucun changement".into()],
                output_log,
                error: None,
            };
        }

        UpdateType::ConfigOnly => {
            // 1. D'abord le git pull sécurisé
            steps_executed.push("Git pull sécurisé de la configuration".into());
            if let Err(e) = execute_secure_git_pull(&config_dir, &mut output_log) {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some(e),
                };
            }

            // 2. nixos-rebuild switch
            steps_executed.push("Déploiement système : nixos-rebuild switch".into());
            output_log.push_str("\n--- Exécution de nixos-rebuild switch ---\n");
            let res = run_switch_command(&config_dir, false);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nixos-rebuild switch".into()),
                };
            }
        }

        UpdateType::PackagesOnly => {
            // 1. nixos-rebuild switch -u directement
            steps_executed.push("Mise à jour des paquets : nixos-rebuild switch -u".into());
            output_log.push_str("--- Exécution de nixos-rebuild switch -u ---\n");
            let res = run_switch_command(&config_dir, true);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nixos-rebuild switch -u".into()),
                };
            }
        }

        UpdateType::Both => {
            // 1. D'abord le git pull sécurisé
            steps_executed.push("1. Git pull sécurisé de la nouvelle configuration".into());
            if let Err(e) = execute_secure_git_pull(&config_dir, &mut output_log) {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some(e),
                };
            }

            // 2. Ensuite nixos-rebuild switch -u
            steps_executed.push("2. Déploiement système : nixos-rebuild switch".into());
            output_log.push_str("\n--- Exécution de nixos-rebuild switch -u (Configuration + Paquets) ---\n");
            let res = run_switch_command(&config_dir, false);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nixos-rebuild switch -u".into()),
                };
            }
        }
    }

    output_log.push_str("\n✔ Mise à jour terminée avec succès !\n");
    append_live_log("\n✔ Mise à jour terminée avec succès !\n");

    if let Ok(mut guard) = UPDATE_CACHE.lock() {
        let (short_c, full_c) = get_local_commit_from_fs(&config_dir);
        let clean_os_telemetry = OsTelemetry {
            repo_name: "Chomiam/noos-nas".to_string(),
            local_commit: short_c.clone(),
            local_commit_full: full_c.clone(),
            remote_commit: Some(short_c.clone()),
            remote_commit_full: Some(full_c.clone()),
            commit_message: Some("Configuration système NixOS synchronisée.".to_string()),
            commits_behind: 0,
            pending_commits: vec![],
            changed_files: vec![],
            git_status: "Arbre de travail propre".to_string(),
            update_available: false,
        };
        let clean_dash_ver = env!("CARGO_PKG_VERSION").to_string();
        let clean_dashboard_telemetry = DashboardTelemetry {
            repo_name: "Chomiam/noos-nas-dashboard".to_string(),
            running_version: clean_dash_ver.clone(),
            running_commit: None,
            target_version: Some(clean_dash_ver.clone()),
            target_commit: None,
            remote_commit: None,
            update_available: false,
            status_text: format!("À jour (version v{} active)", clean_dash_ver),
        };
        let clean_status = UpdateCheckStatus {
            os_telemetry: clean_os_telemetry,
            dashboard_telemetry: clean_dashboard_telemetry,
            dashboard_update_available: false,
            config_update_available: false,
            config_local_commit: short_c,
            config_local_commit_full: full_c,
            config_remote_commit: None,
            config_remote_commit_full: None,
            config_commit_message: None,
            config_commits_behind: 0,
            config_pending_commits: vec![],
            config_changed_files: vec![],
            config_git_status: "Arbre de travail propre".to_string(),
            package_updates_available: false,
            package_updates_count: 0,
            package_details: vec![],
            flake_inputs_status: vec![],
            package_updates_list: vec![],
            update_type: UpdateType::None,
            status_text: "✨ Système et configuration à jour".to_string(),
            config_dir: dir_str,
            last_checked: current_time_formatted(),
            is_updating: false,
            system_generation: get_current_system_generation(),
            system_generations_count: get_system_generations_count(),
            channel: get_update_channel(),
        };
        *guard = Some((Instant::now(), clean_status));
    }

    ApplyUpdateResult {
        success: true,
        steps_executed,
        output_log,
        error: None,
    }
}

pub fn start_detached_update(force_packages: bool) -> Result<(), String> {
    let current_state = get_update_progress();
    if current_state.is_running {
        return Err("Une mise à jour est déjà en cours d'exécution.".to_string());
    }

    let cur_gen = get_current_system_generation();
    let initial_state = UpdateProgressState {
        is_running: true,
        stage: "starting".to_string(),
        step_index: 1,
        total_steps: 4,
        progress_percent: 5,
        status_title: "Initialisation de la mise à jour...".to_string(),
        status_detail: "Préparation de l'environnement d'exécution...".to_string(),
        error: None,
        started_at: Some(current_time_formatted()),
        completed_at: None,
        completed_timestamp: None,
        target_commit: None,
        generation_before: cur_gen,
        generation_after: None,
        dashboard_restarting: false,
        log_tail: "🚀 Démarrage de la mise à jour Noos...\n".to_string(),
        total_derivations: None,
        current_derivation_index: None,
        current_package_name: None,
        failed_units: Vec::new(),
        warning: None,
    };
    save_update_progress(&initial_state);

    let exe = if Path::new("/run/current-system/sw/bin/noos-nas-dashboard").exists() {
        PathBuf::from("/run/current-system/sw/bin/noos-nas-dashboard")
    } else {
        env::current_exe().unwrap_or_else(|_| PathBuf::from("/proc/self/exe"))
    };
    let exe_str = exe.display().to_string();

    let mut extra_args = vec![];
    if force_packages {
        extra_args.push("--force-packages");
    }

    // 1. systemd-run si disponible (cgroup indépendant)
    let systemd_run = "/run/current-system/sw/bin/systemd-run";
    if Path::new(systemd_run).exists() {
        let mut cmd = Command::new(systemd_run);
        let complete_path = "/run/wrappers/bin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin";
        cmd.args([
            "--unit=noos-system-update",
            "--property=KillMode=process",
            &format!("--setenv=PATH={}", complete_path),
            "--setenv=NIX_PATH=nixpkgs=flake:nixpkgs",
            "--description=Noos System Update Runner",
            "--",
            &exe_str,
            "--run-system-update",
        ]);
        for a in &extra_args {
            cmd.arg(a);
        }
        if let Ok(status) = cmd.status() {
            if status.success() {
                return Ok(());
            }
        }
    }

    // 2. Fallback thread indépendant si systemd-run a échoué
    std::thread::spawn(move || {
        run_detached_update_process(force_packages);
    });

    Ok(())
}

pub fn run_detached_update_process(force_packages: bool) {
    let mut state = get_update_progress();
    state.is_running = true;
    let config_dir = resolve_config_dir();
    let dir_str = config_dir.display().to_string();

    let cached = get_cached_status();
    let update_type = if force_packages {
        UpdateType::PackagesOnly
    } else if let Some(ref c) = cached {
        if c.update_type == UpdateType::None {
            UpdateType::PackagesOnly
        } else {
            c.update_type.clone()
        }
    } else {
        UpdateType::Both
    };

    // --- Étape 1 : Git pull si nécessaire ---
    if update_type == UpdateType::ConfigOnly || update_type == UpdateType::Both {
        state.stage = "git_pull".to_string();
        state.step_index = 1;
        state.total_steps = 4;
        state.progress_percent = 8;
        state.status_title = "Synchronisation de la configuration...".to_string();
        state.status_detail = "Récupération des nouveautés depuis GitHub (noos-nas)...".to_string();
        save_update_progress(&state);

        let mut pull_log = String::new();
        if let Err(e) = execute_secure_git_pull(&config_dir, &mut pull_log) {
            state.is_running = false;
            state.stage = "failed".to_string();
            state.status_title = "Échec de synchronisation Git".to_string();
            state.status_detail = e.clone();
            state.error = Some(e);
            state.log_tail.push_str(&pull_log);
            save_update_progress(&state);
            return;
        }
        state.log_tail.push_str(&pull_log);
    }

    // --- Étape 2 : Déploiement système (nixos-rebuild switch) ---
    state.stage = "building".to_string();
    state.step_index = 2;
    state.total_steps = 4;
    state.progress_percent = 28;
    state.status_title = "Construction & téléchargement du système...".to_string();
    state.status_detail = "Compilation des dérivations NixOS et téléchargement des paquets binaires...".to_string();
    save_update_progress(&state);

    // Synchronisation proactive du hash du Dashboard et de nixpkgs dans flake.lock
    if cached.as_ref().map(|c| c.dashboard_update_available || c.package_updates_available).unwrap_or(false) || force_packages {
        let current_ch = get_update_channel();
        let target_branch = if current_ch == "testing" { "testing" } else { "stable" };
        append_live_log(&format!("Synchronisation de l'entrée flake du Dashboard (noos-nas-dashboard, canal {})...\n", current_ch));
        let nix_bin = nix_binary();
        let _ = Command::new(&nix_bin)
            .args(["flake", "update", "noos-nas-dashboard", "--override-input", "noos-nas-dashboard", &format!("github:Chomiam/noos-nas-dashboard/{}", target_branch)])
            .current_dir(&config_dir)
            .output();

        if force_packages || cached.as_ref().map(|c| c.package_updates_available).unwrap_or(false) {
            append_live_log("Mise à jour proactive de l'entrée nixpkgs dans flake.lock...\n");
            let _ = Command::new(&nix_bin)
                .args(["flake", "update", "nixpkgs"])
                .current_dir(&config_dir)
                .output();
        }

        // Indexer immédiatement flake.lock dans Git pour que nixos-rebuild prenne en compte les nouveautés
        let _ = git_cmd(&dir_str)
            .args(["add", "flake.lock"])
            .output();
    }

    let nixos_rebuild = nixos_rebuild_binary();
    let mut args = vec!["switch", "--flake", &dir_str];
    // Ne jamais écraser flake.lock avec --recreate-lock-file :
    // 1. flake.lock a été validé ou mis à jour via Git.
    // 2. --recreate-lock-file est déprécié et provoque un contournement vers le cache tarball Nix (~/.cache/nix/tarballs).
    if force_packages {
        args.push("--refresh");
    }
    let mut cmd = create_switch_command(&nixos_rebuild, &args, &config_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut switch_success = false;

    if let Ok(mut child) = cmd.spawn() {
        use std::io::BufRead;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel::<String>();

        let tx_out = tx.clone();
        if let Some(out) = stdout {
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(out);
                for line in reader.lines().flatten() {
                    let _ = tx_out.send(line);
                }
            });
        }

        let tx_err = tx.clone();
        if let Some(err) = stderr {
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(err);
                for line in reader.lines().flatten() {
                    let _ = tx_err.send(line);
                }
            });
        }
        drop(tx);

        let mut lines = Vec::new();
        let mut total_derivations: Option<u32> = None;
        let mut current_derivation_index: u32 = 0;

        for line in rx {
            lines.push(line.clone());
            append_live_log(&format!("{}
", line));

            // 1. Détection du nombre total de dérivations
            if line.contains("derivations will be built") {
                if let Some(num_str) = line.split_whitespace().nth(1) {
                    if let Ok(n) = num_str.parse::<u32>() {
                        total_derivations = Some(n);
                        state.total_derivations = Some(n);
                        state.progress_percent = 30.max(state.progress_percent);
                        state.status_detail = format!("{} dérivation(s) à compiler et assembler...", n);
                        save_update_progress(&state);
                    }
                }
            }

            // 2. Détection de compilation unitaire
            if line.contains("building '") && line.contains(".drv'") {
                current_derivation_index += 1;
                state.current_derivation_index = Some(current_derivation_index);

                let pkg_name = if let Some(start) = line.find("/nix/store/") {
                    let sub = &line[start + 11..];
                    if let Some(dash) = sub.find('-') {
                        let after_dash = &sub[dash + 1..];
                        if let Some(end) = after_dash.find(".drv") {
                            after_dash[..end].to_string()
                        } else {
                            after_dash.to_string()
                        }
                    } else {
                        sub.to_string()
                    }
                } else {
                    line.clone()
                };

                state.current_package_name = Some(pkg_name.clone());

                if let Some(total) = total_derivations {
                    let ratio = (current_derivation_index as f32 / total.max(1) as f32).min(1.0);
                    // Échelle Étape 2 (Build NixOS) : de 30% à 68% (calibrée visuellement entre bulle 2 et bulle 3)
                    let calculated = 30 + (ratio * 38.0) as u32;
                    state.progress_percent = calculated.max(state.progress_percent);
                    state.status_title = format!("Construction du système [{}/{}]", current_derivation_index, total);
                    state.status_detail = format!("Compilation de {}...", pkg_name);
                } else {
                    state.progress_percent = 50.max(state.progress_percent);
                    state.status_detail = format!("Compilation de {}...", pkg_name);
                }
                save_update_progress(&state);
            }

            // 3. Détection de téléchargement binaire
            if line.contains("copying path '") || line.contains("paths will be fetched") {
                if let Some(start) = line.find("/nix/store/") {
                    let sub = &line[start + 11..];
                    let pkg_name = sub.split('-').nth(1).unwrap_or(sub);
                    let clean_pkg = pkg_name.trim_end_matches('\'').trim_end_matches("...");
                    state.status_detail = format!("Téléchargement du paquet binaire {}...", clean_pkg);
                } else {
                    state.status_detail = "Téléchargement des paquets binaires du système...".to_string();
                }
                state.progress_percent = 32.max(state.progress_percent);
                save_update_progress(&state);
            }

            // 4. Comparatif de fermeture
            if line.starts_with("<<< /nix/store/") || line.starts_with(">>> /nix/store/") {
                state.progress_percent = 70.max(state.progress_percent);
                state.status_title = "Vérification des paquets modifiés...".to_string();
                state.status_detail = "Comparaison de l'ancienne et de la nouvelle génération...".to_string();
                save_update_progress(&state);
            }

            // 5. Activation système (Étape 3 : Activation)
            if line.contains("Activating configuration") || line.contains("switching to system configuration") || line.contains("setting up /etc") {
                state.stage = "activating".to_string();
                state.step_index = 3;
                state.progress_percent = 75.max(state.progress_percent);
                state.status_title = "Activation de la configuration...".to_string();
                state.status_detail = "Mise à jour du bootloader et démarrage des services...".to_string();
                save_update_progress(&state);
            }

            if state.step_index >= 3 && (line.contains("reloading the following units:") || line.contains("restarting the following units:") || line.contains("starting the following units:")) {
                state.progress_percent = 85.max(state.progress_percent);
                save_update_progress(&state);
            }

            // 6. Redémarrage dashboard (strictement pendant l'activation, jamais pendant le build .drv !)
            if (state.step_index >= 3 || state.stage == "activating") &&
               (line.contains("stopping the following units:") && line.contains("noos-nas-dashboard")) ||
                line.contains("stopping noos-nas-dashboard") ||
                line.contains("restarting noos-nas-dashboard") {
                state.progress_percent = 92.max(state.progress_percent);
                state.dashboard_restarting = true;
                state.status_title = "Redémarrage du Dashboard...".to_string();
                state.status_detail = "Le nouveau service Web prend le relais...".to_string();
                save_update_progress(&state);
            }
        }

        let mut exit_code: Option<i32> = None;
        let mut child_wait_err: Option<String> = None;
        match child.wait() {
            Ok(status) => {
                switch_success = status.success();
                exit_code = status.code();
            }
            Err(e) => {
                child_wait_err = Some(e.to_string());
            }
        }

        let (failed_units, root_err) = extract_switch_diagnostics(exit_code, &lines);
        if !failed_units.is_empty() {
            append_live_log(&format!("\n⚠️ Diagnostic système : {} unité(s) systemd en échec : {}\n", failed_units.len(), failed_units.join(", ")));
            let systemctl_bin = find_bin(&[
                "/run/current-system/sw/bin/systemctl",
                "systemctl",
                "/usr/bin/systemctl",
            ]);
            for u in &failed_units {
                if let Ok(st) = Command::new(&systemctl_bin).args(["status", u, "--no-pager", "-n", "5"]).output() {
                    let st_text = String::from_utf8_lossy(&st.stdout);
                    if !st_text.trim().is_empty() {
                        append_live_log(&format!("--- [Journal systemd : {}] ---\n{}\n", u, st_text.trim()));
                    }
                }
            }
        }

        let cur_gen = get_current_system_generation();
        let gen_advanced = match (&cur_gen, &state.generation_before) {
            (Some(new_g), Some(old_g)) => new_g != old_g,
            (Some(_), None) => true,
            _ => false,
        };

        let full_output = if lines.len() > 80 {
            lines[lines.len() - 80..].join("\n")
        } else {
            lines.join("\n")
        };
        state.log_tail = full_output;

        // Une mise à jour avec code 2, 3 ou 4 où la génération NixOS a progressé
        // signifie que le système a bien été compilé, bootloader installé et profil basculé.
        let is_effective_update = switch_success || (gen_advanced && matches!(exit_code, Some(2) | Some(3) | Some(4)));

        if is_effective_update {
            // S'assurer que le service noos-nas-dashboard est bien relancé avec le nouveau binaire
            let _ = Command::new("/run/current-system/sw/bin/systemctl")
                .args(["try-restart", "noos-nas-dashboard.service"])
                .status();

            let now_ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            state.is_running = false;
            state.stage = "completed".to_string();
            state.step_index = 4;
            state.total_steps = 4;
            state.progress_percent = 100;
            state.generation_after = cur_gen.clone();
            state.completed_at = Some(current_time_formatted());
            state.completed_timestamp = Some(now_ts);
            state.dashboard_restarting = false;

            if !failed_units.is_empty() || !switch_success {
                let code_num = exit_code.unwrap_or(4);
                let failed_str = if failed_units.is_empty() { "services secondaires".to_string() } else { failed_units.join(", ") };
                state.status_title = "Mise à jour appliquée avec avertissements".to_string();
                state.status_detail = format!(
                    "Le système est actif sur la génération {}. Avertissement (code {}) : {} n'a/ont pas pu démarrer.",
                    cur_gen.unwrap_or_else(|| "suivante".to_string()),
                    code_num,
                    failed_str
                );
                state.warning = Some(format!("Code de sortie {}. Unités en échec : {}", code_num, failed_str));
                state.failed_units = failed_units;
            } else {
                state.status_title = "Mise à jour terminée avec succès !".to_string();
                state.status_detail = format!("Le système est actif sur la génération {}.", cur_gen.clone().unwrap_or_else(|| "suivante".to_string()));
            }
            save_update_progress(&state);
        } else {
            state.is_running = false;
            state.stage = "failed".to_string();
            state.status_title = "Échec de la mise à jour".to_string();
            state.failed_units = failed_units.clone();

            let code_num = exit_code.unwrap_or(-1);
            let detail = if !failed_units.is_empty() {
                format!(
                    "Le processus s'est terminé avec le code {}. Unité(s) en échec : {}. Consultez les journaux ci-dessous.",
                    code_num,
                    failed_units.join(", ")
                )
            } else if let Some(ref r_err) = root_err {
                format!("Le processus s'est terminé avec le code {} : {}", code_num, r_err)
            } else if let Some(ref w_err) = child_wait_err {
                format!("Erreur d'exécution du processus : {}", w_err)
            } else {
                format!("Le processus s'est terminé avec le code {}. Consultez le journal des opérations ci-dessous.", code_num)
            };

            state.status_detail = detail.clone();
            state.error = Some(detail);
            save_update_progress(&state);
        }
    } else {
        state.is_running = false;
        state.stage = "failed".to_string();
        state.status_title = "Échec de la mise à jour".to_string();
        let err_msg = "Impossible de lancer la commande de déploiement (nixos-rebuild).".to_string();
        state.status_detail = err_msg.clone();
        state.error = Some(err_msg);
        save_update_progress(&state);
    }
}

pub fn extract_switch_diagnostics(_exit_code: Option<i32>, lines: &[String]) -> (Vec<String>, Option<String>) {
    let mut failed_units: Vec<String> = Vec::new();
    let mut root_error: Option<String> = None;

    for line in lines {
        let trimmed = line.trim();

        // 1. Détection des avertissements de switch-to-configuration NixOS
        if trimmed.contains("the following units failed:")
            || trimmed.contains("the following units failed to start:")
            || trimmed.contains("the following units failed to restart:")
            || trimmed.contains("the following units failed to reload:")
            || trimmed.contains("failed to start the following units:")
        {
            if let Some(pos) = trimmed.find(':') {
                let units_part = &trimmed[pos + 1..];
                for u in units_part.split(',') {
                    let cleaned = u.trim().trim_matches('\'').trim_matches('"');
                    if !cleaned.is_empty() && !failed_units.contains(&cleaned.to_string()) {
                        failed_units.push(cleaned.to_string());
                    }
                }
            }
        } else if trimmed.starts_with("Failed to start ") || trimmed.starts_with("Failed to restart ") {
            let u = trimmed
                .trim_start_matches("Failed to start ")
                .trim_start_matches("Failed to restart ")
                .trim()
                .trim_end_matches('.');
            if !u.is_empty() && !failed_units.contains(&u.to_string()) {
                failed_units.push(u.to_string());
            }
        } else if trimmed.starts_with("× ") {
            let rest = trimmed.trim_start_matches("× ");
            if let Some(unit) = rest.split_whitespace().next() {
                let u = unit.trim().to_string();
                if (u.ends_with(".service") || u.ends_with(".mount") || u.ends_with(".target") || u.ends_with(".socket"))
                    && !failed_units.contains(&u)
                {
                    failed_units.push(u);
                }
            }
        }

        // 2. Détection d'erreurs de montage ou de filesystem
        if (trimmed.starts_with("mount:") || trimmed.contains("wrong fs type") || trimmed.contains("Failed to mount"))
            && root_error.is_none()
        {
            root_error = Some(trimmed.to_string());
        }

        // 3. Détection d'erreurs Nix générales
        if (trimmed.starts_with("error:") || trimmed.contains("builder for") && trimmed.contains("failed"))
            && root_error.is_none()
        {
            root_error = Some(trimmed.to_string());
        }
    }

    // 4. Interrogation directe de systemctl si des unités sont en échec
    let systemctl_bin = find_bin(&[
        "/run/current-system/sw/bin/systemctl",
        "systemctl",
        "/usr/bin/systemctl",
    ]);
    if let Ok(out) = Command::new(&systemctl_bin)
        .args(["--failed", "--no-legend", "--plain"])
        .output()
    {
        if out.status.success() {
            let stdout_str = String::from_utf8_lossy(&out.stdout);
            for l in stdout_str.lines() {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if let Some(unit) = parts.first() {
                    let u = unit.trim().trim_start_matches('●').trim().to_string();
                    if (u.ends_with(".service") || u.ends_with(".mount") || u.ends_with(".target") || u.ends_with(".socket"))
                        && !failed_units.contains(&u)
                    {
                        failed_units.push(u);
                    }
                }
            }
        }
    }

    (failed_units, root_error)
}


pub fn create_switch_command(bin: &str, args: &[&str], config_dir: &Path) -> Command {
    let is_root = is_root_process();
    let complete_path = "/run/wrappers/bin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin";
    if is_root {
        let mut cmd = Command::new(bin);
        cmd.args(args);
        cmd.current_dir(config_dir);
        cmd.env("PATH", complete_path);
        cmd.env("HOME", "/root");
        cmd.env("USER", "root");
        cmd.env("NH_FLAKE", config_dir.display().to_string());
        cmd.env("NH_BYPASS_ROOT_CHECK", "1");
        cmd.env("NH_ELEVATION_STRATEGY", "none");
        cmd
    } else {
        let mut cmd = create_user_command(bin, args);
        cmd.current_dir(config_dir);
        cmd
    }
}

fn run_switch_command(config_dir: &Path, update_inputs: bool) -> (bool, String) {
    let dir_str = config_dir.display().to_string();

    let nixos_rebuild = nixos_rebuild_binary();
    let mut args = vec!["switch", "--flake", &dir_str];
    if update_inputs {
        let current_ch = get_update_channel();
        let nix_bin = nix_binary();
        if current_ch == "testing" {
            let _ = Command::new(&nix_bin)
                .args(["flake", "lock", "--override-input", "noos-nas-dashboard", "github:Chomiam/noos-nas-dashboard/testing"])
                .current_dir(config_dir)
                .output();
        } else {
            let _ = Command::new(&nix_bin)
                .args(["flake", "lock", "--override-input", "noos-nas-dashboard", "github:Chomiam/noos-nas-dashboard/stable"])
                .current_dir(config_dir)
                .output();
        }
        args.push("--refresh");
    }
    let mut cmd = create_switch_command(&nixos_rebuild, &args, config_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            use std::io::BufRead;
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let stdout_handle = std::thread::spawn(move || {
                let mut out_str = String::new();
                if let Some(out) = stdout {
                    let reader = std::io::BufReader::new(out);
                    for line in reader.lines().flatten() {
                        let l = format!("{}\n", line);
                        append_live_log(&l);
                        out_str.push_str(&l);
                    }
                }
                out_str
            });

            let stderr_handle = std::thread::spawn(move || {
                let mut err_str = String::new();
                if let Some(err) = stderr {
                    let reader = std::io::BufReader::new(err);
                    for line in reader.lines().flatten() {
                        let l = format!("{}\n", line);
                        append_live_log(&l);
                        err_str.push_str(&l);
                    }
                }
                err_str
            });

            let out_text = stdout_handle.join().unwrap_or_default();
            let err_text = stderr_handle.join().unwrap_or_default();
            let mut combined = out_text;
            if !err_text.is_empty() {
                combined.push_str(&err_text);
            }

            let status = child.wait().map(|s| s.success()).unwrap_or(false);
            let cleaned = sanitize_terminal_output(&combined);
            (status, cleaned)
        }
        Err(e) => (false, format!("Impossible d'exécuter {} : {}", nixos_rebuild, e)),
    }
}

fn sanitize_terminal_output(raw: &str) -> String {
    let mut out_lines = Vec::new();
    for line in raw.split('\n') {
        if line.contains('\r') {
            let parts: Vec<&str> = line.split('\r').filter(|p| !p.trim().is_empty()).collect();
            if let Some(last) = parts.last() {
                out_lines.push(last.to_string());
            } else {
                out_lines.push(String::new());
            }
        } else {
            out_lines.push(line.to_string());
        }
    }
    out_lines.join("\n")
}

fn current_time_formatted() -> String {
    let output = Command::new("date")
        .args(["+%H:%M:%S"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "Récemment".into());
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver_valid_three_digits() {
        assert_eq!(parse_semver("0.3.32"), Some((0, 3, 32)));
        assert_eq!(parse_semver("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_semver("26.05.1"), Some((26, 5, 1)));
    }

    #[test]
    fn test_parse_semver_two_digits() {
        assert_eq!(parse_semver("0.3"), Some((0, 3, 0)));
        assert_eq!(parse_semver("26.05"), Some((26, 5, 0)));
    }

    #[test]
    fn test_parse_semver_invalid() {
        assert_eq!(parse_semver("invalid"), None);
        assert_eq!(parse_semver(""), None);
        assert_eq!(parse_semver("v1.2.3"), None);
    }

    #[test]
    fn test_compare_semver_equal() {
        assert_eq!(compare_semver("0.3.32", "0.3.32"), 0);
    }

    #[test]
    fn test_compare_semver_greater_patch() {
        assert!(compare_semver("0.3.33", "0.3.32") > 0);
    }

    #[test]
    fn test_compare_semver_greater_minor() {
        assert!(compare_semver("0.4.0", "0.3.32") > 0);
    }

    #[test]
    fn test_compare_semver_greater_major() {
        assert!(compare_semver("1.0.0", "0.9.9") > 0);
    }

    #[test]
    fn test_compare_semver_less() {
        assert!(compare_semver("0.3.29", "0.3.32") < 0);
    }

    #[test]
    fn test_split_pkg_name_and_version_simple() {
        let (name, ver) = split_pkg_name_and_version("htop-3.3.0");
        assert_eq!(name, "htop");
        assert_eq!(ver, "3.3.0");
    }

    #[test]
    fn test_split_pkg_name_and_version_with_subnames() {
        let (name, ver) = split_pkg_name_and_version("noos-nas-dashboard-0.3.32");
        assert_eq!(name, "noos-nas-dashboard");
        assert_eq!(ver, "0.3.32");
    }

    #[test]
    fn test_split_pkg_name_and_version_with_dashes_in_version() {
        let (name, ver) = split_pkg_name_and_version("glibc-2.39-52");
        assert_eq!(name, "glibc");
        assert_eq!(ver, "2.39-52");
    }

    #[test]
    fn test_split_pkg_name_and_version_no_digits() {
        let (name, ver) = split_pkg_name_and_version("custom-package");
        assert_eq!(name, "custom-package");
        assert_eq!(ver, "dernière version");
    }

    #[test]
    fn test_parse_nix_dry_run_fetches_and_builds() {
        let sample = r#"
these 2 paths will be fetched (25.4 MiB download, 89.2 MiB unpacked):
  /nix/store/h9ab3j68r7987p1r9km31a61y79a29y0-linux-6.6.21
  /nix/store/5kl3m930c2j21k29sm2918am2901a09z-openssh-9.7p1
these 1 derivations will be built:
  /nix/store/a812m10s921j291m0192a0912ma0912m-noos-nas-dashboard-0.3.32.drv
"#;
        let items = parse_nix_dry_run(sample);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].name, "linux");
        assert_eq!(items[0].new_version, Some("6.6.21".into()));
        assert_eq!(items[0].action, "download");
        assert_eq!(items[1].name, "openssh");
        assert_eq!(items[1].new_version, Some("9.7p1".into()));
        assert_eq!(items[1].action, "download");
        assert_eq!(items[2].name, "noos-nas-dashboard");
        assert_eq!(items[2].new_version, Some("0.3.32".into()));
        assert_eq!(items[2].action, "build");
    }

    #[test]
    fn test_parse_nix_dry_run_filters_system_units() {
        let sample = r#"
these 2 paths will be fetched:
  /nix/store/11111111111111111111111111111111-system-units
  /nix/store/22222222222222222222222222222222-unit-sshd.service
  /nix/store/33333333333333333333333333333333-etc
  /nix/store/44444444444444444444444444444444-btop-1.3.2
"#;
        let items = parse_nix_dry_run(sample);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "btop");
    }

    #[test]
    fn test_detect_package_updates_list_includes_dashboard_when_newer() {
        let dummy_path = Path::new("/tmp/noos-test-nonexistent");
        let list = detect_package_updates_list(
            dummy_path,
            false,
            false,
            true, // dashboard update available
            "0.3.29",
            Some("0.3.33"),
            &[],
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "noos-nas-dashboard");
        assert_eq!(list[0].current_version, "v0.3.29");
        assert_eq!(list[0].new_version, Some("v0.3.33".into()));
    }

    #[test]
    fn test_detect_package_updates_list_includes_flake_inputs() {
        let dummy_path = Path::new("/tmp/noos-test-nonexistent");
        let flake_inputs = vec![
            FlakeInputStatus {
                name: "nixpkgs".into(),
                locked_rev: "7fc6f2c".into(),
                remote_rev: Some("8ab92e1".into()),
                has_update: true,
                channel_or_ref: "nixos-26.05".into(),
            },
        ];
        let list = detect_package_updates_list(
            dummy_path,
            true,
            false,
            false,
            "0.3.32",
            None,
            &flake_inputs,
        );
        assert!(list.iter().any(|item| item.name == "Canal nixpkgs" && item.action == "flake"));
    }

    #[test]
    fn test_set_and_get_update_channel() {
        let res = set_update_channel("testing");
        assert_eq!(res, Ok("testing".to_string()));
        assert_eq!(get_update_channel(), "testing");

        let res2 = set_update_channel("stable");
        assert_eq!(res2, Ok("stable".to_string()));
        assert_eq!(get_update_channel(), "stable");

        // Fallback to stable for unknown channel
        let res3 = set_update_channel("experimental");
        assert_eq!(res3, Ok("stable".to_string()));
    }

    #[test]
    fn test_sanitize_terminal_output() {
        let raw = "Progress 10%\rProgress 50%\rProgress 100%\nDone";
        let cleaned = sanitize_terminal_output(raw);
        assert_eq!(cleaned, "Progress 100%\nDone");
    }
}

