#!/usr/bin/env bash
# ==============================================================================
# Script de Release et Déploiement CI/CD Infaillible — Noos NAS Dashboard
# Garantit l'alignement atomique entre Dashboard, Cachix et flake.lock de l'OS.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DASHBOARD_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
NAS_DIR="$(cd "${DASHBOARD_DIR}/../steveos-nas" && pwd)"

NEW_VERSION="${1:-}"
COMMIT_MSG="${2:-}"

if [[ -z "${NEW_VERSION}" ]]; then
  echo "❌ Usage: $0 <version> [message_commit]"
  echo "Exemple: $0 0.3.11 \"Intégration nouvelles métriques ZFS\""
  exit 1
fi

echo "🚀 [1/6] Vérifications syntaxiques locales..."
(cd "${DASHBOARD_DIR}" && cargo check --quiet)
(cd "${DASHBOARD_DIR}" && node -c frontend/js/app.js)
echo "✅ Vérifications syntaxiques réussies."

echo "📝 [2/6] Incrémentation des versions vers v${NEW_VERSION}..."
sed -i "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" "${DASHBOARD_DIR}/Cargo.toml"
sed -i "s/version = \".*\";/version = \"${NEW_VERSION}\";/" "${DASHBOARD_DIR}/default.nix"
(cd "${DASHBOARD_DIR}" && cargo check --quiet) # met à jour Cargo.lock

echo "📦 [3/6] Commit Git & Tagging v${NEW_VERSION}..."
TAG_NAME="v${NEW_VERSION}"
FULL_MSG="Release v${NEW_VERSION} : ${COMMIT_MSG:-Mise à jour du Dashboard}"

(cd "${DASHBOARD_DIR}" && git add Cargo.toml Cargo.lock default.nix frontend/ src/)
(cd "${DASHBOARD_DIR}" && git commit -m "${FULL_MSG}") || true
(cd "${DASHBOARD_DIR}" && git tag -a "${TAG_NAME}" -m "${FULL_MSG}")
(cd "${DASHBOARD_DIR}" && git push origin main --tags)
echo "✅ Poussé sur GitHub (noos-nas-dashboard)."

echo "⏳ [4/6] Attente de la compilation GitHub Actions & injection Cachix..."
sleep 5
RUN_ID=$(cd "${DASHBOARD_DIR}" && gh run list --limit 1 --json databaseId -q '.[0].databaseId')
echo "Surveillance du run #${RUN_ID}..."
(cd "${DASHBOARD_DIR}" && gh run watch "${RUN_ID}" --exit-status)
echo "✅ Binaire compilé et injecté avec succès dans Cachix (steveos)."

echo "🧬 [5/6] Propagation automatique dans le flake.lock de l'OS (noos-nas)..."
if [[ -d "${NAS_DIR}" ]]; then
  (cd "${NAS_DIR}" && git pull --ff-only origin main)
  (cd "${NAS_DIR}" && nix flake lock --update-input noos-nas-dashboard)
  
  echo "🔍 Validation déclarative NixOS..."
  (cd "${NAS_DIR}" && nix eval .#nixosConfigurations.noos-nas.config.system.build.toplevel.drvPath >/dev/null)
  
  (cd "${NAS_DIR}" && git add flake.lock)
  (cd "${NAS_DIR}" && git commit -m "chore(flake): mise à jour de noos-nas-dashboard vers v${NEW_VERSION} (${COMMIT_MSG:-release})" || true)
  (cd "${NAS_DIR}" && git push origin main)
  echo "✅ flake.lock de l'OS mis à jour et poussé sur GitHub (noos-nas) !"
else
  echo "⚠️ Répertoire ../steveos-nas introuvable, propagation flake.lock ignorée."
fi

echo "🎉 [6/6] TERMINÉ AVEC SUCCÈS ! La mise à jour v${NEW_VERSION} est instantanément déployable sur le NAS."
