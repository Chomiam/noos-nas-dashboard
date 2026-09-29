// ==========================================================================
// STEvE_OS NAS Edition — Application Client (Vanilla JavaScript)
// ==========================================================================

const AUTH_TOKEN_KEY = "steveos_auth_token";
let currentUserSession = null;
let isAppInitialized = false;

function getAuthToken() {
  return sessionStorage.getItem(AUTH_TOKEN_KEY) || localStorage.getItem(AUTH_TOKEN_KEY);
}

function setAuthToken(token, remember) {
  if (remember) {
    localStorage.setItem(AUTH_TOKEN_KEY, token);
    sessionStorage.removeItem(AUTH_TOKEN_KEY);
  } else {
    sessionStorage.setItem(AUTH_TOKEN_KEY, token);
    localStorage.removeItem(AUTH_TOKEN_KEY);
  }
}

function clearAuthToken() {
  sessionStorage.removeItem(AUTH_TOKEN_KEY);
  localStorage.removeItem(AUTH_TOKEN_KEY);
  currentUserSession = null;
}

// Global fetch interceptor: injects Authorization Bearer & handles 401
const originalFetch = window.fetch;
window.fetch = async function(...args) {
  let [resource, config] = args;
  config = config || {};

  const token = getAuthToken();
  if (token) {
    if (!config.headers) {
      config.headers = {};
    }
    if (config.headers instanceof Headers) {
      if (!config.headers.has("Authorization")) {
        config.headers.set("Authorization", `Bearer ${token}`);
      }
    } else if (Array.isArray(config.headers)) {
      config.headers.push(["Authorization", `Bearer ${token}`]);
    } else {
      if (!config.headers["Authorization"]) {
        config.headers["Authorization"] = `Bearer ${token}`;
      }
    }
  }

  const response = await originalFetch(resource, config);

  // If 401 on an API route (excluding login / status), trigger login modal
  if (
    response.status === 401 &&
    typeof resource === "string" &&
    resource.startsWith("/api/") &&
    !resource.startsWith("/api/auth/login") &&
    !resource.startsWith("/api/auth/status")
  ) {
    clearAuthToken();
    updateUserSessionUI(null);
    showLoginModal();
  }

  return response;
};

let activeTab = "tab-overview";
let lastUpdateStatus = null;
let isUpdatingNow = false;

document.addEventListener("DOMContentLoaded", () => {
  checkAuthSession();
});

function initApp() {
  refreshAll(false);
  updateSftpUri();
  checkForUpdates(false);
  fetchPowerStatus();
  fetchTrashCount();
  initDragAndDrop();
}

function setupPolling() {
  // Rafraîchissement fréquent des métriques CPU/RAM/Temp (3 secondes)
  setInterval(() => {
    if (activeTab === "tab-overview") {
      loadSystem();
    }
  }, 3000);

  // Rafraîchissement périodique des services, réseau et stockage (12 secondes)
  setInterval(() => {
    if (activeTab === "tab-storage") loadStorage();
    if (activeTab === "tab-network") loadNetwork();
    if (activeTab === "tab-containers") refreshContainersAndStore();
  }, 12000);

  // Vérification périodique des mises à jour en arrière-plan (30 secondes)
  setInterval(() => {
    checkForUpdates(false);
  }, 30000);

  setInterval(() => {
    fetchPowerStatus();
  }, 15000);

  setInterval(() => {
    fetchTrashCount();
  }, 30000);
}

// --------------------------------------------------------------------------
// GESTION DES ONGLETS
// --------------------------------------------------------------------------
function switchTab(tabId) {
  activeTab = tabId;
  const updateHeaderBtn = document.getElementById("header-update-btn");
  if (updateHeaderBtn) {
    updateHeaderBtn.classList.toggle("active-view", tabId === "tab-updates");
  }
  document.querySelectorAll(".tab-btn").forEach(btn => {
    btn.classList.toggle("active", btn.getAttribute("data-tab") === tabId);
  });

  document.querySelectorAll(".tab-pane").forEach(pane => {
    pane.classList.toggle("active", pane.id === tabId);
  });

  if (tabId === "tab-overview") loadSystem();
  if (tabId === "tab-files") navigateToPath(currentFolderPath);
  if (tabId === "tab-updates") checkForUpdates(false);
  if (tabId === "tab-storage") loadStorage();
  if (tabId === "tab-network") loadNetwork();
  if (tabId === "tab-containers") refreshContainersAndStore();
  if (tabId === "tab-logs") loadLogs();
}

// --------------------------------------------------------------------------
// CHARGEMENT GLOBAL
// --------------------------------------------------------------------------
async function refreshAll(showFeedback = false) {
  try {
    await Promise.all([
      loadSystem(),
      loadStorage(),
      loadServices(),
      loadNetwork(),
      checkForUpdates(false),
      loadHardwareInfo(),
      loadSmartInfo(),
      loadLatestSpeedtest()
    ]);
    if (showFeedback) {
      showToast("Données du NAS actualisées !", "success");
    }
  } catch (e) {
    if (showFeedback) {
      showToast("Erreur lors de l'actualisation : " + e, "error");
    }
  }
}

// --------------------------------------------------------------------------
// SYSTÈME, CPU & RAM
// --------------------------------------------------------------------------
async function loadSystem() {
  try {
    const res = await fetch("/api/system");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const sys = json.data;

    // Header
    const hostEl = document.getElementById("header-hostname");
    if (hostEl) hostEl.textContent = `${sys.hostname} • ${sys.os_name}`;

    const uptimeEl = document.getElementById("header-uptime");
    if (uptimeEl) uptimeEl.textContent = `Uptime: ${sys.uptime_formatted}`;

    // CPU
    const cpuVal = document.getElementById("cpu-usage-val");
    if (cpuVal) cpuVal.textContent = `${sys.cpu_usage_percent}%`;

    const cpuCores = document.getElementById("cpu-cores-text");
    if (cpuCores) cpuCores.textContent = `${sys.cpu_cores} cœurs (Load: ${sys.load_avg[0]})`;

    const cpuBar = document.getElementById("cpu-progress-bar");
    if (cpuBar) cpuBar.style.width = `${Math.min(sys.cpu_usage_percent, 100)}%`;

    const cpuModel = document.getElementById("cpu-model-sub");
    if (cpuModel) cpuModel.textContent = sys.cpu_model;

    // RAM
    const ramVal = document.getElementById("ram-usage-val");
    if (ramVal) ramVal.textContent = `${sys.ram_usage_percent}%`;

    const ramUsed = document.getElementById("ram-used-text");
    if (ramUsed) {
      const usedGb = (sys.ram_used_bytes / (1024 ** 3)).toFixed(1);
      const totalGb = (sys.ram_total_bytes / (1024 ** 3)).toFixed(1);
      ramUsed.textContent = `${usedGb} / ${totalGb} Go`;
    }

    const ramBar = document.getElementById("ram-progress-bar");
    if (ramBar) ramBar.style.width = `${Math.min(sys.ram_usage_percent, 100)}%`;

    const ramFree = document.getElementById("ram-free-sub");
    if (ramFree) {
      const freeGb = (sys.ram_free_bytes / (1024 ** 3)).toFixed(1);
      ramFree.textContent = `${freeGb} Go disponibles`;
    }

    // Température CPU
    if (sys.temperatures && sys.temperatures.length > 0) {
      const tempVal = document.getElementById("temp-cpu-val");
      if (tempVal) tempVal.textContent = `${sys.temperatures[0].temp_c} °C`;
    }
  } catch (err) {
    console.warn("Erreur fetch /api/system:", err);
  }
}

// --------------------------------------------------------------------------
// MISES À JOUR INTELLIGENTES (STEvE_OS UPDATE ENGINE)
// --------------------------------------------------------------------------
async function checkForUpdates(force = false) {
  const refreshBtn = document.getElementById("btn-refresh-updates");
  if (force && refreshBtn) {
    refreshBtn.disabled = true;
    refreshBtn.innerHTML = `<span>⏳</span> Recherche...`;
  }
  try {
    const res = await fetch(`/api/updates/status${force ? '?force=true' : ''}`);
    const json = await res.json();
    if (!json.success || !json.data) return;

    lastUpdateStatus = json.data;
    renderUpdatesUI(lastUpdateStatus);
  } catch (err) {
    console.warn("Erreur fetch /api/updates/status:", err);
  } finally {
    if (force && refreshBtn) {
      refreshBtn.disabled = false;
      refreshBtn.innerHTML = `<span>🔄</span> Vérifier maintenant`;
    }
  }
}

function startLiveLogPolling() {
  const termBody = document.getElementById("update-terminal-body");
  const interval = setInterval(async () => {
    try {
      const res = await fetch("/api/updates/logs");
      const json = await res.json();
      if (json.success && json.data && json.data.logs) {
        if (termBody) {
          termBody.innerHTML = parseAnsiToHtml(json.data.logs);
          termBody.scrollTop = termBody.scrollHeight;
        }
      }
    } catch (e) {
      // ignore
    }
  }, 400);

  return async () => {
    clearInterval(interval);
    try {
      const res = await fetch("/api/updates/logs");
      const json = await res.json();
      if (json.success && json.data && json.data.logs) {
        if (termBody) {
          termBody.innerHTML = parseAnsiToHtml(json.data.logs);
          termBody.scrollTop = termBody.scrollHeight;
        }
      }
    } catch (e) {}
  };
}

function renderUpdatesUI(status) {
  const dot = document.getElementById("update-indicator-dot");
  const pillText = document.getElementById("header-update-text");
  const navBadge = document.getElementById("nav-updates-badge");

  const heroBanner = document.getElementById("overview-update-banner");
  const heroIcon = document.getElementById("overview-update-icon");
  const heroTitle = document.getElementById("overview-update-title");
  const heroSub = document.getElementById("overview-update-subtitle");

  const stratIcon = document.getElementById("strategy-icon");
  const stratTitle = document.getElementById("strategy-title");
  const stratDesc = document.getElementById("strategy-desc");
  const step1 = document.getElementById("pipe-step-1");
  const step2 = document.getElementById("pipe-step-2");
  const step3 = document.getElementById("pipe-step-3");

  const btnUpdate = document.getElementById("btn-intelligent-update");
  const btnIcon = document.getElementById("btn-update-icon");
  const btnLabel = document.getElementById("btn-update-label");

  const lastTime = document.getElementById("updates-last-checked-time");
  if (lastTime) lastTime.textContent = status.last_checked ? "Vérifié à " + status.last_checked : "";

  // 1. Mise à jour de la carte Git Configuration
  const gitLocal = document.getElementById("git-local-sha");
  const gitRemote = document.getElementById("git-remote-sha");
  const gitBadge = document.getElementById("git-status-badge");
  const gitTree = document.getElementById("git-working-tree-status");
  const gitCommitsWrap = document.getElementById("git-pending-commits-wrap");
  const gitCommitsList = document.getElementById("git-pending-commits-list");
  const gitFilesWrap = document.getElementById("git-changed-files-wrap");

  if (gitLocal) {
    gitLocal.textContent = status.config_local_commit || "--";
    gitLocal.title = status.config_local_commit_full || status.config_local_commit || "";
  }
  if (gitRemote) {
    gitRemote.textContent = status.config_remote_commit || status.config_local_commit || "--";
    gitRemote.title = status.config_remote_commit_full || "";
  }
  if (gitTree) {
    gitTree.textContent = status.config_git_status || "Arbre propre";
  }

  if (status.config_update_available) {
    if (gitBadge) {
      gitBadge.className = "badge badge-info";
      gitBadge.textContent = "📥 " + (status.config_commits_behind || 1) + " révision(s) sur GitHub";
    }
    if (gitCommitsWrap) {
      gitCommitsWrap.style.display = "block";
      if (gitCommitsList) {
        if (status.config_pending_commits && status.config_pending_commits.length > 0) {
          gitCommitsList.innerHTML = status.config_pending_commits.map(c => 
            `<div style="padding:3px 0; border-bottom:1px solid rgba(255,255,255,0.05);"><span style="color:var(--mauve); font-weight:bold;">${c.hash}</span> <span style="color:var(--text);">${c.message}</span> <span style="color:var(--subtext0);">(${c.author}, ${c.date})</span></div>`
          ).join("");
        } else {
          gitCommitsList.innerHTML = `<div style="color:var(--subtext0);">Nouveaux commits disponibles sur Chomiam/steve_os-nix</div>`;
        }
      }
      if (gitFilesWrap) {
        if (status.config_changed_files && status.config_changed_files.length > 0) {
          gitFilesWrap.innerHTML = `<strong>Fichiers modifiés :</strong> ` + status.config_changed_files.map(f => 
            `<span style="display:inline-block; background:rgba(255,255,255,0.06); padding:1px 5px; border-radius:3px; margin:2px 4px 2px 0;">${f}</span>`
          ).join("");
        } else {
          gitFilesWrap.innerHTML = "";
        }
      }
    }
  } else {
    if (gitBadge) {
      gitBadge.className = "badge badge-success";
      gitBadge.textContent = "✔ Configuration synchronisée";
    }
    if (gitCommitsWrap) gitCommitsWrap.style.display = "none";
  }

  // 2. Mise à jour de la carte Flake Inputs & Paquets Nixpkgs
  const flakeNixpkgs = document.getElementById("flake-nixpkgs-sha");
  const pkgSummary = document.getElementById("packages-status-summary");
  const flakeDetails = document.getElementById("flake-inputs-detail-text");
  const pkgCountLabel = document.getElementById("packages-count-label");

  if (status.flake_inputs_status && status.flake_inputs_status.length > 0) {
    const nixpkgsInput = status.flake_inputs_status.find(i => i.name.startsWith("nixpkgs"));
    if (nixpkgsInput && flakeNixpkgs) {
      flakeNixpkgs.textContent = nixpkgsInput.locked_rev;
    }
    if (flakeDetails) {
      const summaryList = status.flake_inputs_status.map(i => `${i.name}: ${i.locked_rev}${i.has_update ? " ➔ " + (i.remote_rev || "màj") : ""}`);
      flakeDetails.textContent = summaryList.join(" | ");
    }
  } else {
    if (flakeNixpkgs) flakeNixpkgs.textContent = "cf5e765";
  }

  const pkgList = status.package_updates_list || [];
  const totalPkgsCount = pkgList.length;

  if (pkgSummary) {
    if (status.package_updates_available || totalPkgsCount > 0) {
      pkgSummary.innerHTML = `<span class="badge badge-warning">📦 ${totalPkgsCount > 0 ? totalPkgsCount : status.package_updates_count} màj prête(s)</span>`;
    } else {
      pkgSummary.innerHTML = `<span class="badge badge-success">✔ Paquets à jour</span>`;
    }
  }

  if (pkgCountLabel) {
    pkgCountLabel.textContent = totalPkgsCount + " paquet(s) détecté(s)";
  }

  // 3. Remplissage du tableau détaillé des paquets
  const listBadge = document.getElementById("packages-list-badge");
  const tbody = document.getElementById("packages-update-tbody");

  if (listBadge) {
    listBadge.textContent = totalPkgsCount + " paquet(s) identifié(s)";
    listBadge.className = totalPkgsCount > 0 ? "badge badge-warning" : "badge badge-info";
  }

  if (tbody) {
    if (totalPkgsCount === 0) {
      tbody.innerHTML = `<tr><td colspan="5" style="text-align:center; color:var(--subtext0); padding:24px;">✨ Aucun paquet en attente de mise à jour. Le système est parfaitement aligné avec la configuration.</td></tr>`;
    } else {
      tbody.innerHTML = pkgList.map(item => {
        let actionBadge = `<span class="badge badge-info">🔄 Mise à jour</span>`;
        if (item.action === "add") {
          actionBadge = `<span class="badge badge-success">➕ Nouveau</span>`;
        } else if (item.action === "download") {
          actionBadge = `<span class="badge badge-mauve">📥 Téléchargement</span>`;
        } else if (item.action === "build") {
          actionBadge = `<span class="badge badge-warning">⚙️ Recompilation</span>`;
        } else if (item.action === "remove") {
          actionBadge = `<span class="badge badge-danger">➖ Retrait</span>`;
        }

        return `<tr>
          <td><strong style="color:var(--text); font-size:0.9rem;">${item.name}</strong></td>
          <td>${actionBadge}</td>
          <td><code style="font-size:0.8rem;">${item.current_version}</code></td>
          <td><code style="color:var(--teal); font-weight:bold; font-size:0.8rem;">${item.new_version || "--"}</code></td>
          <td><span style="color:var(--subtext0); font-size:0.78rem;">${item.size || "Nixpkgs Flake"}</span></td>
        </tr>`;
      }).join("");
    }
  }

  // 4. Si une mise à jour est en cours
  if (status.is_updating || isUpdatingNow) {
    if (dot) dot.className = "update-indicator-dot pulse-yellow";
    if (pillText) pillText.textContent = "⏳ Mise à jour en cours...";
    if (btnUpdate) {
      btnUpdate.disabled = true;
      if (btnIcon) btnIcon.textContent = "⏳";
      if (btnLabel) btnLabel.textContent = "Mise à jour en cours d'exécution...";
    }
    return;
  }

  if (btnUpdate) btnUpdate.disabled = false;

  const hasConfig = status.config_update_available;
  const hasPkgs = status.package_updates_available || totalPkgsCount > 0;

  // Réinitialiser les étapes de pipeline
  if (step1) step1.className = "pipeline-step";
  if (step2) step2.className = "pipeline-step";
  if (step3) step3.className = "pipeline-step";

  if (hasConfig && hasPkgs) {
    // Both: First Git pull, then nh os switch -u
    if (dot) dot.className = "update-indicator-dot pulse-mauve";
    if (pillText) pillText.textContent = "⚡ Màj Config & Paquets";
    if (navBadge) { navBadge.style.display = "inline-block"; navBadge.textContent = "⚡ 2"; }

    if (heroBanner) heroBanner.className = "update-hero-banner has-updates";
    if (heroIcon) heroIcon.textContent = "⚡";
    if (heroTitle) heroTitle.textContent = "Nouvelle configuration & paquets système disponibles !";
    if (heroSub) heroSub.textContent = "1. Git Pull sécurisé (GitHub) ➔ 2. Validation Nix ➔ 3. nh os switch -u";

    if (stratIcon) stratIcon.textContent = "⚡";
    if (stratTitle) stratTitle.textContent = "Mise à jour complète recommandée (Configuration + Paquets)";
    if (stratDesc) stratDesc.textContent = "Une nouvelle version de la configuration est présente sur GitHub, et des mises à jour de paquets sont prêtes à être appliquées.";
    if (step1) step1.className = "pipeline-step active";
    if (step2) step2.className = "pipeline-step active";
    if (step3) { step3.className = "pipeline-step active"; step3.textContent = "3. nh os switch -u"; }

    if (btnIcon) btnIcon.textContent = "⚡";
    if (btnLabel) btnLabel.textContent = "Lancer la mise à jour complète (Git Pull + nh os switch -u)";
  } else if (hasConfig) {
    // Config only
    if (dot) dot.className = "update-indicator-dot pulse-blue";
    if (pillText) pillText.textContent = "📥 Màj Config dispo";
    if (navBadge) { navBadge.style.display = "inline-block"; navBadge.textContent = "1"; }

    if (heroBanner) heroBanner.className = "update-hero-banner has-updates";
    if (heroIcon) heroIcon.textContent = "📥";
    if (heroTitle) heroTitle.textContent = "Nouvelle configuration disponible sur GitHub !";
    if (heroSub) heroSub.textContent = "Git Pull sécurisé avec test de syntaxe et rollback automatique";

    if (stratIcon) stratIcon.textContent = "📥";
    if (stratTitle) stratTitle.textContent = "Mise à jour de configuration détectée";
    if (stratDesc) stratDesc.textContent = "Le dépôt distant Chomiam/steve_os-nix contient de nouveaux commits prêts à être déployés.";
    if (step1) step1.className = "pipeline-step active";
    if (step2) step2.className = "pipeline-step active";
    if (step3) { step3.className = "pipeline-step active"; step3.textContent = "3. nh os switch"; }

    if (btnIcon) btnIcon.textContent = "📥";
    if (btnLabel) btnLabel.textContent = "Mettre à jour la configuration (Git Pull + nh os switch)";
  } else if (hasPkgs) {
    // Packages only
    if (dot) dot.className = "update-indicator-dot pulse-peach";
    if (pillText) pillText.textContent = "📦 Màj Paquets dispo";
    if (navBadge) { navBadge.style.display = "inline-block"; navBadge.textContent = "1"; }

    if (heroBanner) heroBanner.className = "update-hero-banner has-updates";
    if (heroIcon) heroIcon.textContent = "📦";
    if (heroTitle) heroTitle.textContent = "Mises à jour de paquets système disponibles !";
    if (heroSub) heroSub.textContent = "Nouveaux paquets ou paquets modifiés prêts à être activés via nh os switch -u";

    if (stratIcon) stratIcon.textContent = "📦";
    if (stratTitle) stratTitle.textContent = "Mise à jour des paquets système (Nixpkgs / Flake)";
    if (stratDesc) stratDesc.textContent = "Des paquets nécessitent une installation ou mise à niveau. Cliquez pour déployer avec nh os switch -u.";
    if (step3) { step3.className = "pipeline-step active"; step3.textContent = "1. nh os switch -u"; }

    if (btnIcon) btnIcon.textContent = "📦";
    if (btnLabel) btnLabel.textContent = "Mettre à jour les paquets (nh os switch -u)";
  } else {
    // Up to date
    if (dot) dot.className = "update-indicator-dot";
    if (pillText) pillText.textContent = "✨ Système à jour";
    if (navBadge) navBadge.style.display = "none";

    if (heroBanner) heroBanner.className = "update-hero-banner";
    if (heroIcon) heroIcon.textContent = "✨";
    if (heroTitle) heroTitle.textContent = "Votre STEvE_OS NAS est parfaitement à jour";
    if (heroSub) heroSub.textContent = `Dernière vérification à ${status.last_checked} • Configuration & Paquets synchronisés`;

    if (stratIcon) stratIcon.textContent = "✨";
    if (stratTitle) stratTitle.textContent = "Système et configuration à jour";
    if (stratDesc) stratDesc.textContent = `Tous les composants sont alignés sur le dépôt distant et le canal Nixpkgs (vérifié à ${status.last_checked}).`;

    if (btnIcon) btnIcon.textContent = "🔄";
    if (btnLabel) btnLabel.textContent = "Rechercher à nouveau les mises à jour";
  }
}


async function triggerIntelligentUpdate() {
  if (isUpdatingNow) return;

  const btnUpdate = document.getElementById("btn-intelligent-update");
  const btnLabel = document.getElementById("btn-update-label");
  const btnIcon = document.getElementById("btn-update-icon");
  const termBody = document.getElementById("update-terminal-body");
  const termStatus = document.getElementById("terminal-update-status");

  // Si tout est à jour, un clic force la re-vérification
  if (lastUpdateStatus && lastUpdateStatus.update_type === "None") {
    showToast("Recherche de mises à jour forcée...", "info");
    await checkForUpdates(true);
    showToast(lastUpdateStatus.status_text, "info");
    return;
  }

  if (!confirm("Voulez-vous lancer la mise à jour intelligente ?\nLe système garantit un rollback en cas d'erreur de syntaxe.")) {
    return;
  }

  isUpdatingNow = true;
  if (btnUpdate) btnUpdate.disabled = true;
  if (btnIcon) btnIcon.textContent = "⏳";
  if (btnLabel) btnLabel.textContent = "Mise à jour en cours d'exécution...";

  if (termStatus) {
    termStatus.style.display = "inline-block";
    termStatus.className = "badge badge-warning";
    termStatus.textContent = "⏳ En cours...";
  }

  if (termBody) {
    termBody.textContent = "🚀 Lancement de la mise à jour intelligente...\nCette opération peut prendre quelques minutes selon les paquets à compiler/télécharger.\nVeuillez ne pas éteindre le NAS.\n\n";
  }

  showToast("Lancement de la mise à jour intelligente...", "info");

  const stopLogPolling = startLiveLogPolling();

  try {
    const res = await fetch("/api/updates/apply", { method: "POST" });
    const json = await res.json();
    const result = json.data || {};
    await stopLogPolling();

    if (json.success && result.success) {
      if (termStatus) {
        termStatus.className = "badge badge-success";
        termStatus.textContent = "✔ Succès";
      }
      showToast("Mise à jour STEvE_OS appliquée avec succès !", "success");
    } else {
      if (termStatus) {
        termStatus.className = "badge badge-danger";
        termStatus.textContent = "❌ Échec / Rollback";
      }
      showToast("Échec de la mise à jour : " + (result.error || json.message || "Erreur inconnue"), "error");
    }
  } catch (err) {
    await stopLogPolling();
    if (termBody) termBody.innerHTML += parseAnsiToHtml("\nErreur réseau lors de l'appel API : " + err);
    if (termStatus) {
      termStatus.className = "badge badge-danger";
      termStatus.textContent = "❌ Erreur réseau";
    }
    showToast("Erreur de connexion : " + err, "error");
  } finally {
    isUpdatingNow = false;
    await checkForUpdates(false);
  }
}

async function triggerForcePackagesUpdate() {
  if (isUpdatingNow) return;

  if (!confirm("Voulez-vous forcer la mise à jour des paquets Nixpkgs (nh os switch -u) ?")) {
    return;
  }

  isUpdatingNow = true;
  const termBody = document.getElementById("update-terminal-body");
  const termStatus = document.getElementById("terminal-update-status");

  if (termStatus) {
    termStatus.style.display = "inline-block";
    termStatus.className = "badge badge-warning";
    termStatus.textContent = "⏳ nh os switch -u en cours...";
  }

  if (termBody) {
    termBody.textContent = "🚀 Exécution forcée de nh os switch -u...\nActualisation des entrées du flake et recompilation des paquets système.\n\n";
  }

  showToast("Lancement de nh os switch -u...", "info");

  const stopLogPolling = startLiveLogPolling();

  try {
    const res = await fetch("/api/updates/apply?force_packages=true", { method: "POST" });
    const json = await res.json();
    const result = json.data || {};
    await stopLogPolling();

    if (json.success && result.success) {
      if (termStatus) {
        termStatus.className = "badge badge-success";
        termStatus.textContent = "✔ Succès";
      }
      showToast("Mise à jour des paquets terminée avec succès !", "success");
    } else {
      if (termStatus) {
        termStatus.className = "badge badge-danger";
        termStatus.textContent = "❌ Échec";
      }
      showToast("Erreur lors de la mise à jour : " + (result.error || json.message), "error");
    }
  } catch (err) {
    await stopLogPolling();
    if (termBody) termBody.innerHTML += parseAnsiToHtml("\nErreur réseau : " + err);
    showToast("Erreur réseau : " + err, "error");
  } finally {
    isUpdatingNow = false;
    await checkForUpdates(false);
  }
}

function clearUpdateTerminal() {
  const termBody = document.getElementById("update-terminal-body");
  const termStatus = document.getElementById("terminal-update-status");
  if (termBody) termBody.innerHTML = "Console prête.";
  if (termStatus) termStatus.style.display = "none";
}

// --------------------------------------------------------------------------
// STOCKAGE & GESTION DES POOLS RAID
// --------------------------------------------------------------------------
let cachedStorageDisks = [];
let currentSelectedRaidLevel = "raid5";
let raidSyncPollInterval = null;


// --------------------------------------------------------------------------
// BASCULE VUE GRILLE / TABLEAU POUR LES DISQUES
// --------------------------------------------------------------------------
let currentDisksView = 'grid';
function setDisksView(view) {
  currentDisksView = view;
  const btnGrid = document.getElementById('btn-view-grid');
  const btnTable = document.getElementById('btn-view-table');
  const gridContainer = document.getElementById('disks-container');
  const tableContainer = document.getElementById('disks-table-container');

  if (view === 'grid') {
    btnGrid?.classList.add('active');
    btnTable?.classList.remove('active');
    if (gridContainer) gridContainer.style.display = 'grid';
    if (tableContainer) tableContainer.style.display = 'none';
  } else {
    btnTable?.classList.add('active');
    btnGrid?.classList.remove('active');
    if (gridContainer) gridContainer.style.display = 'none';
    if (tableContainer) tableContainer.style.display = 'block';
  }
}

