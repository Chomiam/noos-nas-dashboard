# 🧠 Guide Technique, REX & Matrice de Sécurité — Noos NAS Dashboard (`skills.md`)

> **Document de référence pour les agents IA et développeurs travaillant sur `noos-nas-dashboard`.**
> Ce fichier recense l'architecture interne, les failles de sécurité découvertes, les incohérences techniques, les anti-patterns et les règles impératives à respecter lors de chaque modification.

---

## 🏛️ 1. Architecture Générale & Cartographie des Modules

Le tableau de bord est composé d'un backend asynchrone Rust (framework **Axum** + runtime **Tokio**) s'exécutant avec les privilèges `root` sur l'OS hôte NixOS, et d'une interface monopage Frontend en **Vanilla JS / CSS** thématisée avec la palette **Catppuccin Mocha**.

### Modules Backend (`src/`)

| Fichier | Rôle & Responsabilités | Risques critiques associés |
| :--- | :--- | :--- |
| `src/main.rs` | Point d'entrée, configuration des couches middleware (anti-cache, CORS), liaison du socket TCP et lancement des tâches de fond. | `CorsLayer::permissive()` combiné aux cookies d'authentification ; arrêt du serveur si panic. |
| `src/auth.rs` | Authentification PAM Linux (`/etc/shadow`, `vars.nix`), génération des tokens de session (64 hex), middleware d'authentification (`auth_middleware`). | Bypass d'authentification sur les routes finissant par `/vnc` ; absence de vérification `is_admin` dans les autres modules. |
| `src/api.rs` | Passerelle de routage API REST (3000+ lignes). Câble les routes HTTP aux contrôleurs. | Absence de vérification du rôle administrateur (`session.is_admin`) sur les opérations destructives. |
| `src/terminal.rs` | Console shell interactive web, exécution de commandes via `runuser` / `bash`, validation sudo PAM. | Exécution de commandes arbitraires accessible sans contrôle de rôle administrateur. |
| `src/files.rs` | Gestionnaire de fichiers complet : CRUD, navigation, streaming vidéo/audio, prévisualisation RAW/HEIC, archivage zip/tar/7z, protection des points de montage. | Absence de confinement de racine (jail / sandbox) : lecture et écriture arbitraire possible sur tout le disque si connecté. |
| `src/storage.rs` | Moteur de stockage : gestion RAID (mdadm/LVM2), formatage (btrfs, ext4, xfs), partitionnement (sfdisk), automounting `/mnt`, surveillance SMART. | Blocage HTTP lors de `chown -R` récursifs ; panique potentielle sur `unwrap()` si `sfdisk` retourne vide. |
| `src/docker_store.rs` | App Store Docker Compose : téléchargement depuis `noos_nas_store`, substitution de ports/variables, orchestration `docker compose up/down`. | Altération des permissions de dossiers système si `data_dir` arbitraire est fourni. |
| `src/games.rs` | Gestionnaire de serveurs de jeux (moteur Pterodactyl Eggs conteneurisé). | Injection de commande shell dans `docker exec ... sh -c "echo '...'"` ; délai de grâce d'arrêt trop court (`-t 2` au lieu de `-t 30`). |
| `src/updates.rs` | Mises à jour déclaratives NixOS (`nixos-rebuild`, `nh os switch`), inspection Git locale et distante, gestion des générations système. | Verrouillage du process de mise à jour ; synchronisation `vars.nix`. |
| `src/samba.rs` | Génération dynamique de `samba_shares.conf`, rechargement à chaud de `samba-smbd`, gestion des sessions et fichiers verrouillés. | Injection de directives smb.conf si les champs `comment` ou `name` contiennent des retours à la ligne `\n`. |
| `src/sftp.rs` | Génération dynamique de `sftp_shares.conf` (OpenSSH), gestion des blocs `Match User` et `ChrootDirectory`. | Conflit fatal entre les permissions `2775 user:storage` et l'obligation OpenSSH d'avoir un dossier root:root non-inscriptible. |
| `src/dns.rs` | Configuration DNS à chaud (`/etc/resolv.conf`) et déclarative (`dns.json`), libération du port 53 (`systemd-resolved`). | **Injection de commande Root** dans `bash -c "echo '...' > /etc/resolv.conf"` via `custom_servers`. |
| `src/vms.rs` | Hyperviseur KVM/QEMU via `virsh` et `virt-install`, console VNC via proxy WebSocket, téléchargement d'ISOs. | Console VNC non authentifiée (`/vnc`) ; injection d'arguments curl sur téléchargement d'ISOs. |
| `src/wireguard.rs` | Serveur VPN WireGuard, génération de clés privées/publiques, persistance JSON des pairs. | Fuite intégrale des clés privées de tous les clients sur `GET /api/wireguard/clients`. |
| `src/services.rs` | Monitoring et contrôle des démons systemd (`systemctl start/stop/restart/reload`). | Contrôle de services sans allowlist (possibilité de stopper `noos-nas-dashboard` ou `sshd`). |
| `src/youtube.rs` | Téléchargement multimédia via `yt-dlp`. | Argument injection sur URL non assainie (`--exec`). |
| `src/trash.rs` | Corbeille temporaire conforme FreeDesktop, purge automatique d'arrière-plan. | Calcul erroné de home dir (`/home/<user>` au lieu de `get_user_home`). |

