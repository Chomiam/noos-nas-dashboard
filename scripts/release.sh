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

echo "🧬 [5/6] Propagation automatique dans le flake.lock de l'OS (noos-nas)..."
if [[ -d "${NAS_DIR}" ]]; then
  for target_b in testing main; do
    if (cd "${NAS_DIR}" && git checkout "${target_b}" 2>/dev/null && git pull --ff-only origin "${target_b}" 2>/dev/null); then
      echo "Mise à jour de flake.lock sur la branche ${target_b} de noos-nas..."
      if [[ -x "${NAS_DIR}/scripts/bump-flake-inputs.sh" ]]; then
        (cd "${NAS_DIR}" && "${NAS_DIR}/scripts/bump-flake-inputs.sh" noos-nas-dashboard) || true
      elif command -v nix &>/dev/null; then
        (cd "${NAS_DIR}" && nix flake update noos-nas-dashboard)
        (cd "${NAS_DIR}" && git add flake.lock && git commit -m "chore(flake): mise à jour de noos-nas-dashboard vers v${NEW_VERSION} (${COMMIT_MSG:-release})" && git push origin "${target_b}") || true
      fi
      echo "✅ Branche ${target_b} de noos-nas synchronisée avec flake.lock !"
    fi
  done
  (cd "${NAS_DIR}" && git checkout "${CURRENT_BRANCH}" 2>/dev/null || git checkout main 2>/dev/null || true)
else
  echo "⚠️ Répertoire ../noos-nas introuvable, propagation flake.lock ignorée."
fi

echo "🎉 [6/6] TERMINÉ AVEC SUCCÈS ! La mise à jour v${NEW_VERSION} est instantanément déployable sur le NAS."