async function loadStorage() {
  try {
    const res = await fetch("/api/storage");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const data = json.data;
    cachedStorageDisks = data.physical_disks || [];

    // Main Storage Metric
    if (data.pools && data.pools.length > 0) {
      const mainPool = data.pools[0];
      const storageVal = document.getElementById("storage-main-val");
      if (storageVal) storageVal.textContent = `${mainPool.usage_percent}%`;

      const storageText = document.getElementById("storage-main-text");
      if (storageText) storageText.textContent = `${mainPool.used_human} / ${mainPool.total_human}`;

      const storageBar = document.getElementById("storage-progress-bar");
      if (storageBar) storageBar.style.width = `${Math.min(mainPool.usage_percent, 100)}%`;

      const storageFree = document.getElementById("storage-free-sub");
      if (storageFree) storageFree.textContent = `${mainPool.free_human} libres sur ${mainPool.mountpoint}`;
    }

    // Bannière de synchronisation active
    updateRaidSyncBanner(data.active_sync);

    // 1. Grille des Grappes RAID Logiques
    const raidsContainer = document.getElementById("logical-raids-container");
    if (raidsContainer) {
      if (!data.logical_raids || data.logical_raids.length === 0) {
        raidsContainer.innerHTML = `
          <div style="grid-column: 1 / -1; padding: 26px; text-align: center; background: var(--surface0); border-radius: var(--radius-md); border: 1px dashed rgba(255,255,255,0.12);">
            <div style="font-size: 2.2rem; margin-bottom: 8px;">🛡️</div>
            <div style="font-weight: 700; color: var(--text); font-size: 1.05rem; margin-bottom: 4px;">Aucune grappe RAID configurée</div>
            <div style="font-size: 0.84rem; color: var(--subtext0); max-width: 480px; margin: 0 auto 16px auto;">
              Vos disques de stockage sont disponibles. Créez un pool RAID pour sécuriser vos données contre les pannes matérielles.
            </div>
            <button type="button" class="btn btn-primary btn-sm" onclick="openCreateRaidModal()">
              <span>➕</span> Créer un Pool RAID (RAID 5 Recommandé)
            </button>
          </div>
        `;
      } else {
        raidsContainer.innerHTML = data.logical_raids.map(r => {
          let healthBadgeClass = "badge-success";
          if (r.health.includes("sync") || r.health.includes("Reconstruction")) healthBadgeClass = "badge-warning";
          if (r.health.includes("Dégradé") || r.status.includes("degraded")) healthBadgeClass = "badge-danger";

          const membersPills = (r.members || []).map(m => {
            const shortName = m.replace("/dev/", "");
            return `<span class="member-disk-pill">💿 ${escapeHtml(shortName)}</span>`;
          }).join("");

          return `
            <div class="raid-card">
              <div class="raid-card-header">
                <div>
                  <div class="raid-card-title">
                    <span>🛡️ ${escapeHtml(r.name)}</span>
                    <span class="badge badge-accent" style="font-size:0.72rem;">${escapeHtml(r.level)}</span>
                  </div>
                  <div class="raid-card-device">${escapeHtml(r.device)} &bull; ${escapeHtml(r.filesystem)}</div>
                </div>
                <span class="badge ${healthBadgeClass}">${escapeHtml(r.health)}</span>
              </div>

              <div class="metric-value-row" style="margin: 8px 0 4px 0;">
                <span class="metric-value" style="font-size:1.35rem;">${r.usage_percent}%</span>
                <span class="metric-unit">${escapeHtml(r.used_bytes ? formatFileSize(r.used_bytes) : "0 o")} / ${escapeHtml(r.size_human)}</span>
              </div>

              <div class="metric-progress-wrap">
                <div class="metric-progress-bar ${r.usage_percent > 85 ? 'progress-red' : 'progress-peach'}" style="width: ${Math.min(r.usage_percent, 100)}%;"></div>
              </div>

              <div style="display:flex; justify-content:space-between; align-items:center; font-size:0.78rem; color:var(--subtext0); margin-top:2px;">
                <span>Point de montage : <strong>${escapeHtml(r.mountpoint || 'Non monté')}</strong></span>
                <span>${escapeHtml(r.free_bytes ? formatFileSize(r.free_bytes) : r.size_human)} libres</span>
              </div>

              <div style="margin-top:4px;">
                <div style="font-size:0.72rem; color:var(--subtext0); margin-bottom:4px;">Disques physiques membres :</div>
                <div class="member-disks-wrap">${membersPills}</div>
              </div>

              <div class="raid-card-actions" style="display:flex; justify-content:space-between; align-items:center; border-top:1px solid rgba(255,255,255,0.06); padding-top:10px; margin-top:8px;">
                <div>
                  ${r.mountpoint 
                    ? `<span style="font-size:0.8rem; color:var(--green); font-weight:600;">📁 Monté sur ${escapeHtml(r.mountpoint)}</span>`
                    : `<span style="font-size:0.8rem; color:var(--peach); font-weight:600;">⚠️ Volume non monté</span>`}
                </div>
                <div>
                  ${r.mountpoint
                    ? `<div style="display:inline-flex; gap:6px;">
  <button type="button" class="btn btn-secondary btn-xs" onclick="openRepairPermissionsModal('${escapeHtml(r.mountpoint)}')"><span>🛡️</span> Permissions</button>
  <button type="button" class="btn btn-secondary btn-xs" onclick="umountVolume('${escapeHtml(r.mountpoint)}')"><span>⏏️</span> Démonter</button>
</div>`
                    : `<button type="button" class="btn btn-primary btn-xs" onclick="openMountVolumeModal('${escapeHtml(r.name)}', '${escapeHtml(r.device)}', '${escapeHtml(r.level)}')"><span>📁</span> Monter dans /mnt</button>`}
                </div>
              </div>
            </div>
          `;
        }).join("");
      }
    }

    // 2. Grille des Systèmes de Fichiers & Montages
    const poolsContainer = document.getElementById("pools-container");
    if (poolsContainer) {
      poolsContainer.innerHTML = data.pools.map(p => `
        <div class="pool-card">
          <div class="pool-header">
            <div>
              <div class="pool-name">📁 ${escapeHtml(p.name)} (${escapeHtml(p.mountpoint)})</div>
              <div style="font-size:0.78rem; color:var(--subtext0);">Système: ${escapeHtml(p.filesystem)}</div>
            </div>
            <span class="badge ${p.health === 'ONLINE' ? 'badge-success' : 'badge-warning'}">${escapeHtml(p.health)}</span>
          </div>

          <div class="metric-value-row" style="margin: 12px 0 6px 0;">
            <span class="metric-value" style="font-size:1.4rem;">${p.usage_percent}%</span>
            <span class="metric-unit">${p.used_human} / ${p.total_human}</span>
          </div>

          <div class="metric-progress-wrap">
            <div class="metric-progress-bar ${p.usage_percent > 85 ? 'progress-red' : 'progress-peach'}" style="width: ${Math.min(p.usage_percent, 100)}%;"></div>
          </div>
          <div style="display:flex; justify-content:space-between; align-items:center; font-size:0.78rem; color:var(--subtext0); margin-top:8px;">
            <span>${p.free_human} disponibles</span>
            <span>Mode: <code>${escapeHtml(p.permissions_mode || '0755')}</code></span>
          </div>

          <!-- Barre de diagnostic des permissions -->
          <div style="margin-top:10px; border-top:1px solid rgba(255,255,255,0.06); padding-top:8px;">
            ${p.needs_permission_repair
              ? `<div style="display:flex; justify-content:space-between; align-items:center; background:rgba(250,179,135,0.12); border:1px solid rgba(250,179,135,0.3); border-radius:6px; padding:6px 10px; font-size:0.76rem;">
                   <span style="color:var(--peach); font-weight:700;">⚠️ Écriture restreinte (${escapeHtml(p.owner_user || 'root')}:${escapeHtml(p.owner_group || 'root')})</span>
                   <button type="button" class="btn btn-warning btn-xs" onclick="openRepairPermissionsModal('${escapeHtml(p.mountpoint)}')">🔧 Réparer</button>
                 </div>`
              : `<div style="display:flex; justify-content:space-between; align-items:center; font-size:0.75rem;">
                   <span style="color:var(--green); font-weight:600;">🛡️ Propriétaire : ${escapeHtml(p.owner_user || 'chomiam')}:${escapeHtml(p.owner_group || 'storage')} (Écriture OK)</span>
                   <button type="button" class="btn btn-secondary btn-xs" onclick="openRepairPermissionsModal('${escapeHtml(p.mountpoint)}')">🔧 Permissions</button>
                 </div>`}
          </div>
        </div>
      `).join("");
    }

    // 3. Mise à jour du compteur de disques
    const countBadge = document.getElementById("disks-count-badge");
    if (countBadge && data.physical_disks) {
      const hddCount = data.physical_disks.filter(d => d.is_rotational).length;
      const ssdCount = data.physical_disks.length - hddCount;
      countBadge.textContent = `${data.physical_disks.length} Disques (${hddCount} HDD SATA • ${ssdCount} NVMe)`;
    }

    // 4. Grille des Baies de Disques Physiques
    const disksContainer = document.getElementById("disks-container");
    if (disksContainer && data.physical_disks) {
      disksContainer.innerHTML = data.physical_disks.map(d => {
        const isStandby = d.power_state.toLowerCase().includes("veille") || d.power_state.toLowerCase().includes("standby");
        const stateBadge = isStandby 
          ? `<span class="badge badge-warning">🌙 Veille</span>`
          : `<span class="badge badge-success"><span class="status-pulse-dot"></span> Actif</span>`;

        let roleBadge = `<span class="badge badge-secondary">${escapeHtml(d.role)}</span>`;
        if (d.is_system) {
          roleBadge = `<span class="badge badge-primary">🔒 Système NixOS</span>`;
        } else if (d.role.includes("Membre")) {
          roleBadge = `<span class="badge badge-accent">🛡️ ${escapeHtml(d.role)}</span>`;
        } else if (d.role.includes("Libre")) {
          roleBadge = `<span class="badge badge-success">✨ Libre</span>`;
        }

        const spindownBtn = d.is_rotational
          ? `<button type="button" class="btn btn-secondary btn-xs" onclick="triggerSpindown('${escapeHtml(d.name)}')"><span>🌙</span> Mettre en veille</button>`
          : `<span style="font-size:0.74rem; color:var(--subtext0);">⚡ Flash NVMe</span>`;

        const formatBtn = !d.is_system
          ? `<button type="button" class="btn btn-danger btn-xs" onclick="openFormatDiskModalFor('${escapeHtml(d.path)}')"><span>🧹</span> Formater...</button>`
          : `<span class="bay-protected-badge" title="Disque système protégé"><span>🔒</span> Protégé</span>`;

        const isNvme = d.disk_type.includes("NVMe");
        const driveIcon = isNvme ? "⚡" : "💿";

        let tempClass = "tele-green";
        if (d.temperature_c > 45) tempClass = "tele-orange";
        if (d.temperature_c > 52) tempClass = "tele-red";
        const tempDisplay = d.temperature_c > 0 ? `${d.temperature_c} °C` : (isStandby ? "Veille" : "N/A");

        const smartClass = d.smart_status.includes("PASS") || d.smart_status.includes("Sain") ? "tele-green" : "tele-red";

        return `
          <div class="disk-bay-card">
            <div class="bay-card-header">
              <div class="bay-identifier-wrap">
                <span class="bay-badge"><span class="bay-slot-icon">🖴</span> ${escapeHtml(d.bay_label || d.name)}</span>
                <span class="bay-dev-code">${escapeHtml(d.path)}</span>
              </div>
              <div class="bay-badges-row">
                ${roleBadge}
                ${stateBadge}
              </div>
            </div>

            <div class="bay-hero-body">
              <div class="bay-graphic-box">${driveIcon}</div>
              <div class="bay-info-hero">
                <div class="bay-capacity-val">${escapeHtml(d.size_human)}</div>
                <div class="bay-model-name" title="${escapeHtml(d.model)}">${escapeHtml(d.model)}</div>
                <div class="bay-serial-text">S/N : <code>${escapeHtml(d.serial)}</code></div>
              </div>
            </div>

            <div class="bay-telemetry-grid">
              <div class="bay-tele-item">
                <span class="bay-tele-label">🌡️ Température</span>
                <span class="bay-tele-val ${tempClass}">${tempDisplay}</span>
              </div>
              <div class="bay-tele-item">
                <span class="bay-tele-label">🛡️ S.M.A.R.T.</span>
                <span class="bay-tele-val ${smartClass}">${escapeHtml(d.smart_status)}</span>
              </div>
              <div class="bay-tele-item">
                <span class="bay-tele-label">⚙️ Technologie</span>
                <span class="bay-tele-val">${escapeHtml(d.disk_type)}</span>
              </div>
              <div class="bay-tele-item">
                <span class="bay-tele-label">💤 Mode</span>
                <span class="bay-tele-val">${escapeHtml(d.power_state)}</span>
              </div>
            </div>

            <div class="bay-actions-footer">
              <div>${spindownBtn}</div>
              <div>${formatBtn}</div>
            </div>
          </div>
        `;
      }).join("");
    }

    // 5. Tableau Détaillé des Disques Physiques
    const tableBody = document.getElementById("disks-table-body");
    if (tableBody && data.physical_disks) {
      tableBody.innerHTML = data.physical_disks.map(d => {
        const isStandby = d.power_state.toLowerCase().includes("veille") || d.power_state.toLowerCase().includes("standby");
        const isNvme = d.disk_type.includes("NVMe");
        const driveIcon = isNvme ? "⚡" : "💿";

        const spindownBtn = d.is_rotational
          ? `<button type="button" class="btn btn-secondary btn-xs" onclick="triggerSpindown('${escapeHtml(d.name)}')">🌙 Veille</button>`
          : `<span style="font-size:0.72rem; color:var(--subtext0);">Flash</span>`;

        const formatBtn = !d.is_system
          ? `<button type="button" class="btn btn-danger btn-xs" onclick="openFormatDiskModalFor('${escapeHtml(d.path)}')">🧹 Formater</button>`
          : `<span class="badge badge-secondary" style="font-size:0.7rem;">🔒 Protégé</span>`;

        let tempColor = "var(--green)";
        if (d.temperature_c > 45) tempColor = "var(--peach)";
        if (d.temperature_c > 52) tempColor = "var(--red)";

        return `
          <tr>
            <td><span class="bay-badge">${escapeHtml(d.bay_label || d.name)}</span></td>
            <td><strong style="font-family:var(--font-mono); font-size:0.85rem;">${escapeHtml(d.path)}</strong></td>
            <td>
              <div style="font-weight:600; font-size:0.84rem;">${escapeHtml(d.model)}</div>
              <div style="font-size:0.72rem; color:var(--subtext0); font-family:var(--font-mono);">S/N: ${escapeHtml(d.serial)}</div>
            </td>
            <td><strong style="font-family:var(--font-mono); font-size:0.95rem;">${escapeHtml(d.size_human)}</strong></td>
            <td>${driveIcon} ${escapeHtml(d.disk_type)}</td>
            <td><span class="badge ${d.is_system ? 'badge-primary' : (d.role.includes('Membre') ? 'badge-accent' : 'badge-secondary')}">${escapeHtml(d.role)}</span></td>
            <td><strong style="color:${tempColor}; font-family:var(--font-mono);">${d.temperature_c > 0 ? d.temperature_c + ' °C' : 'N/A'}</strong></td>
            <td><span class="badge ${d.smart_status.includes('PASS') || d.smart_status.includes('Sain') ? 'badge-success' : 'badge-danger'}">${escapeHtml(d.smart_status)}</span></td>
            <td><span class="badge ${isStandby ? 'badge-warning' : 'badge-success'}">${isStandby ? '🌙 Veille' : '⚡ Actif'}</span></td>
            <td style="text-align:right;">
              <div style="display:inline-flex; gap:6px;">
                ${spindownBtn}
                ${formatBtn}
              </div>
            </td>
          </tr>
        `;
      }).join("");
    }
  } catch (err) {
    console.warn("Erreur fetch /api/storage:", err);
  }
}

function updateRaidSyncBanner(syncData) {
  const banner = document.getElementById("raid-sync-banner");
  if (!banner) return;

  if (syncData) {
    banner.style.display = "block";
    const title = document.getElementById("raid-sync-title");
    const details = document.getElementById("raid-sync-details");
    const badge = document.getElementById("raid-sync-percent-badge");
    const bar = document.getElementById("raid-sync-bar");

    if (title) title.textContent = `${syncData.action} de la grappe ${syncData.array} en cours`;
    if (details) details.textContent = `Vitesse : ${syncData.speed_mb_s} Mo/s &bull; Fin estimée : ${syncData.finish_human}`;
    if (badge) badge.textContent = `${syncData.percent.toFixed(1)}%`;
    if (bar) bar.style.width = `${Math.min(syncData.percent, 100)}%`;

    if (!raidSyncPollInterval) {
      raidSyncPollInterval = setInterval(pollRaidSyncProgress, 2500);
    }
  } else {
    banner.style.display = "none";
    if (raidSyncPollInterval) {
      clearInterval(raidSyncPollInterval);
      raidSyncPollInterval = null;
    }
  }
}

async function pollRaidSyncProgress() {
  try {
    const res = await fetch("/api/storage/raids/progress");
    const json = await res.json();
    if (json.success) {
      updateRaidSyncBanner(json.data);
    }
  } catch (e) {
    console.warn("Poll raid progress error:", e);
  }
}

// --------------------------------------------------------------------------
// MODALE ASSISTANT CRÉATION DE POOL RAID
// --------------------------------------------------------------------------
function openCreateRaidModal() {
  const modal = document.getElementById("create-raid-modal");
  const checklist = document.getElementById("raid-disks-checklist");
  const errEl = document.getElementById("raid-submit-error");
  if (!modal || !checklist) return;

  if (errEl) errEl.textContent = "";

  // Filtrer les disques éligibles (non-système)
  const eligibleDisks = cachedStorageDisks.filter(d => !d.is_system);

  if (eligibleDisks.length === 0) {
    checklist.innerHTML = `<div style="color:var(--subtext0); padding:10px; text-align:center;">Aucun disque de stockage disponible pour le RAID.</div>`;
  } else {
    checklist.innerHTML = eligibleDisks.map((d, idx) => {
      // Par défaut pour 4 disques, tous cochés pour RAID 5
      const checked = idx < 4 ? "checked" : "";
      return `
        <label class="raid-disk-check-item">
          <div style="display:flex; align-items:center; gap:10px;">
            <input type="checkbox" class="raid-disk-checkbox" value="${escapeHtml(d.path)}" data-size="${escapeHtml(d.size_human)}" onchange="updateRaidPreview()" ${checked} style="accent-color:var(--mauve); width:16px; height:16px;">
            <div>
              <strong style="color:var(--text); font-size:0.88rem;">${escapeHtml(d.path)}</strong>
              <span style="font-size:0.75rem; color:var(--subtext0); margin-left:6px;">${escapeHtml(d.model)}</span>
            </div>
          </div>
          <span class="badge badge-secondary">${escapeHtml(d.size_human)}</span>
        </label>
      `;
    }).join("");
  }

  currentSelectedRaidLevel = "raid5";
  updateRaidLevelPickerUI();
  updateRaidPreview();
  updateMountRaidHint();
  updateMountFsHint();
  modal.style.display = "flex";
}

function closeCreateRaidModal() {
  const modal = document.getElementById("create-raid-modal");
  if (modal) modal.style.display = "none";
}

function selectRaidLevel(level) {
  currentSelectedRaidLevel = level;
  updateRaidLevelPickerUI();
  updateRaidPreview();
}

function updateRaidLevelPickerUI() {
  const cards = document.querySelectorAll(".raid-level-card");
  cards.forEach(c => {
    c.classList.toggle("active", c.getAttribute("data-level") === currentSelectedRaidLevel);
  });
}

function updateRaidPreview() {
  const checkboxes = document.querySelectorAll(".raid-disk-checkbox:checked");
  const count = checkboxes.length;

  const rawEl = document.getElementById("raid-preview-raw");
  const netEl = document.getElementById("raid-preview-net");
  const resEl = document.getElementById("raid-preview-resilience");
  const btnSubmit = document.getElementById("btn-submit-raid");
  const errEl = document.getElementById("raid-submit-error");

  // Estimation de taille moyenne par disque (ex: 3.6 To -> 3.64)
  let unitSize = 3.64; // To
  if (checkboxes.length > 0) {
    const s0 = checkboxes[0].getAttribute("data-size") || "3,6T";
    const num = parseFloat(s0.replace(",", ".").replace("T", "").replace("G", ""));
    if (!isNaN(num) && num > 0) {
      unitSize = s0.includes("G") ? num / 1024 : num;
    }
  }

  const rawTotal = (count * unitSize).toFixed(1);
  let netTotal = 0;
  let resilienceText = "Sélectionnez des disques";
  let resilienceBadge = "badge-secondary";
  let isValid = false;
  let minReq = 2;

  switch (currentSelectedRaidLevel) {
    case "raid5":
      minReq = 3;
      if (count >= 3) {
        netTotal = ((count - 1) * unitSize).toFixed(1);
        resilienceText = "Tolère la panne de 1 disque complet";
        resilienceBadge = "badge-success";
        isValid = true;
      } else {
        resilienceText = "RAID 5 requiert au moins 3 disques";
        resilienceBadge = "badge-danger";
      }
      break;

    case "raid1":
      minReq = 2;
      if (count >= 2) {
        netTotal = unitSize.toFixed(1);
        resilienceText = "Miroir 1:1 (Tolère la panne de 1 disque)";
        resilienceBadge = "badge-success";
        isValid = true;
      } else {
        resilienceText = "RAID 1 requiert au moins 2 disques";
        resilienceBadge = "badge-danger";
      }
      break;

    case "raid6":
      minReq = 4;
      if (count >= 4) {
        netTotal = ((count - 2) * unitSize).toFixed(1);
        resilienceText = "Tolère la panne simultanée de 2 disques";
        resilienceBadge = "badge-success";
        isValid = true;
      } else {
        resilienceText = "RAID 6 requiert au moins 4 disques";
        resilienceBadge = "badge-danger";
      }
      break;

    case "raid10":
      minReq = 4;
      if (count >= 4 && count % 2 === 0) {
        netTotal = ((count / 2) * unitSize).toFixed(1);
        resilienceText = "Performance maximale + Tolérance aux pannes";
        resilienceBadge = "badge-accent";
        isValid = true;
      } else {
        resilienceText = "RAID 10 requiert un nombre pair de disques (min 4)";
        resilienceBadge = "badge-danger";
      }
      break;

    case "raid0":
      minReq = 2;
      if (count >= 2) {
        netTotal = (count * unitSize).toFixed(1);
        resilienceText = "0 parité (Perte totale si 1 disque lâche)";
        resilienceBadge = "badge-danger";
        isValid = true;
      } else {
        resilienceText = "RAID 0 requiert au moins 2 disques";
        resilienceBadge = "badge-danger";
      }
      break;

    case "linear":
      minReq = 1;
      if (count >= 1) {
        netTotal = (count * unitSize).toFixed(1);
        resilienceText = "Concaténation simple (JBOD)";
        resilienceBadge = "badge-warning";
        isValid = true;
      }
      break;
  }

  if (rawEl) rawEl.textContent = `${rawTotal} To`;
  if (netEl) netEl.textContent = `${netTotal} To`;
  if (resEl) {
    resEl.textContent = resilienceText;
    resEl.className = `badge ${resilienceBadge}`;
  }

  if (btnSubmit) {
    btnSubmit.disabled = !isValid;
    btnSubmit.classList.toggle("disabled", !isValid);
  }

  if (errEl) {
    errEl.textContent = isValid ? "" : `Sélection insuffisante (${count}/${minReq} disques requis pour ${currentSelectedRaidLevel.toUpperCase()}).`;
  }
}

async function submitCreateRaid() {
  const nameInput = document.getElementById("raid-input-name");
  const fstypeSelect = document.getElementById("raid-input-fstype");
  const mountInput = document.getElementById("raid-input-mount");
  const errEl = document.getElementById("raid-submit-error");
  const btnSubmit = document.getElementById("btn-submit-raid");

  const name = (nameInput?.value || "").trim();
  if (!name) {
    if (errEl) errEl.textContent = "Veuillez renseigner un nom pour le pool.";
    return;
  }

  const checkedDisks = Array.from(document.querySelectorAll(".raid-disk-checkbox:checked")).map(cb => cb.value);
  if (checkedDisks.length < 2 && currentSelectedRaidLevel !== "linear") {
    if (errEl) errEl.textContent = "Sélectionnez au moins 2 disques.";
    return;
  }

  const confirmMsg = `ATTENTION : Vous êtes sur le point de créer un pool ${currentSelectedRaidLevel.toUpperCase()} avec ${checkedDisks.length} disques :\n${checkedDisks.join(", ")}.\n\nToutes les données existantes sur ces disques seront définitivement effacées.\n\nVoulez-vous continuer ?`;
  if (!confirm(confirmMsg)) return;

  if (btnSubmit) {
    btnSubmit.disabled = true;
    btnSubmit.textContent = "Initialisation en cours...";
  }

  try {
    const res = await fetch("/api/storage/raids/create", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name,
        level: currentSelectedRaidLevel,
        devices: checkedDisks,
        fs_type: fstypeSelect?.value || "btrfs",
        mountpoint: mountInput?.value || `/mnt/${name}`,
      })
    });

    const json = await res.json();
    if (json.success) {
      showToast(json.data || "Pool RAID créé avec succès !", "success");
      closeCreateRaidModal();
      loadStorage();
    } else {
      if (errEl) errEl.textContent = json.message || "Échec de création du RAID";
      showToast("Erreur : " + (json.message || "Échec"), "error");
    }
  } catch (err) {
    if (errEl) errEl.textContent = "Erreur de connexion : " + err;
    showToast("Erreur lors de la création : " + err, "error");
  } finally {
    if (btnSubmit) {
      btnSubmit.disabled = false;
      btnSubmit.textContent = "🚀 Créer et Initialiser la Grappe RAID";
    }
  }
}

// --------------------------------------------------------------------------
// MODALE FORMATAGE SÉCURISÉ DE DISQUE
// --------------------------------------------------------------------------
function openFormatDiskModal() {
  openFormatDiskModalFor(null);
}

function openFormatDiskModalFor(preselectPath) {
  const modal = document.getElementById("format-disk-modal");
  const select = document.getElementById("format-select-device");
  const chk = document.getElementById("format-confirm-checkbox");
  const btn = document.getElementById("btn-submit-format");

  if (!modal || !select) return;

  const eligibleDisks = cachedStorageDisks.filter(d => !d.is_system);

  if (eligibleDisks.length === 0) {
    select.innerHTML = `<option value="">Aucun disque disponible pour formatage</option>`;
  } else {
    select.innerHTML = eligibleDisks.map(d => {
      const selected = (preselectPath && d.path === preselectPath) ? "selected" : "";
      return `<option value="${escapeHtml(d.path)}" ${selected}>${escapeHtml(d.path)} (${escapeHtml(d.size_human)}) &mdash; ${escapeHtml(d.model)}</option>`;
    }).join("");
  }

  if (chk) chk.checked = false;
  if (btn) {
    btn.disabled = true;
    btn.classList.add("disabled");
  }

  modal.style.display = "flex";
}

function closeFormatDiskModal() {
  const modal = document.getElementById("format-disk-modal");
  if (modal) modal.style.display = "none";
}

function toggleFormatSubmitBtn() {
  const chk = document.getElementById("format-confirm-checkbox");
  const btn = document.getElementById("btn-submit-format");
  if (chk && btn) {
    btn.disabled = !chk.checked;
    btn.classList.toggle("disabled", !chk.checked);
  }
}

function updateFormatWarning() {
  const chk = document.getElementById("format-confirm-checkbox");
  if (chk) chk.checked = false;
  toggleFormatSubmitBtn();
}

async function submitFormatDisk() {
  const select = document.getElementById("format-select-device");
  const fstypeSelect = document.getElementById("format-select-fstype");
  const labelInput = document.getElementById("format-input-label");
  const btn = document.getElementById("btn-submit-format");

  const device = select?.value;
  if (!device) return;

  const confirmMsg = `DANGER : Confirmez-vous le formatage COMPLET du disque ${device} en ${fstypeSelect?.value.toUpperCase()} ?\nToutes les données existantes seront perdues.`;
  if (!confirm(confirmMsg)) return;

  if (btn) {
    btn.disabled = true;
    btn.textContent = "Formatage en cours...";
  }

  try {
    const res = await fetch("/api/storage/disks/format", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        device,
        fs_type: fstypeSelect?.value || "btrfs",
        label: labelInput?.value || "STORAGE",
      })
    });

    const json = await res.json();
    if (json.success) {
      showToast(json.data || "Disque formaté avec succès !", "success");
      closeFormatDiskModal();
      loadStorage();
    } else {
      showToast("Échec du formatage : " + (json.message || "Erreur"), "error");
    }
  } catch (err) {
    showToast("Erreur lors du formatage : " + err, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "🧹 Formater Définitivement";
    }
  }
}


// --------------------------------------------------------------------------
// SERVICES & DOCKER
// --------------------------------------------------------------------------
async function loadServices() {
  try {
    const res = await fetch("/api/services");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const data = json.data;

    // Quick services in overview
    const quickWrap = document.getElementById("quick-services-wrap");
    if (quickWrap) {
      quickWrap.innerHTML = data.services.map(s => `
        <div style="background:var(--mantle); border:1px solid rgba(255,255,255,0.06); padding:8px 14px; border-radius:var(--radius-md); display:flex; align-items:center; gap:10px;">
          <span class="status-indicator ${s.is_active ? 'status-online' : 'status-offline'}"></span>
          <span style="font-size:0.85rem; font-weight:600;">${escapeHtml(s.display_name)}</span>
          <span style="font-size:0.75rem; color:var(--subtext0);">(${escapeHtml(s.sub_state)})</span>
          <button type="button" class="btn btn-secondary btn-xs" onclick="restartService('${escapeHtml(s.unit_name)}')">🔄</button>
        </div>
      `).join("");
    }

    // Docker containers
    const contContainer = document.getElementById("containers-container");
    if (contContainer) {
      if (data.containers.length === 0) {
        contContainer.innerHTML = `<p style="color:var(--subtext0); font-size:0.9rem;">Aucun conteneur Docker en cours d'exécution.</p>`;
      } else {
        contContainer.innerHTML = data.containers.map(c => `
          <div class="container-card">
            <div class="container-header">
              <span class="container-name">🐳 ${escapeHtml(c.name)}</span>
              <span class="badge ${c.is_running ? 'badge-success' : 'badge-warning'}">${escapeHtml(c.status)}</span>
            </div>
            <div class="container-image">Image: ${escapeHtml(c.image)}</div>
            <div style="margin-top:12px; display:flex; justify-content:space-between; align-items:center;">
              <code style="font-size:0.75rem; color:var(--subtext0);">${escapeHtml(c.id.substring(0, 12))}</code>
              <button type="button" class="btn btn-secondary btn-xs" onclick="restartContainer('${escapeHtml(c.id)}')">Redémarrer</button>
            </div>
          </div>
        `).join("");
      }
    }
  } catch (err) {
    console.warn("Erreur fetch /api/services:", err);
  }
}


// --------------------------------------------------------------------------
// PARE-FEU MODULAIRE
// --------------------------------------------------------------------------
async function loadFirewall() {
  try {
    const res = await fetch("/api/firewall");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const fw = json.data;

    const fwBadge = document.getElementById("firewall-status-badge");
    if (fwBadge) {
      fwBadge.textContent = fw.is_enabled ? "🟢 Pare-feu Actif" : "🔴 Pare-feu Désactivé";
      fwBadge.className = `badge ${fw.is_enabled ? 'badge-success' : 'badge-danger'}`;
    }

    const tbody = document.getElementById("firewall-tbody");
    if (tbody) {
      const allPorts = [...fw.tcp_ports, ...fw.udp_ports];
      tbody.innerHTML = allPorts.map(p => `
        <tr>
          <td><strong style="font-family:var(--font-mono); color:var(--mauve);">${p.port}</strong></td>
          <td><span class="badge badge-info">${escapeHtml(p.protocol)}</span></td>
          <td>${escapeHtml(p.service_name)}</td>
          <td><span class="badge badge-success">🟢 ${escapeHtml(p.status)}</span></td>
        </tr>
      `).join("");
    }

    // Fail2ban
    const bannedWrap = document.getElementById("banned-ips-list");
    if (bannedWrap) {
      if (fw.banned_ips.length === 0) {
        bannedWrap.innerHTML = `<span style="color:var(--green); font-size:0.85rem;">✔ Aucune adresse IP actuellement bannie. Système sain.</span>`;
      } else {
        bannedWrap.innerHTML = fw.banned_ips.map(ip => `
          <span class="badge badge-danger">${escapeHtml(ip)}</span>
        `).join("");
      }
    }
  } catch (err) {
    console.warn("Erreur fetch /api/firewall:", err);
  }
}

// --------------------------------------------------------------------------
// LOGS EN TEMPS RÉEL
// --------------------------------------------------------------------------
async function loadLogs() {
  const select = document.getElementById("select-log-unit");
  const unit = select ? select.value : "sshd";
  const content = document.getElementById("terminal-content");
  const title = document.getElementById("terminal-unit-title");

  if (title) title.textContent = `journalctl -u ${unit} -n 50`;
  if (content) content.textContent = "Lecture du journal système...";

  try {
    const res = await fetch(`/api/logs?unit=${unit}&lines=50`);
    const json = await res.json();
    if (json.success && json.data) {
      if (content) content.textContent = json.data;
    } else {
      if (content) content.textContent = "Aucun log disponible pour ce service.";
    }
  } catch (e) {
    if (content) content.textContent = "Erreur de lecture des logs : " + e;
  }
}

// --------------------------------------------------------------------------
// ACTIONS SYSTÈME & DISQUES
// --------------------------------------------------------------------------
async function triggerSpindown(diskName) {
  if (!confirm(`Confirmer la mise en veille immédiate (spindown) du disque ${diskName} ?`)) return;

  try {
    const res = await fetch(`/api/storage/${diskName}/spindown`, { method: "POST" });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Disque mis en veille", "success");
      loadStorage();
    } else {
      showToast(json.message || "Erreur spindown", "error");
    }
  } catch (e) {
    showToast("Erreur : " + e, "error");
  }
}

async function restartService(unit) {
  showToast(`Redémarrage de ${unit}...`, "info");
  try {
    const res = await fetch(`/api/service/${unit}/restart`, { method: "POST" });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Service redémarré avec succès", "success");
      loadServices();
    } else {
      showToast(json.message || "Erreur de redémarrage", "error");
    }
  } catch (e) {
    showToast("Erreur : " + e, "error");
  }
}

async function restartContainer(id) {
  showToast(`Redémarrage du conteneur ${id.substring(0, 8)}...`, "info");
  try {
    const res = await fetch(`/api/service/docker/restart`, { method: "POST" });
    const json = await res.json();
    if (json.success) {
      showToast("Conteneur redémarré avec succès", "success");
      loadServices();
    }
  } catch (e) {
    showToast("Erreur : " + e, "error");
  }
}

// --------------------------------------------------------------------------
// UTILITAIRES & PRESSE-PAPIER
// --------------------------------------------------------------------------
function updateSftpUri() {
  const uriEl = document.getElementById("sftp-connection-uri");
  if (uriEl) {
    const host = location.hostname || "192.168.1.139";
    uriEl.textContent = `sftp://chomiam@${host}:22`;
  }
}

function copySftpUri() {
  const uriEl = document.getElementById("sftp-connection-uri");
  const icon = document.getElementById("sftp-copy-icon");
  if (!uriEl) return;

  navigator.clipboard.writeText(uriEl.textContent).then(() => {
    if (icon) icon.textContent = "✅";
    showToast("Lien sFTP direct copié dans le presse-papier !", "success");
    setTimeout(() => { if (icon) icon.textContent = "📋"; }, 2000);
  }).catch(err => {
    showToast("Erreur copie : " + err, "error");
  });
}

function copyText(text) {
  navigator.clipboard.writeText(text).then(() => {
    showToast("Copié dans le presse-papier !", "success");
  });
}

function copyLogs() {
  const content = document.getElementById("terminal-content");
  if (content) {
    copyText(content.textContent);
  }
}

function showToast(message, type = "info") {
  const container = document.getElementById("toast-container");
  if (!container) return;

  const toast = document.createElement("div");
  toast.className = "toast";
  toast.innerHTML = `<span>${type === 'success' ? '✅' : type === 'error' ? '❌' : 'ℹ️'}</span> <span>${escapeHtml(message)}</span>`;
  container.appendChild(toast);

  setTimeout(() => {
    toast.remove();
  }, 3500);
}

