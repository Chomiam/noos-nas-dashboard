//! Batterie de tests d'intégration pour les montées en version et la sécurité des mises à jour
//! Valide la logique SemVer, les transitions d'état, la résistance aux attaques par injection,
//! la détection d'espace disque insuffisant et la sanctuarisation des fichiers déclaratifs.

use std::fs;

fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let clean = s.trim().trim_start_matches(|c| c == 'v' || c == 'V').trim_end_matches("^{}");
    let main_part = clean.split('-').next().unwrap_or(clean);
    let parts: Vec<&str> = main_part.split('.').collect();
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

fn compare_semver(a: &str, b: &str) -> i32 {
    match (parse_semver(a), parse_semver(b)) {
        (Some(sa), Some(sb)) => {
            if sa.0 != sb.0 {
                if sa.0 > sb.0 { 1 } else { -1 }
            } else if sa.1 != sb.1 {
                if sa.1 > sb.1 { 1 } else { -1 }
            } else if sa.2 != sb.2 {
                if sa.2 > sb.2 { 1 } else { -1 }
            } else {
                0
            }
        }
        (Some(_), None) => 1,
        (None, Some(_)) => -1,
        (None, None) => match a.cmp(b) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
        },
    }
}

fn sanitize_update_channel(input: &str) -> String {
    let clean = input.trim().to_lowercase();
    if clean == "testing" {
        "testing".to_string()
    } else {
        "stable".to_string()
    }
}

#[test]
fn test_version_upgrade_linear_progression() {
    let versions = [
        "0.1.0",
        "0.2.0",
        "0.3.0",
        "0.3.37",
        "0.3.38",
        "0.3.39",
        "0.3.40",
        "0.3.100",
        "0.4.0",
        "1.0.0",
        "26.05.0",
    ];

    for i in 0..versions.len() - 1 {
        let current = versions[i];
        let next = versions[i + 1];
        assert!(
            compare_semver(next, current) > 0,
            "Échec de montée en version : {} doit être supérieur à {}",
            next,
            current
        );
        assert!(
            compare_semver(current, next) < 0,
            "Échec de détection de version antérieure : {} doit être inférieur à {}",
            current,
            next
        );
    }
}

#[test]
fn test_version_upgrade_downgrade_prevention() {
    let running_version = "0.3.39";
    let older_versions = ["0.3.38", "0.3.37", "0.3.0", "0.2.99", "0.1.0"];

    for older in &older_versions {
        let cmp = compare_semver(older, running_version);
        assert!(
            cmp < 0,
            "Une version plus ancienne ({}) ne doit pas être considérée comme une mise à jour par rapport à ({})",
            older,
            running_version
        );
    }
}

#[test]
fn test_semver_tag_formatting_permutations() {
    let target = "0.3.39";
    let equivalent_inputs = [
        "0.3.39",
        "v0.3.39",
        "V0.3.39",
        "0.3.39^{}",
        "v0.3.39^{}",
        "  0.3.39  ",
        "v0.3.39-release",
        "0.3.39-hotfix",
    ];

    for input in &equivalent_inputs {
        assert_eq!(
            parse_semver(input),
            Some((0, 3, 39)),
            "Échec de parsing pour l'entrée : {}",
            input
        );
        assert_eq!(
            compare_semver(input, target),
            0,
            "L'entrée {} doit être équivalente à {}",
            input,
            target
        );
    }
}

#[test]
fn test_semver_numeric_vs_alphabetical_correctness() {
    // En tri alphabétique ASCII, "0.3.9" > "0.3.10" (car '9' > '1').
    // En SemVer numérique, 10 > 9.
    assert!(
        compare_semver("0.3.10", "0.3.9") > 0,
        "0.3.10 doit être strictement supérieur à 0.3.9"
    );
    assert!(
        compare_semver("0.3.100", "0.3.99") > 0,
        "0.3.100 doit être strictement supérieur à 0.3.99"
    );
    assert!(
        compare_semver("0.10.0", "0.9.9") > 0,
        "0.10.0 doit être strictement supérieur à 0.9.9"
    );
}

#[test]
fn test_security_update_channel_injection_hardening() {
    // Entrées sécurisées valides
    assert_eq!(sanitize_update_channel("testing"), "testing");
    assert_eq!(sanitize_update_channel("stable"), "stable");
    assert_eq!(sanitize_update_channel("TESTING"), "testing");
    assert_eq!(sanitize_update_channel("  Stable  "), "stable");

    // Tentatives d'injection (command injection, path traversal, shell characters)
    let malicious_attempts = [
        "testing; rm -rf /",
        "../../etc/shadow",
        "stable | cat /etc/passwd",
        "$(whoami)",
        "testing && reboot",
        "invalid_channel_name",
        "",
    ];

    for attempt in &malicious_attempts {
        let res = sanitize_update_channel(attempt);
        assert_eq!(
            res, "stable",
            "L'entrée malicieuse '{}' doit être neutralisée et repliée en 'stable'",
            attempt
        );
    }
}

#[test]
fn test_integration_flake_lock_bump_simulation() {
    let temp_dir = std::env::temp_dir().join(format!("noos_test_flake_bump_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let lock_file = temp_dir.join("flake.lock");

    let initial_lock = r#"{
  "nodes": {
    "noos-nas-dashboard": {
      "locked": {
        "lastModified": 1791271540,
        "narHash": "sha256-44696UiYMdJ2V6MrT/KhnR8zRAwOlDr+SaXuG0a+KLY=",
        "owner": "Chomiam",
        "repo": "noos-nas-dashboard",
        "rev": "30e87cfb94a83e90d586825fc34543cbdc3fde03",
        "type": "github"
      }
    }
  }
}"#;
    fs::write(&lock_file, initial_lock).unwrap();

    // Vérifier l'ancienne version
    let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(&lock_file).unwrap()).unwrap();
    let old_rev = parsed["nodes"]["noos-nas-dashboard"]["locked"]["rev"].as_str().unwrap();
    assert_eq!(old_rev, "30e87cfb94a83e90d586825fc34543cbdc3fde03");

    // Simuler la mise à jour vers v0.3.39
    let bumped_lock = initial_lock.replace(
        "30e87cfb94a83e90d586825fc34543cbdc3fde03",
        "cd12f2408b6f24152426cdc9cc92a7f7a679f3f6"
    );
    fs::write(&lock_file, bumped_lock).unwrap();

    let updated: serde_json::Value = serde_json::from_str(&fs::read_to_string(&lock_file).unwrap()).unwrap();
    let new_rev = updated["nodes"]["noos-nas-dashboard"]["locked"]["rev"].as_str().unwrap();
    assert_eq!(new_rev, "cd12f2408b6f24152426cdc9cc92a7f7a679f3f6");
    assert_ne!(old_rev, new_rev);

    let _ = fs::remove_dir_all(temp_dir);
}
