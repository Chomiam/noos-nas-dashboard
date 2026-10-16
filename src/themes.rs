//! ============================================================================
//! 🎨 MOTEUR & BIBLIOTHÈQUE DE THÈMES UNIVERSELS — NOOS NAS DASHBOARD
//! ============================================================================
//!
//! Ce module gère la synchronisation avec le dépôt GitHub officiel des thèmes
//! (`https://github.com/Chomiam/noos-nas-theme`), le cache local, la validation
//! du format universel de thème (`format_version: "1.0"`), l'importation de
//! thèmes personnalisés et la persistance du thème actif.
//! ============================================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

const THEME_REPO_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Chomiam/noos-nas-theme/main/catalog.json";
const THEME_CACHE_DIR: &str = "/var/cache/noos-nas-dashboard";
const THEME_CACHE_FILE: &str = "/var/cache/noos-nas-dashboard/themes_cache.json";
const THEME_STATE_DIR: &str = "/var/lib/noos-nas";
const ACTIVE_THEME_FILE: &str = "/var/lib/noos-nas/active_theme.json";
const CUSTOM_THEMES_FILE: &str = "/var/lib/noos-nas/custom_themes.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemePreview {
    pub background: String,
    pub surface: String,
    pub card: String,
    pub text: String,
    pub primary: String,
    pub secondary: String,
    #[serde(default)]
    pub success: String,
    #[serde(default)]
    pub warning: String,
    #[serde(default)]
    pub danger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeUiTokens {
    #[serde(default)]
    pub radius_sm: Option<String>,
    #[serde(default)]
    pub radius_md: Option<String>,
    #[serde(default)]
    pub radius_lg: Option<String>,
    #[serde(default)]
    pub radius_xl: Option<String>,
    #[serde(default)]
    pub shadow_card: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoosTheme {
    #[serde(default = "default_format_version")]
    pub format_version: String,
    pub id: String,
    pub name: String,
    #[serde(default = "default_author")]
    pub author: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_mode")]
    pub mode: String, // "dark" | "light"
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_custom: bool,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub preview: ThemePreview,
    pub palette: HashMap<String, String>,
    #[serde(default)]
    pub ui: ThemeUiTokens,
}

fn default_format_version() -> String {
    "1.0".to_string()
}
fn default_author() -> String {
    "Communauté Noos".to_string()
}
fn default_version() -> String {
    "1.0.0".to_string()
}
fn default_mode() -> String {
    "dark".to_string()
}
fn default_icon() -> String {
    "🎨".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeCatalog {
    #[serde(default = "default_format_version")]
    pub format_version: String,
    pub repository: String,
    pub updated_at: String,
    pub default_theme: String,
    #[serde(default)]
    pub total_themes: usize,
    pub themes: Vec<NoosTheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemesOverview {
    pub active_theme_id: String,
    pub repository: String,
    pub format_version: String,
    pub themes: Vec<NoosTheme>,
}

#[derive(Debug, Deserialize)]
pub struct SelectThemeRequest {
    pub theme_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ActiveThemeState {
    pub active_theme_id: String,
}

const REQUIRED_PALETTE_KEYS: &[&str] = &[
    "base", "mantle", "crust", "surface0", "surface1", "surface2", "overlay0",
    "subtext0", "subtext1", "text", "lavender", "blue", "sapphire", "sky",
    "teal", "green", "yellow", "peach", "maroon", "red", "mauve", "pink",
    "flamingo", "rosewater",
];

/// Valide qu'un thème respecte le standard universel Noos NAS Theme v1.0.
pub fn validate_universal_theme(theme: &NoosTheme) -> Result<(), String> {
    let clean_id = theme.id.trim();
    if clean_id.is_empty()
        || !clean_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(
            "L'identifiant 'id' du thème doit être en kebab-case (lettres minuscules, chiffres et tirets uniquement).".into(),
        );
    }

    if theme.name.trim().is_empty() {
        return Err("Le champ 'name' du thème est obligatoire.".into());
    }

    if theme.mode != "dark" && theme.mode != "light" {
        return Err("Le champ 'mode' doit valoir 'dark' ou 'light'.".into());
    }

    for &key in REQUIRED_PALETTE_KEYS {
        match theme.palette.get(key) {
            Some(val) if is_valid_hex_color(val) => {}
            Some(val) => {
                return Err(format!(
                    "Couleur invalide pour 'palette.{}' ('{}') : format hexadécimal #RRGGBB requis.",
                    key, val
                ));
            }
            None => {
                return Err(format!(
                    "Variable obligatoire manquante dans 'palette' : '{}'.",
                    key
                ));
            }
        }
    }

    Ok(())
}

fn is_valid_hex_color(s: &str) -> bool {
    let t = s.trim();
    t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn get_active_theme_id() -> String {
    if let Ok(txt) = std::fs::read_to_string(ACTIVE_THEME_FILE) {
        if let Ok(st) = serde_json::from_str::<ActiveThemeState>(&txt) {
            if !st.active_theme_id.trim().is_empty() {
                return st.active_theme_id;
            }
        }
    }
    "catppuccin-mocha".to_string()
}

pub fn set_active_theme_id(theme_id: &str) -> Result<String, String> {
    let clean_id = theme_id.trim().to_lowercase();
    if clean_id.is_empty() {
        return Err("Identifiant de thème invalide.".into());
    }

    let overview = get_themes_overview(false);
    if !overview.themes.iter().any(|t| t.id == clean_id) {
        return Err(format!("Le thème '{}' est introuvable dans la bibliothèque.", clean_id));
    }

    let _ = std::fs::create_dir_all(THEME_STATE_DIR);
    let state = ActiveThemeState {
        active_theme_id: clean_id.clone(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(ACTIVE_THEME_FILE, json);
    }

    Ok(clean_id)
}

pub fn load_custom_themes() -> Vec<NoosTheme> {
    if let Ok(txt) = std::fs::read_to_string(CUSTOM_THEMES_FILE) {
        if let Ok(mut list) = serde_json::from_str::<Vec<NoosTheme>>(&txt) {
            for t in &mut list {
                t.is_custom = true;
            }
            return list;
        }
    }
    Vec::new()
}

pub fn import_custom_theme(mut theme: NoosTheme) -> Result<NoosTheme, String> {
    theme.id = theme.id.trim().to_lowercase();
    validate_universal_theme(&theme)?;

    if theme.id == "catppuccin-mocha" {
        return Err("Impossible d'écraser le thème officiel par défaut 'catppuccin-mocha'.".into());
    }

    theme.is_default = false;
    theme.is_custom = true;

    let mut custom_list = load_custom_themes();
    custom_list.retain(|t| t.id != theme.id);
    custom_list.push(theme.clone());

    let _ = std::fs::create_dir_all(THEME_STATE_DIR);
    let json = serde_json::to_string_pretty(&custom_list)
        .map_err(|e| format!("Erreur de sérialisation JSON : {}", e))?;
    std::fs::write(CUSTOM_THEMES_FILE, json)
        .map_err(|e| format!("Impossible d'enregistrer le thème personnalisé : {}", e))?;

    Ok(theme)
}

pub fn delete_custom_theme(theme_id: &str) -> Result<String, String> {
    let clean_id = theme_id.trim().to_lowercase();
    let mut custom_list = load_custom_themes();
    let before = custom_list.len();
    custom_list.retain(|t| t.id != clean_id);

    if custom_list.len() == before {
        return Err("Ce thème personnalisé n'existe pas ou est un thème officiel.".into());
    }

    let _ = std::fs::create_dir_all(THEME_STATE_DIR);
    if let Ok(json) = serde_json::to_string_pretty(&custom_list) {
        let _ = std::fs::write(CUSTOM_THEMES_FILE, json);
    }

    if get_active_theme_id() == clean_id {
        let _ = set_active_theme_id("catppuccin-mocha");
    }

    Ok(format!("Thème personnalisé '{}' supprimé.", clean_id))
}

pub fn get_themes_overview(force_refresh: bool) -> ThemesOverview {
    let cache_dir = Path::new(THEME_CACHE_DIR);
    let cache_file = Path::new(THEME_CACHE_FILE);

    let mut json_text = None;

    if !force_refresh {
        json_text = std::fs::read_to_string(cache_file).ok();
    }

    if json_text.is_none() {
        if let Ok(out) = Command::new("curl")
            .args([
                "-s",
                "-L",
                "--connect-timeout",
                "3",
                "--max-time",
                "6",
                THEME_REPO_CATALOG_URL,
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            if text.trim().starts_with('{') && text.contains("\"themes\"") {
                let _ = std::fs::create_dir_all(cache_dir);
                let _ = std::fs::write(cache_file, &text);
                json_text = Some(text);
            }
        }
    }

    if json_text.is_none() {
        json_text = std::fs::read_to_string(cache_file).ok();
    }

    let catalog: ThemeCatalog = if let Some(txt) = json_text {
        serde_json::from_str(&txt).unwrap_or_else(|_| get_embedded_default_catalog())
    } else {
        get_embedded_default_catalog()
    };

    let mut themes = catalog.themes;
    let custom_themes = load_custom_themes();
    for ct in custom_themes {
        if let Some(pos) = themes.iter().position(|t| t.id == ct.id) {
            themes[pos] = ct;
        } else {
            themes.push(ct);
        }
    }

    let active_theme_id = get_active_theme_id();

    ThemesOverview {
        active_theme_id,
        repository: catalog.repository,
        format_version: catalog.format_version,
        themes,
    }
}

pub fn get_embedded_default_catalog() -> ThemeCatalog {
    serde_json::from_str(EMBEDDED_CATALOG_JSON).expect("Catalogue de thèmes embarqué valide")
}

const EMBEDDED_CATALOG_JSON: &str = r##"{
  "format_version": "1.0",
  "repository": "https://github.com/Chomiam/noos-nas-theme",
  "updated_at": "2026-10-16",
  "default_theme": "catppuccin-mocha",
  "total_themes": 6,
  "themes": [
    {
      "format_version": "1.0",
      "id": "catppuccin-mocha",
      "name": "Catppuccin Mocha",
      "author": "Noos NAS Team (Catppuccin)",
      "version": "1.0.0",
      "mode": "dark",
      "is_default": true,
      "icon": "🌙",
      "file": "themes/catppuccin-mocha.json",
      "description": "Thème officiel par défaut de Noos NAS — nuances nocturnes apaisantes aux accents pastel mauve et lavande.",
      "tags": ["officiel", "défaut", "sombre", "pastel"],
      "preview": {
        "background": "#11111b",
        "surface": "#1e1e2e",
        "card": "#313244",
        "text": "#cdd6f4",
        "primary": "#cba6f7",
        "secondary": "#89b4fa",
        "success": "#a6e3a1",
        "warning": "#fab387",
        "danger": "#f38ba8"
      },
      "palette": {
        "base": "#1e1e2e",
        "mantle": "#181825",
        "crust": "#11111b",
        "surface0": "#313244",
        "surface1": "#45475a",
        "surface2": "#585b70",
        "overlay0": "#6c7086",
        "subtext0": "#a6adc8",
        "subtext1": "#bac2de",
        "text": "#cdd6f4",
        "lavender": "#b4befe",
        "blue": "#89b4fa",
        "sapphire": "#74c7ec",
        "sky": "#89dceb",
        "teal": "#94e2d5",
        "green": "#a6e3a1",
        "yellow": "#f9e2af",
        "peach": "#fab387",
        "maroon": "#eba0ac",
        "red": "#f38ba8",
        "mauve": "#cba6f7",
        "pink": "#f5c2e7",
        "flamingo": "#f2cdcd",
        "rosewater": "#f5e0dc"
      },
      "ui": {
        "radius_sm": "8px",
        "radius_md": "12px",
        "radius_lg": "16px",
        "radius_xl": "20px",
        "shadow_card": "0 8px 24px rgba(0, 0, 0, 0.35)"
      }
    },
    {
      "format_version": "1.0",
      "id": "tokyo-night",
      "name": "Tokyo Night",
      "author": "Noos NAS Team",
      "version": "1.0.0",
      "mode": "dark",
      "is_default": false,
      "icon": "🌆",
      "file": "themes/tokyo-night.json",
      "description": "Inspiré des lumières néon de Tokyo la nuit — bleu nuit profond réhaussé d'accents cyan électrique et indigo.",
      "tags": ["sombre", "néon", "cyber", "bleu"],
      "preview": {
        "background": "#13141c",
        "surface": "#1a1b26",
        "card": "#24283b",
        "text": "#c0caf5",
        "primary": "#7aa2f7",
        "secondary": "#bb9af7",
        "success": "#9ece6a",
        "warning": "#ff9e64",
        "danger": "#f7768e"
      },
      "palette": {
        "base": "#1a1b26",
        "mantle": "#16161e",
        "crust": "#13141c",
        "surface0": "#24283b",
        "surface1": "#2f3549",
        "surface2": "#414868",
        "overlay0": "#565f89",
        "subtext0": "#9aa5ce",
        "subtext1": "#a9b1d6",
        "text": "#c0caf5",
        "lavender": "#b4f9f8",
        "blue": "#2ac3de",
        "sapphire": "#7dcfff",
        "sky": "#89ddff",
        "teal": "#73daca",
        "green": "#9ece6a",
        "yellow": "#e0af68",
        "peach": "#ff9e64",
        "maroon": "#db4b4b",
        "red": "#f7768e",
        "mauve": "#7aa2f7",
        "pink": "#bb9af7",
        "flamingo": "#ff757f",
        "rosewater": "#cfc9c2"
      },
      "ui": {
        "radius_sm": "8px",
        "radius_md": "12px",
        "radius_lg": "16px",
        "radius_xl": "20px",
        "shadow_card": "0 10px 28px rgba(8, 10, 18, 0.5)"
      }
    },
    {
      "format_version": "1.0",
      "id": "nord-aurora",
      "name": "Nord Aurora",
      "author": "Noos NAS Team",
      "version": "1.0.0",
      "mode": "dark",
      "is_default": false,
      "icon": "❄️",
      "file": "themes/nord-aurora.json",
      "description": "Palette arctique givrée et minimaliste — tons gris-bleu polaire accompagnés des reflets d'aurores boréales.",
      "tags": ["sombre", "arctique", "minimaliste", "froid"],
      "preview": {
        "background": "#1d212a",
        "surface": "#2e3440",
        "card": "#3b4252",
        "text": "#eceff4",
        "primary": "#88c0d0",
        "secondary": "#81a1c1",
        "success": "#a3be8c",
        "warning": "#d08770",
        "danger": "#bf616a"
      },
      "palette": {
        "base": "#2e3440",
        "mantle": "#242933",
        "crust": "#1d212a",
        "surface0": "#3b4252",
        "surface1": "#434c5e",
        "surface2": "#4c566a",
        "overlay0": "#616e88",
        "subtext0": "#d8dee9",
        "subtext1": "#e5e9f0",
        "text": "#eceff4",
        "lavender": "#8fbcbb",
        "blue": "#81a1c1",
        "sapphire": "#5e81ac",
        "sky": "#88c0d0",
        "teal": "#8fbcbb",
        "green": "#a3be8c",
        "yellow": "#ebcb8b",
        "peach": "#d08770",
        "maroon": "#c9747c",
        "red": "#bf616a",
        "mauve": "#88c0d0",
        "pink": "#b48ead",
        "flamingo": "#d08770",
        "rosewater": "#e5e9f0"
      },
      "ui": {
        "radius_sm": "8px",
        "radius_md": "12px",
        "radius_lg": "16px",
        "radius_xl": "20px",
        "shadow_card": "0 8px 24px rgba(15, 18, 24, 0.45)"
      }
    },
    {
      "format_version": "1.0",
      "id": "dracula-pro",
      "name": "Dracula Pro",
      "author": "Noos NAS Team",
      "version": "1.0.0",
      "mode": "dark",
      "is_default": false,
      "icon": "🧛",
      "file": "themes/dracula-pro.json",
      "description": "Contraste gothique vibrant et élégant — obsidienne pourpre, violet vampirique, rose néon et vert émeraude.",
      "tags": ["sombre", "contraste", "violet", "vibrant"],
      "preview": {
        "background": "#191a21",
        "surface": "#282a36",
        "card": "#343746",
        "text": "#f8f8f2",
        "primary": "#bd93f9",
        "secondary": "#ff79c6",
        "success": "#50fa7b",
        "warning": "#ffb86c",
        "danger": "#ff5555"
      },
      "palette": {
        "base": "#282a36",
        "mantle": "#21222c",
        "crust": "#191a21",
        "surface0": "#343746",
        "surface1": "#44475a",
        "surface2": "#565970",
        "overlay0": "#6272a4",
        "subtext0": "#bfbfbf",
        "subtext1": "#e2e2dc",
        "text": "#f8f8f2",
        "lavender": "#caa9fa",
        "blue": "#8be9fd",
        "sapphire": "#6be5fd",
        "sky": "#8be9fd",
        "teal": "#50fa7b",
        "green": "#50fa7b",
        "yellow": "#f1fa8c",
        "peach": "#ffb86c",
        "maroon": "#ff6e6e",
        "red": "#ff5555",
        "mauve": "#bd93f9",
        "pink": "#ff79c6",
        "flamingo": "#ff92d0",
        "rosewater": "#f8f8f2"
      },
      "ui": {
        "radius_sm": "8px",
        "radius_md": "12px",
        "radius_lg": "16px",
        "radius_xl": "20px",
        "shadow_card": "0 10px 26px rgba(10, 10, 15, 0.55)"
      }
    },
    {
      "format_version": "1.0",
      "id": "gruvbox-dark",
      "name": "Gruvbox Material",
      "author": "Noos NAS Team",
      "version": "1.0.0",
      "mode": "dark",
      "is_default": false,
      "icon": "🍂",
      "file": "themes/gruvbox-dark.json",
      "description": "Ambiance rétro-industrielle chaleureuse aux tons ambrés et terreux — idéal pour réduire la fatigue visuelle.",
      "tags": ["sombre", "chaud", "rétro", "ambre"],
      "preview": {
        "background": "#1d2021",
        "surface": "#282828",
        "card": "#3c3836",
        "text": "#ebdbb2",
        "primary": "#fe8019",
        "secondary": "#83a598",
        "success": "#b8bb26",
        "warning": "#fabd2f",
        "danger": "#fb4934"
      },
      "palette": {
        "base": "#282828",
        "mantle": "#222222",
        "crust": "#1d2021",
        "surface0": "#3c3836",
        "surface1": "#504945",
        "surface2": "#665c54",
        "overlay0": "#7c6f64",
        "subtext0": "#bdae93",
        "subtext1": "#d5c4a1",
        "text": "#ebdbb2",
        "lavender": "#d3869b",
        "blue": "#83a598",
        "sapphire": "#458588",
        "sky": "#8ec07c",
        "teal": "#689d6a",
        "green": "#b8bb26",
        "yellow": "#fabd2f",
        "peach": "#fe8019",
        "maroon": "#cc241d",
        "red": "#fb4934",
        "mauve": "#fe8019",
        "pink": "#d3869b",
        "flamingo": "#d65d0e",
        "rosewater": "#fbf1c7"
      },
      "ui": {
        "radius_sm": "8px",
        "radius_md": "12px",
        "radius_lg": "16px",
        "radius_xl": "20px",
        "shadow_card": "0 8px 24px rgba(12, 10, 8, 0.5)"
      }
    },
    {
      "format_version": "1.0",
      "id": "catppuccin-latte",
      "name": "Catppuccin Latte (Clair)",
      "author": "Noos NAS Team (Catppuccin)",
      "version": "1.0.0",
      "mode": "light",
      "is_default": false,
      "icon": "☀️",
      "file": "themes/catppuccin-latte.json",
      "description": "Thème clair lumineux et doux — papier crème apaisant, typographie ardoise haute lisibilité et accents pastel soutenus.",
      "tags": ["clair", "lumineux", "pastel", "jour"],
      "preview": {
        "background": "#dce0e8",
        "surface": "#eff1f5",
        "card": "#e6e9ef",
        "text": "#4c4f69",
        "primary": "#8839ef",
        "secondary": "#1e66f5",
        "success": "#40a02b",
        "warning": "#fe640b",
        "danger": "#d20f39"
      },
      "palette": {
        "base": "#eff1f5",
        "mantle": "#e6e9ef",
        "crust": "#dce0e8",
        "surface0": "#ccd0da",
        "surface1": "#bcc0cc",
        "surface2": "#acb0be",
        "overlay0": "#8c8fa1",
        "subtext0": "#6c6f85",
        "subtext1": "#5c5f77",
        "text": "#4c4f69",
        "lavender": "#7287fd",
        "blue": "#1e66f5",
        "sapphire": "#209fb5",
        "sky": "#04a5e5",
        "teal": "#179299",
        "green": "#40a02b",
        "yellow": "#df8e1d",
        "peach": "#fe640b",
        "maroon": "#e64553",
        "red": "#d20f39",
        "mauve": "#8839ef",
        "pink": "#ea76cb",
        "flamingo": "#dd7878",
        "rosewater": "#dc8a78"
      },
      "ui": {
        "radius_sm": "8px",
        "radius_md": "12px",
        "radius_lg": "16px",
        "radius_xl": "20px",
        "shadow_card": "0 6px 20px rgba(76, 79, 105, 0.12)"
      }
    }
  ]
}"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_themes_catalog_is_valid() {
        let catalog = get_embedded_default_catalog();
        assert_eq!(catalog.default_theme, "catppuccin-mocha");
        assert_eq!(catalog.themes.len(), 6);
        for theme in &catalog.themes {
            assert!(
                validate_universal_theme(theme).is_ok(),
                "Theme {} failed validation",
                theme.id
            );
        }
    }
}
