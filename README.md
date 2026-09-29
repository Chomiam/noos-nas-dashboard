# 🚀 STEvE_OS NAS Dashboard

Tableau de bord web moderne, léger et ultra-réactif pour **STEvE_OS NAS Edition**.  
Développé avec un backend performant en **Rust (Axum + Tokio)** et une interface client en **Vanilla JavaScript** habillée du thème **Catppuccin Mocha**.

---

## 🌟 Fonctionnalités

- **Port Réseau Isolé (9339) :** Choisi pour éviter tout conflit avec les ports usuels (80, 443, 8080, 9090, 8096, 53317).
- **Télémétrie Système Temps Réel :** Charge CPU, RAM, Uptime, températures, capteurs matériels.
- **Transcodage & GPU :** Détection automatique du contrôleur graphique (Intel QuickSync, AMD VA-API, Nvidia NVENC), surveillance `/dev/dri`, flux actifs.
- **Stockage & Disques :** Jauges des pools montés, liste des disques physiques (SATA HDD / NVMe SSD), état de veille (Spindown) et déclenchement de mise en veille en un clic (`hdparm -y`).
- **Partages Réseau & sFTP :** Copie en un clic de l'URL sFTP directe (`sftp://user@<ip>:22`), partages Samba (SMB), sessions actives d'utilisateurs connectés.
- **Conteneurs Docker :** Liste des conteneurs actifs et arrêtés avec actions de redémarrage.
- **Pare-Feu Modulaire :** Tableau visuel de tous les ports TCP et UDP autorisés par chaque service du NAS, surveillance des adresses bannies par Fail2ban.
- **Console & Journaux :** Visualiseur de logs `journalctl` en temps réel par service.

---

## 🚀 Lancement & Test en Développement

```bash
# Démarrer le serveur localement sur le port 9339 :
cargo run

# Ou via Nix :
nix run
```

Accédez ensuite à l'interface dans votre navigateur :
👉 **http://localhost:9339** (ou `http://<IP_DU_NAS>:9339`)