function escapeHtml(str) {
  if (!str) return "";
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

function parseAnsiToHtml(raw) {
  if (!raw) return "";

  let s = String(raw);

  // 1. Simuler l effacement de ligne : \x1b[1G ou \x1b[2K -> retour chariot \r
  s = s.replace(/\x1b\[\?[0-9;]*[a-zA-Z]/g, "")
       .replace(/\x1b\[[0-9;]*[GgKk]/g, "\r");

  // 2. Traiter les retours chariot (\r) par ligne pour ne garder que le dernier etat
  const rawLines = s.split("\n");
  const processedLines = [];
  for (let line of rawLines) {
    if (line.includes("\r")) {
      const parts = line.split("\r").filter(p => p.trim().length > 0);
      line = parts.length > 0 ? parts[parts.length - 1] : "";
    }
    processedLines.push(line);
  }
  s = processedLines.join("\n");

  // 3. Dedupliquer les blocs d arbre / graphiques redessines en boucle (ex: nix-output-monitor)
  const graphMarker = "┏━ Dependency Graph:";
  if (s.includes(graphMarker)) {
    const parts = s.split(graphMarker);
    if (parts.length > 2) {
      s = parts[0] + graphMarker + parts[parts.length - 1];
    }
  }

  // 4. Nettoyer les repetitions residuelles de spinners (ex: ⏱ 0s⏱ 0s⏱ 1s -> ⏱ 1s)
  s = s.replace(/(?:⏱\s*\d+s)+/g, (match) => {
    const subMatches = match.match(/⏱\s*\d+s/g);
    return subMatches ? subMatches[subMatches.length - 1] : match;
  });

  // 5. Nettoyer les sequences curseur restantes (ex: curseur haut/bas \x1b[10A)
  s = s.replace(/\x1b\[[0-9;]*[A-FJST]/g, "");

  // 6. Echapper HTML pour la securite
  s = escapeHtml(s);

  // Mapper les codes ANSI vers les classes CSS Catppuccin Mocha
  const colorMap = {
    "1": "ansi-bold",
    "2": "ansi-dim",
    "30": "ansi-black",
    "31": "ansi-red",
    "32": "ansi-green",
    "33": "ansi-yellow",
    "34": "ansi-blue",
    "35": "ansi-magenta",
    "36": "ansi-cyan",
    "37": "ansi-white",
    "90": "ansi-bright-black",
    "91": "ansi-bright-red",
    "92": "ansi-bright-green",
    "93": "ansi-bright-yellow",
    "94": "ansi-bright-blue",
    "95": "ansi-bright-magenta",
    "96": "ansi-bright-cyan",
    "97": "ansi-bright-white"
  };

  let openTagsCount = 0;
  s = s.replace(/\x1b\[([0-9;]*)m/g, (match, p1) => {
    const codes = p1 ? p1.split(";") : ["0"];
    let out = "";
    for (const code of codes) {
      const c = code.trim();
      if (c === "0" || c === "") {
        while (openTagsCount > 0) {
          out += "</span>";
          openTagsCount--;
        }
      } else if (colorMap[c]) {
        out += `<span class="${colorMap[c]}">`;
        openTagsCount++;
      }
    }
    return out;
  });

  while (openTagsCount > 0) {
    s += "</span>";
    openTagsCount--;
  }

  return s;
}


// --------------------------------------------------------------------------
// CONSOLE & TERMINAL BASH INTERACTIF
// --------------------------------------------------------------------------
let activeConsoleSubTab = "logs";
let terminalCwd = "/etc/nixos";
let termHistory = [];
let termHistoryIdx = -1;
let currentInputDraft = "";

function switchConsoleSubTab(subTab) {
  activeConsoleSubTab = subTab;

  const btnLogs = document.getElementById("btn-side-logs");
  const btnTerm = document.getElementById("btn-side-terminal");
  const paneLogs = document.getElementById("subpane-logs");
  const paneTerm = document.getElementById("subpane-terminal");

  if (btnLogs) btnLogs.classList.toggle("active", subTab === "logs");
  if (btnTerm) btnTerm.classList.toggle("active", subTab === "terminal");

  if (paneLogs) paneLogs.style.display = subTab === "logs" ? "block" : "none";
  if (paneTerm) paneTerm.style.display = subTab === "terminal" ? "block" : "none";

  if (subTab === "terminal") {
    const input = document.getElementById("bash-input");
    if (input) setTimeout(() => input.focus(), 50);
  } else {
    loadLogs();
  }
}

async function submitBashCommand() {
  const input = document.getElementById("bash-input");
  if (!input) return;

  const cmd = input.value.trim();
  if (!cmd) return;

  termHistory.push(cmd);
  termHistoryIdx = termHistory.length;
  currentInputDraft = "";
  input.value = "";
  hideAutocompleteDropdown();

  appendCommandToTerminal(cmd, terminalCwd);

  try {
    const res = await fetch("/api/terminal/exec", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        command: cmd,
        cwd: terminalCwd
      })
    });
    const json = await res.json();
    const result = json.data || {};

    if (result.cwd) {
      terminalCwd = result.cwd;
      updateTerminalPrompt();
    }

    appendResultToTerminal(result);
  } catch (err) {
    appendResultToTerminal({
      success: false,
      stdout: "",
      stderr: "Erreur de communication avec le serveur : " + err,
      exit_code: -1,
      duration_ms: 0
    });
  }
}

function appendCommandToTerminal(cmd, cwd) {
  const body = document.getElementById("bash-terminal-body");
  if (!body) return;

  const entry = document.createElement("div");
  entry.className = "term-history-entry";
  entry.innerHTML = `
    <div class="term-cmd-line">
      <div>
        <span class="term-cmd-prompt">chomiam@steveos-nas:<b>${escapeHtml(formatShortCwd(cwd))}</b>$</span>
        <span class="term-cmd-text">${escapeHtml(cmd)}</span>
      </div>
      <span class="badge badge-warning">⏳ En cours</span>
    </div>
    <div class="term-output-placeholder"></div>
  `;
  body.appendChild(entry);
  body.scrollTop = body.scrollHeight;
}

function appendResultToTerminal(res) {
  const body = document.getElementById("bash-terminal-body");
  if (!body) return;

  const lastEntry = body.lastElementChild;
  if (!lastEntry) return;

  const badge = lastEntry.querySelector(".badge");
  if (badge) {
    if (res.success) {
      badge.className = "badge badge-success term-cmd-badge";
      badge.textContent = `✔ 0 (${res.duration_ms}ms)`;
    } else {
      badge.className = "badge badge-danger term-cmd-badge";
      badge.textContent = `❌ ${res.exit_code} (${res.duration_ms}ms)`;
    }
  }

  const outputPlaceholder = lastEntry.querySelector(".term-output-placeholder");
  if (outputPlaceholder) {
    let outHtml = "";
    if (res.stdout) {
      outHtml += `<div class="term-stdout">${parseAnsiToHtml(res.stdout)}</div>`;
    }
    if (res.stderr) {
      outHtml += `<div class="term-stderr">${parseAnsiToHtml(res.stderr)}</div>`;
    }
    outputPlaceholder.innerHTML = outHtml;
  }

  body.scrollTop = body.scrollHeight;
}

function updateTerminalPrompt() {
  const promptLabel = document.getElementById("bash-prompt-label");
  const cwdBadge = document.getElementById("term-cwd-badge");
  const shortCwd = formatShortCwd(terminalCwd);

  if (promptLabel) {
    promptLabel.innerHTML = `chomiam@steveos-nas:<b>${escapeHtml(shortCwd)}</b>$`;
  }
  if (cwdBadge) {
    cwdBadge.textContent = `📁 ${shortCwd}`;
  }
}

function formatShortCwd(cwd) {
  if (cwd.startsWith("/home/chomiam")) {
    return "~" + cwd.substring("/home/chomiam".length);
  }
  return cwd;
}

function runQuickCommand(cmd) {
  switchConsoleSubTab("terminal");
  const input = document.getElementById("bash-input");
  if (input) {
    input.value = cmd;
    submitBashCommand();
  }
}

function clearBashTerminal() {
  const body = document.getElementById("bash-terminal-body");
  if (!body) return;
  body.innerHTML = `
    <div class="term-welcome-msg">
      <span style="color:var(--mauve); font-weight:bold;">🚀 STEvE_OS Interactive Bash Console</span> — Écran effacé.<br>
      <span style="color:var(--subtext0); font-size:0.8rem;">• Touche <kbd>Tab</kbd> : Autocomplétion • Flèches <kbd>↑</kbd> / <kbd>↓</kbd> : Historique • <kbd>Ctrl+L</kbd> : Effacer</span>
    </div>
  `;
}

function copyBashTerminal() {
  const body = document.getElementById("bash-terminal-body");
  if (body) {
    copyText(body.innerText);
  }
}

// --------------------------------------------------------------------------
// AUTOCOMPLÉTION & GESTION CLAVIER (TAB, ARROWS)
// --------------------------------------------------------------------------
document.addEventListener("DOMContentLoaded", () => {
  setupBashInputListeners();
});

function setupBashInputListeners() {
  const input = document.getElementById("bash-input");
  if (!input) return;

  input.addEventListener("keydown", async (e) => {
    if (e.key === "Tab") {
      e.preventDefault();
      await handleTabCompletion(input);
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      if (termHistory.length === 0) return;
      if (termHistoryIdx === termHistory.length) {
        currentInputDraft = input.value;
      }
      if (termHistoryIdx > 0) {
        termHistoryIdx--;
        input.value = termHistory[termHistoryIdx];
      }
      hideAutocompleteDropdown();
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (termHistoryIdx < termHistory.length - 1) {
        termHistoryIdx++;
        input.value = termHistory[termHistoryIdx];
      } else if (termHistoryIdx === termHistory.length - 1) {
        termHistoryIdx = termHistory.length;
        input.value = currentInputDraft;
      }
      hideAutocompleteDropdown();
      return;
    }

    if (e.key === "Enter") {
      e.preventDefault();
      submitBashCommand();
      return;
    }

    if (e.ctrlKey && (e.key === "l" || e.key === "L")) {
      e.preventDefault();
      clearBashTerminal();
      return;
    }

    if (e.key === "Escape") {
      hideAutocompleteDropdown();
      return;
    }
  });

  document.addEventListener("click", (e) => {
    if (!e.target.closest("#term-autocomplete-dropdown") && e.target !== input) {
      hideAutocompleteDropdown();
    }
  });
}

async function handleTabCompletion(input) {
  const val = input.value;

  try {
    const res = await fetch("/api/terminal/complete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        prefix: val,
        cwd: terminalCwd
      })
    });
    const json = await res.json();
    const suggestions = (json.data && json.data.suggestions) || [];

    if (suggestions.length === 1) {
      applyCompletion(input, suggestions[0]);
      hideAutocompleteDropdown();
    } else if (suggestions.length > 1) {
      renderAutocompleteDropdown(suggestions, input);
    } else {
      hideAutocompleteDropdown();
    }
  } catch (err) {
    console.warn("Erreur completion:", err);
  }
}

function applyCompletion(input, suggestion) {
  const currentVal = input.value;
  if (suggestion.startsWith(currentVal)) {
    input.value = suggestion + (suggestion.endsWith("/") ? "" : " ");
  } else {
    const tokens = currentVal.split(" ");
    tokens[tokens.length - 1] = suggestion;
    input.value = tokens.join(" ") + (suggestion.endsWith("/") ? "" : " ");
  }
}

function renderAutocompleteDropdown(suggestions, input) {
  const dropdown = document.getElementById("term-autocomplete-dropdown");
  if (!dropdown) return;

  dropdown.innerHTML = suggestions.map((s, idx) => `
    <div class="autocomplete-item ${idx === 0 ? 'selected' : ''}" onclick="selectSuggestion('${escapeHtml(s)}')">
      <span>💡 ${escapeHtml(s)}</span>
      <span style="font-size:0.72rem; color:var(--subtext0);">${s.endsWith('/') ? 'dossier' : 'commande / fichier'}</span>
    </div>
  `).join("");

  dropdown.style.display = "block";
}

function selectSuggestion(s) {
  const input = document.getElementById("bash-input");
  if (input) {
    applyCompletion(input, s);
    input.focus();
  }
  hideAutocompleteDropdown();
}

function hideAutocompleteDropdown() {
  const dropdown = document.getElementById("term-autocomplete-dropdown");
  if (dropdown) dropdown.style.display = "none";
}

// --------------------------------------------------------------------------
// EXPLORATEUR DE FICHIERS (FILE MANAGER)
// --------------------------------------------------------------------------
let currentFolderPath = "/home/chomiam";
let isTrashView = false;
let trashOverview = null;
let selectedTrashItem = null;
let currentFolderParent = null;
let currentEntries = [];
let fileViewMode = "grid";
let fileClipboard = null; // { action: 'copy' | 'cut', path: string, name: string }
let selectedFileItem = null;

async function navigateToPath(targetPath) {
  isTrashView = false;
  selectedTrashItem = null;
  const normalTb = document.getElementById("files-toolbar-normal");
  const trashTb = document.getElementById("files-toolbar-trash");
  if (normalTb) normalTb.style.display = "flex";
  if (trashTb) trashTb.style.display = "none";
  if (!targetPath) return;

  try {
    const res = await fetch(`/api/files/list?path=${encodeURIComponent(targetPath)}`);
    const json = await res.json();

    if (!json.success || !json.data) {
      showToast(json.message || "Impossible d'ouvrir ce dossier", "error");
      return;
    }

    const data = json.data;
    currentFolderPath = data.current_path;
    currentFolderParent = data.parent_path;
    currentEntries = data.entries || [];

    updateFilesBreadcrumbs(currentFolderPath);
    updateSidebarNavActive(currentFolderPath);
    renderFilesList(currentEntries);
    updateFilesStatusBar(data.total_items, data.total_size_bytes);
  } catch (err) {
    showToast("Erreur lors de la navigation : " + err, "error");
  }
}

function updateSidebarNavActive(path) {
  if (path === "/corbeille" || isTrashView) {
    document.querySelectorAll(".files-nav-item").forEach(item => item.classList.remove("active"));
    const el = document.getElementById("fnav-trash");
    if (el) el.classList.add("active");
    return;
  }
  const normPath = path ? path.replace(/\/+$/, '') : '';
  const mapping = {
    "/home/chomiam": "fnav-home",
    "/home/chomiam/documents": "fnav-docs",
    "/home/chomiam/images": "fnav-pics",
    "/home/chomiam/videos": "fnav-vids",
    "/home/chomiam/musique": "fnav-music",
    "/home/chomiam/telechargements": "fnav-dl",
    "/home/chomiam/downloads": "fnav-dl",
    "/home/chomiam/pictures": "fnav-pics",
    "/home/chomiam/music": "fnav-music",
    "/": "fnav-root",
    "/mnt/storage/shares": "fnav-shares",
    "/mnt/storage/media": "fnav-media",
    "/etc/nixos": "fnav-nixos"
  };

  document.querySelectorAll(".files-nav-item").forEach(item => item.classList.remove("active"));
  const activeId = mapping[path];
  if (activeId) {
    const el = document.getElementById(activeId);
    if (el) el.classList.add("active");
  }
}

function updateFilesBreadcrumbs(path) {
  const container = document.getElementById("files-breadcrumbs");
  if (!container) return;

  const parts = path.split("/").filter(Boolean);
  let html = `<span class="crumb-item ${parts.length === 0 ? 'active' : ''}" onclick="navigateToPath('/')">🗄️ /</span>`;

  let accumulated = "";
  parts.forEach((part, idx) => {
    accumulated += "/" + part;
    const isLast = idx === parts.length - 1;
    const thisPath = accumulated;
    html += `<span class="crumb-separator">/</span>`;
    html += `<span class="crumb-item ${isLast ? 'active' : ''}" onclick="navigateToPath('${escapeHtml(thisPath)}')">${escapeHtml(part)}</span>`;
  });

  container.innerHTML = html;
}

function renderFilesList(entries) {
  const gridWrap = document.getElementById("files-grid-wrap");
  const tableBody = document.getElementById("files-table-tbody");
  const gridContainer = document.getElementById("files-grid-wrap");
  const tableContainer = document.getElementById("files-table-wrap");

  if (!gridWrap || !tableBody) return;

  if (entries.length === 0) {
    const emptyHtml = `<div style="grid-column:1/-1; padding:40px; text-align:center; color:var(--subtext0);">📁 Dossier vide</div>`;
    gridWrap.innerHTML = emptyHtml;
    tableBody.innerHTML = `<tr><td colspan="4" style="text-align:center; color:var(--subtext0); padding:30px;">📁 Dossier vide</td></tr>`;
    return;
  }

  // Rendu Grille
  gridWrap.innerHTML = entries.map(item => {
    const icon = getFileIcon(item);
    let cardPreview = `<div class="file-card-icon">${icon}</div>`;
    if (isImageFile(item.name, item.category)) {
      const thumbUrl = `/api/files/image-view?path=${encodeURIComponent(item.path)}&thumb=true`;
      cardPreview = `<div class="file-card-icon file-card-img-preview" style="width:100%; height:80px; max-height:80px; overflow:hidden; border-radius:6px; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,0.35);"><img src="${thumbUrl}" loading="lazy" alt="${escapeHtml(item.name)}" style="width:100%; height:100%; max-width:100%; max-height:100%; object-fit:cover; display:block; border-radius:5px;" onerror="this.onerror=null; this.parentElement.className='file-card-icon'; this.parentElement.style='width:100%; height:80px; display:flex; align-items:center; justify-content:center; font-size:2.4rem;'; this.parentElement.innerHTML='${icon}';"></div>`;
    }
    return `
      <div class="file-card" style="min-width:0; overflow:hidden;"
           data-path="${escapeHtml(item.path)}"
           onclick="handleFileClick(event, '${escapeHtml(item.path)}', ${item.is_dir})"
           ondblclick="handleFileDblClick('${escapeHtml(item.path)}', ${item.is_dir})"
           oncontextmenu="handleItemContextMenu(event, '${escapeHtml(item.path)}')">
        ${cardPreview}
        <div class="file-card-name" title="${escapeHtml(item.name)}">${escapeHtml(item.name)}</div>
        <div class="file-card-meta">${escapeHtml(item.size_human)}</div>
      </div>
    `;
  }).join("");

  // Rendu Liste / Table
  tableBody.innerHTML = entries.map(item => {
    const icon = getFileIcon(item);
    return `
      <tr data-path="${escapeHtml(item.path)}"
          onclick="handleFileClick(event, '${escapeHtml(item.path)}', ${item.is_dir})"
          ondblclick="handleFileDblClick('${escapeHtml(item.path)}', ${item.is_dir})"
          oncontextmenu="handleItemContextMenu(event, '${escapeHtml(item.path)}')">
        <td>
          <span style="font-size:1.1rem; margin-right:8px;">${icon}</span>
          <strong style="color:var(--text);">${escapeHtml(item.name)}</strong>
        </td>
        <td style="color:var(--subtext0); font-family:var(--font-mono); font-size:0.8rem;">${escapeHtml(item.size_human)}</td>
        <td style="color:var(--subtext0); font-size:0.8rem;">${escapeHtml(item.modified)}</td>
        <td><code style="color:var(--mauve); font-size:0.75rem;">${escapeHtml(item.permissions)}</code></td>
      </tr>
    `;
  }).join("");

  // Affichage selon le mode
  if (fileViewMode === "grid") {
    gridContainer.style.display = "grid";
    tableContainer.style.display = "none";
  } else {
    gridContainer.style.display = "none";
    tableContainer.style.display = "table";
  }
}

function isDocumentFile(fileName, category) {
  if (!fileName) return false;
  // Documents texte : .pdf, .docx, .doc, .odt, .rtf
  // Tableurs : .xlsx, .xls, .ods, .csv
  // Présentations : .pptx, .ppt, .odp
  return /\.(pdf|docx|doc|odt|rtf|xlsx|xls|ods|csv|pptx|ppt|odp)$/i.test(fileName);
}

function getFileIcon(item) {
  if (item.is_dir) return "📁";
  const name = item.name.toLowerCase();
  if (/\.(nef|nrw|cr2|cr3|crw|arw|srf|sr2|dng|raf|rw2|orf|pef|3fr|raw)$/i.test(name)) return "📷";
  if (/\.(heic|heif|hif)$/i.test(name)) return "📱";
  if (/\.(pcx|tga|targa|dds)$/i.test(name)) return "🎨";
  if (/\.pdf$/i.test(name)) return "📕";
  if (/\.(docx|doc|odt|rtf)$/i.test(name)) return "📘";
  if (/\.(xlsx|xls|ods|csv)$/i.test(name)) return "📊";
  if (/\.(pptx|ppt|odp)$/i.test(name)) return "📽️";
  switch (item.category) {
    case "image": return "🖼️";
    case "video": return "🎬";
    case "audio": return "🎵";
    case "document": return "📄";
    case "archive": return "📦";
    case "code": return "💻";
    default: return "📄";
  }
}

function handleFileClick(e, path, isDir) {
  e.stopPropagation();
  selectedFileItem = currentEntries.find(i => i.path === path) || null;

  document.querySelectorAll(".file-card, .files-table-view tr").forEach(el => {
    el.classList.toggle("selected", el.getAttribute("data-path") === path);
  });
}

function isNvimEditableFile(fileName, category) {
  if (!fileName) return false;
  if (category === "image" || category === "video" || category === "audio" || category === "archive") {
    return false;
  }
  if (/\.(pdf|doc|docx|odt|xls|xlsx|ppt|pptx|epub|bin|exe|so|dll|dylib|iso|img)$/i.test(fileName)) {
    return false;
  }
  if (category === "code") return true;

  const codeExtRegex = /\.(fish|nix|sh|bash|zsh|nu|ksh|csh|txt|md|markdown|rst|log|conf|config|ini|cfg|json|json5|jsonc|toml|yaml|yml|xml|env|service|timer|target|socket|desktop|rs|js|mjs|cjs|ts|tsx|jsx|py|c|cpp|cc|cxx|h|hpp|go|lua|vim|sql|php|rb|html|htm|css|scss|sass|less|diff|patch|lock|csv|tsv|properties|theme|rules)$/i;
  if (codeExtRegex.test(fileName)) return true;

  const exactNamesRegex = /^(makefile|dockerfile|containerfile|justfile|rakefile|gemfile|cmakelists\.txt|license|readme|\.gitignore|\.gitattributes|\.bashrc|\.bash_profile|\.profile|\.zshrc|\.zshenv|\.fishrc|\.vimrc|\.nanorc|\.editorconfig|\.env|flake\.lock|cargo\.lock)$/i;
  if (exactNamesRegex.test(fileName)) return true;

  if (category === "document") return true;
  return false;
}

function handleFileDblClick(path, isDir) {
  if (isDir) {
    navigateToPath(path);
  } else {
    const item = currentEntries.find(i => i.path === path);
    const fileName = item ? item.name : path.split("/").pop();
    const cat = item ? item.category : "";

    if (isImageFile(fileName, cat)) {
      openImageModal(path, fileName, item);
    } else if (isDocumentFile(fileName, cat)) {
      openDocModal(path, fileName, item);
    } else if (cat === "video" || /\.(mp4|mkv|webm|avi|mov|m4v|flv)$/i.test(fileName)) {
      openMpvModal(path, fileName);
    } else if (cat === "audio" || /\.(mp3|flac|wav|aac|ogg|m4a|opus|wma)$/i.test(fileName)) {
      openAudioModal(path, fileName, item ? item.size_bytes : 0);
    } else if (isNvimEditableFile(fileName, cat)) {
      openNvimModal(path, fileName);
    } else {
      showToast(`Fichier : ${fileName}`, "info");
    }
  }
}

function navigateUpFolder() {
  if (currentFolderParent) {
    navigateToPath(currentFolderParent);
  } else {
    showToast("Vous êtes déjà à la racine du système.", "info");
  }
}

function refreshCurrentFolder() {
  navigateToPath(currentFolderPath);
}

function setFileViewMode(mode) {
  fileViewMode = mode;
  const btnGrid = document.getElementById("btn-view-grid");
  const btnTable = document.getElementById("btn-view-table");

  if (btnGrid) btnGrid.classList.toggle("active", mode === "grid");
  if (btnTable) btnTable.classList.toggle("active", mode === "table");

  renderFilesList(currentEntries);
}

function filterFilesList() {
  const query = (document.getElementById("files-filter-input")?.value || "").toLowerCase().trim();
  if (!query) {
    renderFilesList(currentEntries);
    return;
  }
  const filtered = currentEntries.filter(i => i.name.toLowerCase().includes(query));
  renderFilesList(filtered);
}

function updateFilesStatusBar(count, sizeBytes) {
  const countEl = document.getElementById("files-status-count");
  const pathEl = document.getElementById("files-current-path-text");
  const clipEl = document.getElementById("files-clipboard-status");

  if (countEl) countEl.textContent = `${count} élément${count > 1 ? 's' : ''}`;
  if (pathEl) pathEl.textContent = currentFolderPath;

  if (clipEl) {
    if (fileClipboard) {
      clipEl.textContent = `${fileClipboard.action === 'cut' ? '✂️ Couper' : '📋 Copier'} : ${fileClipboard.name}`;
    } else {
      clipEl.textContent = "";
    }
  }
}

// --------------------------------------------------------------------------
// MENU CONTEXTUEL CLIC DROIT
// --------------------------------------------------------------------------
function handleItemContextMenu(e, path) {
  e.preventDefault();
  e.stopPropagation();

  if (isTrashView) {
    handleTrashContextMenu(e, path);
    return;
  }

  selectedFileItem = currentEntries.find(i => i.path === path) || null;

  const menu = document.getElementById("files-context-menu");
  if (!menu) return;

  // Activer / désactiver les options
  const ctxOpen = document.getElementById("ctx-open");
  const ctxPaste = document.getElementById("ctx-paste");
  const ctxViewImage = document.getElementById("ctx-view-image");
  const ctxEdit = document.getElementById("ctx-edit-nvim");
  const ctxPlay = document.getElementById("ctx-play-video");
  const ctxAudio = document.getElementById("ctx-play-audio");

  if (ctxOpen) ctxOpen.style.display = selectedFileItem && selectedFileItem.is_dir ? "flex" : "none";
  if (ctxPaste) ctxPaste.classList.toggle("disabled", !fileClipboard);

  const isVideo = selectedFileItem && !selectedFileItem.is_dir &&
    (selectedFileItem.category === "video" || /\.(mp4|mkv|webm|avi|mov|m4v|flv)$/i.test(selectedFileItem.name));
  const isAudio = selectedFileItem && !selectedFileItem.is_dir &&
    (selectedFileItem.category === "audio" || /\.(mp3|flac|wav|aac|ogg|m4a|opus|wma)$/i.test(selectedFileItem.name));
  const isEditable = selectedFileItem && !selectedFileItem.is_dir &&
    isNvimEditableFile(selectedFileItem.name, selectedFileItem.category);

  const isDoc = selectedFileItem && !selectedFileItem.is_dir && isDocumentFile(selectedFileItem.name, selectedFileItem.category);
  const ctxViewDoc = document.getElementById("ctx-view-doc");
  if (ctxViewDoc) ctxViewDoc.style.display = isDoc ? "flex" : "none";

  const isImage = selectedFileItem && !selectedFileItem.is_dir && isImageFile(selectedFileItem.name, selectedFileItem.category);
  if (ctxViewImage) ctxViewImage.style.display = isImage ? "flex" : "none";
  if (ctxEdit) ctxEdit.style.display = isEditable ? "flex" : "none";
  if (ctxPlay) ctxPlay.style.display = isVideo ? "flex" : "none";
  if (ctxAudio) ctxAudio.style.display = isAudio ? "flex" : "none";

  positionContextMenu(menu, e.clientX, e.clientY);
}

function handleBackgroundContextMenu(e) {
  if (e.target.closest(".file-card") || e.target.closest("tr")) return;

  e.preventDefault();
  selectedFileItem = null;

  const menu = document.getElementById("files-context-menu");
  if (!menu) return;

  const ctxOpen = document.getElementById("ctx-open");
  const ctxCopy = document.getElementById("ctx-copy");
  const ctxCut = document.getElementById("ctx-cut");
  const ctxRename = document.getElementById("ctx-rename");
  const ctxDelete = document.getElementById("ctx-delete");
  const ctxPaste = document.getElementById("ctx-paste");

  if (ctxOpen) ctxOpen.style.display = "none";
  if (ctxCopy) ctxCopy.style.display = "none";
  if (ctxCut) ctxCut.style.display = "none";
  if (ctxRename) ctxRename.style.display = "none";
  if (ctxDelete) ctxDelete.style.display = "none";

  const ctxEdit = document.getElementById("ctx-edit-nvim");
  const ctxPlay = document.getElementById("ctx-play-video");
  const ctxAudio = document.getElementById("ctx-play-audio");
  if (ctxEdit) ctxEdit.style.display = "none";
  if (ctxPlay) ctxPlay.style.display = "none";
  if (ctxAudio) ctxAudio.style.display = "none";

  if (ctxPaste) {
    ctxPaste.style.display = "flex";
    ctxPaste.classList.toggle("disabled", !fileClipboard);
  }

  positionContextMenu(menu, e.clientX, e.clientY);
}

function positionContextMenu(menu, x, y) {
  // Rétablir l'affichage des actions de fichier si sélectionné
  if (selectedFileItem) {
    ["ctx-copy", "ctx-cut", "ctx-rename", "ctx-delete"].forEach(id => {
      const el = document.getElementById(id);
      if (el) el.style.display = "flex";
    });
  }

  menu.style.display = "block";
  const menuWidth = menu.offsetWidth;
  const menuHeight = menu.offsetHeight;

  const posX = (x + menuWidth > window.innerWidth) ? (window.innerWidth - menuWidth - 10) : x;
  const posY = (y + menuHeight > window.innerHeight) ? (window.innerHeight - menuHeight - 10) : y;

  menu.style.left = `${posX}px`;
  menu.style.top = `${posY}px`;
}

document.addEventListener("click", () => {
  const menu = document.getElementById("files-context-menu");
  if (menu) menu.style.display = "none";
});

// --------------------------------------------------------------------------
// ACTIONS : CRÉER, RENOMMER, SUPPRIMER, COPIER, COUPER, COLLER
// --------------------------------------------------------------------------
async function triggerFileAction(action) {
  const menu = document.getElementById("files-context-menu");
  if (menu) menu.style.display = "none";

  switch (action) {
    case "open":
      if (selectedFileItem && selectedFileItem.is_dir) {
        navigateToPath(selectedFileItem.path);
      }
      break;

    case "new-folder":
      promptCreateFolder();
      break;

    case "edit-nvim":
      if (selectedFileItem && !selectedFileItem.is_dir) {
        openNvimModal(selectedFileItem.path, selectedFileItem.name);
      }
      break;

    case "view-doc":
      if (selectedFileItem && !selectedFileItem.is_dir) {
        openDocModal(selectedFileItem.path, selectedFileItem.name, selectedFileItem);
      }
      break;

    case "view-image":
      if (selectedFileItem && !selectedFileItem.is_dir) {
        openImageModal(selectedFileItem.path, selectedFileItem.name, selectedFileItem);
      }
      break;

    case "play-video":
      if (selectedFileItem && !selectedFileItem.is_dir) {
        openMpvModal(selectedFileItem.path, selectedFileItem.name);
      }
      break;

    case "play-audio":
      if (selectedFileItem && !selectedFileItem.is_dir) {
        openAudioModal(selectedFileItem.path, selectedFileItem.name, selectedFileItem.size_bytes);
      }
      break;

    case "copy":
      if (!selectedFileItem) return;
      fileClipboard = { action: "copy", path: selectedFileItem.path, name: selectedFileItem.name };
      showToast(`Copié : ${selectedFileItem.name}`, "info");
      updateFilesStatusBar(currentEntries.len, 0);
      break;

    case "cut":
      if (!selectedFileItem) return;
      fileClipboard = { action: "cut", path: selectedFileItem.path, name: selectedFileItem.name };
      showToast(`Coupé : ${selectedFileItem.name}`, "info");
      updateFilesStatusBar(currentEntries.len, 0);
      break;

    case "paste":
      if (!fileClipboard) return;
      await pasteClipboardItem();
      break;

    case "rename":
      if (!selectedFileItem) return;
      promptRename(selectedFileItem);
      break;

    case "delete":
      if (!selectedFileItem) return;
      confirmDelete(selectedFileItem, false);
      break;

    case "delete-permanent":
      if (!selectedFileItem) return;
      confirmDelete(selectedFileItem, true);
      break;

    case "trash-restore":
      if (selectedTrashItem) {
        restoreTrashItem(selectedTrashItem.id);
      }
      break;

    case "trash-delete":
      if (selectedTrashItem) {
        deleteTrashPermanent(selectedTrashItem.id);
      }
      break;
  }
}