---

## 🚨 2. Matrice d'Audit des Failles de Sécurité (Vulnérabilités Identifiées)

### 🔴 Faille Critique 1 : Contournement de l'Authentification VNC
- **Emplacement** : [`src/auth.rs:590`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/auth.rs#L590)
- **Mécanisme** :
  ```rust
  if path.ends_with("/auth/login") || path.ends_with("/auth/status") || path.ends_with("/auth/me") || path.ends_with("/vnc") {
      return next.run(req).await;
  }
  ```
  La route WebSocket VNC (`/api/vms/:name/vnc`) se termine par `/vnc`.
- **Impact** : N'importe quel utilisateur sur le réseau local peut accéder directement à la console graphique interactive de n'importe quelle machine virtuelle sans aucun identifiant.
- **Remédiation** : Retirer `path.ends_with("/vnc")` des exemptions publiques et valider le token de session lors de l'établissement du handshake WebSocket (via query string `?token=...` ou cookie). Remplacer les `ends_with` par une égalité exacte d'URL (`path == "/api/auth/login"`, etc.).

---

### 🔴 Faille Critique 2 : Absence de Contrôle d'Accès Basé sur les Rôles (RBAC / Élévation de Privilèges)
- **Emplacement** : [`src/api.rs`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/api.rs) (toutes les routes d'administration)
- **Mécanisme** : Le backend vérifie uniquement la présence d'un token valide dans `auth_middleware`. Bien que `Session.is_admin` soit calculé dans `auth.rs` et vérifié dans `src/users.rs`, **aucun autre module ne vérifie si l'utilisateur est admin**.
- **Impact** : Un compte non-administrateur créé pour un simple accès Samba ou sFTP peut :
  - Exécuter des commandes root via le terminal (`/api/terminal/exec`).
  - Détruire les disques et grappes RAID (`/api/storage/raids/destroy`).
  - Lire `/etc/shadow` ou écrire dans n'importe quel fichier système (`/api/files/read`, `/api/files/write`).
  - Éteindre ou redémarrer la machine physique (`/api/system/power/immediate`).
- **Remédiation** : Créer un middleware dédié `admin_required` ou injecter systématiquement la vérification `session.is_admin` via un extracteur Axum typé (`AdminSession`) sur toutes les routes sensibles.

---

### 🔴 Faille Critique 3 : Injection de Commande Shell Root dans la Configuration DNS
- **Emplacement** : [`src/dns.rs:395`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/dns.rs#L395)
- **Mécanisme** :
  ```rust
  let _ = Command::new("sudo").args(["bash", "-c", &format!("echo '{}' > /etc/resolv.conf", resolv_content)]).output();
  ```
  `resolv_content` est construit à partir des chaînes fournies dans `req.custom_servers`. Aucune validation de format IP (IPv4/IPv6) n'est effectuée sur ces chaînes (`src/dns.rs:314`).
- **Impact** : Si une valeur contenant des apostrophes et des commandes shell est injectée (ex: `1.1.1.1'; touch /tmp/pwned; echo '`), `bash` l'exécute avec les privilèges `root`.
- **Remédiation** :
  1. Valider strictement chaque entrée de serveur DNS avec `std::net::IpAddr::from_str`.
  2. Bannir `bash -c "echo '...' > ..."` et écrire directement dans le fichier via `std::fs::write` ou `std::io::Write` (le processus tournant déjà en root).

---

### 🔴 Faille Critique 4 : Injection de Commande Shell dans les Conteneurs de Jeux
- **Emplacement** : [`src/games.rs:2473`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/games.rs#L2473)
- **Mécanisme** :
  ```rust
  Command::new("docker").args(["exec", "-i", &container_name, "sh", "-c", &format!("echo '{}'", clean_cmd)])
  ```
  `clean_cmd` est inséré directement dans la chaîne shell sans échappement des apostrophes.
- **Impact** : Évasion de la commande dans le conteneur Docker.
- **Remédiation** : Ne pas passer par un sous-shell `sh -c`. Si une écriture sur stdin du processus est souhaitée, attacher le flux stdin de `docker exec -i <container>` et écrire directement les octets de la commande sans passer par un interpréteur shell.

---

### 🟠 Faille Majeure 5 : Traversée de Répertoire & Écrasement des Droits Système dans Docker Store
- **Emplacement** : [`src/docker_store.rs:415-437`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/docker_store.rs#L415-L437)
- **Mécanisme** :
  `req.data_dir` est accepté sans contrôle de confinement. Le code applique ensuite :
  `chown -R <user>:users <data_dir>` et `chmod -R 0775 <data_dir>`.
- **Impact** : La transmission d'un chemin comme `/etc` ou `/var` reconfigure l'ensemble des permissions du système d'exploitation hôte, détruisant l'intégrité de NixOS.
- **Remédiation** : Confinier strictement `app_dir` à `/home/<user>/docker/<clean_id>` ou vérifier que le chemin résolu se situe exclusivement sous `/home/<user>/docker/` ou `/mnt/storage/docker/`.

---

### 🟠 Faille Majeure 6 : Fuite des Clés Privées VPN WireGuard
- **Emplacement** : [`src/wireguard.rs:379-386`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/wireguard.rs#L379-L386) & [`src/api.rs:325`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/api.rs#L325)
- **Mécanisme** : La structure `WireguardClient` sérialise `private_key` et `config_text`. L'endpoint `GET /api/wireguard/clients` retourne la liste complète sans masquer les secrets.
- **Impact** : Tout utilisateur connecté peut voler les clés privées des autres utilisateurs du VPN et usurper leur identité réseau.
- **Remédiation** : Créer un DTO `WireguardClientSummary` sans `private_key` ni `config_text` pour la liste globale. La clé privée ne doit être accessible qu'à la création ou via un endpoint spécifique protégé.

---

### 🟠 Faille Majeure 7 : Combinaison CorsLayer Permissif & Authentification Cookie
- **Emplacement** : [`src/main.rs:147`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/main.rs#L147) & [`src/auth.rs:398-403`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/auth.rs#L398-L403)
- **Mécanisme** : Le serveur active `CorsLayer::permissive()`. Les requêtes API du frontend s'appuient principalement sur le cookie `noos_token`.
- **Impact** : Vulnérabilité aux requêtes inter-origines (CSRF / fuite de données d'API) si un administrateur consulte une page web malveillante sur son navigateur alors que sa session NAS est ouverte.
- **Remédiation** :
  1. Restreindre CORS aux origines locales strictes (IP du NAS, localhost).
  2. Positionner le cookie en `SameSite=Strict`.
  3. Exiger l'en-tête `Authorization: Bearer <token>` sur toutes les requêtes d'action (POST/PUT/DELETE) plutôt que de s'en remettre passivement aux cookies.

---

### 🟡 Faille Moyenne 8 : Fuite de Mots de Passe en Ligne de Commande & Injection d'Options CIFS
- **Emplacement** : [`src/remote_shares.rs:851-860`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/remote_shares.rs#L851-L860)
- **Mécanisme** : `mount -t cifs -o vers=3.0,...,password=<pwd>` expose le mot de passe dans la table des processus `/proc`. Les caractères comme les virgules ne sont pas filtrés.
- **Remédiation** : Passer les identifiants via un fichier temporaire de crédentiels sécurisé (`credentials=/tmp/...`, avec droits `0600`) ou via descripteur de fichier.

---

### 🟡 Faille Moyenne 9 : Argument Injection dans `yt-dlp` et `curl`
- **Emplacement** : [`src/youtube.rs:126, 285`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/youtube.rs#L126), [`src/vms.rs:635`](file:///home/chomiam/Projects/steveos-nas-dashboard/src/vms.rs#L635)
- **Mécanisme** : L'URL transmise est ajoutée directement comme argument CLI sans séparateur `--`.
- **Impact** : Si l'URL commence par des tirets (`--exec`, `-K`), l'outil l'interprète comme un commutateur d'options.
- **Remédiation** : Toujours insérer l'argument séparateur `"--"` avant les arguments positionnels utilisateur, et valider que l'URL commence obligatoirement par `http://` ou `https://`.

---

## 🧩 3. Incohérences Architecturales & Bogues Détectés

### 1. Conflit Fondamental : Droits de Stockage (`2775`) vs Chroot sFTP OpenSSH
- **Le Conflit** :
  - Dans `src/storage.rs:2559`, tout volume monté est configuré en :
    `chown -R <user>:storage /mnt/...` et `chmod 2775 /mnt/...`.
  - Dans `src/sftp.rs:258`, les partages sFTP sont configurés avec `ChrootDirectory <chemin>`.
  - **Règle absolue d'OpenSSH** : Tous les dossiers composants le chemin d'un `ChrootDirectory` doivent obligatoirement appartenir à `root:root` et ne peuvent posséder aucun droit d'écriture pour un groupe ou tout autre utilisateur (`chmod 755` max).
  - **Résultat** : Tout montage créé via l'onglet Stockage est automatiquement rejeté par `sshd` s'il est utilisé comme racine chroot sFTP (`fatal: bad ownership or modes for chroot directory`).
- **Solution Recommandée** :
  - Pour les partages sFTP, chrooter dans un dossier parent appartenant à `root:root` (ex: `/mnt/storage/sftp_jail/<user>`), et créer à l'intérieur un sous-dossier de données (ex: `/data`) appartenant à `<user>:storage`.

---

### 2. Délai d'Arrêt Docker Dégradé pour les Serveurs de Jeu (`stop -t 2`)
- **Le Bogue** :
  - Dans `src/games.rs` lignes 310, 316, 2429 :
    `docker stop -t 2 <container_name>`
  - 2 secondes est insuffisant pour permettre aux serveurs de jeu de vider leur cache et sauvegarder les chunks de la carte sur disque. Docker envoie un `SIGKILL` (code 137).
- **Règle REX** : Toujours utiliser un timeout d'au moins 25 à 30 secondes (`docker stop -t 30`) pour les conteneurs de serveurs de jeux.

---

### 3. Collision et Écrasement des Rôles Utilisateurs sFTP Multi-Partages
- **Le Bogue** :
  - Dans `src/sftp.rs:240-254` :
    `let mut user_rules: HashMap<String, (String, bool, bool)> = HashMap::new();`
  - Si un même utilisateur est assigné à la fois au partage A et au partage B, la deuxième itération écrase la première dans la HashMap. L'utilisateur ne peut accéder qu'au dernier partage traité dans la boucle.

---

### 4. Fonctions JavaScript Dupliquées et Inversion de Portée (Hoisting)
- **`escapeHtml`** :
  - Ligne 3390 : Définition sécurisée (vérifie `if (!str) return "";`).
  - Ligne 6174 : Définition non sécurisée (appelle directement `str.replace(...)`).
  - Par hoisting, la définition non sécurisée prend le dessus. Tout appel avec `null` ou `undefined` lève un `TypeError: Cannot read properties of undefined`.
- **`togglePasswordVisibility`** :
  - Ligne 6724 : Définition paramétrée `(inputId, btnEl)`.
  - Ligne 12988 : Définition sans argument ciblant en dur `login-password`.
  - Les boutons de formulaire dans les modales appellent la version à 0 argument et échouent.
- **`copyLogs`** :
  - Déclarée deux fois à 90 lignes d'intervalle (lignes 3106 et 3196).

---

### 5. Bouton HTML Orphelin (`loadStorageOverview`)
- **Dans `frontend/index.html:1486`** :
  `<button onclick="loadStorageOverview()">`
- La fonction n'existe pas dans `app.js` (son nom réel est `loadStorage()`). Le clic produit une erreur console `ReferenceError`.

---

### 6. IDs HTML Dupliqués (Violations de Spécification DOM)
- `id="btn-view-grid"` présent à la fois à la ligne 813 (Fichiers) et à la ligne 2794 (Serveurs de jeu).
- `id="pill-filter-all"` présent à la fois à la ligne 2540 (Conteneurs Docker) et à la ligne 2715 (Images Docker).
- `document.getElementById` ne sélectionne que la première occurrence.

---

### 7. Risques de Panique Rust (`unwrap()` non sécurisés)
- `src/storage.rs:2753` : `new_part_path = lines.last().unwrap().clone();` sur la sortie de partitionnement. Si le tableau de lignes est vide, crash du thread Tokio.
- `src/vms.rs:454, 635` : `disk_path.to_str().unwrap()`. Si le chemin contient des séquences non-UTF8 valides, panique.

---

## 🛠️ 4. Directives Impératives pour les Développements Futurs

### Règle 1 : Zéro Commande Shell par Concaténation de Chaînes
- Ne jamais utiliser `Command::new("bash").args(["-c", &format!("... {}", user_input)])`.
- Toujours utiliser `Command::new(binary).args([arg1, arg2, arg3])` avec des arguments séparés et typés.
- Si des redirections de fichiers sont nécessaires, exécuter l'écriture via les fonctions standards de Rust (`std::fs::File`, `std::fs::write`).

### Règle 2 : Sanity Check Obligatoire sur les Chemins Système
- Tout chemin fourni par le client doit être validé via canonisation (`canonicalize()`).
- Aucun chemin ne doit permettre de sortir des répertoires désignés (`/home/<user>`, `/mnt/storage`, etc.).
- Les opérations `chown` ou `chmod` récursives ne doivent **JAMAIS** accepter un chemin arbitraire de l'utilisateur sans s'assurer qu'il ne s'agit pas d'une racine système (`/`, `/etc`, `/var`, `/usr`, `/boot`, `/nix`).

### Règle 3 : Validation Systématique des Privilèges Administrateur (`is_admin`)
- Toute action modifiant la configuration matérielle, les démons système, les partitions, les règles de pare-feu, les paquets NixOS ou exécutant du code dans le terminal doit impérativement exiger que `session.is_admin == true`.
- Renvoyer immédiatement un statut HTTP `403 Forbidden` en cas de privilèges insuffisants.

### Règle 4 : Validation des Entrées Réseau
- Les adresses IP et résolveurs DNS doivent être validés via `IpAddr::from_str`.
- Les noms d'hôtes doivent être validés contre une expression régulière stricte `^[a-zA-Z0-9.-]+$`.
- Les numéros de ports doivent être contraints dans l'intervalle `1..=65535`.

### Règle 5 : Résilience Frontend & Anti-Regression
- Ne jamais assigner de variables sans mot-clé `let` ou `const`.
- Toujours vérifier l'existence des éléments DOM (`if (el)`) avant de modifier leurs propriétés (`textContent`, `style`).
- Toute fonction appelée dans un attribut `onclick` doit être testée et exister formellement dans `app.js`.

---

## 📋 5. Rappel du Cycle CI/CD & Règles de Release (AGENTS.md)

1. **Pas de build lourd local** : Uniquement `cargo check` et `node -c`.
2. **Incrément de version** :
   - `Cargo.toml` (`version = "X.Y.Z"`)
   - `default.nix` (`version = "X.Y.Z"`)
3. **Tags Git & Cachix** : Créer le tag `vX.Y.Z` et pousser pour déclencher GitHub Actions CI/CD avec le cache Cachix `steveos`.
4. **Commits en français détaillés** : Documenter l'objectif, l'architecture, les endpoints et la version.
5. **Traçabilité des anomalies** : Créer systématiquement une GitHub Issue `[BUG-YYYYMMDD-XX]` pour chaque correction de bogue.
