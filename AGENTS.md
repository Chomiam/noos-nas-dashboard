# 📋 Directives de Développement & Cycle CI/CD — Noos Dashboard

> **CONSIGNE IMPÉRATIVE POUR L'AGENT IA ET TOUT DÉVELOPPEUR :**
> À chaque modification du projet `noos-nas-dashboard`, respecter scrupuleusement les 3 règles fondamentales suivantes.

---

## 🚫 Règle n°1 : Pas de build lourd sur la machine locale
- **Ne jamais lancer de compilation complète ou lourde (`nix build`, recompilation d'ISO, etc.) sur la machine locale.**
- La machine locale de travail ne doit exécuter que des vérifications syntaxiques ultra-rapides (`cargo check`, vérification node) et l'écriture de code.
- La compilation binaire est obligatoirement déléguée aux serveurs distants via **GitHub Actions** (`.github/workflows/build.yml`).

---

## 🏷️ Règle n°2 : Nouveau numéro de version & Déclenchement GitHub Actions
- Pour chaque lot de modifications validé :
  1. **Incrémenter systématiquement le numéro de version** dans :
     - `Cargo.toml` (`version = "X.Y.Z"`)
     - `default.nix` (`version = "X.Y.Z"`)
  2. **Créer un tag git** de release correspondant :
     ```bash
     git tag -a vX.Y.Z -m "Release vX.Y.Z : résumé des nouveautés"
     ```
  3. **Pousser sur GitHub** (`git push origin main --tags`) pour déclencher automatiquement le workflow GitHub Actions CI/CD.

---

## ✍️ Règle n°3 : Commits avec explications détaillées en français
- Ne pas faire de commits vagues ou minimalistes.
- Chaque commit doit contenir une explication détaillée en français précisant :
  - **L'objectif** : ce qui a été demandé et pourquoi.
  - **L'architecture** : les composants UI (HTML/CSS/JS) et backend Rust impactés.
  - **Les endpoints / services** : les nouvelles routes ou fonctionnalités ajoutées.
  - **Le numéro de version** attribué.

---

## 🧬 Règle n°4 : Vérification systématique nix-ld pour toute dépendance native
- Pour toute nouvelle dépendance, outil externe ou bibliothèque requis par le dashboard ou ses services sous-jacents, vérifier systématiquement si des bibliothèques partagées (`.so`) associées doivent être ajoutées dans `modules/services/nix-ld.nix` côté `noos-nas`.

---

## 🏷️ Règle n°5 : Traçabilité des Anomalies & Création Obligatoire de GitHub Issue (Bug Tracking)
- **Déclenchement systématique dès qu'un problème, dysfonctionnement ou bug est identifié** :
  1. **Création obligatoire préalable d'une GitHub Issue :**
     - Ouvrir une GitHub Issue sur le dépôt concerné (via `gh issue create` ou l'interface web GitHub).
     - Format du titre de l'Issue : `[BUG-YYYYMMDD-XX]: Résumé synthétique de l'anomalie` (ex: `[BUG-20260930-01]`).
     - Le corps de l'Issue consigne obligatoirement :
       - **Symptôme & Contexte** : Message d'erreur exact, comportement anormal, logs.
       - **Composant(s) impacté(s)** : Backend Rust, routes Axum, composants Frontend (JS/CSS/HTML).
       - **Cause racine (RCA)** : Analyse technique de l'origine de la défaillance.
       - **Stratégie de résolution** : Correctifs appliqués et tests de validation syntaxique.
  2. **Traçabilité obligatoire dans les Commits :**
     - Le commit de correction doit impérativement reprendre cet identifiant et le numéro d'Issue :
       - Dans l'entête : `fix(scope)[BUG-YYYYMMDD-XX]: description du correctif (#num_issue)`
       - Dans le corps explicatif du commit : référence explicite et fermeture de la GitHub Issue (`Closes #num_issue` ou `Fixes #num_issue`).
  3. **Objectif :** Corrélation directe et traçabilité absolue entre GitHub Issues, rapports techniques et historique Git.

---

## ⚡ Règle n°6 : Garantie d'alimentation du cache binaire Cachix (steveos)
- À chaque nouvelle mise à jour ou release du Dashboard :
  1. Le workflow GitHub Actions (`.github/workflows/build.yml`) **doit obligatoirement compiler le binaire et le pousser dans le cache Cachix officiel `steveos`** via le secret `CACHIX_AUTH_TOKEN`.
  2. L'agent IA ou le développeur doit **systématiquement surveiller et attendre la réussite du run GitHub Actions** (`gh run list` / `gh run view`) avant de déclarer la mise à jour prête.
  3. Aucun déploiement de mise à jour côté NAS ne doit nécessiter une compilation Rust locale : le cache binaire `steveos.cachix.org` doit être alimenté sans exception.

---

## 🔗 Règle n°7 : Propagation obligatoire du hash dans `noos-nas` (`flake.lock`)
- **Dès la complétion du build Cachix sur GitHub Actions :**
  1. **Mettre à jour immédiatement l'input dans le dépôt de configuration de l'OS (`../steveos-nas`) :**
     ```bash
     cd ../steveos-nas
     nix flake lock --update-input noos-nas-dashboard
     nix eval .#nixosConfigurations.noos-nas.config.system.build.toplevel.drvPath
     git commit -am "chore(flake): mise à jour de noos-nas-dashboard vers vX.Y.Z"
     git push origin main
     ```
  2. **Alternative recommandée :** Utiliser le script automatisé [`scripts/release.sh`](file:///home/chomiam/Projects/steveos-nas-dashboard/scripts/release.sh) qui enchaîne l'intégralité du cycle sans risque d'omission.
  3. **Principe fondamental :** Sans cette propagation sur GitHub, le NAS reconstruit l'ancien binaire verrouillé dans `flake.lock` et ne bascule jamais sur la nouvelle version.