async function promptCreateFolder() {
  const name = prompt("Nom du nouveau dossier :", "Nouveau_Dossier");
  if (!name || !name.trim()) return;

  try {
    const res = await fetch("/api/files/mkdir", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        path: currentFolderPath,
        name: name.trim()
      })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Dossier créé avec succès !", "success");
      refreshCurrentFolder();
    } else {
      showToast(json.message || "Erreur de création du dossier", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}

async function promptRename(item) {
  const newName = prompt(`Renommer "${item.name}" en :`, item.name);
  if (!newName || !newName.trim() || newName.trim() === item.name) return;

  try {
    const res = await fetch("/api/files/rename", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        path: item.path,
        new_name: newName.trim()
      })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Élément renommé avec succès !", "success");
      refreshCurrentFolder();
    } else {
      showToast(json.message || "Erreur de renommage", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}

async function confirmDelete(item, permanent = false) {
  if (permanent) {
    const isDir = item.is_dir;
    const msg = isDir 
      ? `Êtes-vous sûr de vouloir supprimer DÉFINITIVEMENT le dossier "${item.name}" et tout son contenu ? Cette action est irréversible.`
      : `Êtes-vous sûr de vouloir supprimer DÉFINITIVEMENT le fichier "${item.name}" ? Cette action est irréversible.`;

    if (!confirm(msg)) return;
  }

  try {
    const res = await fetch("/api/files/delete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path: item.path, permanent: permanent })
    });
    const json = await res.json();
    if (json.success) {
      if (permanent) {
        showToast(json.message || "Suppression définitive effectuée.", "success");
      } else {
        showToast(json.message || `'${item.name}' déplacé dans la corbeille (rétention 30 jours)`, "success");
      }
      refreshCurrentFolder();
      fetchTrashCount();
    } else {
      showToast(json.message || "Échec de suppression", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}

async function pasteClipboardItem() {
  if (!fileClipboard) return;

  const endpoint = fileClipboard.action === "cut" ? "/api/files/move" : "/api/files/copy";
  showToast(`${fileClipboard.action === 'cut' ? 'Déplacement' : 'Copie'} de ${fileClipboard.name}...`, "info");

  try {
    const res = await fetch(endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        src_path: fileClipboard.path,
        dest_dir: currentFolderPath
      })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Opération terminée avec succès !", "success");
      if (fileClipboard.action === "cut") {
        fileClipboard = null;
      }
      refreshCurrentFolder();
    } else {
      showToast(json.message || "Erreur lors du collage", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}


// --------------------------------------------------------------------------
// SPÉCIFICATIONS MATÉRIELLES (HARDWARE INVENTORY)
// --------------------------------------------------------------------------
async function loadHardwareInfo() {
  try {
    const res = await fetch("/api/hardware");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const hw = json.data;

    // CPU
    const cpuModel = document.getElementById("hw-cpu-model");
    const cpuArch = document.getElementById("hw-cpu-arch");
    const cpuCores = document.getElementById("hw-cpu-cores");
    const cpuFreq = document.getElementById("hw-cpu-freq");
    const cpuCache = document.getElementById("hw-cpu-cache");
    const cpuVirt = document.getElementById("hw-cpu-virt");

    if (cpuModel) cpuModel.textContent = hw.cpu.model;
    if (cpuArch) cpuArch.textContent = hw.cpu.architecture;
    if (cpuCores) cpuCores.textContent = `${hw.cpu.total_cores} Cœurs / ${hw.cpu.total_threads} Threads (${hw.cpu.sockets} Sockets)`;
    if (cpuFreq) cpuFreq.textContent = hw.cpu.base_frequency_ghz;
    if (cpuCache) cpuCache.textContent = hw.cpu.cache;
    if (cpuVirt) cpuVirt.textContent = hw.cpu.virtualization;

    // Carte mère
    const mbName = document.getElementById("hw-mb-name");
    const mbVendor = document.getElementById("hw-mb-vendor");
    const mbChipset = document.getElementById("hw-mb-chipset");
    const mbSockets = document.getElementById("hw-mb-sockets");
    const mbBios = document.getElementById("hw-mb-bios");

    if (mbName) mbName.textContent = hw.motherboard.product_name;
    if (mbVendor) mbVendor.textContent = hw.motherboard.vendor;
    if (mbChipset) mbChipset.textContent = hw.motherboard.chipset;
    if (mbSockets) mbSockets.textContent = hw.motherboard.board_name;
    if (mbBios) mbBios.textContent = `${hw.motherboard.bios_version} (${hw.motherboard.bios_date})`;

    // RAM
    const ramTotal = document.getElementById("hw-ram-total");
    const ramType = document.getElementById("hw-ram-type");
    const ramChannels = document.getElementById("hw-ram-channels");
    const ramAvail = document.getElementById("hw-ram-avail");

    if (ramTotal) ramTotal.textContent = `${hw.memory.total_gb} Go (${hw.memory.mem_type})`;
    if (ramType) ramType.textContent = hw.memory.mem_type;
    if (ramChannels) ramChannels.textContent = hw.memory.channels;
    if (ramAvail) ramAvail.textContent = `${hw.memory.available_gb} Go disponibles (${hw.memory.free_gb} Go libres)`;

    // GPU
    const gpuModel = document.getElementById("hw-gpu-model");
    const gpuVram = document.getElementById("hw-gpu-vram");
    const gpuDriver = document.getElementById("hw-gpu-driver");

    if (gpuModel) gpuModel.textContent = hw.gpu.model;
    if (gpuVram) gpuVram.textContent = hw.gpu.vram;
    if (gpuDriver) gpuDriver.textContent = hw.gpu.driver;

    // Réseau
    const netList = document.getElementById("hw-network-list");
    if (netList && hw.network_adapters && hw.network_adapters.length > 0) {
      netList.innerHTML = hw.network_adapters.map(a => `
        <div class="hw-spec-row" style="margin-bottom:6px; padding-bottom:6px; border-bottom:1px solid rgba(255,255,255,0.04);">
          <div>
            <strong style="color:var(--text);">${a.interface_name}</strong>
            <span style="font-size:0.75rem; color:var(--subtext0); margin-left:6px;">(${a.controller_model})</span>
            <div style="font-size:0.75rem; color:var(--subtext0); font-family:var(--font-mono); margin-top:2px;">MAC: ${a.mac_address}</div>
          </div>
          <div style="text-align:right;">
            <span class="badge ${a.is_up ? 'badge-success' : 'badge-secondary'}">
              ${a.is_up ? (a.speed_mbps > 0 ? a.speed_mbps + ' Mbps' : 'Actif') : 'Déconnecté'}
            </span>
            ${a.ipv4 ? `<div style="font-size:0.75rem; color:var(--teal); font-family:var(--font-mono); font-weight:600; margin-top:2px;">${a.ipv4}</div>` : ''}
          </div>
        </div>
      `).join("");
    }
  } catch (err) {
    console.warn("Erreur loadHardwareInfo:", err);
  }
}

// --------------------------------------------------------------------------
// SANTÉ & ÉTAT S.M.A.R.T. DES DISQUES
// --------------------------------------------------------------------------
async function loadSmartInfo() {
  try {
    const res = await fetch("/api/smart");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const smart = json.data;

    // Badge global
    const badge = document.getElementById("smart-global-badge");
    if (badge) {
      if (smart.critical_disks > 0) {
        badge.className = "badge badge-danger";
        badge.textContent = `🚨 ${smart.critical_disks} disque(s) en échec SMART !`;
      } else if (smart.warning_disks > 0) {
        badge.className = "badge badge-warning";
        badge.textContent = `⚠ ${smart.warning_disks} alerte(s) SMART détectée(s)`;
      } else {
        badge.className = "badge badge-success";
        badge.textContent = `✔ Tous les disques sont sains (${smart.healthy_disks}/${smart.total_disks})`;
      }
    }

    // Remplissage de la table
    const tbody = document.getElementById("smart-table-tbody");
    if (tbody && smart.disks) {
      if (smart.disks.length === 0) {
        tbody.innerHTML = `<tr><td colspan="8" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun disque physique détecté.</td></tr>`;
      } else {
        tbody.innerHTML = smart.disks.map(d => {
          let statusBadge = `<span class="badge badge-success">✔ Sain (PASSED)</span>`;
          if (!d.passed) {
            statusBadge = `<span class="badge badge-danger">✖ Échec SMART</span>`;
          } else if (d.status_text && d.status_text.includes("Attention")) {
            statusBadge = `<span class="badge badge-warning">⚠ ${d.status_text}</span>`;
          }

          let tempDisplay = `--`;
          if (d.temperature_c !== null && d.temperature_c !== undefined) {
            const cls = d.temperature_c > 50 ? 'temp-badge-hot' : (d.temperature_c > 40 ? 'temp-badge-warm' : 'temp-badge-cool');
            tempDisplay = `<span class="${cls}">${d.temperature_c} °C</span>`;
          }

          let healthDetail = `--`;
          if (d.nvme_health_percentage !== null && d.nvme_health_percentage !== undefined) {
            healthDetail = `<strong style="color:var(--teal);">${d.nvme_health_percentage}% vie</strong> <span style="font-size:0.75rem; color:var(--subtext0);">(0 alerte)</span>`;
          } else {
            const realloc = d.reallocated_sectors !== null && d.reallocated_sectors !== undefined ? d.reallocated_sectors : 0;
            const pending = d.pending_sectors !== null && d.pending_sectors !== undefined ? d.pending_sectors : 0;
            const isWarn = realloc > 5 || pending > 0;
            healthDetail = `<span style="color:${isWarn ? 'var(--yellow)' : 'var(--text)'};">${realloc} réalloué(s) | ${pending} attente</span>`;
          }

          return `
            <tr>
              <td><code style="font-weight:bold; color:var(--mauve); font-size:0.88rem;">${d.device}</code></td>
              <td>
                <div style="font-weight:700; color:var(--text); font-size:0.88rem;">${d.model}</div>
                <div style="font-size:0.75rem; color:var(--subtext0); font-family:var(--font-mono);">S/N: ${d.serial}</div>
              </td>
              <td><span style="font-size:0.8rem; color:var(--subtext1);">${d.disk_type}</span></td>
              <td><strong style="color:var(--text);">${d.capacity}</strong></td>
              <td>${statusBadge}</td>
              <td>${tempDisplay}</td>
              <td style="font-size:0.82rem; font-family:var(--font-mono);">${d.power_on_hours ? d.power_on_hours.toLocaleString() + ' h' : '--'}</td>
              <td style="font-size:0.82rem;">${healthDetail}</td>
            </tr>
          `;
        }).join("");
      }
    }
  } catch (err) {
    console.warn("Erreur loadSmartInfo:", err);
  }
}

// --------------------------------------------------------------------------
// TEST DE DÉBIT RÉSEAU (SPEEDTEST INTERNET)
// --------------------------------------------------------------------------
async function loadLatestSpeedtest() {
  try {
    const res = await fetch("/api/speedtest/latest");
    const json = await res.json();
    if (!json.success || !json.data) return;

    renderSpeedtestResult(json.data);
  } catch (err) {
    console.warn("Erreur loadLatestSpeedtest:", err);
  }
}

function renderSpeedtestResult(r) {
  const downVal = document.getElementById("speedtest-down-val");
  const upVal = document.getElementById("speedtest-up-val");
  const pingVal = document.getElementById("speedtest-ping-val");
  const downBar = document.getElementById("speedtest-down-bar");
  const upBar = document.getElementById("speedtest-up-bar");
  const pingSub = document.getElementById("speedtest-ping-sub");
  const serverName = document.getElementById("speedtest-server-name");
  const metaInfo = document.getElementById("speedtest-meta-info");

  if (downVal) downVal.textContent = r.download_mbps > 0 ? r.download_mbps.toFixed(1) : "--";
  if (upVal) upVal.textContent = r.upload_mbps > 0 ? r.upload_mbps.toFixed(1) : "--";
  if (pingVal) pingVal.textContent = r.latency_ms > 0 ? r.latency_ms.toFixed(1) : "--";

  if (downBar) downBar.style.width = Math.min(100, (r.download_mbps / 1000) * 100) + "%";
  if (upBar) upBar.style.width = Math.min(100, (r.upload_mbps / 1000) * 100) + "%";

  if (pingSub) pingSub.textContent = `Min: ${r.min_latency_ms.toFixed(1)} ms | Max: ${r.max_latency_ms.toFixed(1)} ms`;
  if (serverName) serverName.textContent = r.server_name || "Cloudflare Edge";
  if (metaInfo) metaInfo.textContent = `Testé à ${r.timestamp} (${r.duration_seconds}s) • IP: ${r.client_ip}`;
}

async function triggerSpeedtest() {
  const btn = document.getElementById("btn-run-speedtest");
  const icon = document.getElementById("speedtest-btn-icon");
  const label = document.getElementById("speedtest-btn-label");
  const banner = document.getElementById("speedtest-status-banner");
  const bannerText = document.getElementById("speedtest-running-text");

  if (btn) btn.disabled = true;
  if (icon) icon.textContent = "⏳";
  if (label) label.textContent = "Mesure en cours...";
  if (banner) {
    banner.style.display = "flex";
    if (bannerText) bannerText.textContent = "Test en cours... Analyse de la latence et des débits fibre Cloudflare...";
  }

  showToast("Lancement du Speedtest réseau du NAS...", "info");

  try {
    const res = await fetch("/api/speedtest/run", { method: "POST" });
    const json = await res.json();
    if (json.success && json.data) {
      renderSpeedtestResult(json.data);
      showToast(`Speedtest terminé ! 📥 ${json.data.download_mbps} Mbps | 📤 ${json.data.upload_mbps} Mbps`, "success");
    } else {
      showToast("Échec de la mesure de débit", "error");
    }
  } catch (err) {
    showToast("Erreur lors du Speedtest : " + err, "error");
  } finally {
    if (btn) btn.disabled = false;
    if (icon) icon.textContent = "🚀";
    if (label) label.textContent = "Relancer le Speedtest";
    if (banner) banner.style.display = "none";
  }
}


// --------------------------------------------------------------------------
// GLISSER-DÉPOSER & GESTIONNAIRE DE TÉLÉVERSEMENT (UPLOAD)
// --------------------------------------------------------------------------
function initDragAndDrop() {
  const container = document.getElementById("files-view-container");
  const overlay = document.getElementById("files-drop-overlay");
  const targetLabel = document.getElementById("drop-target-path-text");
  if (!container || !overlay) return;

  let dragCounter = 0;

  container.addEventListener("dragenter", (e) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter++;
    if (e.dataTransfer && e.dataTransfer.types && Array.from(e.dataTransfer.types).includes("Files")) {
      overlay.style.display = "flex";
      if (targetLabel) targetLabel.textContent = `Destination : ${currentFolderPath}`;
    }
  });

  container.addEventListener("dragover", (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  });

  container.addEventListener("dragleave", (e) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter--;
    if (dragCounter <= 0) {
      dragCounter = 0;
      overlay.style.display = "none";
    }
  });

  container.addEventListener("drop", (e) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter = 0;
    overlay.style.display = "none";

    const files = e.dataTransfer ? e.dataTransfer.files : null;
    if (files && files.length > 0) {
      let targetPath = currentFolderPath;
      const targetCard = e.target.closest(".file-card, tr");
      if (targetCard) {
        const itemPath = targetCard.getAttribute("data-path");
        const item = currentEntries.find(i => i.path === itemPath);
        if (item && item.is_dir) {
          targetPath = item.path;
        }
      }
      uploadFiles(files, targetPath);
    }
  });
}

function handleFileSelect(e) {
  const files = e.target.files;
  if (files && files.length > 0) {
    uploadFiles(files, currentFolderPath);
  }
  e.target.value = "";
}

function uploadFiles(fileList, targetDir) {
  if (!fileList || fileList.length === 0) return;

  const tray = document.getElementById("upload-tray");
  const summary = document.getElementById("upload-tray-summary");
  const speedBadge = document.getElementById("upload-speed-badge");
  const progressBar = document.getElementById("upload-overall-bar");
  const filesListEl = document.getElementById("upload-files-list");
  const icon = document.getElementById("upload-tray-icon");

  if (!tray) return;

  tray.style.display = "block";
  tray.classList.remove("minimized");
  if (icon) icon.textContent = "⏳";
  if (progressBar) progressBar.style.width = "0%";
  if (speedBadge) speedBadge.textContent = "Calcul...";

  const totalFiles = fileList.length;
  if (summary) summary.textContent = `Téléversement (${totalFiles} fichier${totalFiles > 1 ? "s" : ""})`;

  if (filesListEl) {
    filesListEl.innerHTML = "";
    Array.from(fileList).forEach(file => {
      const row = document.createElement("div");
      row.className = "upload-file-row";
      row.innerHTML = `
        <span class="upload-file-name" title="${escapeHtml(file.name)}">📄 ${escapeHtml(file.name)}</span>
        <div class="upload-file-meta">
          <span>${formatFileSize(file.size)}</span>
          <span class="status-indicator">⏳ 0%</span>
        </div>
      `;
      filesListEl.appendChild(row);
    });
  }

  const formData = new FormData();
  Array.from(fileList).forEach(file => {
    formData.append("files", file);
  });

  const startTime = Date.now();
  let lastLoaded = 0;
  let lastTime = startTime;

  const xhr = new XMLHttpRequest();
  xhr.open("POST", `/api/files/upload?dir=${encodeURIComponent(targetDir)}`);

  xhr.upload.onprogress = (e) => {
    if (e.lengthComputable) {
      const percent = Math.round((e.loaded / e.total) * 100);
      if (progressBar) progressBar.style.width = `${percent}%`;

      const now = Date.now();
      const timeDiff = (now - lastTime) / 1000;
      if (timeDiff >= 0.4) {
        const bytesDiff = e.loaded - lastLoaded;
        const speed = bytesDiff / timeDiff;
        if (speedBadge) speedBadge.textContent = `${formatSpeed(speed)}`;
        lastLoaded = e.loaded;
        lastTime = now;
      }

      if (summary) {
        summary.textContent = `Téléversement (${percent}%) - ${formatFileSize(e.loaded)} / ${formatFileSize(e.total)}`;
      }

      if (filesListEl) {
        const rows = filesListEl.querySelectorAll(".upload-file-row .status-indicator");
        rows.forEach(ind => {
          if (percent === 100) {
            ind.textContent = "⏳ Écriture disque...";
          } else {
            ind.textContent = `⏵ ${percent}%`;
          }
        });
      }
    }
  };

  xhr.onload = () => {
    if (xhr.status >= 200 && xhr.status < 300) {
      if (progressBar) progressBar.style.width = "100%";
      if (icon) icon.textContent = "✔";
      if (summary) summary.textContent = `Transfert terminé (${totalFiles} fichier${totalFiles > 1 ? "s" : ""})`;
      if (speedBadge) speedBadge.textContent = "Terminé";

      if (filesListEl) {
        const rows = filesListEl.querySelectorAll(".upload-file-row .status-indicator");
        rows.forEach(ind => {
          ind.textContent = "✔ Prêt";
          ind.style.color = "var(--green)";
        });
      }

      showToast(`Téléversement de ${totalFiles} fichier(s) réussi !`, "success");
      navigateToPath(currentFolderPath);
    } else {
      if (icon) icon.textContent = "❌";
      if (summary) summary.textContent = "Échec du téléversement";
      if (speedBadge) speedBadge.textContent = "Erreur";
      showToast("Erreur lors du transfert : " + (xhr.responseText || xhr.statusText), "error");
    }
  };

  xhr.onerror = () => {
    if (icon) icon.textContent = "❌";
    if (summary) summary.textContent = "Erreur réseau";
    if (speedBadge) speedBadge.textContent = "Erreur";
    showToast("Erreur réseau pendant le téléversement", "error");
  };

  xhr.send(formData);
}

function formatFileSize(bytes) {
  if (!bytes || bytes === 0) return "0 o";
  const k = 1024;
  const sizes = ["o", "Ko", "Mo", "Go", "To"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

function formatSpeed(bytesPerSec) {
  return `${formatFileSize(bytesPerSec)}/s`;
}

function toggleUploadTrayMinimize() {
  const tray = document.getElementById("upload-tray");
  const btn = document.getElementById("upload-tray-min-btn");
  if (!tray) return;
  tray.classList.toggle("minimized");
  if (btn) btn.textContent = tray.classList.contains("minimized") ? "□" : "_";
}

function closeUploadTray() {
  const tray = document.getElementById("upload-tray");
  if (tray) tray.style.display = "none";
}

// --------------------------------------------------------------------------
// MODALE ÉDITEUR CONTEXTUEL STYLE NEOVIM & COLORATION SYNTAXIQUE
// --------------------------------------------------------------------------
let nvimCurrentPath = null;
let nvimOriginalContent = "";
let nvimIsDirty = false;
let nvimCurrentLang = "text";
let nvimSyntaxEnabled = true;

const NVIM_LANG_META = {
  fish: { name: "Fish Shell", icon: "🐟", tag: "fish", ext: [".fish"] },
  nix: { name: "NixOS", icon: "❄️", tag: "nix", ext: [".nix"] },
  bash: { name: "Shell / Bash", icon: "🐚", tag: "sh", ext: [".sh", ".bash", ".zsh", ".ksh", ".csh"] },
  python: { name: "Python", icon: "🐍", tag: "python", ext: [".py", ".pyw"] },
  rust: { name: "Rust", icon: "🦀", tag: "rust", ext: [".rs"] },
  javascript: { name: "JavaScript", icon: "⚡", tag: "javascript", ext: [".js", ".mjs", ".cjs"] },
  typescript: { name: "TypeScript", icon: "🔷", tag: "typescript", ext: [".ts", ".tsx", ".jsx"] },
  json: { name: "JSON", icon: "📋", tag: "json", ext: [".json", ".json5", ".jsonc"] },
  toml: { name: "TOML", icon: "⚙️", tag: "toml", ext: [".toml"] },
  yaml: { name: "YAML", icon: "📑", tag: "yaml", ext: [".yaml", ".yml"] },
  ini: { name: "INI / Config", icon: "🔧", tag: "ini", ext: [".ini", ".conf", ".cfg", ".service", ".timer", ".target", ".socket", ".desktop", ".env"] },
  markdown: { name: "Markdown", icon: "📜", tag: "markdown", ext: [".md", ".markdown"] },
  html: { name: "HTML / XML", icon: "🌐", tag: "html", ext: [".html", ".htm", ".xml", ".svg"] },
  css: { name: "CSS", icon: "🎨", tag: "css", ext: [".css", ".scss", ".sass", ".less"] },
  c: { name: "C / C++", icon: "🔣", tag: "c", ext: [".c", ".cpp", ".cc", ".cxx", ".h", ".hpp"] },
  text: { name: "Texte Brut", icon: "📄", tag: "text", ext: [".txt", ".log"] },
};

const NVIM_LANG_RULES = {
  fish: [
    { type: "comment", regex: /#.*/ },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "variable", regex: /\$[\w_]+|\$\{[^}]+\}/ },
    { type: "keyword", regex: /\b(?:function|end|if|else|switch|case|for|in|while|return|exit|break|continue|begin|and|or|not)\b/ },
    { type: "builtin", regex: /\b(?:set|echo|read|test|contains|count|string|path|status|math|command|builtin|source|alias|bind|complete|functions|history|jobs|random|abbr|argparse)\b/ },
    { type: "number", regex: /\b\d+\b/ },
    { type: "option", regex: /(?<=\s)-{1,2}[a-zA-Z0-9_\-]+/ },
    { type: "operator", regex: /[=><!|;&]/ }
  ],
  nix: [
    { type: "comment", regex: /#.*|\/\*[\s\S]*?\*\// },
    { type: "string", regex: /''[\s\S]*?''|"(?:\\.|[^"\\])*"/ },
    { type: "variable", regex: /\$\{[^}]+\}/ },
    { type: "keyword", regex: /\b(?:let|in|inherit|import|with|rec|if|then|else|assert)\b/ },
    { type: "builtin", regex: /\b(?:builtins|true|false|null)\b/ },
    { type: "property", regex: /\b[a-zA-Z_][a-zA-Z0-9_\-\.]*(?=\s*=)/ },
    { type: "number", regex: /\b\d+\b/ }
  ],
  bash: [
    { type: "comment", regex: /#.*/ },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "variable", regex: /\$[\w_]+|\$\{[^}]+\}/ },
    { type: "keyword", regex: /\b(?:if|then|elif|else|fi|for|while|until|do|done|case|esac|function|return|exit|break|continue|local|export|readonly|declare|select|time)\b/ },
    { type: "builtin", regex: /\b(?:echo|read|cd|pwd|set|unset|shift|source|alias|trap|test|eval|exec|true|false)\b/ },
    { type: "number", regex: /\b\d+\b/ },
    { type: "option", regex: /(?<=\s)-{1,2}[a-zA-Z0-9_\-]+/ }
  ],
  python: [
    { type: "comment", regex: /#.*/ },
    { type: "string", regex: /"""[\s\S]*?"""|'''[\s\S]*?'''|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "decorator", regex: /@[\w_]+/ },
    { type: "keyword", regex: /\b(?:def|class|import|from|return|if|elif|else|for|while|try|except|finally|with|as|yield|async|await|lambda|pass|break|continue|raise|global|nonlocal|assert|del)\b/ },
    { type: "builtin", regex: /\b(?:True|False|None|self|cls|print|len|range|enumerate|zip|map|filter|int|str|float|bool|list|dict|set|tuple|open|super)\b/ },
    { type: "number", regex: /\b\d+(?:\.\d+)?\b/ }
  ],
  rust: [
    { type: "comment", regex: /\/\/.*|\/\*[\s\S]*?\*\// },
    { type: "string", regex: /r#"[^"]*"#|"(?:\\.|[^"\\])*"|'\\?.'(?=[^\w])/ },
    { type: "keyword", regex: /\b(?:fn|let|mut|pub|struct|enum|trait|impl|type|const|static|use|mod|crate|super|self|Self|match|if|else|for|while|loop|return|break|continue|async|await|unsafe|where|move|ref|as)\b/ },
    { type: "type", regex: /\b(?:i8|i16|i32|i64|i128|isize|u8|u16|u32|u64|u128|usize|f32|f64|bool|char|str|String|Option|Result|Some|None|Ok|Err|Vec|Box|Rc|Arc|HashMap|HashSet)\b/ },
    { type: "macro", regex: /\b[\w_]+!(?=[(\[{])/ },
    { type: "number", regex: /\b\d+(?:_\d+)*(?:\.\d+)?(?:[eE][+-]?\d+)?(?:u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f32|f64)?\b/ }
  ],
  javascript: [
    { type: "comment", regex: /\/\/.*|\/\*[\s\S]*?\*\// },
    { type: "string", regex: /`[\s\S]*?`|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "keyword", regex: /\b(?:const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|default|new|this|typeof|instanceof|void|delete|throw|try|catch|finally|class|extends|super|import|export|from|as|default|async|await|yield)\b/ },
    { type: "builtin", regex: /\b(?:true|false|null|undefined|NaN|Infinity|console|window|document|Math|JSON|Promise|Array|Object|String|Number|Boolean|Date|RegExp)\b/ },
    { type: "number", regex: /\b\d+(?:\.\d+)?\b/ }
  ],
  typescript: [
    { type: "comment", regex: /\/\/.*|\/\*[\s\S]*?\*\// },
    { type: "string", regex: /`[\s\S]*?`|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "keyword", regex: /\b(?:const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|default|new|this|typeof|instanceof|void|delete|throw|try|catch|finally|class|extends|super|import|export|from|as|default|async|await|yield|type|interface|enum|implements|declare|readonly|abstract|keyof)\b/ },
    { type: "type", regex: /\b(?:string|number|boolean|any|unknown|never|void)\b/ },
    { type: "builtin", regex: /\b(?:true|false|null|undefined|console|Promise|Array|Object)\b/ },
    { type: "number", regex: /\b\d+(?:\.\d+)?\b/ }
  ],
  json: [
    { type: "property", regex: /"(?:\\.|[^"\\])*"(?=\s*:)/ },
    { type: "string", regex: /"(?:\\.|[^"\\])*"/ },
    { type: "number", regex: /-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/ },
    { type: "builtin", regex: /\b(?:true|false|null)\b/ }
  ],
  toml: [
    { type: "comment", regex: /#.*/ },
    { type: "section", regex: /^\s*\[\[?[^\]]+\]\]?/m },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "property", regex: /^[ \t]*[\w\.\-]+(?=\s*=)/m },
    { type: "builtin", regex: /\b(?:true|false)\b/ },
    { type: "number", regex: /\b\d+(?:\.\d+)?\b/ }
  ],
  yaml: [
    { type: "comment", regex: /#.*/ },
    { type: "property", regex: /^[ \t]*[\w\.\-]+(?=\s*:)/m },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "builtin", regex: /\b(?:true|false|null|yes|no)\b/ },
    { type: "number", regex: /\b\d+(?:\.\d+)?\b/ }
  ],
  ini: [
    { type: "comment", regex: /[#;].*/ },
    { type: "section", regex: /^\s*\[[^\]]+\]/m },
    { type: "property", regex: /^[ \t]*[\w\.\-]+(?=\s*=)/m },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ }
  ],
  markdown: [
    { type: "header", regex: /^#{1,6}\s+.*$/m },
    { type: "code", regex: /`[^`\n]+`/ },
    { type: "bold", regex: /\*\*[^*]+\*\*|__[^_]+__/ },
    { type: "italic", regex: /\*[^*]+\*|_[^_]+_/ },
    { type: "link", regex: /\[[^\]]+\]\([^)]+\)/ },
    { type: "list", regex: /^\s*(?:[-*+]|\d+\.)\s+/m }
  ],
  html: [
    { type: "comment", regex: /<!--[\s\S]*?-->/ },
    { type: "tag", regex: /<\/?[\w\-]+/ },
    { type: "attribute", regex: /\b[\w\-]+(?=\s*=)/ },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ }
  ],
  css: [
    { type: "comment", regex: /\/\*[\s\S]*?\*\// },
    { type: "property", regex: /[\w\-]+(?=\s*:)/ },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/ },
    { type: "number", regex: /\b\d+(?:\.\d+)?(?:px|rem|em|%|vh|vw|s|ms|deg)?\b/ }
  ],
  c: [
    { type: "comment", regex: /\/\/.*|\/\*[\s\S]*?\*\// },
    { type: "string", regex: /"(?:\\.|[^"\\])*"|'\\?.'(?=[^\w])/ },
    { type: "preproc", regex: /^\s*#\s*[a-zA-Z_]+/m },
    { type: "keyword", regex: /\b(?:auto|break|case|char|const|continue|default|do|double|else|enum|extern|float|for|goto|if|int|long|register|return|short|signed|sizeof|static|struct|switch|typedef|union|unsigned|void|volatile|while|class|namespace|template|typename|public|private|protected|virtual|override|new|delete|inline|constexpr)\b/ },
    { type: "number", regex: /\b\d+(?:\.\d+)?\b/ }
  ]
};

let nvimCombinedRegexCache = {};

function getNvimCombinedRegex(lang) {
  if (nvimCombinedRegexCache[lang]) return nvimCombinedRegexCache[lang];
  const rules = NVIM_LANG_RULES[lang];
  if (!rules) return null;
  const source = rules.map(r => `(${r.regex.source})`).join("|");
  const regex = new RegExp(source, "gm");
  nvimCombinedRegexCache[lang] = regex;
  return regex;
}

function escapeHtml(str) {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

function highlightNvimCode(code, lang) {
  if (!lang || lang === "text" || !NVIM_LANG_RULES[lang]) {
    return escapeHtml(code);
  }

  const rules = NVIM_LANG_RULES[lang];
  const combined = getNvimCombinedRegex(lang);
  combined.lastIndex = 0;

  let lastIndex = 0;
  let html = "";
  let match;

  while ((match = combined.exec(code)) !== null) {
    if (match.index > lastIndex) {
      html += escapeHtml(code.slice(lastIndex, match.index));
    }

    let tokenType = "text";
    for (let i = 0; i < rules.length; i++) {
      if (match[i + 1] !== undefined) {
        tokenType = rules[i].type;
        break;
      }
    }

    html += `<span class="tok-${tokenType}">${escapeHtml(match[0])}</span>`;
    lastIndex = combined.lastIndex;
  }

  if (lastIndex < code.length) {
    html += escapeHtml(code.slice(lastIndex));
  }

  return html;
}

function detectNvimLanguage(fileName, content = "") {
  if (!fileName) return "text";
  const lower = fileName.toLowerCase();

  if (lower === "flake.lock") return "json";
  if (lower === "cargo.lock") return "toml";
  if (lower === "makefile" || lower === "justfile" || lower === "dockerfile" || lower === "containerfile") return "bash";
  if (lower === ".gitignore" || lower === ".env") return "ini";
  if (lower.endsWith("rc") && (lower.includes("bash") || lower.includes("zsh"))) return "bash";
  if (lower === ".fishrc" || lower.endsWith(".fish")) return "fish";

  for (const [lang, meta] of Object.entries(NVIM_LANG_META)) {
    if (meta.ext.some(ext => lower.endsWith(ext))) {
      return lang;
    }
  }

  if (content) {
    const firstLine = content.split("\n")[0] || "";
    if (firstLine.startsWith("#!")) {
      if (firstLine.includes("fish")) return "fish";
      if (firstLine.includes("bash") || firstLine.includes("sh") || firstLine.includes("zsh")) return "bash";
      if (firstLine.includes("python")) return "python";
      if (firstLine.includes("node")) return "javascript";
    }
  }

  return "text";
}

function updateNvimLanguageUI(lang) {
  const meta = NVIM_LANG_META[lang] || { name: "Texte Brut", icon: "📄", tag: "text" };
  const badge = document.getElementById("nvim-lang-badge");
  const select = document.getElementById("nvim-lang-select");
  const statusLang = document.getElementById("nvim-status-lang");

  if (badge) badge.textContent = `${meta.icon} ${meta.name}`;
  if (select) select.value = lang;
  if (statusLang) statusLang.textContent = `ft=${meta.tag}`;
}

function updateNvimHighlighting() {
  const textarea = document.getElementById("nvim-textarea");
  const codeEl = document.getElementById("nvim-highlight-code");
  if (!textarea || !codeEl) return;

  if (!nvimSyntaxEnabled) {
    codeEl.innerHTML = "";
    return;
  }

  const text = textarea.value;
  let html = highlightNvimCode(text, nvimCurrentLang);
  if (text.endsWith("\n")) {
    html += " ";
  }
  codeEl.innerHTML = html;
}

function toggleNvimSyntax() {
  nvimSyntaxEnabled = !nvimSyntaxEnabled;
  const btn = document.getElementById("nvim-btn-syntax");
  const textarea = document.getElementById("nvim-textarea");
  const highlightLayer = document.getElementById("nvim-highlight-layer");

  if (btn) {
    btn.classList.toggle("active", nvimSyntaxEnabled);
    btn.textContent = nvimSyntaxEnabled ? "🎨 Coloration : ON" : "🎨 Coloration : OFF";
  }

  if (textarea && highlightLayer) {
    if (nvimSyntaxEnabled) {
      textarea.classList.remove("syntax-disabled");
      highlightLayer.style.display = "block";
      updateNvimHighlighting();
      syncNvimEditorScroll();
    } else {
      textarea.classList.add("syntax-disabled");
      highlightLayer.style.display = "none";
    }
  }

  showToast(nvimSyntaxEnabled ? "Coloration syntaxique activée" : "Coloration syntaxique désactivée", "info");
}

function onNvimLangChange(lang) {
  nvimCurrentLang = lang;
  updateNvimLanguageUI(lang);
  updateNvimHighlighting();
  syncNvimEditorScroll();
}

function openNvimViewerWithContent(titleText, contentText, lang = "yaml", pathHint = "docker-compose.yml") {
  const modal = document.getElementById("nvim-modal");
  const title = document.getElementById("nvim-file-title");
  const pathLabel = document.getElementById("nvim-status-path");
  const sizeLabel = document.getElementById("nvim-file-size");
  const textarea = document.getElementById("nvim-textarea");
  const badge = document.getElementById("nvim-dirty-badge");
  const modeLabel = document.getElementById("nvim-status-mode");

  if (!modal || !textarea) return;

  nvimCurrentPath = null;
  nvimIsDirty = false;
  textarea.readOnly = true;

  if (title) title.textContent = titleText;
  if (pathLabel) pathLabel.textContent = `${pathHint} [Lecture Seule]`;
  if (badge) {
    badge.textContent = "Lecture Seule";
    badge.className = "badge badge-info";
    badge.style.display = "inline-block";
  }
  if (modeLabel) {
    modeLabel.textContent = "VIEW";
    modeLabel.className = "nvim-status-mode";
  }

  nvimCurrentLang = lang;
  updateNvimLanguageUI(lang);

  nvimOriginalContent = contentText;
  textarea.value = contentText;
  if (sizeLabel) sizeLabel.textContent = `${contentText.length} octets`;

  updateNvimLineNumbers();
  updateNvimCursorPos();
  updateNvimHighlighting();
  syncNvimEditorScroll();

  modal.style.display = "flex";
  textarea.focus();
}

async function openNvimModal(path, fileName) {
  nvimCurrentPath = path;
  nvimIsDirty = false;

  const modal = document.getElementById("nvim-modal");
  const title = document.getElementById("nvim-file-title");
  const pathLabel = document.getElementById("nvim-status-path");
  const sizeLabel = document.getElementById("nvim-file-size");
  const textarea = document.getElementById("nvim-textarea");
  const badge = document.getElementById("nvim-dirty-badge");
  const modeLabel = document.getElementById("nvim-status-mode");

  if (!modal || !textarea) return;

  const baseName = fileName || path.split("/").pop();
  if (title) title.textContent = baseName;
  if (pathLabel) pathLabel.textContent = path;
  if (badge) badge.style.display = "none";
  if (modeLabel) {
    modeLabel.textContent = "NORMAL";
    modeLabel.className = "nvim-status-mode";
  }

  nvimCurrentLang = detectNvimLanguage(baseName);
  updateNvimLanguageUI(nvimCurrentLang);

  textarea.value = "Chargement en cours...";
  updateNvimHighlighting();
  modal.style.display = "flex";

  try {
    const res = await fetch(`/api/files/read?path=${encodeURIComponent(path)}`);
    const json = await res.json();
    if (!json.success || !json.data) {
      showToast(json.message || "Impossible de lire le fichier", "error");
      closeNvimModal();
      return;
    }

    const data = json.data;
    nvimOriginalContent = data.content;
    textarea.value = data.content;
    if (sizeLabel) sizeLabel.textContent = formatFileSize(data.size_bytes) + (data.is_truncated ? " (tronqué)" : "");

    // Détection affinée avec le contenu du fichier (Shebang, etc.)
    nvimCurrentLang = detectNvimLanguage(baseName, data.content);
    updateNvimLanguageUI(nvimCurrentLang);

    updateNvimLineNumbers();
    updateNvimCursorPos();
    updateNvimHighlighting();
    syncNvimEditorScroll();
    textarea.focus();
  } catch (err) {
    showToast("Erreur lors de l'ouverture du fichier : " + err, "error");
    closeNvimModal();
  }
}

function onNvimContentChange() {
  const textarea = document.getElementById("nvim-textarea");
  const badge = document.getElementById("nvim-dirty-badge");
  const modeLabel = document.getElementById("nvim-status-mode");

  if (!textarea) return;

  nvimIsDirty = (textarea.value !== nvimOriginalContent);
  if (badge) badge.style.display = nvimIsDirty ? "inline-block" : "none";
  if (modeLabel) {
    modeLabel.textContent = "INSERT";
    modeLabel.className = "nvim-status-mode insert";
  }

  updateNvimLineNumbers();
  updateNvimCursorPos();
  updateNvimHighlighting();
}

function updateNvimLineNumbers() {
  const textarea = document.getElementById("nvim-textarea");
  const lineNumbers = document.getElementById("nvim-line-numbers");
  if (!textarea || !lineNumbers) return;

  const count = textarea.value.split("\n").length;
  let linesHtml = "";
  for (let i = 1; i <= count; i++) {
    linesHtml += `${i}<br>`;
  }
  lineNumbers.innerHTML = linesHtml;
}

function syncNvimEditorScroll() {
  const textarea = document.getElementById("nvim-textarea");
  const highlight = document.getElementById("nvim-highlight-layer");
  const lineNumbers = document.getElementById("nvim-line-numbers");
  if (!textarea) return;

  if (highlight) {
    highlight.scrollTop = textarea.scrollTop;
    highlight.scrollLeft = textarea.scrollLeft;
  }
  if (lineNumbers) {
    lineNumbers.scrollTop = textarea.scrollTop;
  }
}

function updateNvimCursorPos() {
  const textarea = document.getElementById("nvim-textarea");
  const posLabel = document.getElementById("nvim-status-pos");
  if (!textarea || !posLabel) return;

  const selStart = textarea.selectionStart || 0;
  const lines = textarea.value.substring(0, selStart).split("\n");
  const line = lines.length;
  const col = lines[lines.length - 1].length + 1;
  posLabel.textContent = `L: ${line} | C: ${col}`;
}

function handleNvimKeydown(e) {
  const textarea = document.getElementById("nvim-textarea");
  const modeLabel = document.getElementById("nvim-status-mode");

  if (e.key === "Tab") {
    e.preventDefault();
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    textarea.value = textarea.value.substring(0, start) + "  " + textarea.value.substring(end);
    textarea.selectionStart = textarea.selectionEnd = start + 2;
    onNvimContentChange();
    return;
  }

  if ((e.ctrlKey || e.metaKey) && e.key === "s") {
    e.preventDefault();
    saveNvimFile();
    return;
  }

  if (e.key === "Escape") {
    if (modeLabel) {
      modeLabel.textContent = "NORMAL";
      modeLabel.className = "nvim-status-mode";
    }
    return;
  }

  if (modeLabel && modeLabel.textContent === "NORMAL") {
    if (e.key === "i" || e.key === "a" || e.key === "o") {
      modeLabel.textContent = "INSERT";
      modeLabel.className = "nvim-status-mode insert";
    }
  }

  setTimeout(updateNvimCursorPos, 10);
}

async function saveNvimFile() {
  const textarea = document.getElementById("nvim-textarea");
  if (!textarea || textarea.readOnly || !nvimCurrentPath) {
    showToast("Ce document est en lecture seule (:w refusé)", "warning");
    return;
  }
  const badge = document.getElementById("nvim-dirty-badge");
  const modeLabel = document.getElementById("nvim-status-mode");
  if (!textarea) return;

  try {
    const res = await fetch("/api/files/write", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        path: nvimCurrentPath,
        content: textarea.value
      })
    });
    const json = await res.json();
    if (json.success) {
      nvimOriginalContent = textarea.value;
      nvimIsDirty = false;
      if (badge) badge.style.display = "none";
      if (modeLabel) {
        modeLabel.textContent = "NORMAL";
        modeLabel.className = "nvim-status-mode";
      }
      showToast("Fichier enregistré (:w) avec succès !", "success");
    } else {
      showToast("Échec de l'enregistrement : " + (json.message || "Erreur"), "error");
    }
  } catch (err) {
    showToast("Erreur lors de la sauvegarde : " + err, "error");
  }
}

function closeNvimModal() {
  if (nvimIsDirty) {
    if (!confirm("Des modifications ne sont pas enregistrées. Voulez-vous quitter sans sauvegarder (:q!) ?")) {
      return;
    }
  }
  const modal = document.getElementById("nvim-modal");
  const textarea = document.getElementById("nvim-textarea");
  if (modal) modal.style.display = "none";
  if (textarea) textarea.readOnly = false;
  nvimCurrentPath = null;
  nvimIsDirty = false;
}

// --------------------------------------------------------------------------
// MODALE LECTEUR VIDÉO STYLE MPV
// --------------------------------------------------------------------------
let mpvKeyHandler = null;

function openMpvModal(path, fileName) {
  const modal = document.getElementById("mpv-modal");
  const title = document.getElementById("mpv-video-title");
  const video = document.getElementById("mpv-video-element");
  const nativeBtn = document.getElementById("mpv-open-native-btn");

  if (!modal || !video) return;

  const streamUrl = `/api/files/stream?path=${encodeURIComponent(path)}`;
  if (title) title.textContent = fileName || path.split("/").pop();
  if (nativeBtn) nativeBtn.href = streamUrl;

  video.src = streamUrl;
  video.currentTime = 0;
  modal.style.display = "flex";
  video.play().catch(() => {});

  if (mpvKeyHandler) window.removeEventListener("keydown", mpvKeyHandler);
  mpvKeyHandler = (e) => handleMpvKeydown(e, video);
  window.addEventListener("keydown", mpvKeyHandler);
}

function handleMpvKeydown(e, video) {
  if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;

  const modal = document.getElementById("mpv-modal");
  if (!modal || modal.style.display === "none") return;

  switch (e.code) {
    case "Space":
    case "KeyP":
      e.preventDefault();
      if (video.paused) {
        video.play();
        showMpvOsd("▶ Lecture");
      } else {
        video.pause();
        showMpvOsd("⏸ Pause");
      }
      break;

    case "ArrowLeft":
      e.preventDefault();
      const stepLeft = e.shiftKey ? 1 : 5;
      video.currentTime = Math.max(0, video.currentTime - stepLeft);
      showMpvOsd(`⏪ -${stepLeft}s`);
      break;

    case "ArrowRight":
      e.preventDefault();
      const stepRight = e.shiftKey ? 1 : 5;
      video.currentTime = Math.min(video.duration || 999999, video.currentTime + stepRight);
      showMpvOsd(`⏩ +${stepRight}s`);
      break;

    case "ArrowUp":
      e.preventDefault();
      video.volume = Math.min(1, parseFloat((video.volume + 0.05).toFixed(2)));
      showMpvOsd(`🔊 Volume : ${Math.round(video.volume * 100)}%`);
      break;

    case "ArrowDown":
      e.preventDefault();
      video.volume = Math.max(0, parseFloat((video.volume - 0.05).toFixed(2)));
      showMpvOsd(`🔉 Volume : ${Math.round(video.volume * 100)}%`);
      break;

    case "KeyM":
      e.preventDefault();
      video.muted = !video.muted;
      showMpvOsd(video.muted ? "🔇 Muet" : "🔊 Son réactivé");
      break;

    case "KeyF":
      e.preventDefault();
      if (!document.fullscreenElement) {
        video.requestFullscreen().catch(() => {});
      } else {
        document.exitFullscreen().catch(() => {});
      }
      break;

    case "BracketLeft":
      e.preventDefault();
      video.playbackRate = Math.max(0.25, parseFloat((video.playbackRate - 0.25).toFixed(2)));
      showMpvOsd(`⏱ Vitesse : ${video.playbackRate}x`);
      break;

    case "BracketRight":
      e.preventDefault();
      video.playbackRate = Math.min(3, parseFloat((video.playbackRate + 0.25).toFixed(2)));
      showMpvOsd(`⏱ Vitesse : ${video.playbackRate}x`);
      break;

    case "KeyQ":
    case "Escape":
      e.preventDefault();
      closeMpvModal();
      break;
  }
}

let osdTimeout = null;
function showMpvOsd(text) {
  const osd = document.getElementById("mpv-osd-hint");
  if (!osd) return;

  osd.textContent = text;
  osd.style.display = "block";
  osd.style.animation = "none";
  osd.offsetHeight;
  osd.style.animation = "osdFade 1.2s ease-out forwards";

  if (osdTimeout) clearTimeout(osdTimeout);
  osdTimeout = setTimeout(() => {
    osd.style.display = "none";
  }, 1200);
}

function closeMpvModal() {
  const modal = document.getElementById("mpv-modal");
  const video = document.getElementById("mpv-video-element");

  if (video) {
    video.pause();
    video.removeAttribute("src");
    video.load();
  }

  if (mpvKeyHandler) {
    window.removeEventListener("keydown", mpvKeyHandler);
    mpvKeyHandler = null;
  }

  if (modal) modal.style.display = "none";
}

function handleModalOverlayClick(e, modalId) {
  if (e.target.id === modalId) {
    if (modalId === "image-modal") closeImageModal();
    if (modalId === "nvim-modal") closeNvimModal();
    if (modalId === "mpv-modal") closeMpvModal();
    if (modalId === "audio-modal") closeAudioModal();
    if (modalId === "create-raid-modal") closeCreateRaidModal();
    if (modalId === "format-disk-modal") closeFormatDiskModal();
    if (modalId === "doc-modal") closeDocModal();
  }
}


// --------------------------------------------------------------------------
// MODALE LECTEUR AUDIO HIFI & ÉQUALISEUR SPECTRE EN DIRECT
// --------------------------------------------------------------------------
let audioCtx = null;
let audioSourceNode = null;
let audioAnalyserNode = null;
let audioAnimId = null;
let audioPeakHeights = [];
let audioKeyHandler = null;
let currentAudioPath = null;
const audioSpeeds = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
let audioSpeedIndex = 2;

function openAudioModal(path, fileName, sizeBytes) {
  currentAudioPath = path;
  const modal = document.getElementById("audio-modal");
  const title = document.getElementById("audio-file-title");
  const formatBadge = document.getElementById("audio-format-badge");
  const sizeLabel = document.getElementById("audio-file-size");
  const audioEl = document.getElementById("audio-element");
  const playIcon = document.getElementById("audio-play-icon");
  const loopBtn = document.getElementById("audio-btn-loop");
  const speedBtn = document.getElementById("audio-btn-speed");
  const progressBar = document.getElementById("audio-scrub-progress");
  const thumb = document.getElementById("audio-scrub-thumb");
  const curTime = document.getElementById("audio-current-time");
  const totTime = document.getElementById("audio-total-time");

  if (!modal || !audioEl) return;

  const ext = (fileName || path).split(".").pop().toUpperCase();
  if (title) title.textContent = fileName || path.split("/").pop();
  if (formatBadge) formatBadge.textContent = ext || "AUDIO";
  if (sizeLabel && sizeBytes) sizeLabel.textContent = formatFileSize(sizeBytes);
  if (playIcon) playIcon.textContent = "▶";
  if (progressBar) progressBar.style.width = "0%";
  if (thumb) thumb.style.left = "0%";
  if (curTime) curTime.textContent = "00:00";
  if (totTime) totTime.textContent = "00:00";
  if (loopBtn) loopBtn.classList.remove("active");
  if (speedBtn) speedBtn.textContent = "1.0x";
  audioSpeedIndex = 2;
  audioEl.loop = false;
  audioEl.playbackRate = 1.0;

  const streamUrl = `/api/files/stream?path=${encodeURIComponent(path)}`;
  audioEl.src = streamUrl;

  if (!audioCtx) {
    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    if (AudioContextClass) {
      audioCtx = new AudioContextClass();
      audioSourceNode = audioCtx.createMediaElementSource(audioEl);
      audioAnalyserNode = audioCtx.createAnalyser();
      audioAnalyserNode.fftSize = 128;
      audioAnalyserNode.smoothingTimeConstant = 0.82;
      audioSourceNode.connect(audioAnalyserNode);
      audioAnalyserNode.connect(audioCtx.destination);
    }
  }

  modal.style.display = "flex";

  if (audioCtx && audioCtx.state === "suspended") {
    audioCtx.resume();
  }

  audioEl.play().then(() => {
    if (playIcon) playIcon.textContent = "⏸";
  }).catch(() => {
    if (playIcon) playIcon.textContent = "▶";
  });

  startAudioVisualizer();

  audioEl.ontimeupdate = () => {
    if (!audioEl.duration) return;
    const percent = (audioEl.currentTime / audioEl.duration) * 100;
    if (progressBar) progressBar.style.width = `${percent}%`;
    if (thumb) thumb.style.left = `${percent}%`;
    if (curTime) curTime.textContent = formatAudioTime(audioEl.currentTime);
  };

  audioEl.onloadedmetadata = () => {
    if (totTime && audioEl.duration) totTime.textContent = formatAudioTime(audioEl.duration);
  };

  audioEl.onended = () => {
    if (!audioEl.loop && playIcon) {
      playIcon.textContent = "▶";
    }
  };

  if (audioKeyHandler) window.removeEventListener("keydown", audioKeyHandler);
  audioKeyHandler = (e) => handleAudioKeydown(e, audioEl);
  window.addEventListener("keydown", audioKeyHandler);
}

function startAudioVisualizer() {
  const canvas = document.getElementById("audio-visualizer-canvas");
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth || 680;
  const height = canvas.clientHeight || 152;
  canvas.width = width * dpr;
  canvas.height = height * dpr;
  ctx.scale(dpr, dpr);

  const bufferLength = audioAnalyserNode ? audioAnalyserNode.frequencyBinCount : 32;
  const dataArray = new Uint8Array(bufferLength);
  const barCount = 36;
  if (audioPeakHeights.length !== barCount) {
    audioPeakHeights = new Array(barCount).fill(0);
  }

  function draw() {
    audioAnimId = requestAnimationFrame(draw);

    if (audioAnalyserNode) {
      audioAnalyserNode.getByteFrequencyData(dataArray);
    }

    ctx.clearRect(0, 0, width, height);

    const padding = 12;
    const availableWidth = width - (padding * 2);
    const barSpacing = 4;
    const barWidth = Math.max(4, Math.floor((availableWidth - (barSpacing * (barCount - 1))) / barCount));

    for (let i = 0; i < barCount; i++) {
      const dataIndex = Math.min(Math.floor((i / barCount) * (bufferLength * 0.85)), bufferLength - 1);
      const value = dataArray[dataIndex] || 0;
      const percent = value / 255;
      const maxBarHeight = height - 24;
      const targetHeight = Math.max(3, percent * maxBarHeight);

      if (targetHeight > audioPeakHeights[i]) {
        audioPeakHeights[i] = targetHeight;
      } else {
        audioPeakHeights[i] = Math.max(0, audioPeakHeights[i] - 1.4);
      }

      const x = padding + i * (barWidth + barSpacing);
      const y = height - targetHeight - 4;

      const grad = ctx.createLinearGradient(0, height, 0, y);
      grad.addColorStop(0, "#94e2d5");
      grad.addColorStop(0.35, "#89b4fa");
      grad.addColorStop(0.75, "#cba6f7");
      grad.addColorStop(1, "#f38ba8");

      ctx.fillStyle = grad;
      ctx.beginPath();
      if (ctx.roundRect) {
        ctx.roundRect(x, y, barWidth, targetHeight, [3, 3, 0, 0]);
      } else {
        ctx.rect(x, y, barWidth, targetHeight);
      }
      ctx.fill();

      if (audioPeakHeights[i] > 4) {
        const peakY = height - audioPeakHeights[i] - 8;
        ctx.fillStyle = "#f9e2af";
        ctx.shadowColor = "#cba6f7";
        ctx.shadowBlur = 4;
        ctx.fillRect(x, peakY, barWidth, 2);
        ctx.shadowBlur = 0;
      }
    }
  }

  if (audioAnimId) cancelAnimationFrame(audioAnimId);
  draw();
}

function toggleAudioPlay() {
  const audioEl = document.getElementById("audio-element");
  const playIcon = document.getElementById("audio-play-icon");
  if (!audioEl) return;

  if (audioCtx && audioCtx.state === "suspended") {
    audioCtx.resume();
  }

  if (audioEl.paused) {
    audioEl.play().then(() => {
      if (playIcon) playIcon.textContent = "⏸";
    });
  } else {
    audioEl.pause();
    if (playIcon) playIcon.textContent = "▶";
  }
}

function seekAudio(e) {
  const audioEl = document.getElementById("audio-element");
  const scrubBar = document.getElementById("audio-scrub-bar");
  if (!audioEl || !scrubBar || !audioEl.duration) return;

  const rect = scrubBar.getBoundingClientRect();
  const clickX = e.clientX - rect.left;
  const width = rect.width;
  const ratio = Math.max(0, Math.min(1, clickX / width));
  audioEl.currentTime = ratio * audioEl.duration;
}

function skipAudio(seconds) {
  const audioEl = document.getElementById("audio-element");
  if (!audioEl) return;
  audioEl.currentTime = Math.max(0, Math.min(audioEl.duration || 999999, audioEl.currentTime + seconds));
}

function toggleAudioLoop() {
  const audioEl = document.getElementById("audio-element");
  const loopBtn = document.getElementById("audio-btn-loop");
  if (!audioEl) return;
  audioEl.loop = !audioEl.loop;
  if (loopBtn) loopBtn.classList.toggle("active", audioEl.loop);
  showToast(audioEl.loop ? "Répétition en boucle activée" : "Répétition désactivée", "info");
}

function changeAudioSpeed() {
  const audioEl = document.getElementById("audio-element");
  const speedBtn = document.getElementById("audio-btn-speed");
  if (!audioEl) return;

  audioSpeedIndex = (audioSpeedIndex + 1) % audioSpeeds.length;
  const speed = audioSpeeds[audioSpeedIndex];
  audioEl.playbackRate = speed;
  if (speedBtn) speedBtn.textContent = `${speed.toFixed(1)}x`;
}

function setAudioVolume(val) {
  const audioEl = document.getElementById("audio-element");
  const muteBtn = document.getElementById("audio-btn-mute");
  if (!audioEl) return;
  audioEl.volume = parseFloat(val);
  if (muteBtn) muteBtn.textContent = audioEl.volume === 0 ? "🔇" : "🔊";
}

function toggleAudioMute() {
  const audioEl = document.getElementById("audio-element");
  const muteBtn = document.getElementById("audio-btn-mute");
  const volumeSlider = document.getElementById("audio-volume-slider");
  if (!audioEl) return;

  audioEl.muted = !audioEl.muted;
  if (muteBtn) muteBtn.textContent = audioEl.muted ? "🔇" : "🔊";
  if (volumeSlider) volumeSlider.value = audioEl.muted ? 0 : audioEl.volume;
}

function formatAudioTime(secs) {
  if (isNaN(secs) || secs < 0) return "00:00";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m < 10 ? '0' : ''}${m}:${s < 10 ? '0' : ''}${s}`;
}

function handleAudioKeydown(e, audioEl) {
  if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
  const modal = document.getElementById("audio-modal");
  if (!modal || modal.style.display === "none") return;

  switch (e.code) {
    case "Space":
    case "KeyP":
      e.preventDefault();
      toggleAudioPlay();
      break;

    case "ArrowLeft":
      e.preventDefault();
      skipAudio(e.shiftKey ? -1 : -5);
      break;

    case "ArrowRight":
      e.preventDefault();
      skipAudio(e.shiftKey ? 1 : 5);
      break;

    case "ArrowUp":
      e.preventDefault();
      audioEl.volume = Math.min(1, parseFloat((audioEl.volume + 0.05).toFixed(2)));
      const vSliderUp = document.getElementById("audio-volume-slider");
      if (vSliderUp) vSliderUp.value = audioEl.volume;
      break;

    case "ArrowDown":
      e.preventDefault();
      audioEl.volume = Math.max(0, parseFloat((audioEl.volume - 0.05).toFixed(2)));
      const vSliderDown = document.getElementById("audio-volume-slider");
      if (vSliderDown) vSliderDown.value = audioEl.volume;
      break;

    case "KeyM":
      e.preventDefault();
      toggleAudioMute();
      break;

    case "KeyR":
      e.preventDefault();
      toggleAudioLoop();
      break;

    case "KeyQ":
    case "Escape":
      e.preventDefault();
      closeAudioModal();
      break;
  }
}

function closeAudioModal() {
  const modal = document.getElementById("audio-modal");
  const audioEl = document.getElementById("audio-element");

  if (audioEl) {
    audioEl.pause();
    audioEl.removeAttribute("src");
    audioEl.load();
  }

  if (audioAnimId) {
    cancelAnimationFrame(audioAnimId);
    audioAnimId = null;
  }

  if (audioKeyHandler) {
    window.removeEventListener("keydown", audioKeyHandler);
    audioKeyHandler = null;
  }

  if (modal) modal.style.display = "none";
  currentAudioPath = null;
}


// --------------------------------------------------------------------------
// MODALE MONTAGE ET INITIALISATION DE VOLUME RAID
// --------------------------------------------------------------------------
function updateMountRaidHint() {
  const sel = document.getElementById("mount-select-raid");
  const hint = document.getElementById("mount-raid-hint");
  if (!sel || !hint) return;
  if (sel.value === "raid5") {
    hint.innerHTML = `<span>🛡️</span> <span>Tolérance de panne : 1 disque de secours. Le système reste opérationnel en cas de panne matérielle.</span>`;
  } else {
    hint.innerHTML = `<span>⚡</span> <span>Agrégation linéaire : Aucune tolérance de panne. L'ensemble des 14.6 To est disponible mais sans redondance.</span>`;
  }
}

function updateMountFsHint() {
  const sel = document.getElementById("mount-select-fstype");
  const hint = document.getElementById("mount-fs-hint");
  if (!sel || !hint) return;
  if (sel.value === "btrfs") {
    hint.innerHTML = `<span>✨</span> <span>Btrfs active la compression transparente Zstd pour économiser jusqu'à 30% d'espace disque et protège contre la corruption silencieuse.</span>`;
  } else if (sel.value === "ext4") {
    hint.innerHTML = `<span>🐧</span> <span>Ext4 offre la plus haute compatibilité et une robustesse éprouvée sur les serveurs Linux.</span>`;
  } else if (sel.value === "xfs") {
    hint.innerHTML = `<span>⚡</span> <span>XFS excelle dans les opérations d'E/S parallèles et la gestion de fichiers volumineux (vidéos 4K/8K).</span>`;
  }
}

function openMountVolumeModal(name, device, level) {
  const modal = document.getElementById("mount-volume-modal");
  const title = document.getElementById("mount-modal-title");
  const targetName = document.getElementById("mount-target-name");
  const targetDevice = document.getElementById("mount-target-device");
  const targetLevel = document.getElementById("mount-target-level");
  const infoText = document.getElementById("mount-modal-info-text");
  const vgOptions = document.getElementById("mount-vg-options-wrap");
  const btn = document.getElementById("btn-submit-mount");

  if (!modal) return;

  if (targetName) targetName.value = name || "";
  if (targetDevice) targetDevice.value = device || "";
  if (targetLevel) targetLevel.value = level || "";

  if (title) title.textContent = `Monter le volume : ${name}`;

  const isVg = (level || "").toLowerCase().includes("grappe") || (level || "").toLowerCase().includes("pool");
  if (vgOptions) vgOptions.style.display = isVg ? "block" : "none";

  if (infoText) {
    if (isVg) {
      infoText.innerHTML = `Ce groupe de stockage <strong>${escapeHtml(name)}</strong> (14.6 To bruts sur 4 disques) va être initialisé avec un volume logique <strong>RAID 5</strong> (10.9 To utiles protégés avec parité) et monté directement dans <code>/mnt/storage</code>.`;
    } else {
      infoText.innerHTML = `Le volume <strong>${escapeHtml(device)}</strong> (${escapeHtml(level)}) sera monté et accessible pour vos partages et fichiers dans <code>/mnt/storage</code>.`;
    }
  }

  if (btn) btn.textContent = isVg ? "🚀 Initialiser & Monter dans /mnt/storage" : "📁 Monter le volume";

  modal.style.display = "flex";
}

function closeMountVolumeModal() {
  const modal = document.getElementById("mount-volume-modal");
  if (modal) modal.style.display = "none";
}

async function submitMountVolume() {
  const name = document.getElementById("mount-target-name")?.value;
  const device = document.getElementById("mount-target-device")?.value;
  const raidSelect = document.getElementById("mount-select-raid");
  const lvInput = document.getElementById("mount-input-lvname");
  const fstypeSelect = document.getElementById("mount-select-fstype");
  const pathInput = document.getElementById("mount-input-path");
  const btn = document.getElementById("btn-submit-mount");

  if (!device) return;

  if (btn) {
    btn.disabled = true;
    btn.textContent = "Montage et initialisation en cours...";
  }

  try {
    const res = await fetch("/api/storage/mount", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: name || "storage",
        device: device,
        mountpoint: pathInput?.value || "/mnt/storage",
        fs_type: fstypeSelect?.value || "btrfs",
        raid_type: raidSelect?.value || "raid5",
        lv_name: lvInput?.value || "storage",
      })
    });

    const json = await res.json();
    if (json.success) {
      showToast(json.data || "Volume monté avec succès !", "success");
      closeMountVolumeModal();
      loadStorage();
    } else {
      showToast("Échec du montage : " + (json.message || "Erreur"), "error");
    }
  } catch (e) {
    showToast("Erreur de connexion : " + e, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "🚀 Initialiser & Monter";
    }
  }
}

async function umountVolume(mountpoint) {
  if (!confirm(`Confirmez-vous le démontage de ${mountpoint} ?`)) return;

  try {
    const res = await fetch("/api/storage/umount", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mountpoint })
    });

    const json = await res.json();
    if (json.success) {
      showToast(json.data || "Volume démonté avec succès", "success");
      loadStorage();
    } else {
      showToast(json.message || "Erreur de démontage", "error");
    }
  } catch (e) {
    showToast("Erreur : " + e, "error");
  }
}


// --------------------------------------------------------------------------
// MODALE ET GESTION DE LA RÉPARATION DES PERMISSIONS
// --------------------------------------------------------------------------
function openRepairPermissionsModal(path) {
  const modal = document.getElementById("repair-permissions-modal");
  const pathInput = document.getElementById("repair-perm-path");
  const userInput = document.getElementById("repair-perm-user");
  const groupInput = document.getElementById("repair-perm-group");
  const pwdInput = document.getElementById("repair-perm-password");
  const alertEl = document.getElementById("repair-perm-alert");
  const btn = document.getElementById("btn-submit-repair-perm");

  if (!modal) return;

  if (pathInput) pathInput.value = path || "/mnt/storage";
  if (userInput) userInput.value = "chomiam";
  if (groupInput) groupInput.value = "storage";
  if (pwdInput) pwdInput.value = "";
  if (alertEl) {
    alertEl.style.display = "none";
    alertEl.innerHTML = "";
  }
  if (btn) {
    btn.disabled = false;
    btn.textContent = "🔧 Valider & Réparer les Permissions";
  }

  modal.style.display = "flex";
}

function closeRepairPermissionsModal() {
  const modal = document.getElementById("repair-permissions-modal");
  if (modal) modal.style.display = "none";
}

function togglePermPasswordVisibility() {
  const pwdInput = document.getElementById("repair-perm-password");
  if (pwdInput) {
    pwdInput.type = pwdInput.type === "password" ? "text" : "password";
  }
}

async function submitRepairPermissions() {
  const path = document.getElementById("repair-perm-path")?.value;
  const user = document.getElementById("repair-perm-user")?.value || "chomiam";
  const group = document.getElementById("repair-perm-group")?.value || "storage";
  const pwd = document.getElementById("repair-perm-password")?.value || "";
  const recursive = document.getElementById("repair-perm-recursive")?.checked ?? true;
  const alertEl = document.getElementById("repair-perm-alert");
  const btn = document.getElementById("btn-submit-repair-perm");

  if (!path) return;

  if (btn) {
    btn.disabled = true;
    btn.textContent = "Application des permissions en cours...";
  }
  if (alertEl) alertEl.style.display = "none";

  try {
    const res = await fetch("/api/storage/permissions/repair", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        path,
        target_user: user,
        target_group: group,
        password: pwd ? pwd : null,
        recursive,
      })
    });

    const json = await res.json();
    if (json.success) {
      if (alertEl) {
        alertEl.style.display = "block";
        alertEl.style.background = "rgba(166, 227, 161, 0.15)";
        alertEl.style.border = "1px solid rgba(166, 227, 161, 0.35)";
        alertEl.style.color = "var(--green)";
        alertEl.innerHTML = `<strong>✅ Réparation réussie :</strong><br>${escapeHtml(json.data || "Permissions réparées avec succès !")}`;
      }
      showToast(json.data || "Permissions réparées avec succès !", "success");
      loadStorage();
      setTimeout(() => {
        closeRepairPermissionsModal();
      }, 2200);
    } else {
      if (alertEl) {
        alertEl.style.display = "block";
        alertEl.style.background = "rgba(243, 139, 168, 0.15)";
        alertEl.style.border = "1px solid rgba(243, 139, 168, 0.35)";
        alertEl.style.color = "var(--red)";
        alertEl.innerHTML = `<strong>❌ Échec de la réparation :</strong><br>${escapeHtml(json.message || "Erreur de permissions ou mot de passe incorrect.")}`;
      }
      showToast("Échec : " + (json.message || "Erreur"), "error");
    }
  } catch (err) {
    if (alertEl) {
      alertEl.style.display = "block";
      alertEl.style.background = "rgba(243, 139, 168, 0.15)";
      alertEl.style.border = "1px solid rgba(243, 139, 168, 0.35)";
      alertEl.style.color = "var(--red)";
      alertEl.innerHTML = `<strong>❌ Erreur de connexion :</strong><br>${escapeHtml(String(err))}`;
    }
    showToast("Erreur de connexion : " + err, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "🔧 Valider & Réparer les Permissions";
    }
  }
}


// --------------------------------------------------------------------------
// MODALE VISIONNEUSE D'IMAGES MULTI-FORMATS & EXIF (RAW, HEIC, WEB)
// --------------------------------------------------------------------------
let imageViewerFiles = [];
let imageViewerIndex = 0;
let imageViewerZoom = 1.0;
let imageViewerPanX = 0;
let imageViewerPanY = 0;
let imageViewerRotation = 0;
let imageViewerFlipH = 1;
let isImagePanning = false;
let imagePanStartX = 0;
let imagePanStartY = 0;
let imageViewerKeyHandler = null;
let isExifSidebarOpen = false;

function isImageFile(fileName, category) {
  if (category === "image") return true;
  return /\.(jpg|jpeg|png|webp|gif|svg|bmp|ico|tiff|tif|heic|heif|hif|avif|jxl|nef|nrw|cr2|cr3|crw|arw|srf|sr2|dng|raf|rw2|orf|pef|3fr|psd|raw|pcx|tga|targa|dds)$/i.test(fileName);
}

function openImageModal(path, fileName, item) {
  imageViewerFiles = currentEntries.filter(entry => !entry.is_dir && isImageFile(entry.name, entry.category));
  if (imageViewerFiles.length === 0) {
    imageViewerFiles = [{
      name: fileName || path.split("/").pop(),
      path: path,
      category: "image",
      size_bytes: item ? item.size_bytes : 0,
      size_human: item ? item.size_human : "--",
    }];
  }

  imageViewerIndex = imageViewerFiles.findIndex(f => f.path === path);
  if (imageViewerIndex === -1) imageViewerIndex = 0;

  resetImageTransformState();

  const modal = document.getElementById("image-modal");
  if (!modal) return;
  modal.style.display = "flex";

  initImageViewerEvents();
  renderImageFilmstrip();
  loadActiveImage();

  if (imageViewerKeyHandler) window.removeEventListener("keydown", imageViewerKeyHandler);
  imageViewerKeyHandler = handleImageViewerKeydown;
  window.addEventListener("keydown", imageViewerKeyHandler);
}

function resetImageTransformState() {
  imageViewerZoom = 1.0;
  imageViewerPanX = 0;
  imageViewerPanY = 0;
  imageViewerRotation = 0;
  imageViewerFlipH = 1;
  applyImageTransform();
  updateZoomLabel();
}

function loadActiveImage() {
  if (imageViewerIndex < 0 || imageViewerIndex >= imageViewerFiles.length) return;
  const file = imageViewerFiles[imageViewerIndex];

  const titleEl = document.getElementById("image-viewer-title");
  const logoTag = document.getElementById("image-viewer-logo-tag");
  const formatBadge = document.getElementById("image-viewer-format-badge");
  const resPill = document.getElementById("image-viewer-res-pill");
  const sizePill = document.getElementById("image-viewer-size-pill");
  const counterLabel = document.getElementById("image-counter-label");
  const imgEl = document.getElementById("image-viewer-img");
  const loadingOverlay = document.getElementById("image-viewer-loading");
  const loadingMsg = document.getElementById("image-loading-msg");
  const downloadBtn = document.getElementById("image-viewer-download-btn");

  if (titleEl) titleEl.textContent = file.name;
  if (titleEl) titleEl.title = file.path;
  if (sizePill) sizePill.textContent = file.size_human || (file.size_bytes ? formatFileSize(file.size_bytes) : "--");
  if (counterLabel) counterLabel.textContent = `${imageViewerIndex + 1} / ${imageViewerFiles.length}`;

  const ext = file.name.split(".").pop().toUpperCase();
  if (formatBadge) formatBadge.textContent = ext;

  if (/\.(nef|nrw|cr2|cr3|crw|arw|srf|sr2|dng|raf|rw2|orf|pef|3fr|raw)$/i.test(file.name)) {
    if (logoTag) { logoTag.textContent = "📷 RAW"; logoTag.style.color = "var(--yellow)"; }
    if (loadingMsg) loadingMsg.textContent = `Développement RAW (${ext}) en cours...`;
  } else if (/\.(heic|heif|hif)$/i.test(file.name)) {
    if (logoTag) { logoTag.textContent = "📱 IPHONE"; logoTag.style.color = "var(--teal)"; }
    if (loadingMsg) loadingMsg.textContent = "Décodage High Efficiency HEIC Apple...";
  } else {
    if (logoTag) { logoTag.textContent = "🖼️ IMAGE"; logoTag.style.color = "var(--mauve)"; }
    if (loadingMsg) loadingMsg.textContent = "Chargement de l'image...";
  }

  if (resPill) resPill.textContent = "-- × --";
  if (downloadBtn) {
    downloadBtn.href = `/api/files/stream?path=${encodeURIComponent(file.path)}`;
    downloadBtn.download = file.name;
  }

  if (loadingOverlay) loadingOverlay.style.display = "flex";
  resetImageTransformState();

  const previewUrl = `/api/files/image-view?path=${encodeURIComponent(file.path)}`;
  imgEl.src = previewUrl;

  imgEl.onload = () => {
    if (loadingOverlay) loadingOverlay.style.display = "none";
    if (resPill && imgEl.naturalWidth && imgEl.naturalHeight) {
      const mp = ((imgEl.naturalWidth * imgEl.naturalHeight) / 1000000).toFixed(1);
      resPill.textContent = `${imgEl.naturalWidth} × ${imgEl.naturalHeight} (${mp} MP)`;
    }
  };

  imgEl.onerror = () => {
    if (loadingOverlay) loadingOverlay.style.display = "none";
    showToast(`Impossible de charger l'aperçu de : ${file.name}`, "error");
  };

  updateFilmstripActive();
  loadExifData(file.path);
}

function renderImageFilmstrip() {
  const track = document.getElementById("image-filmstrip-track");
  if (!track) return;

  track.innerHTML = imageViewerFiles.map((file, idx) => {
    const ext = file.name.split(".").pop().toUpperCase();
    const thumbUrl = `/api/files/image-view?path=${encodeURIComponent(file.path)}&thumb=true`;
    return `
      <div class="image-filmstrip-item ${idx === imageViewerIndex ? 'active' : ''}" 
           id="filmstrip-item-${idx}" 
           onclick="selectImageByIndex(${idx})" 
           title="${escapeHtml(file.name)}">
        <img src="${thumbUrl}" loading="lazy" alt="${escapeHtml(file.name)}" onerror="this.src='/favicon.ico';">
        <span class="thumb-ext-badge">${ext}</span>
      </div>
    `;
  }).join("");

  scrollToActiveFilmstrip();
}

function updateFilmstripActive() {
  document.querySelectorAll(".image-filmstrip-item").forEach((el, idx) => {
    el.classList.toggle("active", idx === imageViewerIndex);
  });
  scrollToActiveFilmstrip();
}

function scrollToActiveFilmstrip() {
  const activeEl = document.getElementById(`filmstrip-item-${imageViewerIndex}`);
  if (activeEl) {
    activeEl.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "center" });
  }
}

function selectImageByIndex(idx) {
  if (idx >= 0 && idx < imageViewerFiles.length) {
    imageViewerIndex = idx;
    loadActiveImage();
  }
}

function navigateImageViewer(direction) {
  if (imageViewerFiles.length <= 1) return;
  imageViewerIndex = (imageViewerIndex + direction + imageViewerFiles.length) % imageViewerFiles.length;
  loadActiveImage();
}

function applyImageTransform() {
  const img = document.getElementById("image-viewer-img");
  if (!img) return;
  img.style.transform = `translate(${imageViewerPanX}px, ${imageViewerPanY}px) scale(${imageViewerZoom}) rotate(${imageViewerRotation}deg) scaleX(${imageViewerFlipH})`;
}

function updateZoomLabel() {
  const label = document.getElementById("image-zoom-label");
  if (label) {
    label.textContent = `${Math.round(imageViewerZoom * 100)}%`;
  }
}

function zoomImage(delta) {
  imageViewerZoom = Math.max(0.1, Math.min(8.0, parseFloat((imageViewerZoom + delta).toFixed(2))));
  applyImageTransform();
  updateZoomLabel();
}

function resetZoom() {
  if (imageViewerZoom === 1.0) {
    fitImageToScreen();
  } else {
    imageViewerZoom = 1.0;
    imageViewerPanX = 0;
    imageViewerPanY = 0;
    applyImageTransform();
    updateZoomLabel();
  }
}

function fitImageToScreen() {
  const viewport = document.getElementById("image-viewer-viewport");
  const img = document.getElementById("image-viewer-img");
  if (!viewport || !img || !img.naturalWidth || !img.naturalHeight) {
    imageViewerZoom = 1.0;
    imageViewerPanX = 0;
    imageViewerPanY = 0;
    applyImageTransform();
    updateZoomLabel();
    return;
  }

  const vw = viewport.clientWidth * 0.94;
  const vh = viewport.clientHeight * 0.94;

  const isRotated90 = Math.abs(imageViewerRotation % 180) === 90;
  const imgW = isRotated90 ? img.naturalHeight : img.naturalWidth;
  const imgH = isRotated90 ? img.naturalWidth : img.naturalHeight;

  const scaleW = vw / imgW;
  const scaleH = vh / imgH;
  const scale = Math.min(scaleW, scaleH, 1.0);

  imageViewerZoom = parseFloat(scale.toFixed(2));
  imageViewerPanX = 0;
  imageViewerPanY = 0;
  applyImageTransform();
  updateZoomLabel();
}

function rotateImage(deg) {
  imageViewerRotation = (imageViewerRotation + deg) % 360;
  applyImageTransform();
}

function flipImageHorizontal() {
  imageViewerFlipH = imageViewerFlipH === 1 ? -1 : 1;
  applyImageTransform();
}

function toggleExifSidebar() {
  const sidebar = document.getElementById("image-exif-sidebar");
  const btn = document.getElementById("btn-toggle-exif");
  if (!sidebar) return;

  isExifSidebarOpen = !isExifSidebarOpen;
  sidebar.style.display = isExifSidebarOpen ? "flex" : "none";
  if (btn) btn.classList.toggle("active", isExifSidebarOpen);
}

async function loadExifData(path) {
  const content = document.getElementById("image-exif-content");
  if (!content) return;

  content.innerHTML = `<div class="exif-empty-hint">⏳ Analyse des métadonnées EXIF...</div>`;

  try {
    const res = await fetch(`/api/files/image-info?path=${encodeURIComponent(path)}`);
    const json = await res.json();
    if (!json.success || !json.data) {
      content.innerHTML = `<div class="exif-empty-hint">Aucune métadonnée EXIF détaillée disponible pour cette image.</div>`;
      return;
    }

    const data = json.data;
    let html = "";

    if (data.camera_model || data.camera_make || data.lens) {
      html += `
        <div class="exif-card">
          <div class="exif-card-title">Appareil Photo & Optique</div>
          <div class="exif-camera-model">${escapeHtml(data.camera_make || "")} ${escapeHtml(data.camera_model || "")}</div>
          ${data.lens ? `<div class="exif-lens-model">🔍 ${escapeHtml(data.lens)}</div>` : ''}
        </div>
      `;
    }

    if (data.aperture || data.shutter_speed || data.iso || data.focal_length) {
      html += `
        <div class="exif-card">
          <div class="exif-card-title">Paramètres de Prise de Vue</div>
          <div class="exif-exposure-grid">
            <div class="exif-tile">
              <span class="exif-tile-label">Ouverture</span>
              <span class="exif-tile-val aperture">${escapeHtml(data.aperture || "--")}</span>
            </div>
            <div class="exif-tile">
              <span class="exif-tile-label">Vitesse</span>
              <span class="exif-tile-val shutter">${escapeHtml(data.shutter_speed || "--")}s</span>
            </div>
            <div class="exif-tile">
              <span class="exif-tile-label">Sensibilité</span>
              <span class="exif-tile-val iso">ISO ${escapeHtml(data.iso || "--")}</span>
            </div>
            <div class="exif-tile">
              <span class="exif-tile-label">Focale</span>
              <span class="exif-tile-val focal">${escapeHtml(data.focal_length || "--")}</span>
            </div>
          </div>
        </div>
      `;
    }

    html += `
      <div class="exif-card">
        <div class="exif-card-title">Propriétés de l'Image</div>
        <table class="exif-props-table">
          <tr><td>Format :</td><td>${escapeHtml(data.format)}</td></tr>
          <tr><td>Poids :</td><td>${escapeHtml(data.size_human)}</td></tr>
          ${data.width && data.height ? `<tr><td>Dimensions :</td><td>${data.width} × ${data.height}</td></tr>` : ''}
          ${data.date_taken ? `<tr><td>Date prise :</td><td>${escapeHtml(data.date_taken)}</td></tr>` : ''}
          ${data.exposure_mode ? `<tr><td>Exposition :</td><td>${escapeHtml(data.exposure_mode)}</td></tr>` : ''}
          ${data.white_balance ? `<tr><td>Balance blancs :</td><td>${escapeHtml(data.white_balance)}</td></tr>` : ''}
          ${data.color_space ? `<tr><td>Espace colorimétrique :</td><td>${escapeHtml(data.color_space)}</td></tr>` : ''}
          ${data.software ? `<tr><td>Logiciel :</td><td>${escapeHtml(data.software)}</td></tr>` : ''}
        </table>
      </div>
    `;

    content.innerHTML = html;
  } catch (err) {
    content.innerHTML = `<div class="exif-empty-hint">Impossible de charger les métadonnées : ${escapeHtml(String(err))}</div>`;
  }
}

let imageViewerEventsInitialized = false;
function initImageViewerEvents() {
  if (imageViewerEventsInitialized) return;
  imageViewerEventsInitialized = true;

  const viewport = document.getElementById("image-viewer-viewport");
  if (!viewport) return;

  viewport.addEventListener("wheel", (e) => {
    e.preventDefault();
    const zoomFactor = e.deltaY < 0 ? 1.15 : 0.87;
    const newZoom = Math.max(0.1, Math.min(10.0, parseFloat((imageViewerZoom * zoomFactor).toFixed(2))));

    const rect = viewport.getBoundingClientRect();
    const mouseX = e.clientX - rect.left - rect.width / 2;
    const mouseY = e.clientY - rect.top - rect.height / 2;

    imageViewerPanX -= (mouseX - imageViewerPanX) * (zoomFactor - 1);
    imageViewerPanY -= (mouseY - imageViewerPanY) * (zoomFactor - 1);

    imageViewerZoom = newZoom;
    applyImageTransform();
    updateZoomLabel();
  }, { passive: false });

  viewport.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    if (e.target.closest(".image-nav-arrow")) return;

    isImagePanning = true;
    imagePanStartX = e.clientX - imageViewerPanX;
    imagePanStartY = e.clientY - imageViewerPanY;
    viewport.classList.add("panning");
  });

  window.addEventListener("mousemove", (e) => {
    if (!isImagePanning) return;
    imageViewerPanX = e.clientX - imagePanStartX;
    imageViewerPanY = e.clientY - imagePanStartY;
    applyImageTransform();
  });

  window.addEventListener("mouseup", () => {
    if (isImagePanning) {
      isImagePanning = false;
      viewport.classList.remove("panning");
    }
  });

  viewport.addEventListener("dblclick", (e) => {
    if (e.target.closest(".image-nav-arrow")) return;
    if (imageViewerZoom >= 1.5) {
      fitImageToScreen();
    } else {
      imageViewerZoom = 2.0;
      applyImageTransform();
      updateZoomLabel();
    }
  });
}

