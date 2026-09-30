# 📋 Directives de Développement & Cycle CI/CD — STEvE_OS Dashboard

> **CONSIGNE IMPÉRATIVE POUR L'AGENT IA ET TOUT DÉVELOPPEUR :**
> À chaque modification du projet `steveos-nas-dashboard`, respecter scrupuleusement les 3 règles fondamentales suivantes.

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
- Pour toute nouvelle dépendance, outil externe ou bibliothèque requis par le dashboard ou ses services sous-jacents, vérifier systématiquement si des bibliothèques partagées (`.so`) associées doivent être ajoutées dans `modules/services/nix-ld.nix` côté `steveos-nas`.

---

## 🏷️ Règle n°5 : Traçabilité des Anomalies & Identifiant Unique de Résolution (Bug Tracking)
- **Déclenchement systématique dès qu'un problème, dysfonctionnement ou bug est identifié** :
  1. **Établissement préalable d'un Rapport d'Incident avec Identifiant Unique :**
     - Format standardisé de l'identifiant : `[BUG-YYYYMMDD-XX]` (ex: `[BUG-20260930-01]`).
     - Ce rapport consigne :
       - **Symptôme & Contexte** : Message d'erreur exact, comportement anormal, logs.
       - **Composant(s) impacté(s)** : Backend Rust, routes Axum, composants Frontend (JS/CSS/HTML).
       - **Cause racine (RCA)** : Analyse technique de l'origine du bug.
       - **Stratégie de résolution** : Correctifs appliqués et tests de validation syntaxique.
  2. **Traçabilité obligatoire dans les Commits :**
     - Le commit de correction doit impérativement reprendre cet identifiant :
       - Dans l'entête : `fix(scope)[BUG-YYYYMMDD-XX]: description du correctif`
       - Dans le corps explicatif du commit : référence explicite au rapport d'incident.
  3. **Objectif :** Corrélation directe et sans équivoque entre signalements d'anomalies, rapports et correctifs dans l'historique Git.
