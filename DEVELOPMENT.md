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