function handleImageViewerKeydown(e) {
  if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA" || e.target.tagName === "SELECT") return;

  const modal = document.getElementById("image-modal");
  if (!modal || modal.style.display === "none") return;

  switch (e.code) {
    case "ArrowLeft":
      e.preventDefault();
      navigateImageViewer(-1);
      break;

    case "ArrowRight":
      e.preventDefault();
      navigateImageViewer(1);
      break;

    case "Equal":
    case "NumpadAdd":
      e.preventDefault();
      zoomImage(0.25);
      break;

    case "Minus":
    case "NumpadSubtract":
      e.preventDefault();
      zoomImage(-0.25);
      break;

    case "Digit0":
    case "Numpad0":
      e.preventDefault();
      fitImageToScreen();
      break;

    case "KeyR":
      e.preventDefault();
      rotateImage(e.shiftKey ? -90 : 90);
      break;

    case "KeyH":
      e.preventDefault();
      flipImageHorizontal();
      break;

    case "KeyI":
      e.preventDefault();
      toggleExifSidebar();
      break;

    case "KeyF":
      e.preventDefault();
      toggleImageViewerFullscreen();
      break;

    case "KeyQ":
    case "Escape":
      e.preventDefault();
      closeImageModal();
      break;
  }
}

function toggleImageViewerFullscreen() {
  const windowEl = document.querySelector(".image-viewer-window");
  if (!windowEl) return;

  if (!document.fullscreenElement) {
    windowEl.requestFullscreen().catch(() => {});
  } else {
    document.exitFullscreen().catch(() => {});
  }
}

