use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExecResponse {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub cwd: String,
    pub duration_ms: u128,
}

#[derive(Debug, Deserialize)]
pub struct CompleteRequest {
    pub prefix: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompleteResponse {
    pub suggestions: Vec<String>,
}

pub fn execute_command(req: ExecRequest) -> ExecResponse {
    let start = Instant::now();
    let cmd = req.command.trim();

    let mut current_cwd = resolve_cwd(req.cwd.as_deref());

    if cmd.is_empty() {
        return ExecResponse {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            cwd: current_cwd.display().to_string(),
            duration_ms: start.elapsed().as_millis(),
        };
    }

    // Gestion du cd interactif
    if cmd == "cd" || cmd == "cd ~" {
        let home = env::var("HOME").unwrap_or_else(|_| "/home/chomiam".to_string());
        current_cwd = PathBuf::from(home);
        return ExecResponse {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            cwd: current_cwd.display().to_string(),
            duration_ms: start.elapsed().as_millis(),
        };
    }

    if let Some(target) = cmd.strip_prefix("cd ") {
        let target = target.trim();
        let target_path = if target.starts_with('/') {
            PathBuf::from(target)
        } else if let Some(stripped) = target.strip_prefix("~/") {
            let home = env::var("HOME").unwrap_or_else(|_| "/home/chomiam".to_string());
            PathBuf::from(home).join(stripped)
        } else {
            current_cwd.join(target)
        };

        if target_path.is_dir() {
            if let Ok(canonical) = target_path.canonicalize() {
                current_cwd = canonical;
                return ExecResponse {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                    cwd: current_cwd.display().to_string(),
                    duration_ms: start.elapsed().as_millis(),
                };
            }
        } else {
            return ExecResponse {
                success: false,
                stdout: String::new(),
                stderr: format!("cd: aucun dossier de ce nom : {}\n", target),
                exit_code: 1,
                cwd: current_cwd.display().to_string(),
                duration_ms: start.elapsed().as_millis(),
            };
        }
    }

    // Exécuter dans Bash avec délimiteur de pwd
    let wrapped_cmd = format!("{}; echo \"__PWD_DELIM__\"; pwd", cmd);

    let bash_bin = if Path::new("/run/current-system/sw/bin/bash").exists() {
        "/run/current-system/sw/bin/bash"
    } else {
        "bash"
    };

    let mut cmd_obj = crate::updates::create_user_command(bash_bin, &["-c", &wrapped_cmd]);
    cmd_obj.current_dir(&current_cwd);
    let output = cmd_obj.output();

    match output {
        Ok(out) => {
            let raw_stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit_code = out.status.code().unwrap_or(-1);

            let (clean_stdout, new_pwd) = if let Some(idx) = raw_stdout.rfind("__PWD_DELIM__\n") {
                let stdout_part = &raw_stdout[..idx];
                let pwd_part = raw_stdout[idx + "__PWD_DELIM__\n".len()..].trim();
                (stdout_part.to_string(), pwd_part.to_string())
            } else if let Some(idx) = raw_stdout.rfind("__PWD_DELIM__") {
                let stdout_part = &raw_stdout[..idx];
                let pwd_part = raw_stdout[idx + "__PWD_DELIM__".len()..].trim();
                (stdout_part.to_string(), pwd_part.to_string())
            } else {
                (raw_stdout, current_cwd.display().to_string())
            };

            let final_cwd = if !new_pwd.is_empty() && Path::new(&new_pwd).is_dir() {
                new_pwd
            } else {
                current_cwd.display().to_string()
            };

            ExecResponse {
                success: out.status.success(),
                stdout: clean_stdout,
                stderr,
                exit_code,
                cwd: final_cwd,
                duration_ms: start.elapsed().as_millis(),
            }
        }
        Err(e) => ExecResponse {
            success: false,
            stdout: String::new(),
            stderr: format!("Erreur lors de l'exécution de bash : {}", e),
            exit_code: 127,
            cwd: current_cwd.display().to_string(),
            duration_ms: start.elapsed().as_millis(),
        },
    }
}

pub fn autocomplete(req: CompleteRequest) -> CompleteResponse {
    let prefix = req.prefix.trim_start();
    let current_cwd = resolve_cwd(req.cwd.as_deref());
    let mut suggestions = Vec::new();

    // 1. Suggestions de commandes courantes & CLI
    let common_cmds = [
        "nh os switch",
        "nh os switch -u",
        "nh os test",
        "nh os build",
        "nh clean all",
        "git status",
        "git pull",
        "git push",
        "git diff",
        "git log -n 5 --oneline",
        "systemctl status",
        "systemctl restart",
        "systemctl list-units --type=service",
        "journalctl -u",
        "journalctl -f",
        "docker ps",
        "docker logs",
        "docker restart",
        "zpool status",
        "zfs list",
        "btrfs filesystem show",
        "df -hT",
        "free -h",
        "btop",
        "htop",
        "ip -br a",
        "ss -tulpn",
        "ls -la",
        "cat ",
        "nano ",
        "nvim ",
        "cd ",
        "pwd",
        "uptime",
        "uname -a",
    ];

    for c in common_cmds {
        if c.starts_with(prefix) && c != prefix {
            suggestions.push(c.to_string());
        }
    }

    // 2. Autocomplétion de fichiers / répertoires
    let (search_dir, file_prefix) = if let Some(last_token) = req.prefix.split_whitespace().last() {
        if last_token.contains('/') {
            let p = Path::new(last_token);
            if last_token.ends_with('/') {
                (if last_token.starts_with('/') { PathBuf::from(last_token) } else { current_cwd.join(last_token) }, "")
            } else {
                let parent = p.parent().unwrap_or(Path::new(""));
                let file = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
                (if last_token.starts_with('/') { parent.to_path_buf() } else { current_cwd.join(parent) }, file)
            }
        } else {
            (current_cwd.clone(), last_token)
        }
    } else {
        (current_cwd.clone(), "")
    };

    if let Ok(entries) = fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if file_prefix.is_empty() || name.starts_with(file_prefix) {
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let suffix = if is_dir { "/" } else { "" };
                    suggestions.push(format!("{}{}", name, suffix));
                }
            }
        }
    }

    suggestions.sort();
    suggestions.dedup();
    if suggestions.len() > 15 {
        suggestions.truncate(15);
    }

    CompleteResponse { suggestions }
}

fn resolve_cwd(cwd_opt: Option<&str>) -> PathBuf {
    if let Some(dir) = cwd_opt {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }

    if let Ok(dir) = env::var("STEVEOS_CONFIG_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }

    let p1 = PathBuf::from("/etc/nixos");
    if p1.is_dir() {
        return p1;
    }

    let p2 = PathBuf::from("/home/chomiam/Projects/steveos-nas");
    if p2.is_dir() {
        return p2;
    }

    PathBuf::from("/home/chomiam")
}
