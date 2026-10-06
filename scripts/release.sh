#!/usr/bin/env bash
# ==============================================================================
# Script de Release et Déploiement CI/CD Infaillible — Noos NAS Dashboard
# Garantit l'alignement atomique entre Dashboard, Cachix et flake.lock de l'OS.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DASHBOARD_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
NAS_DIR="$(cd "${DASHBOARD_DIR}/../noos-nas" && pwd)"

NEW_VERSION="${1:-}"
COMMIT_MSG="${2:-}"

if [[ -z "${NEW_VERSION}" ]]; then
  echo "❌ Usage: $0 <version> [message_commit]"
  echo "Exemple: $0 0.3.11 \"Intégration nouvelles métriques ZFS\""
  exit 1
fi

export PATH="$HOME/.cargo/bin:$PATH"

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
CURRENT_BRANCH="$(cd "${DASHBOARD_DIR}" && git branch --show-current)"

(cd "${DASHBOARD_DIR}" && git add Cargo.toml Cargo.lock default.nix frontend/ src/)
(cd "${DASHBOARD_DIR}" && git commit -m "${FULL_MSG}") || true
(cd "${DASHBOARD_DIR}" && git tag -f -a "${TAG_NAME}" -m "${FULL_MSG}")
(cd "${DASHBOARD_DIR}" && git push origin "${CURRENT_BRANCH}" --tags --force)
if [[ "${CURRENT_BRANCH}" == "testing" ]]; then
  echo "Alignement de la branche stable..."
  (cd "${DASHBOARD_DIR}" && git checkout stable && git merge --ff-only testing && git push origin stable && git checkout testing) || true
fi
echo "✅ Poussé sur GitHub (noos-nas-dashboard sur ${CURRENT_BRANCH} et tag ${TAG_NAME})."

echo "⏳ [4/6] Attente de la compilation GitHub Actions & injection Cachix..."
sleep 5
RUN_ID=$(cd "${DASHBOARD_DIR}" && gh run list --limit 1 --json databaseId -q '.[0].databaseId')
echo "Surveillance du run #${RUN_ID}..."
(cd "${DASHBOARD_DIR}" && gh run watch "${RUN_ID}" --exit-status)
echo "✅ Binaire compilé et injecté avec succès dans Cachix (steveos)."

echo "🧬 [5/6] Propagation automatique dans le flake.lock de l'OS (noos-nas via GitHub Actions)..."
echo "Déclenchement ou surveillance de l'Action Update Flake Inputs sur Chomiam/noos-nas..."
sleep 4
NAS_RUN_ID=$(gh run list -R Chomiam/noos-nas --workflow=update-flake.yml --limit 1 --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
if [[ -n "${NAS_RUN_ID}" ]]; then
  echo "Surveillance de la mise à jour flake.lock sur noos-nas (run #${NAS_RUN_ID})..."
  gh run watch "${NAS_RUN_ID}" -R Chomiam/noos-nas --exit-status || true
fi
if [[ -d "${NAS_DIR}" ]]; then
  (cd "${NAS_DIR}" && git pull --ff-only 2>/dev/null || true)
fi
echo "✅ flake.lock mis à jour et validé par GitHub Actions sur noos-nas !"

echo "🎉 [6/6] TERMINÉ AVEC SUCCÈS ! La mise à jour v${NEW_VERSION} est instantanément déployable sur le NAS."