function closeImageModal() {
  const modal = document.getElementById("image-modal");
  const img = document.getElementById("image-viewer-img");

  if (imageViewerKeyHandler) {
    window.removeEventListener("keydown", imageViewerKeyHandler);
    imageViewerKeyHandler = null;
  }

  if (img) img.removeAttribute("src");
  if (modal) modal.style.display = "none";
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    const powerModal = document.getElementById("power-modal");
    if (powerModal && powerModal.style.display !== "none") {
      closePowerModal();
    }
    const docModal = document.getElementById("doc-modal");
    if (docModal && docModal.style.display !== "none") {
      closeDocModal();
    }
  }
});

// ==========================================================================
// GESTION DE L'ALIMENTATION DU NAS (ARRÊT / REBOOT / PLANIFICATION)
// ==========================================================================
let powerStatus = null;
let immediatePowerTarget = null;
let immediateCountdownTimer = null;
let immediateCountdownSec = 3;

function openPowerModal() {
  const modal = document.getElementById("power-modal");
  if (modal) {
    modal.style.display = "flex";
    cancelImmediateConfirm();
    fetchPowerStatus();
  }
}

function closePowerModal() {
  const modal = document.getElementById("power-modal");
  if (modal) modal.style.display = "none";
  cancelImmediateConfirm();
}

function handlePowerBackdropClick(e) {
  if (e.target && e.target.id === "power-modal") {
    closePowerModal();
  }
}

async function fetchPowerStatus() {
  try {
    const res = await fetch("/api/system/power/status");
    const json = await res.json();
    if (json.success && json.data) {
      powerStatus = json.data;
      updatePowerStatusUI();
    }
  } catch (err) {
    console.error("Erreur statut alimentation :", err);
  }
}

function updatePowerStatusUI() {
  const badge = document.getElementById("header-power-badge");
  const alertBox = document.getElementById("power-scheduled-alert");
  const titleEl = document.getElementById("power-scheduled-title");
  const detailEl = document.getElementById("power-scheduled-detail");

  if (!powerStatus || !powerStatus.is_scheduled) {
    if (badge) badge.style.display = "none";
    if (alertBox) alertBox.style.display = "none";
    return;
  }

  if (badge) badge.style.display = "inline-flex";
  if (alertBox) alertBox.style.display = "flex";

  const mode = powerStatus.mode === "reboot" ? "Redémarrage" : "Arrêt";
  if (titleEl) titleEl.textContent = `${mode} planifié en cours`;

  let detailStr = "";
  if (powerStatus.seconds_remaining !== null && powerStatus.seconds_remaining !== undefined) {
    const rem = powerStatus.seconds_remaining;
    if (rem <= 60) {
      detailStr = `Dans moins d'une minute !`;
    } else {
      const min = Math.ceil(rem / 60);
      const hours = Math.floor(min / 60);
      const remMin = min % 60;
      if (hours > 0) {
        detailStr = `Dans environ ${hours}h ${remMin > 0 ? remMin + "min" : ""}`;
      } else {
        detailStr = `Dans environ ${min} minute${min > 1 ? "s" : ""}`;
      }
    }
  }

  if (powerStatus.target_timestamp) {
    const targetDate = new Date(powerStatus.target_timestamp * 1000);
    const targetTimeStr = targetDate.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    detailStr += ` (Prévu à ${targetTimeStr})`;
  }

  if (powerStatus.wall_message) {
    detailStr += ` • « ${powerStatus.wall_message} »`;
  }

  if (detailEl) detailEl.textContent = detailStr;
}

function confirmImmediatePower(action) {
  immediatePowerTarget = action;
  const box = document.getElementById("power-confirm-box");
  const title = document.getElementById("power-confirm-title");
  const desc = document.getElementById("power-confirm-desc");
  const execBtn = document.getElementById("btn-power-confirm-exec");
  const countdownSpan = document.getElementById("power-countdown");

  if (!box) return;

  const isReboot = action === "reboot";
  if (title) title.textContent = isReboot ? "Confirmer le redémarrage du NAS ?" : "Confirmer l'extinction du NAS ?";
  if (desc) desc.textContent = isReboot
    ? "Le système va redémarrer immédiatement et relancer tous les services."
    : "Le serveur va s'éteindre complètement. Vous devrez le rallumer manuellement.";

  if (execBtn) {
    execBtn.className = isReboot ? "btn btn-info btn-sm" : "btn btn-danger btn-sm";
  }

  box.style.display = "flex";
  immediateCountdownSec = 3;
  if (countdownSpan) countdownSpan.textContent = immediateCountdownSec;

  if (immediateCountdownTimer) clearInterval(immediateCountdownTimer);
  immediateCountdownTimer = setInterval(() => {
    immediateCountdownSec--;
    if (countdownSpan) countdownSpan.textContent = immediateCountdownSec;
    if (immediateCountdownSec <= 0) {
      clearInterval(immediateCountdownTimer);
      immediateCountdownTimer = null;
      if (countdownSpan) countdownSpan.parentElement.textContent = "Confirmer maintenant";
    }
  }, 1000);
}

function cancelImmediateConfirm() {
  immediatePowerTarget = null;
  if (immediateCountdownTimer) {
    clearInterval(immediateCountdownTimer);
    immediateCountdownTimer = null;
  }
  const box = document.getElementById("power-confirm-box");
  if (box) box.style.display = "none";
}

async function executeImmediatePower() {
  if (!immediatePowerTarget) return;
  const action = immediatePowerTarget;
  cancelImmediateConfirm();
  closePowerModal();

  showToast(`Envoi de l'ordre de ${action === "reboot" ? "redémarrage" : "mise hors tension"}...`, "warning");

  try {
    const res = await fetch("/api/system/power/immediate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action: action })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Ordre exécuté avec succès.", "success");
    } else {
      showToast(json.message || "Erreur lors de l'exécution.", "error");
    }
  } catch (err) {
    showToast("Le serveur applique l'ordre d'extinction ou de redémarrage...", "info");
  }
}

function updatePowerScheduleLabels() {
  const action = document.querySelector('input[name="power-target-action"]:checked')?.value || "poweroff";
  const btnText = document.getElementById("btn-schedule-power-text");
  if (btnText) {
    btnText.textContent = action === "reboot" ? "⏱️ Programmer le redémarrage" : "⏱️ Programmer l'arrêt";
  }
}

function applyPowerPreset(minutes) {
  const delayInput = document.getElementById("power-delay-input");
  const timeInput = document.getElementById("power-time-input");
  if (delayInput) delayInput.value = minutes;
  if (timeInput) timeInput.value = "";

  document.querySelectorAll(".btn-preset-pill").forEach(btn => {
    btn.classList.toggle("active", btn.textContent.includes(minutes + ""));
  });
}

function clearPowerTimeInput() {
  const timeInput = document.getElementById("power-time-input");
  if (timeInput) timeInput.value = "";
}

function clearPowerDelayInput() {
  const delayInput = document.getElementById("power-delay-input");
  if (delayInput) delayInput.value = "";
  document.querySelectorAll(".btn-preset-pill").forEach(b => b.classList.remove("active"));
}

async function submitPowerSchedule() {
  const action = document.querySelector('input[name="power-target-action"]:checked')?.value || "poweroff";
  const delayVal = document.getElementById("power-delay-input")?.value?.trim();
  const timeVal = document.getElementById("power-time-input")?.value?.trim();
  const wallMsg = document.getElementById("power-wall-msg")?.value?.trim() || null;

  let delayMinutes = null;
  let timeHhmm = null;

  if (delayVal) {
    delayMinutes = parseInt(delayVal, 10);
    if (isNaN(delayMinutes) || delayMinutes < 1) {
      showToast("Veuillez saisir un délai valide en minutes.", "warning");
      return;
    }
  } else if (timeVal) {
    timeHhmm = timeVal;
  } else {
    showToast("Veuillez choisir un préréglage, un délai ou une heure.", "warning");
    return;
  }

  try {
    const res = await fetch("/api/system/power/schedule", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        action: action,
        delay_minutes: delayMinutes,
        time_hhmm: timeHhmm,
        wall_message: wallMsg
      })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Planification enregistrée.", "success");
      fetchPowerStatus();
    } else {
      showToast(json.message || "Échec de planification.", "error");
    }
  } catch (err) {
    showToast("Erreur lors de la planification : " + err, "error");
  }
}

async function cancelScheduledPower() {
  try {
    const res = await fetch("/api/system/power/cancel", { method: "POST" });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Planification annulée.", "success");
      fetchPowerStatus();
    } else {
      showToast(json.message || "Impossible d'annuler.", "error");
    }
  } catch (err) {
    showToast("Erreur lors de l'annulation : " + err, "error");
  }
}


// ==========================================================================
// GESTION DE LA CORBEILLE (TRASH) AVEC RÉTENTION 30 JOURS
// ==========================================================================
async function navigateToTrash() {
  isTrashView = true;
  currentFolderPath = "/corbeille";
  updateSidebarNavActive("/corbeille");

  const normalTb = document.getElementById("files-toolbar-normal");
  const trashTb = document.getElementById("files-toolbar-trash");
  if (normalTb) normalTb.style.display = "none";
  if (trashTb) trashTb.style.display = "flex";

  const breadcrumb = document.getElementById("files-breadcrumbs");
  if (breadcrumb) {
    breadcrumb.innerHTML = '<span class="crumb-item active" style="color:var(--mauve);">🗑️ Corbeille (Rétention automatique 30 jours)</span>';
  }

  await refreshTrash();
}

async function refreshTrash() {
  try {
    const res = await fetch("/api/files/trash");
    const json = await res.json();
    if (!json.success || !json.data) {
      showToast(json.message || "Erreur lors du chargement de la corbeille", "error");
      return;
    }

    trashOverview = json.data;
    updateTrashBadge(trashOverview.total_items);
    renderTrashList(trashOverview.items);
    updateFilesStatusBar(trashOverview.total_items, trashOverview.total_size_bytes);
  } catch (err) {
    showToast("Erreur lors de la récupération de la corbeille : " + err, "error");
  }
}

async function fetchTrashCount() {
  try {
    const res = await fetch("/api/files/trash");
    const json = await res.json();
    if (json.success && json.data) {
      updateTrashBadge(json.data.total_items);
    }
  } catch (err) {
    // Silently ignore
  }
}

function updateTrashBadge(count) {
  const badge = document.getElementById("trash-badge-count");
  if (badge) {
    badge.textContent = count;
    badge.style.display = count > 0 ? "inline-block" : "none";
  }
}

function renderTrashList(items) {
  const gridWrap = document.getElementById("files-grid-wrap");
  const tableBody = document.getElementById("files-table-tbody");
  const gridContainer = document.getElementById("files-grid-wrap");
  const tableContainer = document.getElementById("files-table-wrap");

  if (!gridWrap || !tableBody) return;

  const btnRestore = document.getElementById("btn-trash-restore");
  const btnDeletePerm = document.getElementById("btn-trash-delete-perm");
  if (btnRestore) btnRestore.disabled = true;
  if (btnDeletePerm) btnDeletePerm.disabled = true;

  if (items.length === 0) {
    const emptyHtml = '<div style="grid-column:1/-1; padding:60px 20px; text-align:center; color:var(--subtext0);">' +
      '<div style="font-size:3rem; margin-bottom:12px;">🗑️</div>' +
      '<div style="font-size:1.1rem; font-weight:600; color:var(--text);">La corbeille est vide</div>' +
      '<div style="font-size:0.85rem; margin-top:6px; color:var(--subtext0);">Les fichiers supprimés sont conservés ici pendant 30 jours avant purge définitive.</div>' +
    '</div>';
    gridWrap.innerHTML = emptyHtml;
    tableBody.innerHTML = '<tr><td colspan="6" style="text-align:center; color:var(--subtext0); padding:40px;">🗑️ La corbeille est vide (rétention automatique de 30 jours)</td></tr>';
    return;
  }

  // Rendu Grille
  gridWrap.innerHTML = items.map(item => {
    const icon = getFileIcon(item);
    let cardPreview = '<div class="file-card-icon">' + icon + '</div>';
    if (isImageFile(item.name, item.category)) {
      const thumbUrl = '/api/files/image-view?path=' + encodeURIComponent(item.trash_path) + '&thumb=true';
      cardPreview = '<div class="file-card-icon file-card-img-preview" style="width:100%; height:80px; max-height:80px; overflow:hidden; border-radius:6px; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,0.35);"><img src="' + thumbUrl + '" loading="lazy" alt="' + escapeHtml(item.name) + '" style="width:100%; height:100%; max-width:100%; max-height:100%; object-fit:cover; display:block; border-radius:5px;" onerror="this.onerror=null; this.parentElement.className=\'file-card-icon\'; this.parentElement.style=\'width:100%; height:80px; display:flex; align-items:center; justify-content:center; font-size:2.4rem;\'; this.parentElement.innerHTML=\'' + icon + '\';"></div>';
    }

    const pillClass = item.days_remaining > 10 ? 'days-safe' : (item.days_remaining > 3 ? 'days-warning' : 'days-danger');

    return '<div class="file-card" style="min-width:0; overflow:hidden;" ' +
           'data-trash-id="' + escapeHtml(item.id) + '" ' +
           'onclick="handleTrashCardClick(event, \'' + escapeHtml(item.id) + '\')" ' +
           'ondblclick="promptRestoreTrashItem(\'' + escapeHtml(item.id) + '\', \'' + escapeHtml(item.name) + '\', \'' + escapeHtml(item.original_path) + '\')" ' +
           'oncontextmenu="handleTrashContextMenu(event, \'' + escapeHtml(item.id) + '\')">' +
        cardPreview +
        '<div class="file-card-name" title="' + escapeHtml(item.name) + '">' + escapeHtml(item.name) + '</div>' +
        '<div class="trash-orig-path" title="' + escapeHtml(item.original_path) + '">' + escapeHtml(item.original_path) + '</div>' +
        '<div class="file-card-meta">' + escapeHtml(item.size_human) + '</div>' +
        '<div class="trash-card-days ' + pillClass + '">⏳ ' + item.days_remaining + ' j restants</div>' +
      '</div>';
  }).join("");

  // Rendu Tableau
  tableBody.innerHTML = items.map(item => {
    const icon = getFileIcon(item);
    const pillClass = item.days_remaining > 10 ? 'days-safe' : (item.days_remaining > 3 ? 'days-warning' : 'days-danger');
    return '<tr data-trash-id="' + escapeHtml(item.id) + '" ' +
          'onclick="handleTrashCardClick(event, \'' + escapeHtml(item.id) + '\')" ' +
          'ondblclick="promptRestoreTrashItem(\'' + escapeHtml(item.id) + '\', \'' + escapeHtml(item.name) + '\', \'' + escapeHtml(item.original_path) + '\')" ' +
          'oncontextmenu="handleTrashContextMenu(event, \'' + escapeHtml(item.id) + '\')">' +
        '<td>' +
          '<span style="font-size:1.1rem; margin-right:8px;">' + icon + '</span>' +
          '<strong style="color:var(--text);">' + escapeHtml(item.name) + '</strong>' +
        '</td>' +
        '<td style="color:var(--subtext0); font-family:var(--font-mono); font-size:0.75rem; max-width:200px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;" title="' + escapeHtml(item.original_path) + '">' +
          escapeHtml(item.original_path) +
        '</td>' +
        '<td style="color:var(--subtext0); font-size:0.8rem;">' + escapeHtml(item.deletion_date || "--") + '</td>' +
        '<td>' +
          '<span class="trash-card-days ' + pillClass + '">⏳ ' + item.days_remaining + ' jours</span>' +
        '</td>' +
        '<td style="color:var(--subtext0); font-family:var(--font-mono); font-size:0.8rem;">' + escapeHtml(item.size_human) + '</td>' +
        '<td style="text-align:right;">' +
          '<button type="button" class="btn btn-secondary btn-xs" onclick="event.stopPropagation(); restoreTrashItem(\'' + escapeHtml(item.id) + '\')" title="Restaurer">🔄</button>' +
          '<button type="button" class="btn btn-danger btn-xs" onclick="event.stopPropagation(); deleteTrashPermanent(\'' + escapeHtml(item.id) + '\')" title="Supprimer définitivement" style="margin-left:4px;">❌</button>' +
        '</td>' +
      '</tr>';
  }).join("");

  if (fileViewMode === "grid") {
    gridContainer.style.display = "grid";
    tableContainer.style.display = "none";
  } else {
    gridContainer.style.display = "none";
    tableContainer.style.display = "table";
  }
}

function handleTrashCardClick(e, id) {
  selectedTrashItem = trashOverview?.items.find(i => i.id === id) || null;
  document.querySelectorAll(".file-card, tr").forEach(el => el.classList.remove("selected"));
  const card = document.querySelector('[data-trash-id="' + CSS.escape(id) + '"]');
  if (card) card.classList.add("selected");

  const btnRestore = document.getElementById("btn-trash-restore");
  const btnDeletePerm = document.getElementById("btn-trash-delete-perm");
  if (btnRestore) btnRestore.disabled = !selectedTrashItem;
  if (btnDeletePerm) btnDeletePerm.disabled = !selectedTrashItem;
}

function handleTrashContextMenu(e, id) {
  e.preventDefault();
  e.stopPropagation();
  handleTrashCardClick(e, id);

  const menu = document.getElementById("files-context-menu");
  if (!menu) return;

  document.querySelectorAll(".context-menu-item").forEach(el => el.style.display = "none");
  const ctxRestore = document.getElementById("ctx-trash-restore");
  const ctxDelete = document.getElementById("ctx-trash-delete");
  if (ctxRestore) ctxRestore.style.display = "flex";
  if (ctxDelete) ctxDelete.style.display = "flex";

  positionContextMenu(menu, e.clientX, e.clientY);
}

function promptRestoreTrashItem(id, name, origPath) {
  if (confirm('Voulez-vous restaurer "' + name + '" à son emplacement d\'origine ?\nDestination : ' + origPath)) {
    restoreTrashItem(id);
  }
}

function restoreSelectedTrashItem() {
  if (selectedTrashItem) {
    restoreTrashItem(selectedTrashItem.id);
  } else {
    showToast("Veuillez sélectionner un élément à restaurer.", "warning");
  }
}

function deleteSelectedTrashPermanent() {
  if (selectedTrashItem) {
    deleteTrashPermanent(selectedTrashItem.id);
  } else {
    showToast("Veuillez sélectionner un élément à supprimer.", "warning");
  }
}

