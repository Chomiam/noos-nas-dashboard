// ==========================================================================
// STEvE_OS NAS Edition — Application Client (Vanilla JavaScript)
// ==========================================================================

let activeTab = "tab-overview";
let lastUpdateStatus = null;
let isUpdatingNow = false;

document.addEventListener("DOMContentLoaded", () => {
  initApp();
  setupPolling();
});

function initApp() {
  refreshAll(false);
  updateSftpUri();
  checkForUpdates(false);
}

function setupPolling() {
  // Rafraîchissement fréquent des métriques CPU/RAM/Temp (3 secondes)
  setInterval(() => {
    if (activeTab === "tab-overview") {
      loadSystem();
    }
  }, 3000);

  // Rafraîchissement périodique des services et stockage (12 secondes)
  setInterval(() => {
    if (activeTab === "tab-storage") loadStorage();
    if (activeTab === "tab-shares") loadServices();
    if (activeTab === "tab-containers") loadServices();
  }, 12000);

  // Vérification périodique des mises à jour en arrière-plan (30 secondes)
  setInterval(() => {
    checkForUpdates(false);
  }, 30000);
}

// --------------------------------------------------------------------------
// GESTION DES ONGLETS
// --------------------------------------------------------------------------
function switchTab(tabId) {
  activeTab = tabId;
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
  if (tabId === "tab-shares") loadServices();
  if (tabId === "tab-gpu") loadGpu();
  if (tabId === "tab-containers") loadServices();
  if (tabId === "tab-firewall") loadFirewall();
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
      loadGpu(),
      loadFirewall(),
      checkForUpdates(false)
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
  try {
    const res = await fetch(`/api/updates/status${force ? '?force=true' : ''}`);
    const json = await res.json();
    if (!json.success || !json.data) return;

    lastUpdateStatus = json.data;
    renderUpdatesUI(lastUpdateStatus);
  } catch (err) {
    console.warn("Erreur fetch /api/updates/status:", err);
  }
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

  try {
    const res = await fetch("/api/updates/apply", { method: "POST" });
    const json = await res.json();
    const result = json.data || {};

    if (termBody) {
      termBody.textContent = result.output_log || "Aucun log retourné.";
    }

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
    if (termBody) termBody.textContent += "\nErreur réseau lors de l'appel API : " + err;
    if (termStatus) {
      termStatus.className = "badge badge-danger";
      termStatus.textContent = "❌ Erreur réseau";
    }
    showToast("Erreur de connexion : " + err, "error");
  } finally {
    isUpdatingNow = false;
    await checkForUpdates(true);
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

  try {
    const res = await fetch("/api/updates/apply?force_packages=true", { method: "POST" });
    const json = await res.json();
    const result = json.data || {};

    if (termBody) {
      termBody.textContent = result.output_log || "Aucun log retourné.";
    }

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
    if (termBody) termBody.textContent += "\nErreur réseau : " + err;
    showToast("Erreur réseau : " + err, "error");
  } finally {
    isUpdatingNow = false;
    await checkForUpdates(true);
  }
}

function clearUpdateTerminal() {
  const termBody = document.getElementById("update-terminal-body");
  const termStatus = document.getElementById("terminal-update-status");
  if (termBody) termBody.textContent = "Console prête.";
  if (termStatus) termStatus.style.display = "none";
}

// --------------------------------------------------------------------------
// STOCKAGE & DISQUES
// --------------------------------------------------------------------------
async function loadStorage() {
  try {
    const res = await fetch("/api/storage");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const data = json.data;

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

    // Pools Grid
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
          <div style="font-size:0.78rem; color:var(--subtext0); margin-top:8px;">${p.free_human} disponibles</div>
        </div>
      `).join("");
    }

    // Disks Grid
    const disksContainer = document.getElementById("disks-container");
    if (disksContainer) {
      disksContainer.innerHTML = data.physical_disks.map(d => {
        const isStandby = d.power_state.toLowerCase().includes("veille") || d.power_state.toLowerCase().includes("standby");
        const stateBadge = isStandby 
          ? `<span class="badge badge-warning">🌙 ${escapeHtml(d.power_state)}</span>`
          : `<span class="badge badge-success">⚡ ${escapeHtml(d.power_state)}</span>`;

        const spindownBtn = d.is_rotational
          ? `<button type="button" class="btn btn-secondary btn-xs" onclick="triggerSpindown('${escapeHtml(d.name)}')">Mettre en veille</button>`
          : `<span class="badge badge-info">SSD (Sans moteur)</span>`;

        return `
          <div class="disk-card">
            <div class="disk-header">
              <span class="disk-name">💿 /dev/${escapeHtml(d.name)}</span>
              ${stateBadge}
            </div>
            <div class="disk-model">${escapeHtml(d.model)}</div>
            <div class="disk-meta-grid">
              <div>Capacité : <strong>${d.size_human}</strong></div>
              <div>Type : <strong>${d.is_rotational ? 'HDD Mécanique' : 'SSD Flash'}</strong></div>
              <div>Température : <strong style="color:${d.temperature_c > 45 ? 'var(--red)' : 'var(--green)'};">${d.temperature_c > 0 ? d.temperature_c + ' °C' : 'N/A'}</strong></div>
              <div>Santé S.M.A.R.T : <strong style="color:var(--green);">${escapeHtml(d.smart_status)}</strong></div>
            </div>
            <div class="disk-actions">
              ${spindownBtn}
            </div>
          </div>
        `;
      }).join("");
    }
  } catch (err) {
    console.warn("Erreur fetch /api/storage:", err);
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
// GPU & TRANSCODAGE
// --------------------------------------------------------------------------
async function loadGpu() {
  try {
    const res = await fetch("/api/gpu");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const gpu = json.data;

    const vendorBadge = document.getElementById("gpu-vendor-badge");
    if (vendorBadge) vendorBadge.textContent = gpu.vendor;

    const modelTitle = document.getElementById("gpu-model-title");
    if (modelTitle) modelTitle.textContent = gpu.model_name;

    const driverInfo = document.getElementById("gpu-driver-info");
    if (driverInfo) {
      driverInfo.textContent = `Pilote: ${gpu.driver} • Node: ${gpu.render_node || '/dev/dri/renderD128'} • ${gpu.dri_available ? 'Accélération matérielle active' : 'Accélération non disponible'}`;
    }

    const codecsList = document.getElementById("gpu-codecs-list");
    if (codecsList) {
      codecsList.innerHTML = gpu.hardware_codecs_supported.map(c => `
        <span class="badge badge-mauve" style="font-size:0.82rem; padding:6px 12px;">✔ ${escapeHtml(c)}</span>
      `).join("");
    }
  } catch (err) {
    console.warn("Erreur fetch /api/gpu:", err);
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
      outHtml += `<div class="term-stdout">${escapeHtml(res.stdout)}</div>`;
    }
    if (res.stderr) {
      outHtml += `<div class="term-stderr">${escapeHtml(res.stderr)}</div>`;
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
let currentFolderParent = null;
let currentEntries = [];
let fileViewMode = "grid";
let fileClipboard = null; // { action: 'copy' | 'cut', path: string, name: string }
let selectedFileItem = null;

async function navigateToPath(targetPath) {
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
  const mapping = {
    "/home/chomiam": "fnav-home",
    "/home/chomiam/Documents": "fnav-docs",
    "/home/chomiam/Images": "fnav-pics",
    "/home/chomiam/Vidéos": "fnav-vids",
    "/home/chomiam/Musique": "fnav-music",
    "/home/chomiam/Téléchargements": "fnav-dl",
    "/": "fnav-root",
    "/mnt/storage/shares": "fnav-shares",
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
    return `
      <div class="file-card" 
           data-path="${escapeHtml(item.path)}"
           onclick="handleFileClick(event, '${escapeHtml(item.path)}', ${item.is_dir})"
           ondblclick="handleFileDblClick('${escapeHtml(item.path)}', ${item.is_dir})"
           oncontextmenu="handleItemContextMenu(event, '${escapeHtml(item.path)}')">
        <div class="file-card-icon">${icon}</div>
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

function getFileIcon(item) {
  if (item.is_dir) return "📁";
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

function handleFileDblClick(path, isDir) {
  if (isDir) {
    navigateToPath(path);
  } else {
    showToast(`Fichier : ${path}`, "info");
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

  selectedFileItem = currentEntries.find(i => i.path === path) || null;

  const menu = document.getElementById("files-context-menu");
  if (!menu) return;

  // Activer / désactiver les options
  const ctxOpen = document.getElementById("ctx-open");
  const ctxPaste = document.getElementById("ctx-paste");

  if (ctxOpen) ctxOpen.style.display = selectedFileItem && selectedFileItem.is_dir ? "flex" : "none";
  if (ctxPaste) ctxPaste.classList.toggle("disabled", !fileClipboard);

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
      confirmDelete(selectedFileItem);
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

async function confirmDelete(item) {
  const isDir = item.is_dir;
  const msg = isDir 
    ? `Êtes-vous sûr de vouloir supprimer définitivement le dossier "${item.name}" et tout son contenu ?`
    : `Êtes-vous sûr de vouloir supprimer définitivement le fichier "${item.name}" ?`;

  if (!confirm(msg)) return;

  try {
    const res = await fetch("/api/files/delete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path: item.path })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Suppression effectuée", "success");
      refreshCurrentFolder();
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