async function restoreTrashItem(id) {
  try {
    const res = await fetch("/api/files/trash/restore", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: id })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Élément restauré avec succès.", "success");
      await refreshTrash();
    } else {
      showToast(json.message || "Échec de restauration", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}

async function deleteTrashPermanent(id) {
  if (!confirm("Voulez-vous supprimer définitivement cet élément de la corbeille ? Cette action est irréversible.")) {
    return;
  }

  try {
    const res = await fetch("/api/files/trash/delete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: id })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Élément définitivement supprimé.", "success");
      await refreshTrash();
    } else {
      showToast(json.message || "Échec de la suppression", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}

async function emptyEntireTrash() {
  if (!confirm("Voulez-vous vraiment VIDER TOUTE la corbeille ? Tous les éléments seront définitivement supprimés de manière irréversible.")) {
    return;
  }

  try {
    const res = await fetch("/api/files/trash/empty", {
      method: "POST"
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Corbeille vidée avec succès.", "success");
      await refreshTrash();
    } else {
      showToast(json.message || "Échec de vidage de la corbeille", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}

// ==========================================================================

// ==========================================================================
// TÉLÉCHARGEUR YOUTUBE MP3 / MP4 HAUTE FIDÉLITÉ
// ==========================================================================
let currentYoutubeInfo = null;
let currentYoutubeFormat = "mp4";
let youtubePollInterval = null;
let isYoutubeDescExpanded = false;

function toggleYoutubeCardCollapse() {
  const body = document.getElementById("yt-downloader-body");
  const btn = document.getElementById("btn-yt-collapse");
  if (!body) return;
  if (body.style.display === "none") {
    body.style.display = "block";
    if (btn) btn.textContent = "−";
  } else {
    body.style.display = "none";
    if (btn) btn.textContent = "+";
  }
}

function dismissYoutubeError() {
  const box = document.getElementById("yt-error-box");
  if (box) box.style.display = "none";
}

async function pasteYoutubeClipboard() {
  try {
    const text = await navigator.clipboard.readText();
    if (text) {
      const input = document.getElementById("yt-url-input");
      if (input) {
        input.value = text.trim();
        fetchYoutubePreview();
      }
    } else {
      showToast("Le presse-papier est vide.", "info");
    }
  } catch (err) {
    showToast("Impossible d'accéder au presse-papier : " + err, "error");
  }
}

function setYoutubeFormat(fmt) {
  currentYoutubeFormat = fmt;
  const buttons = document.querySelectorAll(".btn-yt-format");
  buttons.forEach(btn => {
    if (btn.getAttribute("data-format") === fmt) {
      btn.classList.add("active");
    } else {
      btn.classList.remove("active");
    }
  });

  const badge = document.getElementById("yt-ext-badge");
  if (badge) badge.textContent = "." + fmt;

  // Auto-switch destination directory if default
  const destInput = document.getElementById("yt-dest-dir-input");
  if (destInput) {
    if (fmt === "mp3" && destInput.value === "/home/chomiam/videos") {
      destInput.value = "/home/chomiam/musique";
    } else if (fmt === "mp4" && destInput.value === "/home/chomiam/musique") {
      destInput.value = "/home/chomiam/videos";
    }
  }
}

function setYoutubeDestDir(dir) {
  const destInput = document.getElementById("yt-dest-dir-input");
  if (destInput && dir) {
    destInput.value = dir;
  }
}

function toggleYoutubeDesc() {
  const descEl = document.getElementById("yt-preview-desc");
  const btn = document.getElementById("yt-desc-toggle-btn");
  if (!descEl) return;
  isYoutubeDescExpanded = !isYoutubeDescExpanded;
  if (isYoutubeDescExpanded) {
    descEl.classList.add("expanded");
    if (btn) btn.textContent = "Réduire ▲";
  } else {
    descEl.classList.remove("expanded");
    if (btn) btn.textContent = "Lire plus... ▼";
  }
}

function formatDurationSeconds(sec) {
  if (!sec || isNaN(sec)) return "00:00";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  }
  return `${m}:${s.toString().padStart(2, "0")}`;
}

async function fetchYoutubePreview() {
  const urlInput = document.getElementById("yt-url-input");
  const fetchBtn = document.getElementById("btn-yt-fetch");
  const fetchBtnText = document.getElementById("btn-yt-fetch-text");
  const errorBox = document.getElementById("yt-error-box");
  const errorMsg = document.getElementById("yt-error-message");
  const previewBox = document.getElementById("yt-preview-box");

  if (!urlInput) return;
  const url = urlInput.value.trim();

  if (!url) {
    if (errorBox && errorMsg) {
      errorMsg.textContent = "Veuillez saisir ou coller un lien YouTube valide.";
      errorBox.style.display = "flex";
    }
    return;
  }

  // Basic client check
  if (!url.includes("youtube.com") && !url.includes("youtu.be")) {
    if (errorBox && errorMsg) {
      errorMsg.textContent = "Le lien renseigné ne semble pas être une URL YouTube valide (ex: youtube.com ou youtu.be).";
      errorBox.style.display = "flex";
    }
    return;
  }

  // Hide error & preview while loading
  if (errorBox) errorBox.style.display = "none";
  if (previewBox) previewBox.style.display = "none";

  if (fetchBtn) fetchBtn.disabled = true;
  if (fetchBtnText) fetchBtnText.textContent = "⏳ Analyse en cours...";

  try {
    const res = await fetch("/api/youtube/info", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url })
    });
    const json = await res.json();

    if (!json.success || !json.data) {
      throw new Error(json.message || "Impossible de récupérer les informations de la vidéo.");
    }

    const data = json.data;
    currentYoutubeInfo = data;

    // Populate preview
    const thumbEl = document.getElementById("yt-preview-thumb");
    if (thumbEl) {
      thumbEl.src = data.thumbnail || "";
    }

    const durationEl = document.getElementById("yt-preview-duration");
    if (durationEl) {
      durationEl.textContent = data.duration_formatted || formatDurationSeconds(data.duration_seconds);
    }

    const uploaderEl = document.getElementById("yt-preview-uploader");
    if (uploaderEl) {
      uploaderEl.textContent = data.uploader || "Chaîne YouTube";
    }

    const viewsEl = document.getElementById("yt-preview-views");
    if (viewsEl) {
      viewsEl.textContent = data.view_count_formatted || (data.view_count ? Number(data.view_count).toLocaleString("fr-FR") + " vues" : "");
    }

    const titleEl = document.getElementById("yt-preview-title");
    if (titleEl) {
      titleEl.textContent = data.title;
    }

    const descEl = document.getElementById("yt-preview-desc");
    if (descEl) {
      descEl.textContent = data.description || "Aucune description fournie pour cette vidéo.";
      descEl.classList.remove("expanded");
      isYoutubeDescExpanded = false;
      const toggleBtn = document.getElementById("yt-desc-toggle-btn");
      if (toggleBtn) toggleBtn.textContent = "Lire plus... ▼";
    }

    // Default filename (sanitized title)
    const filenameInput = document.getElementById("yt-filename-input");
    if (filenameInput) {
      filenameInput.value = data.default_filename || "video";
    }

    // Default dest dir according to format
    const destInput = document.getElementById("yt-dest-dir-input");
    if (destInput && (!destInput.value || destInput.value === "/home/chomiam/videos" || destInput.value === "/home/chomiam/musique")) {
      destInput.value = currentYoutubeFormat === "mp3" ? "/home/chomiam/musique" : "/home/chomiam/videos";
    }

    if (previewBox) {
      previewBox.style.display = "block";
      previewBox.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  } catch (err) {
    if (errorBox && errorMsg) {
      errorMsg.textContent = "Erreur : " + (err.message || err);
      errorBox.style.display = "flex";
    }
    showToast("Échec de l'analyse : " + (err.message || err), "error");
  } finally {
    if (fetchBtn) fetchBtn.disabled = false;
    if (fetchBtnText) fetchBtnText.textContent = "🔍 Analyser la vidéo";
  }
}

async function startYoutubeDownload() {
  const urlInput = document.getElementById("yt-url-input");
  const destInput = document.getElementById("yt-dest-dir-input");
  const filenameInput = document.getElementById("yt-filename-input");
  const downloadBtn = document.getElementById("btn-yt-download");
  const downloadBtnText = document.getElementById("btn-yt-download-text");

  const url = urlInput ? urlInput.value.trim() : "";
  if (!url) {
    showToast("Veuillez d'abord spécifier un lien YouTube valide.", "error");
    return;
  }

  const dest_dir = destInput ? destInput.value.trim() : (currentYoutubeFormat === "mp3" ? "/home/chomiam/musique" : "/home/chomiam/videos");
  const custom_name = filenameInput ? filenameInput.value.trim() : "";

  if (downloadBtn) downloadBtn.disabled = true;
  if (downloadBtnText) downloadBtnText.textContent = "⏳ Lancement du téléchargement...";

  try {
    const res = await fetch("/api/youtube/download", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: url,
        format: currentYoutubeFormat || "mp4",
        output_dir: dest_dir,
        filename: custom_name || ""
      })
    });

    if (!res.ok) {
      const errText = await res.text();
      let errMsg = errText;
      try {
        const errJson = JSON.parse(errText);
        if (errJson.message) errMsg = errJson.message;
      } catch (_) {}
      throw new Error(errMsg || `Erreur serveur HTTP ${res.status}`);
    }

    const json = await res.json();
    if (!json.success || !json.data) {
      throw new Error(json.message || "Échec du démarrage du téléchargement.");
    }

    showToast("Téléchargement lancé en tâche de fond !", "success");
    
    // Show jobs container and poll immediately
    const jobsContainer = document.getElementById("yt-jobs-container");
    if (jobsContainer) {
      jobsContainer.style.display = "block";
      jobsContainer.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }

    await pollYoutubeJobs();
    startYoutubeJobsPolling();
  } catch (err) {
    showToast("Erreur de téléchargement : " + (err.message || err), "error");
  } finally {
    if (downloadBtn) downloadBtn.disabled = false;
    if (downloadBtnText) downloadBtnText.textContent = "⬇️ Télécharger sur le NAS (Qualité Maximale)";
  }
}

async function pollYoutubeJobs() {
  try {
    const res = await fetch("/api/youtube/jobs");
    if (!res.ok) return;
    const json = await res.json();
    if (!json.success || !Array.isArray(json.data)) return;

    renderYoutubeJobs(json.data);

    // If no jobs are currently downloading, stop high frequency polling
    const hasActiveJobs = json.data.some(j => j.status === "downloading");
    if (!hasActiveJobs && youtubePollInterval) {
      clearInterval(youtubePollInterval);
      youtubePollInterval = null;
    }
  } catch (err) {
    console.error("Erreur lors de la vérification des téléchargements YouTube :", err);
  }
}

function startYoutubeJobsPolling() {
  if (youtubePollInterval) clearInterval(youtubePollInterval);
  youtubePollInterval = setInterval(pollYoutubeJobs, 2000);
}

function renderYoutubeJobs(jobs) {
  const container = document.getElementById("yt-jobs-container");
  const list = document.getElementById("yt-jobs-list");
  if (!container || !list) return;

  if (jobs.length === 0) {
    container.style.display = "none";
    list.innerHTML = "";
    return;
  }

  container.style.display = "block";

  list.innerHTML = jobs.map(job => {
    let statusBadge = "";
    let progressHtml = "";
    let actionBtn = "";

    const pct = job.progress_percent != null ? job.progress_percent : (job.progress_pct || 0);

    if (job.status === "downloading") {
      statusBadge = `<span class="youtube-job-badge downloading">⏳ En cours (${pct.toFixed(0)}%)</span>`;
      actionBtn = `
        <button type="button" class="btn btn-danger btn-xs" onclick="cancelYoutubeDownload('${job.id}')" title="Annuler ce téléchargement">
          🛑 Annuler
        </button>
      `;
      progressHtml = `
        <div class="youtube-job-progress-wrap">
          <div style="display:flex; justify-content:space-between; font-size:0.72rem; color:var(--subtext0);">
            <span>${job.speed || "Téléchargement..."}</span>
            <span>${job.eta ? "ETA: " + job.eta : ""}</span>
          </div>
          <div class="youtube-progress-bar-bg">
            <div class="youtube-progress-bar-fill" style="width: ${Math.max(4, Math.min(100, pct))}%;"></div>
          </div>
        </div>
      `;
    } else if (job.status === "completed") {
      statusBadge = `<span class="youtube-job-badge completed">✅ Terminé</span>`;
    } else if (job.status === "cancelled") {
      statusBadge = `<span class="youtube-job-badge cancelled">⚠️ Annulé</span>`;
      const targetDir = job.output_dir || "/home/chomiam";
      actionBtn = `
        <button type="button" class="btn btn-secondary btn-xs" onclick="navigateToPath('${targetDir.replace(/'/g, "\'")}')" title="Ouvrir le dossier dans le gestionnaire">
          📂 Voir dossier
        </button>
      `;
    } else {
      const errTxt = job.error_message || job.error || "Erreur inconnue";
      statusBadge = `<span class="youtube-job-badge failed" title="${escapeHtml(errTxt)}">❌ Échec</span>`;
    }

    const fmtIcon = job.format === "mp3" ? "🎵 MP3" : "🎬 MP4";
    const displayDir = job.output_dir || job.output_file || "/home/chomiam";
    const errMsg = job.error_message || job.error;

    return `
      <div class="youtube-job-card">
        <div class="youtube-job-info">
          <div class="youtube-job-title">${escapeHtml(job.title || job.url)}</div>
          <div class="youtube-job-meta">
            <span style="font-weight:700; color:var(--mauve);">${fmtIcon}</span>
            <span>📁 ${escapeHtml(displayDir)}</span>
            ${job.file_size ? `<span>💾 ${escapeHtml(job.file_size)}</span>` : ""}
            ${errMsg ? `<span style="color:var(--red);">⚠️ ${escapeHtml(errMsg)}</span>` : ""}
          </div>
        </div>
        <div style="display:flex; align-items:center; gap:12px;">
          ${progressHtml}
          ${statusBadge}
          ${actionBtn}
        </div>
      </div>
    `;
  }).join("");
}

// Initial check when DOM is ready
document.addEventListener("DOMContentLoaded", () => {
  pollYoutubeJobs();
});

async function cancelYoutubeDownload(jobId) {
  if (!confirm("Voulez-vous vraiment annuler ce téléchargement en cours ?")) {
    return;
  }

  try {
    const res = await fetch(`/api/youtube/cancel/${encodeURIComponent(jobId)}`, {
      method: "POST"
    });
    const json = await res.json();
    if (json.success) {
      showToast("Téléchargement annulé.", "info");
      await pollYoutubeJobs();
    } else {
      showToast(json.message || "Impossible d'annuler le téléchargement.", "error");
    }
  } catch (err) {
    showToast("Erreur lors de l'annulation : " + err, "error");
  }
}

async function clearYoutubeJobsHistory() {
  try {
    const res = await fetch("/api/youtube/clear", {
      method: "POST"
    });
    const json = await res.json();
    if (json.success) {
      showToast("Historique nettoyé.", "info");
      await pollYoutubeJobs();
    }
  } catch (err) {
    showToast("Erreur : " + err, "error");
  }
}




// ==========================================================================
// VISUALISEUR UNIVERSEL DE DOCUMENTS BUREAUTIQUES & PDF
// ==========================================================================
let currentDocBlobUrl = null;
let currentDocAbortController = null;

function getDocumentFormatBadge(fileName) {
  const ext = fileName.split(".").pop().toLowerCase();
  switch (ext) {
    case "pdf": return { label: "PDF", icon: "📕", color: "var(--red)" };
    case "docx":
    case "doc": return { label: "WORD", icon: "📘", color: "var(--blue)" };
    case "odt": return { label: "ODT", icon: "📘", color: "var(--blue)" };
    case "rtf": return { label: "RTF", icon: "📄", color: "var(--subtext0)" };
    case "xlsx":
    case "xls": return { label: "EXCEL", icon: "📊", color: "var(--green)" };
    case "ods": return { label: "ODS", icon: "📊", color: "var(--green)" };
    case "csv": return { label: "CSV", icon: "📊", color: "var(--green)" };
    case "pptx":
    case "ppt": return { label: "POWERPOINT", icon: "📽️", color: "var(--peach)" };
    case "odp": return { label: "ODP", icon: "📽️", color: "var(--peach)" };
    default: return { label: ext.toUpperCase(), icon: "📄", color: "var(--mauve)" };
  }
}

async function openDocModal(path, fileName, item) {
  const modal = document.getElementById("doc-modal");
  const titleEl = document.getElementById("doc-file-title");
  const formatBadge = document.getElementById("doc-format-badge");
  const convBadge = document.getElementById("doc-conversion-badge");
  const sizeEl = document.getElementById("doc-file-size");
  const openTabBtn = document.getElementById("doc-open-tab-btn");
  const downloadOrigBtn = document.getElementById("doc-download-orig-btn");
  const loadingContainer = document.getElementById("doc-loading-container");
  const loadingTitle = document.getElementById("doc-loading-title");
  const loadingDesc = document.getElementById("doc-loading-desc");
  const loadingIcon = document.getElementById("doc-loading-icon");
  const errorContainer = document.getElementById("doc-error-container");
  const errorMessage = document.getElementById("doc-error-message");
  const errorDownloadBtn = document.getElementById("doc-error-download-btn");
  const iframe = document.getElementById("doc-iframe");

  if (!modal) return;

  const fName = fileName || path.split("/").pop();
  const formatInfo = getDocumentFormatBadge(fName);
  const isNativePdf = /\.pdf$/i.test(fName);

  if (titleEl) {
    titleEl.textContent = fName;
    titleEl.title = path;
  }

  if (formatBadge) {
    formatBadge.textContent = `${formatInfo.icon} ${formatInfo.label}`;
    formatBadge.style.color = formatInfo.color;
    formatBadge.style.border = `1px solid ${formatInfo.color}`;
    formatBadge.style.background = `rgba(255,255,255,0.05)`;
  }

  if (convBadge) {
    convBadge.style.display = isNativePdf ? "none" : "inline-block";
  }

  if (sizeEl) {
    sizeEl.textContent = item && item.size_human ? item.size_human : "";
  }

  const directPreviewUrl = `/api/documents/preview?path=${encodeURIComponent(path)}`;
  const directDownloadUrl = `/api/files/download?path=${encodeURIComponent(path)}`;

  if (openTabBtn) openTabBtn.href = directPreviewUrl;
  if (downloadOrigBtn) downloadOrigBtn.href = directDownloadUrl;
  if (errorDownloadBtn) errorDownloadBtn.href = directDownloadUrl;

  // Clean previous state
  if (currentDocBlobUrl) {
    URL.revokeObjectURL(currentDocBlobUrl);
    currentDocBlobUrl = null;
  }
  if (currentDocAbortController) {
    currentDocAbortController.abort();
    currentDocAbortController = null;
  }

  if (iframe) {
    iframe.style.display = "none";
    iframe.src = "about:blank";
  }

  if (errorContainer) errorContainer.style.display = "none";
  if (loadingContainer) loadingContainer.style.display = "flex";

  if (loadingIcon) loadingIcon.textContent = formatInfo.icon;
  if (loadingTitle) {
    loadingTitle.textContent = isNativePdf
      ? "Chargement du document PDF..."
      : `Conversion de ${formatInfo.label} en cours...`;
  }
  if (loadingDesc) {
    loadingDesc.textContent = isNativePdf
      ? "Préparation de la vue haute fidélité..."
      : "LibreOffice génère le rendu PDF avec mise en cache instantanée...";
  }

  modal.style.display = "flex";

  // Fetch document with AbortController
  currentDocAbortController = new AbortController();
  try {
    const res = await fetch(directPreviewUrl, {
      signal: currentDocAbortController.signal
    });

    if (!res.ok) {
      const errText = await res.text();
      throw new Error(errText || `Erreur serveur HTTP ${res.status}`);
    }

    const blob = await res.blob();
    const pdfBlob = new Blob([blob], { type: "application/pdf" });
    currentDocBlobUrl = URL.createObjectURL(pdfBlob);

    if (iframe) {
      iframe.src = currentDocBlobUrl;
      iframe.onload = () => {
        if (loadingContainer) loadingContainer.style.display = "none";
        iframe.style.display = "block";
      };
      setTimeout(() => {
        if (loadingContainer) loadingContainer.style.display = "none";
        iframe.style.display = "block";
      }, 500);
    }
  } catch (err) {
    if (err.name === "AbortError") return;
    if (loadingContainer) loadingContainer.style.display = "none";
    if (errorContainer) {
      errorContainer.style.display = "flex";
      if (errorMessage) {
        errorMessage.textContent = err.message || "Erreur de conversion ou de lecture du fichier.";
      }
    }
  }
}

function closeDocModal() {
  const modal = document.getElementById("doc-modal");
  if (modal) modal.style.display = "none";

  if (currentDocAbortController) {
    currentDocAbortController.abort();
    currentDocAbortController = null;
  }

  if (currentDocBlobUrl) {
    URL.revokeObjectURL(currentDocBlobUrl);
    currentDocBlobUrl = null;
  }

  const iframe = document.getElementById("doc-iframe");
  if (iframe) {
    iframe.src = "about:blank";
    iframe.style.display = "none";
  }
}


// ==========================================================================
// SECTION CONTENEURS DOCKER & BOUTIQUE D'APPLICATIONS (APP STORE)
// ==========================================================================

let activeDockerSubTab = "containers";
let activeStoreCategory = "Tous";
let currentStoreCatalog = null;
let currentViewingContainerName = null;

function switchDockerSubTab(subTab) {
  activeDockerSubTab = subTab;
  const btnContainers = document.getElementById("btn-subtab-containers");
  const btnStore = document.getElementById("btn-subtab-store");
  const paneContainers = document.getElementById("docker-pane-containers");
  const paneStore = document.getElementById("docker-pane-store");

  if (btnContainers && btnStore && paneContainers && paneStore) {
    btnContainers.classList.toggle("active", subTab === "containers");
    btnStore.classList.toggle("active", subTab === "store");
    paneContainers.style.display = subTab === "containers" ? "block" : "none";
    paneStore.style.display = subTab === "store" ? "block" : "none";
  }

  if (subTab === "store" && !currentStoreCatalog) {
    loadDockerStore();
  }
}

async function refreshContainersAndStore(showFeedback = false) {
  try {
    await Promise.all([loadDockerContainers(), loadDockerStore()]);
    if (showFeedback) {
      showToast("Conteneurs et boutique actualisés !", "success");
    }
  } catch (err) {
    console.error("Erreur actualisation conteneurs/store:", err);
  }
}

async function loadDockerContainers() {
  const container = document.getElementById("containers-container");
  const countBadge = document.getElementById("running-containers-count");
  if (!container) return;

  try {
    const res = await fetch("/api/docker/containers");
    const json = await res.json();
    if (!json.success || !json.data) {
      container.innerHTML = `<p style="color:var(--red); font-size:0.9rem;">Erreur lors de la récupération des conteneurs.</p>`;
      return;
    }

    const containers = json.data;
    const runningCount = containers.filter(c => c.is_running).length;
    if (countBadge) countBadge.textContent = runningCount;

    if (containers.length === 0) {
      container.innerHTML = `
        <div style="grid-column: 1/-1; background: var(--surface0); border: 1px dashed var(--surface1); border-radius: 14px; padding: 30px; text-align: center;">
          <div style="font-size: 2.2rem; margin-bottom: 8px;">🐳</div>
          <div style="font-weight: 700; color: var(--text); font-size: 1.05rem;">Aucun conteneur Docker en cours d'exécution</div>
          <div style="font-size: 0.85rem; color: var(--subtext0); margin: 6px 0 16px 0;">Découvrez et installez vos premières applications en 1-clic depuis le Store.</div>
          <button type="button" class="btn btn-primary btn-sm" onclick="switchDockerSubTab('store')">🛍️ Découvrir la Boutique d'Applications</button>
        </div>
      `;
      return;
    }

    const host = window.location.hostname;
    container.innerHTML = containers.map(c => {
      const portLink = c.web_port
        ? `<a href="http://${host}:${c.web_port}" target="_blank" class="store-open-link"><span>🚀</span> Ouvrir (Port ${c.web_port}) ↗</a>`
        : "";

      return `
        <div class="container-card">
          <div class="container-header">
            <span class="container-name">🐳 ${escapeHtml(c.name)}</span>
            <span class="badge ${c.is_running ? 'badge-success' : 'badge-warning'}">
              ${c.is_running ? '🟢 En cours' : '🟡 Arrêté'}
            </span>
          </div>

          <div class="container-image">
            <div style="color:var(--subtext1); font-size:0.75rem; margin-bottom:2px;">Image :</div>
            ${escapeHtml(c.image)}
          </div>

          ${c.ports ? `
            <div class="container-ports-badge">
              <span>🔌</span> <span>${escapeHtml(c.ports)}</span>
            </div>
          ` : ""}

          <div style="font-size:0.75rem; color:var(--subtext0);">
            Statut : <strong>${escapeHtml(c.status)}</strong>
          </div>

          <div class="container-actions-row">
            ${portLink}
            ${c.is_running ? `
              <button type="button" class="btn btn-secondary btn-xs" onclick="dockerContainerAction('${escapeHtml(c.name)}', 'restart')">🔄 Redémarrer</button>
              <button type="button" class="btn btn-secondary btn-xs" onclick="dockerContainerAction('${escapeHtml(c.name)}', 'stop')">⏹ Arrêter</button>
            ` : `
              <button type="button" class="btn btn-success btn-xs" onclick="dockerContainerAction('${escapeHtml(c.name)}', 'start')">▶ Démarrer</button>
            `}
            <button type="button" class="btn btn-secondary btn-xs" onclick="openDockerConfigModalForContainer('${escapeHtml(c.name)}')" title="Modifier les variables du conteneur">⚙️ Variables</button>
            <button type="button" class="btn btn-secondary btn-xs" onclick="openDockerLogsModal('${escapeHtml(c.name)}')">📜 Logs</button>
          </div>
        </div>
      `;
    }).join("");
  } catch (err) {
    console.error("Erreur fetch /api/docker/containers:", err);
    container.innerHTML = `<p style="color:var(--red); font-size:0.9rem;">Erreur de communication avec le démon Docker.</p>`;
  }
}

async function loadDockerStore() {
  const grid = document.getElementById("store-apps-grid");
  const catContainer = document.getElementById("store-categories-container");
  if (!grid) return;

  try {
    const res = await fetch("/api/docker/store");
    const json = await res.json();
    if (!json.success || !json.data) {
      grid.innerHTML = `<p style="color:var(--red); font-size:0.9rem;">Impossible de charger la boutique d'applications.</p>`;
      return;
    }

    currentStoreCatalog = json.data;

    // Rendu des catégories
    if (catContainer && currentStoreCatalog.categories) {
      catContainer.innerHTML = currentStoreCatalog.categories.map(cat => `
        <button type="button" class="store-cat-pill ${cat === activeStoreCategory ? 'active' : ''}" onclick="selectStoreCategory('${escapeHtml(cat)}')">
          ${escapeHtml(cat)}
        </button>
      `).join("");
    }

    filterStoreApps();
  } catch (err) {
    console.error("Erreur fetch /api/docker/store:", err);
    grid.innerHTML = `<p style="color:var(--red); font-size:0.9rem;">Erreur de chargement du catalogue store.</p>`;
  }
}

function selectStoreCategory(cat) {
  activeStoreCategory = cat;
  document.querySelectorAll(".store-cat-pill").forEach(pill => {
    pill.classList.toggle("active", pill.textContent.trim() === cat);
  });
  filterStoreApps();
}

function filterStoreApps() {
  const grid = document.getElementById("store-apps-grid");
  const searchInput = document.getElementById("store-search-input");
  if (!grid || !currentStoreCatalog) return;

  const query = (searchInput ? searchInput.value.trim().toLowerCase() : "");
  const filtered = currentStoreCatalog.apps.filter(app => {
    const matchCat = (activeStoreCategory === "Tous" || app.category === activeStoreCategory);
    const matchQuery = !query || app.name.toLowerCase().includes(query) ||
                       app.tagline.toLowerCase().includes(query) ||
                       app.description.toLowerCase().includes(query) ||
                       app.category.toLowerCase().includes(query);
    return matchCat && matchQuery;
  });

  if (filtered.length === 0) {
    grid.innerHTML = `<p style="grid-column:1/-1; color:var(--subtext0); text-align:center; padding:30px;">Aucune application ne correspond à votre recherche.</p>`;
    return;
  }

  const host = window.location.hostname;
  grid.innerHTML = filtered.map(app => {
    const isInstalled = app.is_installed;
    const isRunning = app.is_running;
    const openLink = (isInstalled && isRunning && app.default_port)
      ? `<a href="http://${host}:${app.default_port}" target="_blank" class="store-open-link"><span>🚀</span> Ouvrir (Port ${app.default_port}) ↗</a>`
      : "";

    return `
      <div class="store-app-card">
        <div class="store-app-top">
          <img src="${escapeHtml(app.icon)}" alt="${escapeHtml(app.name)}" class="store-app-icon-img" onerror="this.src='/favicon.ico';">
          <div class="store-app-title-wrap">
            <div class="store-app-name">
              ${escapeHtml(app.name)}
              ${app.recommended ? `<span class="badge badge-warning" style="font-size:0.68rem; padding:2px 6px;">⭐ Recommandé</span>` : ""}
            </div>
            <span class="store-app-cat-badge">${escapeHtml(app.category)}</span>
          </div>
        </div>

        <div class="store-app-desc">${escapeHtml(app.tagline || app.description)}</div>

        <div class="store-app-meta-row">
          <span>Port : <strong class="store-port-tag">${app.default_port || 'N/A'}</strong></span>
          <span>•</span>
          <span>Données : <code style="color:var(--mauve); font-size:0.75rem;">/home/chomiam/docker/${escapeHtml(app.id)}</code></span>
        </div>

        <div class="store-app-bottom">
          <div class="store-bottom-status-row">
            <div class="store-status-pill ${isInstalled ? (isRunning ? 'status-active' : 'status-stopped') : 'status-available'}">
              ${isInstalled ? (isRunning ? '🟢 Active' : '🟡 Arrêtée') : '⚪ Non installée'}
            </div>
            ${openLink}
          </div>

          <div class="store-bottom-actions-row">
            ${isInstalled ? `
              <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openStoreAppModal('${escapeHtml(app.id)}')">
                <span>ℹ️</span> Détails
              </button>
              <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openDockerConfigModal('${escapeHtml(app.id)}')">
                <span>⚙️</span> Variables
              </button>
              <button type="button" class="btn btn-danger btn-xs store-btn-action" onclick="uninstallStoreApp('${escapeHtml(app.id)}', '${escapeHtml(app.name)}')">
                <span>🗑️</span> Désinstaller
              </button>
            ` : `
              <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openStoreAppModal('${escapeHtml(app.id)}')">
                <span>ℹ️</span> Détails
              </button>
              <button type="button" class="btn btn-success btn-xs store-btn-action store-btn-install" onclick="openDockerConfigModal('${escapeHtml(app.id)}')">
                <span>📥</span> Installer
              </button>
            `}
          </div>
        </div>
      </div>
    `;
  }).join("");
}

function openStoreAppModal(appId) {
  if (!currentStoreCatalog) return;
  const app = currentStoreCatalog.apps.find(a => a.id === appId);
  if (!app) return;

  const modal = document.getElementById("store-app-modal");
  const nameEl = document.getElementById("modal-app-name");
  const catEl = document.getElementById("modal-app-category");
  const iconEl = document.getElementById("modal-app-icon");
  const descEl = document.getElementById("modal-app-desc");
  const portEl = document.getElementById("modal-app-port");
  const dataPathEl = document.getElementById("modal-app-data-path");
  const nixPathEl = document.getElementById("modal-app-nix-path");
  const websiteEl = document.getElementById("modal-app-website");
  const actionsEl = document.getElementById("modal-app-actions");

  if (nameEl) nameEl.textContent = app.name;
  if (catEl) catEl.textContent = `${app.category} • Version ${app.version}`;
  if (iconEl) iconEl.src = app.icon;
  if (descEl) descEl.textContent = app.description;
  if (portEl) portEl.textContent = app.default_port ? `TCP ${app.default_port}` : "Aucun";
  if (dataPathEl) dataPathEl.textContent = `/home/chomiam/docker/${app.id}`;
  if (nixPathEl) nixPathEl.textContent = `/etc/nixos/docker/${app.id}.nix`;
  if (websiteEl) {
    websiteEl.href = app.website || "#";
    websiteEl.style.display = app.website ? "inline-flex" : "none";
  }

  if (actionsEl) {
    const isInstalled = app.is_installed;
    actionsEl.innerHTML = isInstalled ? `
      <button type="button" class="btn btn-secondary btn-sm" onclick="openDockerConfigModal('${app.id}'); closeStoreAppModal();">
        ⚙️ Modifier les variables
      </button>
      <button type="button" class="btn btn-danger btn-sm" onclick="uninstallStoreApp('${app.id}', '${escapeHtml(app.name)}'); closeStoreAppModal();">
        🗑️ Désinstaller du NAS
      </button>
    ` : `
      <button type="button" class="btn btn-success btn-sm" onclick="openDockerConfigModal('${app.id}'); closeStoreAppModal();">
        📥 Installer l'application
      </button>
    `;
  }

  if (modal) modal.style.display = "flex";
}

function closeStoreAppModal() {
  const modal = document.getElementById("store-app-modal");
  if (modal) modal.style.display = "none";
}

const DEFAULT_DOCKER_ENVS = {
  "jellyfin": [
    { key: "TZ", value: "Europe/Paris" },
    { key: "PUID", value: "1000" },
    { key: "PGID", value: "100" },
    { key: "UMASK", value: "002" },
    { key: "JELLYFIN_PublishedServerUrl", value: "" }
  ],
  "arcane": [
    { key: "PORT", value: "3552" },
    { key: "ENCRYPTION_KEY", value: "0c8f24b63e073f21f04431b2bd81f6f65bbf5b2571ccaf9eda3dc5eab3486f85" }
  ],
  "uptime-kuma": [
    { key: "TZ", value: "Europe/Paris" },
    { key: "UPTIME_KUMA_PORT", value: "3001" }
  ],
  "qbittorrent": [
    { key: "TZ", value: "Europe/Paris" },
    { key: "PUID", value: "1000" },
    { key: "PGID", value: "100" },
    { key: "WEBUI_PORT", value: "8085" }
  ],
  "vaultwarden": [
    { key: "WEBSOCKET_ENABLED", value: "true" },
    { key: "SIGNUPS_ALLOWED", value: "true" },
    { key: "ROCKET_PORT", value: "80" }
  ],
  "jellyseerr": [
    { key: "TZ", value: "Europe/Paris" },
    { key: "LOG_LEVEL", value: "debug" },
    { key: "PORT", value: "5055" }
  ],
  "immich": [
    { key: "TZ", value: "Europe/Paris" },
    { key: "DB_HOSTNAME", value: "immich-postgres" },
    { key: "DB_USERNAME", value: "postgres" },
    { key: "DB_DATABASE_NAME", value: "immich" }
  ]
};

function installStoreApp(appId, appName) {
  openDockerConfigModal(appId);
}

function openDockerConfigModal(appId, customData = null) {
  const modal = document.getElementById("docker-config-modal");
  if (!modal) return;

  let app = (currentStoreCatalog && currentStoreCatalog.apps)
    ? currentStoreCatalog.apps.find(a => a.id === appId)
    : null;

  if (!app && customData) {
    app = customData;
  }

  const title = app ? app.name : appId;
  const icon = app && app.icon ? app.icon : "/favicon.ico";
  const defaultPort = app && app.default_port ? app.default_port : "";
  const dataDir = `/home/chomiam/docker/${appId}`;

  document.getElementById("config-app-id").value = appId;
  document.getElementById("config-app-title").textContent = `Configuration : ${title}`;
  document.getElementById("config-app-subtitle").textContent = app && app.tagline ? app.tagline : "Variables d'environnement, port et volumes";
  document.getElementById("config-app-icon").src = icon;
  document.getElementById("config-app-port").value = defaultPort;
  document.getElementById("config-app-data-dir").value = dataDir;
  
  const nixTarget = document.getElementById("config-app-nix-target");
  if (nixTarget) nixTarget.textContent = `/etc/nixos/docker/${appId}.nix`;

  updateConfigUrlPreview();

  // Remplir les variables d'environnement
  const container = document.getElementById("config-env-rows-container");
  if (container) {
    container.innerHTML = "";
    const envs = DEFAULT_DOCKER_ENVS[appId] || [
      { key: "TZ", value: "Europe/Paris" },
      { key: "PUID", value: "1000" },
      { key: "PGID", value: "100" }
    ];
    envs.forEach(e => addDockerConfigEnvRow(e.key, e.value));
  }

  // Configuration médiathèque (Jellyfin / Multimédia)
  const mediaSection = document.getElementById("config-media-section");
  const isMediaApp = (appId === "jellyfin") || (app && app.media_support);
  if (mediaSection) {
    if (isMediaApp) {
      mediaSection.style.display = "flex";
      const mediaInput = document.getElementById("config-app-media-dir");
      if (mediaInput && (!mediaInput.value || mediaInput.dataset.app !== appId)) {
        mediaInput.value = "/home/chomiam/videos";
        mediaInput.dataset.app = appId;
      }
      selectMediaPreset(mediaInput ? mediaInput.value : "/home/chomiam/videos", false);
    } else {
      mediaSection.style.display = "none";
    }
  }

  // Configuration Accélération Matérielle GPU (Jellyfin / Transcodage)
  const gpuSection = document.getElementById("config-gpu-section");
  const isGpuApp = (appId === "jellyfin");
  if (gpuSection) {
    if (isGpuApp) {
      gpuSection.style.display = "flex";
      initGpuSettingsForModal();
    } else {
      gpuSection.style.display = "none";
    }
  }

  modal.style.display = "flex";
}

function openDockerConfigModalForContainer(containerName) {
  let app = (currentStoreCatalog && currentStoreCatalog.apps)
    ? currentStoreCatalog.apps.find(a => a.id === containerName)
    : null;

  if (app) {
    openDockerConfigModal(app.id);
  } else {
    openDockerConfigModal(containerName, {
      id: containerName,
      name: containerName,
      icon: "/favicon.ico",
      default_port: "",
      tagline: `Conteneur actif : ${containerName}`
    });
  }
}

function closeDockerConfigModal() {
  const modal = document.getElementById("docker-config-modal");
  if (modal) modal.style.display = "none";
}


function selectMediaPreset(path, updateInput = true) {
  const mediaInput = document.getElementById("config-app-media-dir");
  if (updateInput && mediaInput) {
    mediaInput.value = path;
  }

  const currentVal = (mediaInput && mediaInput.value.trim()) ? mediaInput.value.trim() : path;

  const btnHome = document.getElementById("btn-media-opt-home");
  const btnStorage = document.getElementById("btn-media-opt-storage");

  if (btnHome) {
    btnHome.classList.toggle("active", currentVal.startsWith("/home"));
  }
  if (btnStorage) {
    btnStorage.classList.toggle("active", currentVal.startsWith("/mnt/storage"));
  }

  updateMediaFolderPreviews();
}

function updateMediaFolderPreviews() {
  const mediaInput = document.getElementById("config-app-media-dir");
  const base = (mediaInput && mediaInput.value.trim()) ? mediaInput.value.trim().replace(/\/+$/, "") : "/home/chomiam/videos";

  const pMovies = document.getElementById("preview-folder-movies");
  const pTv = document.getElementById("preview-folder-tv");
  const pAnims = document.getElementById("preview-folder-anims");

  if (pMovies) pMovies.textContent = `${base}/movies`;
  if (pTv) pTv.textContent = `${base}/tv_shows`;
  if (pAnims) pAnims.textContent = `${base}/anims`;
}


function initGpuSettingsForModal() {
  const profileSelect = document.getElementById("config-app-gpu-profile");
  const deviceInput = document.getElementById("config-app-gpu-device");
  const badge = document.getElementById("config-gpu-status-badge");
  const renderPreview = document.getElementById("config-gpu-rendernode-preview");

  let detectedType = "intel";
  let detectedModel = "Intel Arc A380 (QuickSync / iHD)";
  let renderNode = "/dev/dri/renderD128";
  let devicePath = "/dev/dri:/dev/dri";

  if (window.hardwareData && window.hardwareData.gpu) {
    const g = window.hardwareData.gpu;
    if (g.model && g.model.toLowerCase().includes("nvidia")) {
      detectedType = "nvidia";
      detectedModel = g.model;
      devicePath = "--gpus all";
      renderNode = "/dev/nvidia0";
    } else if (g.model && g.model.toLowerCase().includes("amd")) {
      detectedType = "amd";
      detectedModel = g.model;
      devicePath = "/dev/dri:/dev/dri";
      renderNode = g.render_node || "/dev/dri/renderD128";
    } else if (g.render_node) {
      renderNode = g.render_node;
      devicePath = g.device_path || "/dev/dri:/dev/dri";
      detectedModel = g.model;
    }
  }

  if (profileSelect) profileSelect.value = detectedType;
  if (deviceInput) deviceInput.value = devicePath;
  if (badge) badge.textContent = `Détecté : ${detectedModel}`;
  if (renderPreview) renderPreview.textContent = renderNode || devicePath;
}

function onGpuProfileChange(profile) {
  const deviceInput = document.getElementById("config-app-gpu-device");
  const badge = document.getElementById("config-gpu-status-badge");
  const renderPreview = document.getElementById("config-gpu-rendernode-preview");

  if (profile === "intel") {
    if (deviceInput) deviceInput.value = "/dev/dri:/dev/dri";
    if (badge) badge.textContent = "Profil : Intel QuickSync (iHD / VA-API)";
    if (renderPreview) renderPreview.textContent = "/dev/dri/renderD128";
  } else if (profile === "amd") {
    if (deviceInput) deviceInput.value = "/dev/dri:/dev/dri";
    if (badge) badge.textContent = "Profil : AMD Radeon (VA-API / ROCm)";
    if (renderPreview) renderPreview.textContent = "/dev/dri/renderD128";
  } else if (profile === "nvidia") {
    if (deviceInput) deviceInput.value = "--gpus all";
    if (badge) badge.textContent = "Profil : Nvidia NVENC (CUDA / Toolkit)";
    if (renderPreview) renderPreview.textContent = "/dev/nvidia0 (Container Toolkit)";
  } else if (profile === "none") {
    if (deviceInput) deviceInput.value = "none";
    if (badge) badge.textContent = "Transcodage matériel désactivé (CPU)";
    if (renderPreview) renderPreview.textContent = "Aucun (Software FFmpeg)";
  }
}

function openDockerComposeNvimPreview() {
  const appId = document.getElementById("config-app-id").value.trim() || "service";
  const title = document.getElementById("config-app-title").textContent.replace("Configuration : ", "").trim() || appId;
  const port = document.getElementById("config-app-port").value.trim() || "8080";
  const dataDir = document.getElementById("config-app-data-dir").value.trim() || `/home/chomiam/docker/${appId}`;

  const mediaSection = document.getElementById("config-media-section");
  const isMedia = mediaSection && mediaSection.style.display !== "none";
  const mediaDir = (isMedia && document.getElementById("config-app-media-dir"))
    ? document.getElementById("config-app-media-dir").value.trim()
    : "";

  const gpuSection = document.getElementById("config-gpu-section");
  const isGpu = gpuSection && gpuSection.style.display !== "none";
  const gpuDevice = (isGpu && document.getElementById("config-app-gpu-device"))
    ? document.getElementById("config-app-gpu-device").value.trim()
    : "";

  const envRows = document.querySelectorAll("#config-env-rows-container .docker-env-row");
  const envList = [];
  envRows.forEach(r => {
    const k = r.querySelector(".env-key-input").value.trim();
    const v = r.querySelector(".env-val-input").value.trim();
    if (k) envList.push({ key: k, val: v });
  });

  let image = "ghcr.io/getarcaneapp/arcane:latest";
  if (appId === "jellyfin") image = "lscr.io/linuxserver/jellyfin:latest";
  else if (appId === "immich") image = "ghcr.io/immich-app/immich-server:release";
  else if (appId === "qbittorrent") image = "lscr.io/linuxserver/qbittorrent:latest";
  else if (appId === "jellyseerr") image = "fallenbagel/jellyseerr:latest";
  else if (appId === "vaultwarden") image = "vaultwarden/server:latest";
  else if (appId === "uptime-kuma") image = "louislam/uptime-kuma:latest";

  let composeYaml = `# =========================================================================\n`;
  composeYaml += `# 🐳 STEvE_OS NAS Edition — Configuration Docker Compose\n`;
  composeYaml += `# Application  : ${title} (${appId})\n`;
  composeYaml += `# Port hôte    : ${port}\n`;
  composeYaml += `# Données hôte : ${dataDir}\n`;
  composeYaml += `# Mode         : Lecture seule NVIM (Aperçu déclaratif)\n`;
  composeYaml += `# =========================================================================\n\n`;
  composeYaml += `version: "3.8"\n\n`;
  composeYaml += `services:\n`;
  composeYaml += `  ${appId}:\n`;
  composeYaml += `    image: ${image}\n`;
  composeYaml += `    container_name: ${appId}\n`;
  composeYaml += `    restart: unless-stopped\n`;
  composeYaml += `    ports:\n`;
  composeYaml += `      - "${port}:${port}"\n`;

  if (envList.length > 0) {
    composeYaml += `    environment:\n`;
    envList.forEach(e => {
      composeYaml += `      - ${e.key}=${e.val}\n`;
    });
  }

  composeYaml += `    volumes:\n`;
  if (appId === "jellyfin") {
    composeYaml += `      - ${dataDir}/config:/config\n`;
    composeYaml += `      - ${dataDir}/cache:/cache\n`;
    if (mediaDir) {
      composeYaml += `      - ${mediaDir}/movies:/data/movies\n`;
      composeYaml += `      - ${mediaDir}/tv_shows:/data/tv_shows\n`;
      composeYaml += `      - ${mediaDir}/anims:/data/anims\n`;
      composeYaml += `      - ${mediaDir}:/media\n`;
    }
  } else {
    composeYaml += `      - ${dataDir}/data:/data\n`;
  }

  if (isGpu && gpuDevice && gpuDevice !== "none") {
    if (gpuDevice === "--gpus all" || gpuDevice === "nvidia") {
      composeYaml += `    deploy:\n`;
      composeYaml += `      resources:\n`;
      composeYaml += `        reservations:\n`;
      composeYaml += `          devices:\n`;
      composeYaml += `            - driver: nvidia\n`;
      composeYaml += `              count: all\n`;
      composeYaml += `              capabilities: [gpu, video]\n`;
    } else {
      composeYaml += `    devices:\n`;
      composeYaml += `      - ${gpuDevice} # Accélération matérielle (Transcodage GPU)\n`;
      if (gpuDevice === "/dev/dri:/dev/dri") {
        composeYaml += `      - /dev/dri/renderD128:/dev/dri/renderD128\n`;
      }
    }
  }

  composeYaml += `\n# =========================================================================\n`;
  composeYaml += `# ❄️ Équivalent Déclaration NixOS (/etc/nixos/docker/${appId}.nix)\n`;
  composeYaml += `# =========================================================================\n`;
  composeYaml += `# virtualisation.oci-containers.containers.${appId} = {\n`;
  composeYaml += `#   image = "${image}";\n`;
  composeYaml += `#   autoStart = true;\n`;
  composeYaml += `#   ports = [ "${port}:${port}" ];\n`;
  if (isGpu && gpuDevice && gpuDevice !== "none") {
    composeYaml += `#   extraOptions = [ "${gpuDevice === "--gpus all" ? "--gpus=all" : "--device=" + gpuDevice}" ];\n`;
  }
  composeYaml += `# };\n`;

  openNvimViewerWithContent(
    `docker-compose.yml (${title})`,
    composeYaml,
    "yaml",
    `/home/chomiam/docker/${appId}/docker-compose.yml`
  );
}

function updateConfigUrlPreview() {
  const port = document.getElementById("config-app-port").value.trim();
  const preview = document.getElementById("config-app-url-preview");
  if (preview) {
    preview.textContent = port ? `http://${window.location.hostname}:${port}` : "Non exposé";
  }
}

function addDockerConfigEnvRow(key = '', val = '') {
  const container = document.getElementById("config-env-rows-container");
  if (!container) return;

  const row = document.createElement("div");
  row.className = "docker-env-row";
  row.innerHTML = `
    <input type="text" class="form-input env-key-input" placeholder="NOM_VARIABLE" value="${escapeHtml(key)}" spellcheck="false" autocomplete="off">
    <span class="env-sep">=</span>
    <input type="text" class="form-input env-val-input" placeholder="valeur" value="${escapeHtml(val)}" spellcheck="false" autocomplete="off">
    <button type="button" class="env-delete-btn" onclick="this.closest('.docker-env-row').remove()" title="Supprimer la variable">✕</button>
  `;
  container.appendChild(row);
}

async function submitDockerDeploy() {
  const appId = document.getElementById("config-app-id").value.trim();
  if (!appId) {
    showToast("Identifiant d'application manquant", "error");
    return;
  }

  const portVal = document.getElementById("config-app-port").value.trim();
  const dataDir = document.getElementById("config-app-data-dir").value.trim();

  const envVars = {};
  const rows = document.querySelectorAll("#config-env-rows-container .docker-env-row");
  rows.forEach(r => {
    const k = r.querySelector(".env-key-input").value.trim();
    const v = r.querySelector(".env-val-input").value.trim();
    if (k) {
      envVars[k] = v;
    }
  });

  const btn = document.getElementById("btn-submit-docker-deploy");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "⏳ Déploiement en cours...";
  }

  showToast(`Déploiement de ${appId} en cours...`, "info");
  closeDockerConfigModal();

  try {
    const mediaSection = document.getElementById("config-media-section");
    let mediaDirVal = null;
    if (mediaSection && mediaSection.style.display !== "none") {
      const mInput = document.getElementById("config-app-media-dir");
      if (mInput && mInput.value.trim()) {
        mediaDirVal = mInput.value.trim();
      }
    }

    const gpuSection = document.getElementById("config-gpu-section");
    let gpuDeviceVal = null;
    if (gpuSection && gpuSection.style.display !== "none") {
      const gInput = document.getElementById("config-app-gpu-device");
      if (gInput && gInput.value.trim()) {
        gpuDeviceVal = gInput.value.trim();
      }
    }

    const payload = {
      app_id: appId,
      port: portVal ? parseInt(portVal, 10) : null,
      data_dir: dataDir || null,
      media_dir: mediaDirVal || null,
      gpu_device: gpuDeviceVal || null,
      env_vars: envVars
    };

    const res = await fetch("/api/docker/store/install", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    const json = await res.json();
    if (json.success) {
      showToast(json.data || `Application ${appId} configurée avec succès !`, "success");
      setTimeout(() => refreshContainersAndStore(), 2500);
    } else {
      showToast(`Erreur de déploiement : ${json.message}`, "error");
    }
  } catch (err) {
    showToast(`Erreur requête : ${err}`, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "🚀 Déployer l'application";
    }
  }
}

async function uninstallStoreApp(appId, appName) {
  const deleteData = confirm(`Désinstaller l'application '${appName}' ?\n\nCliquez sur OK pour désinstaller.\n(Vous pourrez choisir à l'étape suivante si vous souhaitez conserver ou effacer les données dans /home/chomiam/docker/${appId}).`);
  if (!deleteData) return;

  const purge = confirm(`Voulez-vous également SUPPRIMER définitivement les données de /home/chomiam/docker/${appId} ?\n\n- Cliquez sur OK pour SUPPRIMER les fichiers.\n- Cliquez sur Annuler pour CONSERVER les données de configuration.`);

  showToast(`Désinstallation de ${appName}...`, "info");
  try {
    const res = await fetch("/api/docker/store/uninstall", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ app_id: appId, delete_data: purge })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || `${appName} a été désinstallée avec succès.`, "success");
      setTimeout(() => refreshContainersAndStore(), 2000);
    } else {
      showToast(`Erreur lors de la désinstallation : ${json.message}`, "error");
    }
  } catch (err) {
    showToast(`Erreur de requête : ${err}`, "error");
  }
}

async function dockerContainerAction(name, action) {
  showToast(`Exécution de '${action}' sur '${name}'...`, "info");
  try {
    const res = await fetch(`/api/docker/containers/${encodeURIComponent(name)}/action`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || `Action appliquée sur ${name}`, "success");
      setTimeout(() => loadDockerContainers(), 1000);
    } else {
      showToast(`Erreur : ${json.message}`, "error");
    }
  } catch (err) {
    showToast(`Erreur requête : ${err}`, "error");
  }
}

async function openDockerLogsModal(name) {
  currentViewingContainerName = name;
  const modal = document.getElementById("docker-logs-modal");
  const titleEl = document.getElementById("modal-logs-title");
  const contentEl = document.getElementById("docker-logs-content");

  if (titleEl) titleEl.textContent = `Journaux : ${name}`;
  if (contentEl) contentEl.textContent = "Chargement des journaux...";
  if (modal) modal.style.display = "flex";

  await refreshCurrentDockerLogs();
}

async function refreshCurrentDockerLogs() {
  if (!currentViewingContainerName) return;
  const contentEl = document.getElementById("docker-logs-content");
  try {
    const res = await fetch(`/api/docker/containers/${encodeURIComponent(currentViewingContainerName)}/logs`);
    const json = await res.json();
    if (json.success && json.data) {
      if (contentEl) {
        contentEl.textContent = json.data;
        contentEl.scrollTop = contentEl.scrollHeight;
      }
    } else {
      if (contentEl) contentEl.textContent = json.message || "Aucun journal disponible.";
    }
  } catch (err) {
    if (contentEl) contentEl.textContent = `Erreur de chargement des journaux : ${err}`;
  }
}

function closeDockerLogsModal() {
  const modal = document.getElementById("docker-logs-modal");
  if (modal) modal.style.display = "none";
  currentViewingContainerName = null;
}


// ==========================================================================
// MODULE AUTHENTIFICATION & SESSIONS
// ==========================================================================

async function checkAuthSession() {
  const token = getAuthToken();
  if (!token) {
    updateUserSessionUI(null);
    showLoginModal();
    return;
  }

  try {
    const res = await originalFetch("/api/auth/me", {
      headers: { "Authorization": `Bearer ${token}` }
    });
    if (res.ok) {
      const data = await res.json();
      if (data.authenticated) {
        currentUserSession = data;
        updateUserSessionUI(data);
        hideLoginModal();
        if (!isAppInitialized) {
          isAppInitialized = true;
          initApp();
          setupPolling();
        }
        return;
      }
    }
  } catch (err) {
    console.warn("Erreur vérification session :", err);
  }

  clearAuthToken();
  updateUserSessionUI(null);
  showLoginModal();
}

function showLoginModal() {
  const modal = document.getElementById("login-modal");
  if (modal) {
    modal.style.display = "flex";
    const pwdInput = document.getElementById("login-password");
    if (pwdInput) pwdInput.value = "";
    const errBox = document.getElementById("login-error-box");
    if (errBox) errBox.style.display = "none";
    const userField = document.getElementById("login-username");
    if (userField && !userField.value) {
      userField.focus();
    } else if (pwdInput) {
      pwdInput.focus();
    }
  }
}

function hideLoginModal() {
  const modal = document.getElementById("login-modal");
  if (modal) {
    modal.style.display = "none";
  }
}

function updateUserSessionUI(session) {
  const userPill = document.getElementById("header-user-pill");
  const usernameEl = document.getElementById("header-username");
  const userRoleEl = document.getElementById("header-user-role");

  if (session && session.username) {
    if (userPill) userPill.style.display = "flex";
    if (usernameEl) usernameEl.textContent = session.username;
    if (userRoleEl) userRoleEl.textContent = session.is_admin ? "(Admin)" : "(Utilisateur)";
  } else {
    if (userPill) userPill.style.display = "none";
  }
}

async function handleLoginSubmit(event) {
  if (event) event.preventDefault();
  const usernameInput = document.getElementById("login-username");
  const passwordInput = document.getElementById("login-password");
  const rememberInput = document.getElementById("login-remember");
  const errorBox = document.getElementById("login-error-box");
  const errorMsg = document.getElementById("login-error-msg");
  const submitBtn = document.getElementById("login-submit-btn");

  const username = usernameInput ? usernameInput.value.trim() : "";
  const password = passwordInput ? passwordInput.value : "";
  const remember = rememberInput ? rememberInput.checked : false;

  if (!username || !password) {
    if (errorBox && errorMsg) {
      errorMsg.textContent = "Veuillez renseigner le nom d'utilisateur et le mot de passe.";
      errorBox.style.display = "flex";
    }
    return;
  }

  if (submitBtn) {
    submitBtn.disabled = true;
    submitBtn.innerHTML = `<span>⏳ Connexion...</span>`;
  }
  if (errorBox) errorBox.style.display = "none";

  try {
    const res = await originalFetch("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        username: username,
        password: password,
        remember_me: remember
      })
    });

    const data = await res.json();

    if (res.ok && data.success && data.token) {
      setAuthToken(data.token, remember);
      currentUserSession = {
        username: data.username,
        is_admin: data.is_admin
      };
      updateUserSessionUI(currentUserSession);
      hideLoginModal();

      if (!isAppInitialized) {
        isAppInitialized = true;
        initApp();
        setupPolling();
      }

      showToast(`Bienvenue sur STEvE_OS, ${data.username} !`, "success");
    } else {
      if (errorBox && errorMsg) {
        errorMsg.textContent = data.message || "Identifiants invalides ou accès refusé.";
        errorBox.style.display = "flex";
      }
    }
  } catch (err) {
    if (errorBox && errorMsg) {
      errorMsg.textContent = `Erreur de communication : ${err.message || err}`;
      errorBox.style.display = "flex";
    }
  } finally {
    if (submitBtn) {
      submitBtn.disabled = false;
      submitBtn.innerHTML = `<span>Se connecter</span>`;
    }
  }
}

async function logoutUser() {
  const token = getAuthToken();
  if (token) {
    try {
      await originalFetch("/api/auth/logout", {
        method: "POST",
        headers: {
          "Authorization": `Bearer ${token}`
        }
      });
    } catch (_) {}
  }

  clearAuthToken();
  updateUserSessionUI(null);
  showLoginModal();
  showToast("Vous avez été déconnecté.", "info");
}

function togglePasswordVisibility() {
  const pwdInput = document.getElementById("login-password");
  const toggleBtn = document.getElementById("login-pwd-toggle");
  if (!pwdInput) return;

  if (pwdInput.type === "password") {
    pwdInput.type = "text";
    if (toggleBtn) toggleBtn.textContent = "🙈";
  } else {
    pwdInput.type = "password";
    if (toggleBtn) toggleBtn.textContent = "👁️";
  }
}


// --------------------------------------------------------------------------
// GESTION DU RÉSEAU (VPN, PARE-FEU, SAMBA, SFTP)
// --------------------------------------------------------------------------
let activeNetworkSubtab = "subtab-vpn";
let cachedNetworkData = null;
window.allFirewallPorts = [];

function switchNetworkSubtab(subtabId) {
  activeNetworkSubtab = subtabId;
  document.querySelectorAll(".network-subtab-btn").forEach(btn => {
    btn.classList.toggle("active", btn.getAttribute("data-subtab") === subtabId);
  });
  document.querySelectorAll(".network-subpane").forEach(pane => {
    pane.classList.toggle("active", pane.id === subtabId);
  });
}

async function loadNetwork(showFeedback = false) {
  try {
    const res = await fetch("/api/network");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const net = json.data;
    cachedNetworkData = net;

    // --- 1. Adresses IP & Hôte ---
    const lanIp = net.primary_lan_ip || "127.0.0.1";
    const user = currentUser || "chomiam";

    // Mettre à jour les URI directes
    const sftpUri = `sftp://${user}@${lanIp}:22`;
    const sftpEl = document.getElementById("sftp-connection-uri");
    if (sftpEl) sftpEl.textContent = sftpUri;

    const sambaWin = document.getElementById("samba-uri-windows");
    if (sambaWin) sambaWin.textContent = `\\\\${lanIp}`;

    const sambaMac = document.getElementById("samba-uri-mac");
    if (sambaMac) sambaMac.textContent = `smb://${lanIp}`;

    document.querySelectorAll(".sftp-host-code").forEach(el => {
      el.textContent = lanIp;
    });

    const sftpCmd = document.getElementById("sftp-terminal-cmd");
    if (sftpCmd) sftpCmd.textContent = `sftp ${user}@${lanIp}`;

    // --- 2. VPN (WireGuard Exclusif) ---
    const wg = net.vpn.wireguard;

    const wgBadge = document.getElementById("wg-status-badge");
    if (wgBadge) {
      wgBadge.textContent = wg.is_active ? "🟢 Actif" : "⚪ Inactif";
      wgBadge.className = `badge ${wg.is_active ? "badge-success" : "badge-secondary"}`;
    }

    const wgIface = document.getElementById("wg-interface");
    if (wgIface) wgIface.textContent = wg.interface || "wg0";

    const wgPort = document.getElementById("wg-port");
    if (wgPort) wgPort.textContent = wg.port || 51820;

    const wgSubnet = document.getElementById("wg-subnet");
    if (wgSubnet) wgSubnet.textContent = wg.subnet || "10.100.0.1/24";

    const wgPeers = document.getElementById("wg-peers-count");
    if (wgPeers) wgPeers.textContent = `${wg.peers_count} pair(s) connecté(s)`;

    const wgTraffic = document.getElementById("wg-traffic");
    if (wgTraffic) {
      if (wg.transfer_rx || wg.transfer_tx) {
        wgTraffic.textContent = `RX: ${wg.transfer_rx || "0 B"} | TX: ${wg.transfer_tx || "0 B"}`;
      } else {
        wgTraffic.textContent = "0 B / 0 B";
      }
    }

    const wgPubkey = document.getElementById("wg-pubkey");
    if (wgPubkey) wgPubkey.textContent = wg.public_key || "Générée automatiquement par NixOS";

    // Tableau des pairs WireGuard
    const wgPeersTbody = document.getElementById("wg-peers-tbody");
    if (wgPeersTbody) {
      if (!wg.peers || wg.peers.length === 0) {
        wgPeersTbody.innerHTML = `<tr><td colspan="5" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun client WireGuard actif pour le moment.</td></tr>`;
      } else {
        wgPeersTbody.innerHTML = wg.peers.map(p => `
          <tr>
            <td><code style="font-size:0.75rem; color:var(--teal);">${escapeHtml(p.public_key.substring(0, 16))}...</code></td>
            <td><span class="badge badge-info">${escapeHtml(p.allowed_ips)}</span></td>
            <td>${p.endpoint ? `<code>${escapeHtml(p.endpoint)}</code>` : '<span style="color:var(--subtext0);">Non connecté</span>'}</td>
            <td>${p.latest_handshake ? escapeHtml(p.latest_handshake) : '<span style="color:var(--subtext0);">Jamais</span>'}</td>
            <td>${p.transfer_rx || p.transfer_tx ? `${p.transfer_rx || '0 B'} / ${p.transfer_tx || '0 B'}` : '0 B / 0 B'}</td>
          </tr>
        `).join("");
      }
    }

    // Badge de la sous-navigation
    const badgeVpn = document.getElementById("badge-subtab-vpn");
    if (badgeVpn) {
      badgeVpn.textContent = wg.is_active ? "WireGuard Actif" : "Inactif";
      badgeVpn.className = `subtab-pill-badge ${wg.is_active ? "badge-success" : "badge-secondary"}`;
    }

    // --- 3. Pare-feu & Fail2ban ---
    const fw = net.firewall;
    const fwBadge = document.getElementById("firewall-status-badge");
    if (fwBadge) {
      fwBadge.textContent = fw.is_enabled ? "🟢 Pare-feu Actif" : "🔴 Pare-feu Désactivé";
      fwBadge.className = `badge ${fw.is_enabled ? "badge-success" : "badge-danger"}`;
    }

    const badgeFw = document.getElementById("badge-subtab-firewall");
    if (badgeFw) {
      badgeFw.textContent = fw.is_enabled ? "Actif" : "Désactivé";
      badgeFw.className = `subtab-pill-badge ${fw.is_enabled ? "badge-success" : "badge-danger"}`;
    }

    window.allFirewallPorts = [...fw.tcp_ports, ...fw.udp_ports];
    renderFirewallPorts(window.allFirewallPorts);

    // Fail2ban
    const bannedWrap = document.getElementById("banned-ips-list");
    if (bannedWrap) {
      if (!fw.banned_ips || fw.banned_ips.length === 0) {
        bannedWrap.innerHTML = `<span style="color:var(--green); font-size:0.85rem;">✔ Aucune adresse IP actuellement bannie par SSH. Système sain et sécurisé.</span>`;
      } else {
        bannedWrap.innerHTML = fw.banned_ips.map(ip => `
          <div style="background:var(--mantle); border:1px solid rgba(243,139,168,0.3); padding:4px 10px; border-radius:var(--radius-md); display:inline-flex; align-items:center; gap:8px;">
            <span class="badge badge-danger">${escapeHtml(ip)}</span>
            <button type="button" class="unban-btn" onclick="unbanFirewallIp('${escapeHtml(ip)}')">Débannir</button>
          </div>
        `).join("");
      }
    }

    // --- 4. Samba (SMB) ---
    const samba = net.samba;
    const badgeSamba = document.getElementById("badge-subtab-samba");
    if (badgeSamba) {
      badgeSamba.textContent = samba.is_active ? "Actif" : "Inactif";
      badgeSamba.className = `subtab-pill-badge ${samba.is_active ? "badge-success" : "badge-secondary"}`;
    }

    const smbSessionsTbody = document.getElementById("samba-sessions-tbody");
    if (smbSessionsTbody) {
      if (!samba.active_sessions || samba.active_sessions.length === 0) {
        smbSessionsTbody.innerHTML = `<tr><td colspan="4" style="text-align:center; color:var(--subtext0); padding:16px;">Aucune session Samba active actuellement.</td></tr>`;
      } else {
        smbSessionsTbody.innerHTML = samba.active_sessions.map(s => `
          <tr>
            <td><strong>${escapeHtml(s.user)}</strong></td>
            <td><code>${escapeHtml(s.client_ip)}</code></td>
            <td><span class="badge badge-info">${escapeHtml(s.protocol)}</span></td>
            <td><span class="badge badge-success">${escapeHtml(s.login_time)}</span></td>
          </tr>
        `).join("");
      }
    }

    // --- 5. sFTP (SSH) ---
    const sftp = net.sftp;
    const badgeSftp = document.getElementById("badge-subtab-sftp");
    if (badgeSftp) {
      badgeSftp.textContent = sftp.is_active ? "Port 22" : "Inactif";
      badgeSftp.className = `subtab-pill-badge ${sftp.is_active ? "badge-success" : "badge-secondary"}`;
    }

    const sftpSessionsTbody = document.getElementById("sftp-sessions-tbody");
    if (sftpSessionsTbody) {
      if (!sftp.active_sessions || sftp.active_sessions.length === 0) {
        sftpSessionsTbody.innerHTML = `<tr><td colspan="4" style="text-align:center; color:var(--subtext0); padding:16px;">Aucune session SSH / sFTP active actuellement.</td></tr>`;
      } else {
        sftpSessionsTbody.innerHTML = sftp.active_sessions.map(s => `
          <tr>
            <td><strong>${escapeHtml(s.user)}</strong></td>
            <td><code>${escapeHtml(s.client_ip)}</code></td>
            <td><span class="badge badge-info">${escapeHtml(s.protocol)}</span></td>
            <td><span class="badge badge-success">${escapeHtml(s.login_time)}</span></td>
          </tr>
        `).join("");
      }
    }

    if (showFeedback) {
      showToast("Données réseau actualisées !", "success");
    }
  } catch (err) {
    console.warn("Erreur fetch /api/network:", err);
    if (showFeedback) {
      showToast("Erreur lors de l'actualisation du réseau : " + err, "error");
    }
  }
}

function renderFirewallPorts(ports) {
  const tbody = document.getElementById("firewall-tbody");
  if (!tbody) return;

  if (ports.length === 0) {
    tbody.innerHTML = `<tr><td colspan="4" style="text-align:center; color:var(--subtext0); padding:16px;">Aucun port correspondant.</td></tr>`;
    return;
  }

  tbody.innerHTML = ports.map(p => `
    <tr>
      <td><strong style="font-family:var(--font-mono); color:var(--mauve);">${p.port}</strong></td>
      <td><span class="badge badge-info">${escapeHtml(p.protocol)}</span></td>
      <td>${escapeHtml(p.service_name)}</td>
      <td><span class="badge badge-success">🟢 ${escapeHtml(p.status)}</span></td>
    </tr>
  `).join("");
}

function filterFirewallPorts() {
  const input = document.getElementById("firewall-search-input");
  const query = input ? input.value.trim().toLowerCase() : "";
  if (!query) {
    renderFirewallPorts(window.allFirewallPorts || []);
    return;
  }

  const filtered = (window.allFirewallPorts || []).filter(p => {
    return p.port.toString().includes(query) ||
           p.protocol.toLowerCase().includes(query) ||
           p.service_name.toLowerCase().includes(query);
  });
  renderFirewallPorts(filtered);
}

async function unbanFirewallIp(ip) {
  if (!confirm(`Confirmer le déblocage immédiat de l'adresse IP ${ip} ?`)) return;

  try {
    const res = await fetch("/api/firewall/unban", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ip })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || `IP ${ip} débloquée avec succès !`, "success");
      loadNetwork();
    } else {
      showToast("Erreur lors du déblocage : " + (json.message || "Échec"), "error");
    }
  } catch (e) {
    showToast("Erreur : " + e, "error");
  }
}

async function restartServiceAction(unitName) {
  try {
    showToast(`Redémarrage de ${unitName} en cours...`, "info");
    const res = await fetch(`/api/service/${unitName}/restart`, { method: "POST" });
    const json = await res.json();
    if (json.success) {
      showToast(`Service ${unitName} redémarré avec succès !`, "success");
      loadNetwork();
    } else {
      showToast("Échec : " + (json.message || "Erreur"), "error");
    }
  } catch (e) {
    showToast("Erreur d'action : " + e, "error");
  }
}

async function toggleServiceAction(unitName) {
  try {
    const isCurrentlyActive = cachedNetworkData?.vpn?.wireguard?.is_active ?? false;
    const action = isCurrentlyActive ? "stop" : "start";
    showToast(`${action === "start" ? "Démarrage" : "Arrêt"} de ${unitName}...`, "info");

    const res = await fetch(`/api/service/${unitName}/${action}`, { method: "POST" });
    const json = await res.json();
    if (json.success) {
      showToast(`Action '${action}' appliquée sur ${unitName} !`, "success");
      loadNetwork();
    } else {
      showToast("Échec : " + (json.message || "Erreur"), "error");
    }
  } catch (e) {
    showToast("Erreur : " + e, "error");
  }
}

function copyElementText(elementId, successMsg) {
  const el = document.getElementById(elementId);
  if (!el) return;
  const text = el.textContent || el.innerText;
  if (!text) return;

  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(() => {
      showToast(successMsg || "Texte copié dans le presse-papiers !", "success");
    }).catch(() => {
      prompt("Copiez manuellement l'adresse :", text);
    });
  } else {
    prompt("Copiez manuellement l'adresse :", text);
  }
}
