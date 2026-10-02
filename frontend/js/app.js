// ==========================================================================
// Noos NAS Edition — Application Client (Vanilla JavaScript)
// ==========================================================================

const AUTH_TOKEN_KEY = "noos_auth_token";
let currentUserSession = null;
let isAppInitialized = false;
let currentUserHome = "";

function getCurrentDashboardUsername() {
  if (currentUserSession && currentUserSession.username && currentUserSession.username.trim()) {
    return currentUserSession.username.trim();
  }
  const headerUser = document.getElementById("header-username");
  if (headerUser && headerUser.textContent && headerUser.textContent.trim()) {
    return headerUser.textContent.trim();
  }
  return "noos";
}

function updateSftpQuickUrisWithUser(username) {
  if (!username || username === "root") return;
  const uriClient = document.getElementById("sftp-uri-val-client");
  const uriCli = document.getElementById("sftp-uri-val-cli");
  const uriSshfs = document.getElementById("sftp-uri-val-sshfs");
  const ip = (typeof currentSftpData !== "undefined" && currentSftpData?.primary_lan_ip) || window.location.hostname;
  const port = (typeof currentSftpData !== "undefined" && currentSftpData?.port) || 22;

  if (uriClient) uriClient.textContent = `sftp://${username}@${ip}:${port}`;
  if (uriCli) uriCli.textContent = `sftp -P ${port} ${username}@${ip}`;
  if (uriSshfs) uriSshfs.textContent = `sshfs -p ${port} ${username}@${ip}:/ /mnt/nas`;

  const oldSftpUri = document.getElementById("sftp-connection-uri");
  if (oldSftpUri) oldSftpUri.textContent = `sftp://${username}@${ip}:${port}`;
}

function getUserHome() {
  if (currentUserHome && currentUserHome !== "/root") {
    return currentUserHome;
  }
  if (currentUserSession && currentUserSession.home_dir && currentUserSession.home_dir !== "/root") {
    currentUserHome = currentUserSession.home_dir;
    return currentUserHome;
  }
  const u = (currentUserSession && currentUserSession.username) || "noos";
  return `/home/${u}`;
}

function navigateToUserFolder(sub) {
  const base = getUserHome();
  const target = sub ? `${base}/${sub}` : base;
  navigateToPath(target);
}

function getAuthToken() {
  try {
    const urlParams = new URLSearchParams(window.location.search);
    const qToken = urlParams.get("token") || urlParams.get("auth_token");
    if (qToken) {
      localStorage.setItem(AUTH_TOKEN_KEY, qToken);
      sessionStorage.setItem(AUTH_TOKEN_KEY, qToken);
            document.cookie = `noos_token=${qToken}; path=/; max-age=604800; SameSite=Lax`;
            try {
        const cleanUrl = window.location.pathname + (window.location.hash || "");
        window.history.replaceState({}, document.title, cleanUrl);
      } catch (e) {}
      return qToken;
    }
  } catch (e) {}
  const token = sessionStorage.getItem(AUTH_TOKEN_KEY) 
    || localStorage.getItem(AUTH_TOKEN_KEY)
;
  if (token) {
    try {
      if (!document.cookie.includes("noos_token=")) {
        document.cookie = `noos_token=${token}; path=/; max-age=604800; SameSite=Lax`;
      }

    } catch (e) {}
  }
  return token;
}

function buildAuthenticatedUrl(endpoint, extraParams = {}) {
  const token = getAuthToken();
  const baseOrigin = window.location.origin && window.location.origin !== "null" ? window.location.origin : "http://localhost:9339";
  const url = new URL(endpoint, baseOrigin);
  Object.entries(extraParams).forEach(([k, v]) => {
    if (v !== undefined && v !== null) {
      url.searchParams.set(k, String(v));
    }
  });
  if (token) {
    url.searchParams.set("token", token);
  }
  return url.pathname + url.search;
}

function isVideoFile(fileName, category) {
  if (category === "video") return true;
  return /\.(mp4|mkv|webm|avi|mov|m4v|flv|wmv|ts|3gp)$/i.test(fileName || "");
}

function isAudioFile(fileName, category) {
  if (category === "audio") return true;
  return /\.(mp3|flac|wav|aac|ogg|m4a|opus|wma|aiff|alac)$/i.test(fileName || "");
}

function setAuthToken(token, remember) {
  if (remember) {
    localStorage.setItem(AUTH_TOKEN_KEY, token);
    sessionStorage.setItem(AUTH_TOKEN_KEY, token);
      } else {
    sessionStorage.setItem(AUTH_TOKEN_KEY, token);
    localStorage.removeItem(AUTH_TOKEN_KEY);
  }
  try {
    document.cookie = `noos_token=${token}; path=/; max-age=604800; SameSite=Lax`;
      } catch (e) {}
}

function clearAuthToken() {
  sessionStorage.removeItem(AUTH_TOKEN_KEY);
  localStorage.removeItem(AUTH_TOKEN_KEY);
    try {
    document.cookie = "noos_token=; path=/; max-age=0; expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite=Lax";
    document.cookie = "noos_auth_token=; path=/; max-age=0; expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite=Lax";
      } catch (e) {}
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
    !resource.startsWith("/api/auth/status") &&
    !resource.startsWith("/api/auth/me")
  ) {
    // Si une mise à jour système est en cours, le service peut être en train de redémarrer
    if (isUpdatingNow) {
      console.warn("401 temporaire reçu pendant une mise à jour, mise en attente de reconnexion...");
      await new Promise(r => setTimeout(r, 2000));
      try {
        const retryRes = await originalFetch(resource, config);
        if (retryRes.status !== 401) {
          return retryRes;
        }
      } catch (e) {
        // En attente du redémarrage
      }
      return response;
    }

    clearAuthToken();
    updateUserSessionUI(null);
    document.body.classList.remove("authenticated");
    document.body.classList.add("not-authenticated");
    showLoginModal();
  }

  return response;
};

// --------------------------------------------------------------------------
// PERSISTANCE & ROUTAGE DES ONGLETS & SOUS-ONGLETS
// --------------------------------------------------------------------------
const TAB_IDS = [
  "tab-overview",
  "tab-files",
  "tab-storage",
  "tab-network",
  "tab-containers",
  "tab-games",
  "tab-vms",
  "tab-users",
  "tab-logs",
  "tab-updates"
];

let activeTab = "tab-overview";
let lastUpdateStatus = null;
let isUpdatingNow = false;

function parseHashRoute() {
  const hash = window.location.hash.replace(/^#\/?/, "").trim();
  if (!hash) return { tabId: null, subtab: null };

  const parts = hash.split("/");
  let rawTab = parts[0].toLowerCase();
  const subtab = parts[1] ? parts[1].toLowerCase() : null;

  if (rawTab.startsWith("tab-")) {
    rawTab = rawTab.replace(/^tab-/, "");
  }

  // Alias usuels
  if (rawTab === "docker") rawTab = "containers";
  if (rawTab === "system" || rawTab === "home") rawTab = "overview";

  const targetTabId = `tab-${rawTab}`;
  if (TAB_IDS.includes(targetTabId)) {
    return { tabId: targetTabId, subtab: subtab };
  }

  return { tabId: null, subtab: null };
}

function getStoredTabId() {
  const route = parseHashRoute();
  if (route.tabId) return route.tabId;

  try {
    const local = localStorage.getItem("noos_active_tab");
    if (local && TAB_IDS.includes(local)) return local;

    const session = sessionStorage.getItem("noos_active_tab");
    if (session && TAB_IDS.includes(session)) return session;
  } catch (e) {}

  return "tab-overview";
}

function updateUrlHash(tabId, subtab = null) {
  try {
    const rawTab = tabId.replace(/^tab-/, "");
    let newHash = `#${rawTab}`;
    if (subtab) {
      newHash += `/${subtab.replace(/^subtab-/, "")}`;
    }
    if (window.location.hash !== newHash) {
      window.history.replaceState(null, "", newHash);
    }
  } catch (e) {}
}

function applyInitialTabStateEarly() {
  try {
    const targetTabId = getStoredTabId();
    if (!targetTabId) return;

    activeTab = targetTabId;

    const updateHeaderBtn = document.getElementById("header-update-btn");
    if (updateHeaderBtn) {
      updateHeaderBtn.classList.toggle("active-view", targetTabId === "tab-updates");
    }

    document.querySelectorAll(".tab-btn").forEach(btn => {
      btn.classList.toggle("active", btn.getAttribute("data-tab") === targetTabId);
    });

    document.querySelectorAll(".tab-pane").forEach(pane => {
      pane.classList.toggle("active", pane.id === targetTabId);
    });

    document.documentElement.removeAttribute("data-initial-tab");
  } catch (e) {}
}

function restoreStoredSubtabs(tabId) {
  try {
    if (tabId === "tab-network") {
      const saved = localStorage.getItem("noos_subtab_network");
      if (saved) switchNetworkSubtab(saved, false);
    } else if (tabId === "tab-containers") {
      const saved = localStorage.getItem("noos_subtab_containers");
      if (saved) switchDockerSubTab(saved, false);
    } else if (tabId === "tab-games") {
      const saved = localStorage.getItem("noos_subtab_games");
      if (saved) switchGamesSubtab(saved, false);
    } else if (tabId === "tab-updates") {
      const saved = localStorage.getItem("noos_subtab_updates");
      if (saved) switchUpdateSubtab(saved, false);
    } else if (tabId === "tab-users") {
      const saved = localStorage.getItem("noos_subtab_users");
      if (saved) switchUsersSubtab(saved, false);
    }
  } catch (e) {}
}

function applySubtabRoute(tabId, subtab) {
  if (!subtab) return;
  if (tabId === "tab-network") {
    const fullSubtab = subtab.startsWith("subtab-") ? subtab : `subtab-${subtab}`;
    if (["subtab-vpn", "subtab-firewall", "subtab-samba", "subtab-sftp", "subtab-dns"].includes(fullSubtab)) {
      switchNetworkSubtab(fullSubtab, false);
    }
  } else if (tabId === "tab-containers") {
    const s = subtab === "list" ? "containers" : subtab;
    if (["containers", "store", "images"].includes(s)) {
      switchDockerSubTab(s, false);
    }
  } else if (tabId === "tab-games") {
    if (["servers", "catalog", "console"].includes(subtab)) {
      switchGamesSubtab(subtab, false);
    }
  } else if (tabId === "tab-updates") {
    if (["commits", "packages", "generations"].includes(subtab)) {
      switchUpdateSubtab(subtab, false);
    }
  } else if (tabId === "tab-users") {
    if (["accounts", "groups", "audit", "sessions"].includes(subtab)) {
      switchUsersSubtab(subtab, false);
    }
  }
}

function handleHashNavigation() {
  const route = parseHashRoute();
  const targetTab = route.tabId || "tab-overview";
  if (targetTab !== activeTab) {
    switchTab(targetTab, false);
  }
  if (route.subtab) {
    applySubtabRoute(targetTab, route.subtab);
  }
}

document.addEventListener("DOMContentLoaded", () => {
  applyInitialTabStateEarly();
  checkAuthSession();
  checkActiveStorageJobOnLoad();
  if (typeof restoreDockerDeployStateFromStorage === "function") {
    restoreDockerDeployStateFromStorage();
  }
});

window.addEventListener("hashchange", () => {
  handleHashNavigation();
});

function initApp() {
  try {
    const route = parseHashRoute();
    const targetTab = getStoredTabId();

    // Activer l'onglet déterminé
    switchTab(targetTab, true);

    // Appliquer le sous-onglet approprié
    if (route.subtab) {
      applySubtabRoute(targetTab, route.subtab);
    } else {
      restoreStoredSubtabs(targetTab);
    }

    if (new URLSearchParams(window.location.search).get("preview_reload") === "true") {
      setTimeout(() => { triggerUpdateSuccessReload(true); }, 300);
    }

    if (sessionStorage.getItem("noos_just_updated") === "true") {
      sessionStorage.removeItem("noos_just_updated");
      setTimeout(() => {
        showToast("✨ Le tableau de bord a été actualisé avec succès !", "success");
      }, 700);
    }
  } catch (e) {
    console.error("Erreur initApp onglets :", e);
  }

  refreshAll(false);
  updateSftpUri();
  fetchSystemUpdateChannel();
  checkForUpdates(false);
  checkInitialUpdateProgress();
  fetchPowerStatus();
  fetchTrashCount();
  initDragAndDrop();
  loadPinnedMounts();
  loadRemoteMounts();
  loadKDriveAccounts();
  setFileViewMode(fileViewMode);
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
    if (activeTab === "tab-files") loadPinnedMounts();
  }, 12000);

  // Rafraîchissement dynamique des serveurs de jeu (3.5 secondes pour suivre démarrages/statuts)
  setInterval(() => {
    if (activeTab === "tab-games") {
      loadGameServers();
    }
  }, 3500);

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
function switchTab(tabId, updateHash = true) {
  if (!TAB_IDS.includes(tabId)) {
    tabId = "tab-overview";
  }

  activeTab = tabId;
  try {
    localStorage.setItem("noos_active_tab", tabId);
    sessionStorage.setItem("noos_active_tab", tabId);
    
  } catch (e) {}

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

  // Déterminer le sous-onglet actif pour la mise à jour du hash
  let currentSub = null;
  if (tabId === "tab-network") {
    currentSub = activeNetworkSubtab ? activeNetworkSubtab.replace(/^subtab-/, "") : "vpn";
  } else if (tabId === "tab-containers") {
    currentSub = activeDockerSubTab || "containers";
  } else if (tabId === "tab-games") {
    currentSub = (typeof currentGamesSubtab !== "undefined" ? currentGamesSubtab : "servers");
  } else if (tabId === "tab-updates") {
    currentSub = (typeof updateSubtabCurrent !== "undefined" ? updateSubtabCurrent : "commits");
  } else if (tabId === "tab-users") {
    currentSub = (typeof activeUsersSubtab !== "undefined" ? activeUsersSubtab : "accounts");
  }

  if (updateHash) {
    updateUrlHash(tabId, currentSub);
  }

  if (tabId === "tab-overview") loadSystem();
  if (tabId === "tab-files") {
    try {
      const savedPath = localStorage.getItem("noos_files_path");
      if (savedPath && !savedPath.includes("/chomiam")) {
        currentFolderPath = savedPath;
      } else {
        currentFolderPath = getUserHome();
      }
    } catch (e) {
      currentFolderPath = getUserHome();
    }
    navigateToPath(currentFolderPath || getUserHome());
    loadPinnedMounts();
    loadKDriveAccounts();
  }
  if (tabId === "tab-updates") { checkForUpdates(false); loadGenerations(); }
  if (tabId === "tab-storage") loadStorage();
  if (tabId === "tab-network") {
    loadNetwork();
    if (activeNetworkSubtab === "subtab-vpn") {
      loadWireguardClients();
    }
    if (activeNetworkSubtab === "subtab-dns") {
      loadDnsSettings();
    }
  }
  if (tabId === "tab-containers") refreshContainersAndStore();
  if (tabId === "tab-games") {
    renderGameServers();
    loadGameServers();
    loadEggCatalog();
  }
  if (tabId === "tab-vms") loadVms();
  if (tabId === "tab-users") loadUsersAndGroups();
  if (tabId === "tab-logs") loadLogs();
}

// --------------------------------------------------------------------------
// CHARGEMENT GLOBAL
// --------------------------------------------------------------------------
async function refreshAll(showFeedback = false) {
  try {
    await Promise.allSettled([
      loadSystem(),
      loadStorage(),
      loadServices(),
      loadNetwork(),
      checkForUpdates(false),
      loadGenerations(),
      loadHardwareInfo(),
      loadSmartInfo(),
      loadLatestSpeedtest(),
      loadGameServers()
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
    const cleanHostname = (sys.hostname || 'noos-nas').replace(/^steveos(-|$)/i, 'noos$1');
    if (hostEl) hostEl.textContent = `${cleanHostname} • ${sys.os_name}`;

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
// MISES À JOUR INTELLIGENTES (STEvE_OS RESILIENT UPDATE ENGINE)
// --------------------------------------------------------------------------
let updatePollingTimer = null;
let updateSubtabCurrent = 'commits';

function switchUpdateSubtab(tabName, updateHash = true) {
  updateSubtabCurrent = tabName;
  try {
    localStorage.setItem("noos_subtab_updates", tabName);
  } catch (e) {}
  if (updateHash && activeTab === "tab-updates") {
    updateUrlHash("tab-updates", tabName);
  }
  const btnCommits = document.getElementById("btn-subtab-commits");
  const btnPackages = document.getElementById("btn-subtab-packages");
  const btnGenerations = document.getElementById("btn-subtab-generations");
  const paneCommits = document.getElementById("subtab-pane-commits");
  const panePackages = document.getElementById("subtab-pane-packages");
  const paneGenerations = document.getElementById("subtab-pane-generations");

  if (btnCommits) btnCommits.classList.toggle("active", tabName === "commits");
  if (btnPackages) btnPackages.classList.toggle("active", tabName === "packages");
  if (btnGenerations) btnGenerations.classList.toggle("active", tabName === "generations");
  if (paneCommits) paneCommits.classList.toggle("active", tabName === "commits");
  if (panePackages) panePackages.classList.toggle("active", tabName === "packages");
  if (paneGenerations) paneGenerations.classList.toggle("active", tabName === "generations");

  if (tabName === "generations") {
    loadGenerations();
  }
}

// ==========================================================================
// CANAL DE MISE À JOUR (STABLE / TESTING)
// ==========================================================================
let activeUpdateChannel = (function() {
  try {
    const saved = localStorage.getItem("noos_update_channel");
    if (saved === "testing" || saved === "stable") return saved;
  } catch (e) {}
  return "stable";
})();

function updateChannelSwitchUI(channel) {
  const btnStable = document.getElementById("btn-channel-stable");
  const btnTesting = document.getElementById("btn-channel-testing");
  if (btnStable) btnStable.classList.toggle("active", channel === "stable");
  if (btnTesting) btnTesting.classList.toggle("active", channel === "testing");

  const heroBadge = document.getElementById("hero-channel-badge");
  if (heroBadge) {
    if (channel === "testing") {
      heroBadge.className = "badge badge-channel-indicator badge-testing";
      heroBadge.textContent = "🧪 Canal Testing";
    } else {
      heroBadge.className = "badge badge-channel-indicator badge-stable";
      heroBadge.textContent = "🛡️ Canal Stable";
    }
  }
}

async function setSystemUpdateChannel(newChannel) {
  if (newChannel !== "stable" && newChannel !== "testing") return;
  activeUpdateChannel = newChannel;
  try {
    localStorage.setItem("noos_update_channel", newChannel);
  } catch (e) {}
  updateChannelSwitchUI(newChannel);

  try {
    const res = await fetch("/api/updates/channel", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ channel: newChannel })
    });
    const json = await res.json();
    if (json.success) {
      showToast(`Canal de mise à jour basculé sur ${newChannel === 'testing' ? 'Testing 🧪' : 'Stable 🛡️'}`, "success");
    }
  } catch (err) {
    console.warn("Avis communication backend setSystemUpdateChannel:", err);
  }

  // Relancer immédiatement la vérification des mises à jour pour ce canal
  checkForUpdates(true);
}

async function fetchSystemUpdateChannel() {
  // 1. Initialisation immédiate depuis le stockage local du navigateur
  try {
    const saved = localStorage.getItem("noos_update_channel");
    if (saved === "testing" || saved === "stable") {
      activeUpdateChannel = saved;
      updateChannelSwitchUI(activeUpdateChannel);
    }
  } catch (e) {}

  // 2. Synchronisation avec le backend
  try {
    const res = await fetch("/api/updates/channel");
    const json = await res.json();
    if (json.success && json.data && json.data.channel) {
      // Si aucune préférence locale n'avait été enregistrée, adopter celle du serveur
      const saved = localStorage.getItem("noos_update_channel");
      if (!saved) {
        activeUpdateChannel = json.data.channel;
        try { localStorage.setItem("noos_update_channel", activeUpdateChannel); } catch (e) {}
      }
      updateChannelSwitchUI(activeUpdateChannel);
    }
  } catch (err) {
    console.warn("Erreur fetchSystemUpdateChannel:", err);
  }
}

async function checkForUpdates(force = false) {
  const refreshBtn = document.getElementById("btn-refresh-updates");
  const refreshIcon = document.getElementById("btn-refresh-updates-icon");
  const refreshLabel = document.getElementById("btn-refresh-updates-label");
  const progressBar = document.getElementById("btn-update-progress-bar");

  let progressTimer1 = null;
  let progressTimer2 = null;

  if (force && refreshBtn) {
    refreshBtn.disabled = true;
    refreshBtn.classList.remove("is-success");
    refreshBtn.classList.add("is-checking");
    if (refreshIcon) refreshIcon.textContent = "🔄";
    if (refreshLabel) refreshLabel.textContent = "Interrogation GitHub...";
    if (progressBar) progressBar.style.width = "30%";

    progressTimer1 = setTimeout(() => {
      if (refreshLabel) refreshLabel.textContent = "Analyse des commits & paquets...";
      if (progressBar) progressBar.style.width = "65%";
    }, 400);

    progressTimer2 = setTimeout(() => {
      if (refreshLabel) refreshLabel.textContent = "Comparaison de l'arbre NixOS...";
      if (progressBar) progressBar.style.width = "85%";
    }, 900);
  }

  if (lastUpdateStatus) {
    renderUpdatesUI(lastUpdateStatus);
  }

  try {
    const queryParams = new URLSearchParams();
    if (force) queryParams.set("force", "true");
    if (activeUpdateChannel) queryParams.set("channel", activeUpdateChannel);
    const res = await fetch(`/api/updates/status?${queryParams.toString()}`);
    const json = await res.json();
    if (!json.success || !json.data) return;

    if (progressTimer1) clearTimeout(progressTimer1);
    if (progressTimer2) clearTimeout(progressTimer2);
    if (progressBar) progressBar.style.width = "100%";
    if (refreshLabel && force) refreshLabel.textContent = "Finalisation...";

    lastUpdateStatus = json.data;
    renderUpdatesUI(lastUpdateStatus);

    if (force && refreshBtn) {
      const hasConfigUpdate = !!json.data.config_update_available;
      const hasDashboardUpdate = !!json.data.dashboard_update_available || (json.data.dashboard_telemetry && json.data.dashboard_telemetry.update_available);
      const hasPkgUpdate = !!json.data.package_updates_available;
      const hasAny = hasConfigUpdate || hasDashboardUpdate || hasPkgUpdate;

      refreshBtn.classList.remove("is-checking");
      refreshBtn.classList.add("is-success");
      if (refreshIcon) refreshIcon.textContent = hasAny ? "⚡" : "✨";
      if (refreshLabel) {
        refreshLabel.textContent = hasAny ? "Mises à jour trouvées !" : "Système synchronisé !";
      }

      // Animation lumineuse de l'horodatage
      const lastCheckedTime = document.getElementById("updates-last-checked-time");
      if (lastCheckedTime) {
        lastCheckedTime.classList.add("pulse-updated");
        setTimeout(() => lastCheckedTime.classList.remove("pulse-updated"), 2500);
      }

      setTimeout(() => {
        refreshBtn.classList.remove("is-success");
        refreshBtn.disabled = false;
        if (refreshIcon) refreshIcon.textContent = "🔄";
        if (refreshLabel) refreshLabel.textContent = "Vérifier maintenant";
        if (progressBar) progressBar.style.width = "0%";
      }, 2200);
    }
  } catch (err) {
    if (progressTimer1) clearTimeout(progressTimer1);
    if (progressTimer2) clearTimeout(progressTimer2);
    console.warn("Erreur fetch /api/updates/status:", err);
    if (force && refreshBtn) {
      refreshBtn.classList.remove("is-checking");
      refreshBtn.disabled = false;
      if (refreshIcon) refreshIcon.textContent = "🔄";
      if (refreshLabel) refreshLabel.textContent = "Vérifier maintenant";
      if (progressBar) progressBar.style.width = "0%";
      showToast("Échec de la vérification des mises à jour", "error");
    }
  }
}

function renderUpdatesUI(status) {
  const dot = document.getElementById("update-indicator-dot");
  const pillText = document.getElementById("header-update-text");
  const navBadge = document.getElementById("nav-updates-badge");

  const heroBanner = document.getElementById("overview-update-banner");
  const heroIcon = document.getElementById("overview-update-icon");
  const heroTitle = document.getElementById("overview-update-title");
  const heroSubtitle = document.getElementById("overview-update-subtitle");

  const heroStatusIcon = document.getElementById("hero-status-icon");
  const heroHeading = document.getElementById("hero-status-heading");
  const heroSubheading = document.getElementById("hero-status-subheading");

  const btnSingleUpdate = document.getElementById("btn-single-update");
  const btnSingleUpdateText = document.getElementById("btn-single-update-text");
  const btnSingleUpdateBadge = document.getElementById("btn-single-update-badge");
  const btnSingleUpdateIcon = document.getElementById("btn-single-update-icon");

  const lastCheckedTime = document.getElementById("updates-last-checked-time");
  if (lastCheckedTime && status.last_checked) {
    lastCheckedTime.innerHTML = `<span>🕒</span> Vérifié à ${status.last_checked}`;
  }

  // Synchronisation de l'état du canal actif
  if (status.channel) {
    try {
      const saved = localStorage.getItem("noos_update_channel");
      if (!saved) {
        activeUpdateChannel = status.channel.toLowerCase();
      }
    } catch (e) {
      activeUpdateChannel = status.channel.toLowerCase();
    }
  }
  updateChannelSwitchUI(activeUpdateChannel);

  const hasConfigUpdate = !!status.config_update_available;
  const hasDashboardUpdate = !!status.dashboard_update_available || (status.dashboard_telemetry && status.dashboard_telemetry.update_available);
  const hasPkgUpdate = !!status.package_updates_available;
  const hasAnyUpdate = hasConfigUpdate || hasDashboardUpdate || hasPkgUpdate;

  let totalCount = (status.config_commits_behind || 0) + (status.package_updates_count || 0);
  if (hasDashboardUpdate) totalCount += 1;

  // 1. Indicateurs Globaux (Header & Navbar)
  if (dot) {
    dot.className = "update-indicator-dot";
    if (status.is_updating) {
      dot.classList.add("pulse-peach");
    } else if (hasAnyUpdate) {
      dot.classList.add("pulse-mauve");
    }
  }

  if (pillText) {
    if (status.is_updating) {
      pillText.textContent = "⚙️ Mise à jour en cours...";
    } else if (hasAnyUpdate) {
      const parts = [];
      if (hasDashboardUpdate) parts.push("Dashboard");
      if (status.config_commits_behind > 0) parts.push(`${status.config_commits_behind} commit${status.config_commits_behind > 1 ? 's' : ''}`);
      if (status.package_updates_count > 0) parts.push(`${status.package_updates_count} paquet${status.package_updates_count > 1 ? 's' : ''}`);
      pillText.textContent = `⚡ Màj dispo (${parts.join(', ') || 'nouveau'})`;
    } else {
      pillText.textContent = "✨ Système à jour";
    }
  }

  if (navBadge) {
    if (hasAnyUpdate) {
      navBadge.style.display = "inline-block";
      navBadge.textContent = totalCount > 0 ? totalCount : "!";
    } else {
      navBadge.style.display = "none";
    }
  }

  // 2. Bannière de l'aperçu
  if (heroBanner) {
    if (hasAnyUpdate) {
      heroBanner.classList.add("has-updates");
      if (heroIcon) heroIcon.textContent = "⚡";
      if (heroTitle) heroTitle.textContent = status.status_text;
      const parts = [];
      if (hasDashboardUpdate) {
        const runV = status.dashboard_telemetry ? status.dashboard_telemetry.running_version : "";
        const tgtV = status.dashboard_telemetry ? status.dashboard_telemetry.target_version : "";
        parts.push(`⚡ Dashboard v${runV} → v${tgtV}`);
      }
      if (status.config_commits_behind > 0) {
        parts.push(`🖥️ OS NixOS : ${status.config_commits_behind} commit(s) en attente`);
      }
      if (status.package_updates_count > 0) {
        parts.push(`📦 ${status.package_updates_count} paquet(s) système`);
      }
      if (heroSubtitle) heroSubtitle.textContent = parts.join(" • ") || "Une nouvelle version de Noos NAS Edition est prête à être déployée.";
    } else {
      heroBanner.classList.remove("has-updates");
      if (heroIcon) heroIcon.textContent = "✨";
      if (heroTitle) heroTitle.textContent = "Noos NAS Edition est à jour";
      if (heroSubtitle) heroSubtitle.textContent = "Votre système d'exploitation et votre tableau de bord fonctionnent sur la dernière version.";
    }
  }

  // 3. Hero Card & Bouton Unique
  if (heroStatusIcon) {
    heroStatusIcon.textContent = status.is_updating ? "🔄" : (hasAnyUpdate ? "🚀" : "✨");
  }
  if (heroHeading) {
    if (status.is_updating) {
      heroHeading.textContent = "Mise à jour de Noos en cours d'exécution";
    } else if (hasConfigUpdate && hasDashboardUpdate) {
      heroHeading.textContent = "Mise à jour globale Noos disponible (OS & Dashboard)";
    } else if (hasDashboardUpdate) {
      heroHeading.textContent = "Mise à jour du Dashboard Noos disponible";
    } else if (hasConfigUpdate) {
      heroHeading.textContent = "Mise à jour de la configuration NixOS disponible";
    } else if (hasAnyUpdate) {
      heroHeading.textContent = "Mises à jour système prêtes à être appliquées";
    } else {
      heroHeading.textContent = "Votre système Noos est parfaitement à jour";
    }
  }
  if (heroSubheading) {
    if (status.is_updating) {
      heroSubheading.textContent = "Une opération de déploiement est en cours d'exécution. Suivez la progression ci-dessous.";
    } else if (hasAnyUpdate) {
      const summaryItems = [];
      if (hasDashboardUpdate) {
        const runV = status.dashboard_telemetry ? status.dashboard_telemetry.running_version : "";
        const tgtV = status.dashboard_telemetry ? status.dashboard_telemetry.target_version : "";
        summaryItems.push(`⚡ Dashboard : <b>v${runV} → v${tgtV}</b>`);
      }
      if (status.config_commits_behind > 0) {
        summaryItems.push(`🖥️ OS NixOS : <b>${status.config_commits_behind}</b> nouveau${status.config_commits_behind > 1 ? 'x' : ''} commit${status.config_commits_behind > 1 ? 's' : ''}`);
      }
      if (status.package_updates_count > 0) {
        summaryItems.push(`📦 <b>${status.package_updates_count}</b> paquet${status.package_updates_count > 1 ? 's' : ''} système`);
      }
      heroSubheading.innerHTML = summaryItems.join(" • ") || status.status_text;
    } else {
      heroSubheading.textContent = "Configuration déclarative NixOS et Dashboard Web sont synchronisés avec GitHub.";
    }
  }

  if (btnSingleUpdate) {
    btnSingleUpdate.disabled = status.is_updating;
    if (status.is_updating) {
      if (btnSingleUpdateIcon) btnSingleUpdateIcon.textContent = "⏳";
      if (btnSingleUpdateText) btnSingleUpdateText.textContent = "Mise à jour en cours...";
      if (btnSingleUpdateBadge) btnSingleUpdateBadge.style.display = "none";
    } else if (hasAnyUpdate) {
      if (btnSingleUpdateIcon) btnSingleUpdateIcon.textContent = "🚀";
      let targetLabel = "";
      if (hasConfigUpdate && status.config_remote_commit && status.config_remote_commit !== status.config_local_commit) {
        targetLabel = `vers ${status.config_remote_commit}`;
      } else if (hasDashboardUpdate) {
        const tgtV = (status.dashboard_telemetry && status.dashboard_telemetry.target_version) || "nouvelle version";
        targetLabel = `(Dashboard v${tgtV})`;
      } else if (hasPkgUpdate) {
        const count = status.package_updates_count || 1;
        targetLabel = `(${count} paquet${count > 1 ? 's' : ''})`;
      }
      if (btnSingleUpdateText) btnSingleUpdateText.textContent = `Mettre à jour Noos ${targetLabel}`.trim();
      if (btnSingleUpdateBadge) {
        btnSingleUpdateBadge.style.display = "inline-block";
        btnSingleUpdateBadge.textContent = totalCount > 0 ? `${totalCount} màj` : "Prêt";
      }
    } else {
      if (btnSingleUpdateIcon) btnSingleUpdateIcon.textContent = "🔄";
      if (btnSingleUpdateText) btnSingleUpdateText.textContent = "Réinstaller / Synchroniser le système";
      if (btnSingleUpdateBadge) btnSingleUpdateBadge.style.display = "none";
    }
  }

  // 4. Double Télémétrie : Carte 1 - Configuration OS NixOS
  const localShaEl = document.getElementById("git-local-sha");
  const localGenEl = document.getElementById("version-local-gen");
  const remoteShaEl = document.getElementById("git-remote-sha");
  const remoteSyncEl = document.getElementById("git-sync-status");
  const osBadgeEl = document.getElementById("version-os-badge");
  const localMsgEl = document.getElementById("git-local-msg");

  const osTel = status.os_telemetry || {};
  const localCommit = osTel.local_commit || status.config_local_commit || "--";
  const remoteCommit = osTel.remote_commit || status.config_remote_commit || localCommit;
  const commitsBehind = osTel.commits_behind !== undefined ? osTel.commits_behind : (status.config_commits_behind || 0);

  if (localShaEl) localShaEl.textContent = localCommit;
  if (remoteShaEl) remoteShaEl.textContent = remoteCommit;
  if (localGenEl) {
    const gen = status.system_generation ? `Génération ${status.system_generation}` : "Génération active";
    localGenEl.textContent = `${gen} • NixOS 26.05`;
  }
  if (remoteSyncEl) {
    if (commitsBehind > 0) {
      remoteSyncEl.innerHTML = `<span style="color:var(--yellow); font-weight:700;">En retard de ${commitsBehind} commit${commitsBehind > 1 ? 's' : ''}</span>`;
    } else {
      remoteSyncEl.innerHTML = `<span style="color:var(--green); font-weight:700;">Aligné avec origin/main</span>`;
    }
  }
  if (osBadgeEl) {
    if (hasConfigUpdate) {
      osBadgeEl.className = "badge badge-accent";
      osBadgeEl.textContent = `${commitsBehind || 1} màj dispo`;
    } else {
      osBadgeEl.className = "badge badge-success";
      osBadgeEl.textContent = "À jour";
    }
  }
  if (localMsgEl) {
    if (status.config_pending_commits && status.config_pending_commits.length > 0) {
      localMsgEl.textContent = `Dernier commit : ${status.config_pending_commits[0].message} (${status.config_pending_commits[0].author})`;
    } else {
      localMsgEl.textContent = status.config_commit_message || "Configuration NixOS synchronisée avec la branche main.";
    }
  }

  // 4. Double Télémétrie : Carte 2 - Dashboard Web & Moteurs
  const dashRunningVerEl = document.getElementById("dashboard-running-ver");
  const dashTargetVerEl = document.getElementById("dashboard-target-ver");
  const dashTargetCommitEl = document.getElementById("dashboard-target-commit");
  const dashBadgeEl = document.getElementById("version-dashboard-badge");
  const dashMsgEl = document.getElementById("dashboard-status-msg");

  const dashTel = status.dashboard_telemetry || {};
  const runningVer = dashTel.running_version || "0.2.13";
  const targetVer = dashTel.target_version || runningVer;
  const targetCommit = dashTel.target_commit || "--";

  if (dashRunningVerEl) dashRunningVerEl.textContent = `v${runningVer}`;
  if (dashTargetVerEl) dashTargetVerEl.textContent = `v${targetVer}`;
  if (dashTargetCommitEl) dashTargetCommitEl.textContent = targetCommit;

  if (dashBadgeEl) {
    if (hasDashboardUpdate) {
      dashBadgeEl.className = "badge badge-accent";
      dashBadgeEl.textContent = "Màj disponible";
    } else {
      dashBadgeEl.className = "badge badge-success";
      dashBadgeEl.textContent = "Actif";
    }
  }

  if (dashMsgEl) {
    if (hasDashboardUpdate) {
      dashMsgEl.textContent = `Nouvelle version v${targetVer} prête. Le déploiement compilera ou appliquera le nouveau binaire sans interrompre votre session.`;
    } else {
      dashMsgEl.textContent = `Tableau de bord Noos actif sur la version v${runningVer} (moteur asynchrone Rust Axum).`;
    }
  }

  // 5. Commits en attente
  const pendingCommitsContainer = document.getElementById("pending-commits-list-container");
  const countPendingCommitsEl = document.getElementById("count-pending-commits");
  const pendingCommits = status.config_pending_commits || [];

  if (countPendingCommitsEl) countPendingCommitsEl.textContent = pendingCommits.length;

  if (pendingCommitsContainer) {
    if (pendingCommits.length === 0) {
      pendingCommitsContainer.innerHTML = `<div class="empty-state-notice" style="text-align:center; padding:20px; color:var(--subtext0);">✨ Aucun commit en attente. Votre configuration locale est parfaitement synchronisée avec GitHub.</div>`;
    } else {
      let html = '<div class="commit-timeline-wrap">';
      pendingCommits.forEach(c => {
        html += `
          <div class="commit-timeline-item">
            <a href="https://github.com/Chomiam/noos-nas/commit/${c.hash}" target="_blank" class="commit-sha-badge">${c.hash.substring(0, 7)}</a>
            <div class="commit-details-col">
              <div class="commit-msg-text">${escapeHtml(c.message)}</div>
              <div class="commit-meta-text">Par <strong>${escapeHtml(c.author)}</strong> • ${escapeHtml(c.date)}</div>
            </div>
          </div>
        `;
      });
      html += '</div>';
      pendingCommitsContainer.innerHTML = html;
    }
  }

  // 6. Paquets à mettre à jour
  const packagesTbody = document.getElementById("packages-update-tbody");
  const countPendingPkgsEl = document.getElementById("count-pending-packages");
  const pkgList = status.package_updates_list || [];

  if (countPendingPkgsEl) countPendingPkgsEl.textContent = pkgList.length;

  if (packagesTbody) {
    if (pkgList.length === 0) {
      packagesTbody.innerHTML = `
        <tr>
          <td colspan="4" style="text-align:center; padding:24px; color:var(--subtext0);">
            ✨ Aucun paquet en attente de mise à niveau. Le système NixOS est parfaitement aligné.
          </td>
        </tr>`;
    } else {
      let rows = '';
      pkgList.forEach(pkg => {
        let actionBadge = `<span class="badge badge-info">Mise à niveau</span>`;
        if (pkg.action === "add" || pkg.action === "download") {
          actionBadge = `<span class="badge badge-success">Téléchargement</span>`;
        } else if (pkg.action === "build") {
          actionBadge = `<span class="badge badge-warning">Construction</span>`;
        } else if (pkg.action === "remove") {
          actionBadge = `<span class="badge badge-danger">Suppression</span>`;
        }

        rows += `
          <tr>
            <td style="font-weight:600; color:var(--text); font-family:var(--font-mono);">${escapeHtml(pkg.name)}</td>
            <td>${actionBadge}</td>
            <td style="color:var(--subtext0);">${escapeHtml(pkg.current_version)}</td>
            <td style="color:var(--mauve); font-weight:700;">${escapeHtml(pkg.new_version || 'dernière version')}</td>
          </tr>
        `;
      });
      packagesTbody.innerHTML = rows;
    }
  }

  // 7. Compteur de Générations Système
  const countSystemGenEl = document.getElementById("count-system-generations");
  if (countSystemGenEl) {
    if (status.system_generations_count !== undefined && status.system_generations_count !== null) {
      countSystemGenEl.textContent = status.system_generations_count;
    } else if (generationsData && generationsData.total_count !== undefined) {
      countSystemGenEl.textContent = generationsData.total_count;
    }
  }
}

// --------------------------------------------------------------------------
// DÉCLENCHEMENT DE LA MISE À JOUR (UNIQUE & SÉCURISÉ)
// --------------------------------------------------------------------------
async function triggerSingleUpdate() {
  if (isUpdatingNow) return;

  const hasAnyUpdate = lastUpdateStatus && (lastUpdateStatus.config_update_available || lastUpdateStatus.dashboard_update_available || (lastUpdateStatus.dashboard_telemetry && lastUpdateStatus.dashboard_telemetry.update_available) || lastUpdateStatus.package_updates_available);
  const promptMsg = hasAnyUpdate 
    ? "Voulez-vous lancer la mise à jour de Noos ?\nL'opération s'exécute en arrière-plan et survit aux rafraîchissements de page."
    : "Le système est déjà à jour. Souhaitez-vous forcer une synchronisation et une réévaluation complète de la configuration ?";

  if (!confirm(promptMsg)) return;

  isUpdatingNow = true;
  const btnSingle = document.getElementById("btn-single-update");
  if (btnSingle) btnSingle.disabled = true;

  showToast("Démarrage du processus de mise à jour...", "info");

  try {
    const res = await fetch("/api/updates/start", { method: "POST" });
    const json = await res.json();
    if (!json.success) {
      showToast("Impossible de démarrer la mise à jour : " + (json.message || "Erreur"), "error");
      isUpdatingNow = false;
      if (btnSingle) btnSingle.disabled = false;
      return;
    }
  } catch (err) {
    showToast("Erreur lors de la requête de mise à jour : " + err, "error");
    isUpdatingNow = false;
    if (btnSingle) btnSingle.disabled = false;
    return;
  }

  startPollingUpdateProgress();
}

// --------------------------------------------------------------------------
// SUIVI EN TEMPS RÉEL DU PROGRÈS (RÉSILIENCE AUX COUPURES ET REDÉMARRAGE)
// --------------------------------------------------------------------------
let pollRetryCount = 0;

function startPollingUpdateProgress() {
  if (updatePollingTimer) clearInterval(updatePollingTimer);

  const progressPanel = document.getElementById("update-progress-panel");
  const floatingToast = document.getElementById("update-floating-toast");

  if (progressPanel) progressPanel.style.display = "block";
  if (floatingToast) floatingToast.style.display = "block";

  updatePollingTimer = setInterval(async () => {
    try {
      const res = await fetch("/api/updates/progress");
      const json = await res.json();
      pollRetryCount = 0;

      if (json.success && json.data) {
        updateProgressView(json.data);
      }
    } catch (err) {
      // Coupure réseau normale pendant le redémarrage du dashboard par systemd switch-to-configuration !
      pollRetryCount++;
      handleUpdateReconnectionUI(pollRetryCount);
    }
  }, 1200);
}

function handleUpdateReconnectionUI(retries) {
  const panelTitle = document.getElementById("progress-panel-title");
  const panelDetail = document.getElementById("progress-panel-detail");
  const toastTitle = document.getElementById("update-toast-title");
  const toastDetail = document.getElementById("update-toast-detail");
  const toastIcon = document.getElementById("update-toast-icon");

  const reconnectMsg = `Reconnexion au NAS en cours (${retries}s)... Le tableau de bord redémarre.`;

  if (panelTitle) panelTitle.textContent = "Redémarrage du service tableau de bord...";
  if (panelDetail) panelDetail.textContent = reconnectMsg;
  if (toastTitle) toastTitle.textContent = "Redémarrage du tableau de bord...";
  if (toastDetail) toastDetail.textContent = reconnectMsg;
  if (toastIcon) toastIcon.textContent = "⚡";
}

function updateProgressView(data) {
  const progressPanel = document.getElementById("update-progress-panel");
  const mainBar = document.getElementById("update-main-progress-bar");
  const percentLabel = document.getElementById("progress-percent-label");
  const panelTitle = document.getElementById("progress-panel-title");
  const panelDetail = document.getElementById("progress-panel-detail");
  const panelSpinner = document.getElementById("progress-panel-spinner");

  const floatingToast = document.getElementById("update-floating-toast");
  const toastBar = document.getElementById("update-toast-progress-bar");
  const toastTitle = document.getElementById("update-toast-title");
  const toastDetail = document.getElementById("update-toast-detail");
  const toastIcon = document.getElementById("update-toast-icon");
  const toastClose = document.getElementById("btn-close-update-toast");

  if (data.is_running || data.stage === "completed" || data.stage === "failed") {
    if (floatingToast) floatingToast.style.display = "block";
  }

  // Barre de progression
  const pct = Math.min(100, Math.max(0, data.progress_percent || 0));
  if (mainBar) mainBar.style.width = `${pct}%`;
  if (toastBar) toastBar.style.width = `${pct}%`;
  if (percentLabel) percentLabel.textContent = `${pct}%`;

  // Activité détaillée (Badge [X/N] et nom du paquet en cours)
  const liveActivity = document.getElementById("update-live-activity");
  const badgeDerivation = document.getElementById("update-badge-derivation");
  const currentPackage = document.getElementById("update-current-package");
  const logTerminal = document.getElementById("build-log-terminal");

  if (data.is_running && data.step_index === 2) {
    if (liveActivity) liveActivity.style.display = "flex";
    if (data.total_derivations && data.total_derivations > 0) {
      if (badgeDerivation) {
        badgeDerivation.style.display = "inline-block";
        badgeDerivation.textContent = `[${data.current_derivation_index || 0}/${data.total_derivations}]`;
      }
    } else {
      if (badgeDerivation) badgeDerivation.style.display = "none";
    }

    if (currentPackage) {
      if (data.current_package_name) {
        currentPackage.textContent = `⚙ ${data.current_package_name}`;
      } else {
        currentPackage.textContent = data.status_detail || "Compilation en cours...";
      }
    }
  } else if (!data.is_running) {
    if (liveActivity && data.stage !== "completed" && data.stage !== "failed") {
      liveActivity.style.display = "none";
    }
  }

  // Journal de construction dans l'accordéon
  if (logTerminal && data.log_tail) {
    logTerminal.textContent = data.log_tail;
    // Auto-scroll vers le bas si l'accordéon est visible
    const acc = document.getElementById("build-log-accordion");
    if (acc && acc.style.display !== "none") {
      acc.scrollTop = acc.scrollHeight;
    }
  }

  if (panelTitle) panelTitle.textContent = data.status_title || "Mise à jour Noos";
  if (panelDetail) panelDetail.textContent = data.status_detail || "Exécution des étapes de déploiement...";
  if (toastTitle) toastTitle.textContent = data.status_title || "Mise à jour Noos";
  if (toastDetail) toastDetail.textContent = data.status_detail || "";

  // Stepper visuel 1..4
  for (let step = 1; step <= 4; step++) {
    const node = document.getElementById(`step-node-${step}`);
    const conn = document.getElementById(`step-conn-${step}`);
    if (node) {
      node.classList.remove("active", "completed");
      if (data.step_index === step && data.is_running) {
        node.classList.add("active");
      } else if (data.step_index > step || data.stage === "completed") {
        node.classList.add("completed");
      }
    }
    if (conn) {
      conn.classList.remove("completed", "active");
      if (data.step_index > step || data.stage === "completed") {
        conn.classList.add("completed");
      } else if (data.step_index === step && data.is_running) {
        conn.classList.add("active");
      }
    }
  }

  // Gestion de fin (Succès ou Échec)
  if (data.stage === "completed" || (!data.is_running && data.progress_percent === 100)) {
    if (updatePollingTimer) {
      clearInterval(updatePollingTimer);
      updatePollingTimer = null;
    }
    const wasUpdating = isUpdatingNow;
    isUpdatingNow = false;

    if (data.warning) {
      if (panelSpinner) panelSpinner.textContent = "⚠️";
      if (toastIcon) toastIcon.textContent = "⚠️";
      if (toastTitle) toastTitle.textContent = data.status_title || "Mise à jour appliquée avec avertissements";
      if (toastDetail) toastDetail.textContent = data.status_detail || "Le système Noos a été actualisé avec des avertissements.";
      if (toastBar) {
        toastBar.style.width = "100%";
        toastBar.style.background = "linear-gradient(90deg, var(--peach, #fab387), var(--yellow, #f9e2af))";
      }
      showToast(data.status_detail || "Mise à jour appliquée avec avertissements.", "warning");
    } else {
      if (panelSpinner) panelSpinner.textContent = "✅";
      if (toastIcon) toastIcon.textContent = "🎉";
      if (toastTitle) toastTitle.textContent = data.status_title || "Mise à jour terminée avec succès !";
      if (toastDetail) toastDetail.textContent = data.status_detail || "Le système Noos a été actualisé.";
      if (toastBar) {
        toastBar.style.width = "100%";
        toastBar.style.background = "linear-gradient(90deg, var(--green), var(--teal))";
      }
      showToast("🎉 Noos a été mis à jour avec succès !", "success");
    }

    // Auto-dismiss de la bulle après 10 secondes
    scheduleUpdateToastDismiss(10000);

    // Réactiver le bouton principal
    const btnSingle = document.getElementById("btn-single-update");
    if (btnSingle) btnSingle.disabled = false;

    if (wasUpdating) {
      triggerUpdateSuccessReload();
    } else {
      setTimeout(() => {
        checkForUpdates(false);
        loadSystem();
      }, 1500);
    }
  } else if (data.stage === "failed") {
    if (updatePollingTimer) {
      clearInterval(updatePollingTimer);
      updatePollingTimer = null;
    }
    isUpdatingNow = false;

    if (panelSpinner) panelSpinner.textContent = "❌";
    if (toastIcon) toastIcon.textContent = "❌";
    if (toastClose) toastClose.style.display = "block";
    if (panelTitle) panelTitle.textContent = data.status_title || "Échec de la mise à jour";
    if (panelDetail) panelDetail.textContent = data.status_detail || "Une erreur est survenue lors du déploiement.";

    // Ouvrir automatiquement l'accordéon des journaux pour voir le diagnostic
    const acc = document.getElementById("build-log-accordion");
    if (acc) {
      acc.style.display = "block";
      acc.scrollTop = acc.scrollHeight;
    }

    showToast("Échec de la mise à jour : " + (data.error || data.status_detail), "error");

    const btnSingle = document.getElementById("btn-single-update");
    if (btnSingle) btnSingle.disabled = false;
  }
}

let isReloadingAfterUpdate = false;

function triggerUpdateSuccessReload(skipReload = false) {
  if (isReloadingAfterUpdate && !skipReload) return;
  isReloadingAfterUpdate = true;

  try {
    sessionStorage.setItem("noos_active_tab", activeTab || "tab-overview");
    sessionStorage.setItem("noos_just_updated", "true");
  } catch (e) {}

  // Appliquer le flou et l'atténuation sur toute l'interface
  document.body.classList.add("app-updating-reload");

  // Activer l'overlay de chargement stylisé Catppuccin
  const overlay = document.getElementById("update-reload-overlay");
  if (overlay) {
    overlay.style.display = "flex";
    void overlay.offsetWidth; // Reflow pour transition CSS
    overlay.classList.add("active");

    const bar = document.getElementById("update-reload-progress-bar");
    if (bar) {
      bar.style.width = "0%";
      setTimeout(() => {
        bar.style.width = "100%";
      }, 50);
    }
  }

  if (skipReload) return;

  // Recharger le tableau de bord après 2.6 secondes
  setTimeout(() => {
    const url = new URL(window.location.href);
    url.searchParams.delete("trigger_reload");
    url.searchParams.delete("preview_reload");
    url.searchParams.set("_v", Date.now().toString());
    window.location.replace(url.toString());
  }, 2600);
}

window.triggerUpdateSuccessReload = triggerUpdateSuccessReload;

let updateToastDismissTimer = null;

function scheduleUpdateToastDismiss(delayMs = 10000) {
  if (updateToastDismissTimer) {
    clearTimeout(updateToastDismissTimer);
    updateToastDismissTimer = null;
  }
  updateToastDismissTimer = setTimeout(() => {
    dismissUpdateToast();
  }, delayMs);
}

async function dismissUpdateToast() {
  if (updateToastDismissTimer) {
    clearTimeout(updateToastDismissTimer);
    updateToastDismissTimer = null;
  }
  const floatingToast = document.getElementById("update-floating-toast");
  if (floatingToast) {
    floatingToast.classList.add("toast-fading-out");
    setTimeout(() => {
      floatingToast.style.display = "none";
      floatingToast.classList.remove("toast-fading-out");
    }, 450);
  }
  try {
    await fetch("/api/updates/dismiss", { method: "POST" });
  } catch (e) {}
}

async function checkInitialUpdateProgress() {
  try {
    const res = await fetch("/api/updates/progress");
    const json = await res.json();
    if (json.success && json.data) {
      if (json.data.is_running) {
        isUpdatingNow = true;
        startPollingUpdateProgress();
      } else if (json.data.stage === "completed" && json.data.progress_percent === 100) {
        // Calcul du temps écoulé depuis la fin de la mise à jour
        let elapsedMs = 0;
        if (json.data.completed_timestamp) {
          const nowSec = Math.floor(Date.now() / 1000);
          elapsedMs = Math.max(0, (nowSec - json.data.completed_timestamp) * 1000);
        }

        // Si plus de 10 secondes se sont écoulées, on masque et purge immédiatement
        if (elapsedMs >= 10000) {
          dismissUpdateToast();
          return;
        }

        const remainingMs = Math.max(1000, 10000 - elapsedMs);
        const floatingToast = document.getElementById("update-floating-toast");
        if (floatingToast) floatingToast.style.display = "block";
        const toastClose = document.getElementById("btn-close-update-toast");
        if (toastClose) toastClose.style.display = "block";
        updateProgressView(json.data);
        scheduleUpdateToastDismiss(remainingMs);
      }
    }
  } catch (e) {}
}

// --------------------------------------------------------------------------
// STOCKAGE & GESTION DES POOLS RAID
// --------------------------------------------------------------------------
let cachedStorageDisks = [];
let cachedLogicalRaids = [];
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
    cachedLogicalRaids = data.logical_raids || [];

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

    // Persisted Mounts Map
    const persistedMountsList = data.persisted_mounts || [];
    const persistedMountPoints = new Set(persistedMountsList.map(m => m.mountPoint));
    const persistedDevices = new Set(persistedMountsList.map(m => m.device));

    // Bannière de tâche de stockage active (destruction / création)
    updateStorageRaidJobPanel(activeStorageJob);

    // Bannière de synchronisation active
    updateRaidSyncBanner(data.active_sync);

    // Identifier tous les disques membres de n'importe quelle grappe RAID
    const allRaidMemberDiskNames = new Set();
    if (data.logical_raids) {
      data.logical_raids.forEach(r => {
        (r.members || []).forEach(m => {
          allRaidMemberDiskNames.add(m);
          allRaidMemberDiskNames.add(m.replace("/dev/", ""));
        });
      });
    }

    // 1. SECTION 1 : Grappes RAID & Cadres Dédiés avec Disques Membres
    const raidsContainer = document.getElementById("logical-raids-container");
    if (raidsContainer) {
      if (!data.logical_raids || data.logical_raids.length === 0) {
        raidsContainer.innerHTML = `
          <div style="padding: 26px; text-align: center; background: var(--surface0); border-radius: var(--radius-md); border: 1px dashed rgba(255,255,255,0.12);">
            <div style="font-size: 2.2rem; margin-bottom: 8px;">🛡️</div>
            <div style="font-weight: 700; color: var(--text); font-size: 1.05rem; margin-bottom: 4px;">Aucune grappe RAID active</div>
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

          const isPersisted = r.mountpoint && (persistedMountPoints.has(r.mountpoint) || persistedDevices.has(r.device));
          const persistBadge = isPersisted 
            ? `<span class="badge badge-accent" title="Inscrit déclarativement dans mounts.json (NixOS)">💾 NixOS Persistant</span>` 
            : "";

          // Trouver les disques physiques membres réels correspondants
          const memberDisks = (data.physical_disks || []).filter(d => {
            return (r.members || []).some(m => m.includes(d.name) || m.includes(d.path));
          });

          const memberDisksHtml = memberDisks.length > 0 ? memberDisks.map((d, idx) => {
            const isLast = idx === memberDisks.length - 1;
            const branchIcon = isLast ? "└──" : "├──";
            const isStandby = d.power_state.toLowerCase().includes("veille") || d.power_state.toLowerCase().includes("standby");
            const isNvme = d.disk_type.includes("NVMe");
            const driveIcon = isNvme ? "⚡" : "💿";

            let tempColor = "var(--green)";
            if (d.temperature_c > 45) tempColor = "var(--peach)";
            if (d.temperature_c > 52) tempColor = "var(--red)";
            const tempVal = d.temperature_c > 0 ? `${d.temperature_c} °C` : (isStandby ? "Veille" : "N/A");

            const spindownBtn = d.is_rotational
              ? `<button type="button" class="btn btn-secondary btn-xs" onclick="triggerSpindown('${escapeHtml(d.name)}')"><span>🌙</span> Veille</button>`
              : "";

            return `
              <div class="raid-member-disk-row">
                <div class="raid-member-left">
                  <span class="raid-tree-branch">${branchIcon}</span>
                  <span style="font-size:1.1rem;">${driveIcon}</span>
                  <div class="raid-member-info">
                    <div class="raid-member-model">${escapeHtml(d.model)} <span class="badge badge-secondary" style="font-size:0.68rem; margin-left:4px;">${escapeHtml(d.bay_label || d.name)}</span></div>
                    <div class="raid-member-dev">${escapeHtml(d.path)} &bull; S/N: <code>${escapeHtml(d.serial)}</code></div>
                  </div>
                </div>

                <div class="raid-member-middle">
                  <span class="raid-tele-pill"><strong>${escapeHtml(d.size_human)}</strong></span>
                  <span class="raid-tele-pill" style="color:${tempColor};">🌡️ ${tempVal}</span>
                  <span class="raid-tele-pill" style="color:var(--teal);">🛡️ ${escapeHtml(d.smart_status)}</span>
                  <span class="badge ${isStandby ? 'badge-warning' : 'badge-success'}">${isStandby ? '🌙 Veille' : '✅ Actif'}</span>
                </div>

                <div class="raid-member-actions">
                  ${spindownBtn}
                </div>
              </div>
            `;
          }).join("") : `
            <div style="font-size:0.8rem; color:var(--subtext0); padding:8px 0;">
              Membres déclarés : ${(r.members || []).map(m => `<span class="member-disk-pill">💿 ${escapeHtml(m)}</span>`).join(" ")}
            </div>
          `;

          return `
            <div class="raid-cluster-frame">
              <div class="raid-cluster-header">
                <div class="raid-cluster-title-wrap">
                  <div class="raid-cluster-title">
                    <span>🛡️ ${escapeHtml(r.name)}</span>
                    <span class="badge badge-accent" style="font-size:0.75rem; font-weight:700;">${escapeHtml(r.level)}</span>
                    <span class="badge ${healthBadgeClass}">${escapeHtml(r.health)}</span>
                    <span class="badge ${r.filesystem && !r.filesystem.includes('Non') && !r.filesystem.includes('Inconnu') ? 'badge-primary' : 'badge-secondary'}" style="font-size:0.75rem; font-weight:700;">
                      📂 FS : ${escapeHtml(r.filesystem ? r.filesystem.toUpperCase() : 'NON FORMATÉ')}
                    </span>
                    ${persistBadge}
                  </div>
                  <div class="raid-cluster-device-sub">${escapeHtml(r.device)}</div>
                </div>

                <div class="raid-cluster-actions" style="display:flex; align-items:center; gap:8px; flex-wrap:wrap;">
                  ${r.mountpoint
                    ? `<span style="font-size:0.82rem; color:var(--green); font-weight:700; margin-right:6px;">📁 Monté sur ${escapeHtml(r.mountpoint)}</span>
                       <button type="button" class="btn btn-secondary btn-xs" onclick="openRepairPermissionsModal('${escapeHtml(r.mountpoint)}')"><span>🛡️</span> Permissions 2775</button>
                       <button type="button" class="btn btn-secondary btn-xs" onclick="umountVolume('${escapeHtml(r.mountpoint)}')"><span>⏏️</span> Démonter</button>`
                    : (() => {
                        const isFormatted = Boolean(
                          r.filesystem &&
                          r.filesystem.trim() !== "" &&
                          !r.filesystem.toLowerCase().includes("non") &&
                          !r.filesystem.toLowerCase().includes("inconnu") &&
                          !r.filesystem.toLowerCase().includes("unknown") &&
                          r.filesystem.toLowerCase() !== "none"
                        );
                        return `
                          ${isFormatted ? `
                            <button type="button" class="btn btn-primary btn-xs" onclick="openMountVolumeModal('${escapeHtml(r.name)}', '${escapeHtml(r.device)}', '${escapeHtml(r.level)}', false, '${escapeHtml(r.filesystem || '')}')">
                              <span>⚡</span> Monter sans formater
                            </button>
                          ` : `
                            <span class="badge badge-secondary" style="font-size:0.75rem; color:var(--subtext0); background:rgba(255,255,255,0.06);">⚠️ Formatage requis</span>
                          `}
                          <button type="button" class="btn btn-danger btn-xs" style="border:1px solid rgba(243,139,168,0.4); background:rgba(243,139,168,0.1); color:var(--red);" onclick="openMountVolumeModal('${escapeHtml(r.name)}', '${escapeHtml(r.device)}', '${escapeHtml(r.level)}', true, '${escapeHtml(r.filesystem || '')}')">
                            <span>⚠️</span> Formater le volume
                          </button>
                        `;
                      })()}
                  <button type="button" class="btn btn-danger btn-xs" style="border:1px solid rgba(243,139,168,0.4); background:rgba(243,139,168,0.15); color:var(--red);" onclick="openDestroyRaidModal('${escapeHtml(r.name)}', '${escapeHtml(r.device)}')">
                    <span>🧨</span> Casser la grappe
                  </button>
                </div>
              </div>

              <!-- Jauge d'espace et métriques -->
              <div class="raid-cluster-metrics">
                <div class="raid-cluster-metrics-header">
                  <div>
                    <span class="raid-cluster-metrics-val">${r.usage_percent}%</span>
                    <span class="raid-cluster-metrics-sub" style="margin-left:8px;">${escapeHtml(r.used_bytes ? formatFileSize(r.used_bytes) : "0 o")} utilisés sur ${escapeHtml(r.size_human)}</span>
                  </div>
                  <span style="font-size:0.8rem; color:var(--subtext0);">${escapeHtml(r.free_bytes ? formatFileSize(r.free_bytes) : r.size_human)} libres</span>
                </div>
                <div class="metric-progress-wrap" style="height:8px; margin:0;">
                  <div class="metric-progress-bar ${r.usage_percent > 85 ? 'progress-red' : 'progress-peach'}" style="width: ${Math.min(r.usage_percent, 100)}%;"></div>
                </div>
              </div>

              <!-- Cadre Dédié des Disques Physiques Membres -->
              <div class="raid-members-section">
                <div class="raid-members-title">
                  <span>💿 Disques Physiques Membres de la Grappe (${(r.members || []).length} disques protégés)</span>
                </div>
                <div class="raid-members-list">
                  ${memberDisksHtml}
                </div>
              </div>
            </div>
          `;
        }).join("");
      }
    }

    // 2. SECTION 2 : Disques Physiques Autonomes & Volumes Simples (Hors RAID)
    const standaloneContainer = document.getElementById("standalone-disks-container");
    const standaloneCountBadge = document.getElementById("standalone-disks-count-badge");
    const standaloneDisks = (data.physical_disks || []).filter(d => {
      return !allRaidMemberDiskNames.has(d.name) && !allRaidMemberDiskNames.has(d.path);
    });

    if (standaloneCountBadge) {
      standaloneCountBadge.textContent = `${standaloneDisks.length} Disques Autonomes`;
    }

    if (standaloneContainer) {
      if (standaloneDisks.length === 0) {
        standaloneContainer.innerHTML = `
          <div style="padding: 24px; text-align: center; color: var(--subtext0); font-size: 0.88rem; background: var(--surface0); border-radius: var(--radius-md); border: 1px dashed rgba(255,255,255,0.08);">
            Tous les disques physiques sont membres d'une grappe RAID active.
          </div>
        `;
      } else {
        standaloneContainer.innerHTML = standaloneDisks.map(d => {
          const isStandby = d.power_state.toLowerCase().includes("veille") || d.power_state.toLowerCase().includes("standby");
          const isNvme = d.disk_type.includes("NVMe");
          const driveIcon = isNvme ? "⚡" : "💿";

          let tempColor = "var(--green)";
          if (d.temperature_c > 45) tempColor = "var(--peach)";
          if (d.temperature_c > 52) tempColor = "var(--red)";
          const tempVal = d.temperature_c > 0 ? `${d.temperature_c} °C` : (isStandby ? "Veille" : "N/A");

          const totalDiskBytes = d.size_bytes || 1;
          const partitions = d.partitions || [];

          // Générer les segments de la barre visuelle proportionnelle
          let segmentsHtml = "";
          if (partitions.length > 0) {
            segmentsHtml = partitions.map(p => {
              const pct = Math.max(1.5, Math.min(100, ((p.size_bytes || 0) / totalDiskBytes) * 100));
              let segClass = "is-unmounted";
              if (p.is_free_space) segClass = "is-free";
              else if (p.is_system) segClass = "is-sys";
              else if (p.mountpoint) segClass = "is-mounted";

              const labelText = p.is_free_space
                ? `Libre (${p.size_human})`
                : `${p.name.replace('/dev/', '')} (${p.size_human}${p.fstype ? ` &bull; ${p.fstype.toUpperCase()}` : ''})`;

              return `
                <div class="partition-segment ${segClass}" style="width: ${pct.toFixed(2)}%;" title="${escapeHtml(labelText)}">
                  ${pct > 8 ? `<span>${escapeHtml(p.name.replace('/dev/', ''))}</span>` : ''}
                </div>
              `;
            }).join("");
          } else {
            segmentsHtml = `<div class="partition-segment is-free" style="width: 100%;"><span>Disque non partitionné (${escapeHtml(d.size_human)})</span></div>`;
          }

          // Partitions & Free Space sub-items
          const partitionsRowsHtml = partitions.length > 0 ? partitions.map(p => {
            if (p.is_free_space) {
              return `
                <div class="disk-partition-item is-free-space">
                  <div class="part-col-left">
                    <span style="font-size:1.2rem;">✨</span>
                    <div>
                      <strong style="color:var(--green); font-size:0.88rem;">Espace Libre Non Alloué</strong>
                      <div style="font-size:0.75rem; color:var(--subtext0);">${escapeHtml(p.size_human)} disponibles pour une nouvelle partition</div>
                    </div>
                  </div>
                  <div class="part-col-middle">
                    <span class="badge badge-success" style="font-size:0.75rem; font-weight:700;">${escapeHtml(p.size_human)}</span>
                  </div>
                  <div class="part-col-actions">
                    <button type="button" class="btn btn-primary btn-xs" onclick="openCreatePartitionModal('${escapeHtml(d.path)}', '${escapeHtml(d.model)}', ${p.size_bytes})">
                      <span>➕</span> Créer une Partition
                    </button>
                  </div>
                </div>
              `;
            }

            const isSys = p.is_system;
            const isMounted = !!p.mountpoint;
            const isPersisted = isMounted && persistedMountPoints.has(p.mountpoint);

            let actionsHtml = "";
            if (isSys) {
              actionsHtml = `<span class="badge badge-primary" style="font-size:0.72rem;">🔒 Partition Système NixOS</span>`;
            } else {
              const mountBtn = isMounted
                ? `<button type="button" class="btn btn-secondary btn-xs" onclick="umountVolume('${escapeHtml(p.mountpoint)}')"><span>⏏️</span> Démonter</button>`
                : `<button type="button" class="btn btn-primary btn-xs" onclick="openMountVolumeModal('${escapeHtml(p.label || p.name)}', '${escapeHtml(p.path)}', '${escapeHtml(p.fstype || 'Partition')}', false, '${escapeHtml(p.fstype || '')}')"><span>📁</span> Monter</button>`;

              const delBtn = `<button type="button" class="btn btn-danger btn-xs" style="border:1px solid rgba(243,139,168,0.4); background:rgba(243,139,168,0.1); color:var(--red);" onclick="openDeletePartitionModal('${escapeHtml(p.path)}', '${escapeHtml(p.size_human)}')"><span>🗑️</span> Supprimer</button>`;

              actionsHtml = `
                ${mountBtn}
                ${delBtn}
              `;
            }

            return `
              <div class="disk-partition-item">
                <div class="part-col-left">
                  <span style="font-size:1.15rem;">${isMounted ? '📁' : '🖴'}</span>
                  <div>
                    <div style="font-weight:700; font-family:var(--font-mono); font-size:0.88rem; color:var(--text);">
                      ${escapeHtml(p.name)}
                      ${p.label ? `<span class="badge badge-accent" style="margin-left:6px; font-size:0.68rem;">${escapeHtml(p.label)}</span>` : ''}
                    </div>
                    <div style="font-size:0.75rem; color:var(--subtext0);">
                      ${p.mountpoint ? `<strong style="color:var(--green);">Monté sur ${escapeHtml(p.mountpoint)}</strong>` : 'Non monté'}
                    </div>
                  </div>
                </div>

                <div class="part-col-middle">
                  <span class="raid-tele-pill"><strong>${escapeHtml(p.size_human)}</strong></span>
                  <span class="badge ${p.fstype ? 'badge-secondary' : 'badge-warning'}" style="font-size:0.72rem;">
                    ${escapeHtml(p.fstype ? p.fstype.toUpperCase() : 'NON FORMATÉ')}
                  </span>
                  ${isPersisted ? `<span class="badge badge-accent" style="font-size:0.68rem;">💾 NixOS Persistant</span>` : ''}
                </div>

                <div class="part-col-actions">
                  ${actionsHtml}
                </div>
              </div>
            `;
          }).join("") : `
            <div class="disk-partition-item is-free-space">
              <div class="part-col-left">
                <span style="font-size:1.2rem;">✨</span>
                <div>
                  <strong style="color:var(--green); font-size:0.88rem;">Disque Entier Non Alloué</strong>
                  <div style="font-size:0.75rem; color:var(--subtext0);">${escapeHtml(d.size_human)} disponibles pour créer une partition</div>
                </div>
              </div>
              <div class="part-col-middle">
                <span class="badge badge-success">${escapeHtml(d.size_human)}</span>
              </div>
              <div class="part-col-actions">
                <button type="button" class="btn btn-primary btn-xs" onclick="openCreatePartitionModal('${escapeHtml(d.path)}', '${escapeHtml(d.model)}', ${d.size_bytes})">
                  <span>➕</span> Créer une Partition
                </button>
              </div>
            </div>
          `;

          const spindownBtn = d.is_rotational
            ? `<button type="button" class="btn btn-secondary btn-xs" onclick="triggerSpindown('${escapeHtml(d.name)}')"><span>🌙</span> Veille</button>`
            : "";

          return `
            <div class="standalone-disk-card ${d.is_system ? 'is-system' : ''}">
              <div class="disk-card-header">
                <div class="disk-card-title-group">
                  <div class="disk-card-icon">${driveIcon}</div>
                  <div>
                    <div class="disk-card-title">
                      <span class="badge badge-secondary" style="font-size:0.72rem; font-weight:700;">${escapeHtml(d.bay_label || d.name)}</span>
                      <span>${escapeHtml(d.model)}</span>
                      ${d.is_system ? `<span class="badge badge-primary">🔒 Système NixOS</span>` : ''}
                    </div>
                    <div class="disk-card-sub">
                      <code>${escapeHtml(d.path)}</code> &bull; ${escapeHtml(d.disk_type)} &bull; S/N: <code>${escapeHtml(d.serial)}</code>
                    </div>
                  </div>
                </div>

                <div class="disk-card-telemetry">
                  <span class="raid-tele-pill" style="font-size:0.85rem; font-weight:700; color:var(--text);">${escapeHtml(d.size_human)}</span>
                  <span class="raid-tele-pill" style="color:${tempColor};">🌡️ ${tempVal}</span>
                  <span class="raid-tele-pill" style="color:var(--teal);">🛡️ ${escapeHtml(d.smart_status)}</span>
                  <span class="badge ${isStandby ? 'badge-warning' : 'badge-success'}">${isStandby ? '🌙 Veille' : '✅ Actif'}</span>
                  ${spindownBtn}
                </div>
              </div>

              <!-- Barre Visuelle de Partitionnement -->
              <div class="disk-partition-bar-wrap">
                <div class="disk-partition-bar-label">
                  <span>Structure des partitions physiques</span>
                  <span>Capacité totale : ${escapeHtml(d.size_human)}</span>
                </div>
                <div class="disk-partition-bar">
                  ${segmentsHtml}
                </div>
              </div>

              <!-- Liste Groupée des Partitions & Espaces Libres -->
              <div class="disk-partitions-list">
                ${partitionsRowsHtml}
              </div>
            </div>
          `;
        }).join("");
      }
    }

    // 3. SECTION 3 : Périphériques Amovibles & Médias Externes
    const removableContainer = document.getElementById("removable-devices-container");
    if (removableContainer) {
      const removableList = data.removable_devices || [];
      if (removableList.length === 0) {
        removableContainer.innerHTML = `
          <div style="padding: 28px; text-align: center; background: var(--surface0); border-radius: var(--radius-md); border: 1px dashed rgba(255,255,255,0.1);">
            <div style="font-size: 2.2rem; margin-bottom: 8px;">🔌</div>
            <div style="font-weight: 700; color: var(--text); font-size: 1.05rem; margin-bottom: 4px;">Aucun média externe amovible détecté</div>
            <div style="font-size: 0.84rem; color: var(--subtext0); max-width: 480px; margin: 0 auto;">
              Connectez une clé USB, un disque externe USB ou insérez un disque CD/DVD/Blu-ray. Il sera automatiquement reconnu et accessible ici.
            </div>
          </div>
        `;
      } else {
        removableContainer.innerHTML = `
          <div class="removable-devices-grid">
            ${removableList.map(dev => {
              const icon = dev.is_optical ? "💿" : "🔌";
              const partsList = dev.partitions || [];
              const isMounted = dev.is_mounted || partsList.some(p => p.mountpoint) || !!dev.mountpoint;
              const mountedPart = partsList.find(p => p.mountpoint);
              const unmountedPart = partsList.find(p => !p.mountpoint);
              const mountPath = dev.mountpoint || (mountedPart ? mountedPart.mountpoint : null);

              const partsHtml = partsList.length > 0
                ? `<div class="removable-partitions-list">
                     ${partsList.map(p => {
                       const pMounted = !!p.mountpoint;
                       return `
                         <div class="removable-partition-row">
                           <div class="removable-part-left">
                             <code style="color:var(--text); font-weight:700;">${escapeHtml(p.name)}</code>
                             <span class="badge badge-secondary" style="font-size:0.7rem;">${escapeHtml(p.size_human)}</span>
                             ${p.fstype ? `<span class="badge badge-primary" style="font-size:0.7rem;">${escapeHtml(p.fstype.toUpperCase())}</span>` : ''}
                             ${p.label ? `<span class="badge badge-accent" style="font-size:0.7rem;">${escapeHtml(p.label)}</span>` : ''}
                             ${pMounted 
                               ? `<span class="badge badge-success" style="font-size:0.7rem;">✅ ${escapeHtml(p.mountpoint)}</span>` 
                               : `<span class="badge badge-warning" style="font-size:0.7rem;">⏸️ Non monté</span>`}
                           </div>
                           <div class="removable-part-right">
                             ${pMounted
                               ? `<button type="button" class="btn btn-secondary btn-xs" onclick="openFilesAtPath('${escapeHtml(p.mountpoint)}')" title="Ouvrir dans l'explorateur de fichiers">
                                    <span>📁</span> Explorer
                                  </button>
                                  <button type="button" class="btn btn-secondary btn-xs" onclick="umountVolume('${escapeHtml(p.mountpoint)}')" title="Démonter cette partition">
                                    <span>⏸️</span> Démonter
                                  </button>`
                               : `<button type="button" class="btn btn-glow-mount btn-xs" onclick="openMountRemovableModal('${escapeHtml(p.path)}', '${escapeHtml(p.name)}', '${escapeHtml(dev.model || dev.name)}', '${escapeHtml(p.fstype || '')}', '${escapeHtml(p.label || '')}', '${escapeHtml(p.size_human)}')" title="Monter cette partition">
                                    <span>⚡</span> Monter
                                  </button>`}
                           </div>
                         </div>
                       `;
                     }).join("")}
                   </div>`
                : "";

              let actionButtonsHtml = "";
              if (partsList.length === 0) {
                if (isMounted) {
                  actionButtonsHtml = `
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openFilesAtPath('${escapeHtml(mountPath)}')">
                      <span>📁</span> Explorer
                    </button>
                    <button type="button" class="btn btn-warning btn-sm" onclick="ejectRemovableDevice('${escapeHtml(dev.path)}', '${escapeHtml(dev.model || dev.name)}')">
                      <span>⏏️</span> Éjecter
                    </button>
                  `;
                } else {
                  actionButtonsHtml = `
                    <button type="button" class="btn btn-glow-mount btn-sm" onclick="openMountRemovableModal('${escapeHtml(dev.path)}', '${escapeHtml(dev.name)}', '${escapeHtml(dev.model || dev.name)}', '${escapeHtml(dev.fstype || '')}', '${escapeHtml(dev.label || '')}', '${escapeHtml(dev.size_human)}')">
                      <span>⚡</span> Monter
                    </button>
                    <button type="button" class="btn btn-warning btn-sm" onclick="ejectRemovableDevice('${escapeHtml(dev.path)}', '${escapeHtml(dev.model || dev.name)}')">
                      <span>⏏️</span> Éjecter
                    </button>
                  `;
                }
              } else {
                actionButtonsHtml = `
                  ${unmountedPart ? `
                    <button type="button" class="btn btn-glow-mount btn-sm" onclick="openMountRemovableModal('${escapeHtml(unmountedPart.path)}', '${escapeHtml(unmountedPart.name)}', '${escapeHtml(dev.model || dev.name)}', '${escapeHtml(unmountedPart.fstype || '')}', '${escapeHtml(unmountedPart.label || '')}', '${escapeHtml(unmountedPart.size_human)}')">
                      <span>⚡</span> Monter
                    </button>
                  ` : ''}
                  ${mountedPart ? `
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openFilesAtPath('${escapeHtml(mountedPart.mountpoint)}')">
                      <span>📁</span> Explorer
                    </button>
                  ` : ''}
                  <button type="button" class="btn btn-warning btn-sm" onclick="ejectRemovableDevice('${escapeHtml(dev.path)}', '${escapeHtml(dev.model || dev.name)}')">
                    <span>⏏️</span> Éjecter
                  </button>
                `;
              }

              const statusText = isMounted
                ? (mountPath ? `✅ Prêt &bull; <strong style="color:var(--green); font-family:var(--font-mono);">${escapeHtml(mountPath)}</strong>` : `✅ Connecté et monté`)
                : `⏸️ Périphérique détecté (non monté)`;

              return `
                <div class="removable-device-card ${dev.is_optical ? 'is-optical' : 'is-usb'}">
                  <div class="removable-card-top">
                    <div class="removable-icon-box">${icon}</div>
                    <div class="removable-info-wrap">
                      <div class="removable-model">${escapeHtml(dev.model || dev.name)}</div>
                      <div class="removable-dev-path">
                        <code>${escapeHtml(dev.path)}</code>
                        ${dev.vendor ? ` &bull; ${escapeHtml(dev.vendor)}` : ''}
                      </div>
                      <div class="removable-badges-row">
                        <span class="badge badge-secondary">${escapeHtml(dev.size_human)}</span>
                        ${dev.fstype ? `<span class="badge badge-primary">${escapeHtml(dev.fstype.toUpperCase())}</span>` : ''}
                        ${dev.label ? `<span class="badge badge-accent">${escapeHtml(dev.label)}</span>` : ''}
                        <span class="badge ${isMounted ? 'badge-success' : 'badge-warning'}">
                          ${isMounted ? (mountPath ? `✅ Monté sur ${escapeHtml(mountPath)}` : '✅ Monté') : '⏸️ Non monté'}
                        </span>
                      </div>
                      ${partsHtml}
                    </div>
                  </div>

                  <div class="removable-card-bottom">
                    <div style="font-size:0.8rem; color:var(--subtext0);">
                      ${statusText}
                    </div>
                    <div class="removable-card-actions">
                      ${actionButtonsHtml}
                    </div>
                  </div>
                </div>
              `;
            }).join("")}
          </div>
        `;
      }
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
    if (details) details.textContent = `Vitesse : ${syncData.speed_mb_s} Mo/s • Fin estimée : ${syncData.finish_human}`;
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
let currentSelectedFsType = "btrfs";

function openCreateRaidModal() {
  const modal = document.getElementById("create-raid-modal");
  const checklist = document.getElementById("raid-disks-checklist");
  const errEl = document.getElementById("raid-submit-error");
  if (!modal || !checklist) return;

  if (errEl) errEl.textContent = "";

  // Filtrer les disques éligibles (non-système)
  const eligibleDisks = cachedStorageDisks.filter(d => !d.is_system);

  if (eligibleDisks.length === 0) {
    checklist.innerHTML = `<div style="color:var(--subtext0); padding:20px; text-align:center; background:rgba(30,30,46,0.5); border-radius:10px;">⚠️ Aucun disque physique supplémentaire disponible pour créer un RAID.</div>`;
  } else {
    checklist.innerHTML = eligibleDisks.map((d, idx) => {
      // Par défaut pour 4 disques, tous cochés pour RAID 5
      const isChecked = idx < 4;
      const busType = d.tran ? d.tran.toUpperCase() : (d.path.includes("nvme") ? "NVMe" : "SATA 6Gb/s");
      return `
        <div class="raid-disk-row-card ${isChecked ? "selected" : ""}" id="raid-disk-row-${idx}" onclick="toggleRaidDiskRow('${escapeHtml(d.path)}', ${idx})">
          <div class="raid-disk-row-left">
            <div class="raid-disk-checkbox-custom" id="raid-cb-box-${idx}">
              ${isChecked ? "✔" : ""}
            </div>
            <input type="checkbox" class="raid-disk-checkbox" id="raid-disk-cb-${idx}" value="${escapeHtml(d.path)}" data-size="${escapeHtml(d.size_human)}" data-model="${escapeHtml(d.model)}" data-idx="${idx}" ${isChecked ? "checked" : ""} style="display:none;" onchange="updateRaidPreview()">
            <div class="raid-disk-icon-box">
              <span>💿</span>
              <span class="raid-disk-led"></span>
            </div>
            <div class="raid-disk-meta">
              <div class="raid-disk-title-line">
                <span class="raid-disk-path">${escapeHtml(d.path)}</span>
                <span class="raid-disk-bus-badge">${escapeHtml(busType)}</span>
                <span class="badge badge-success" style="font-size:0.65rem; padding:1px 6px;">Prêt</span>
              </div>
              <div class="raid-disk-model">${escapeHtml(d.model || "Disque de stockage standard")}</div>
            </div>
          </div>
          <div class="raid-disk-row-right">
            <span class="raid-disk-size-pill">${escapeHtml(d.size_human)}</span>
          </div>
        </div>
      `;
    }).join("");
  }

  currentSelectedRaidLevel = "raid5";
  currentSelectedFsType = "btrfs";
  updateRaidLevelPickerUI();
  updateFsTypePickerUI();
  onRaidNameInput();
  updateRaidPreview();
  modal.style.display = "flex";
}

function toggleRaidDiskRow(path, idx) {
  const cb = document.getElementById(`raid-disk-cb-${idx}`);
  const row = document.getElementById(`raid-disk-row-${idx}`);
  const box = document.getElementById(`raid-cb-box-${idx}`);
  if (!cb || !row || !box) return;

  cb.checked = !cb.checked;
  row.classList.toggle("selected", cb.checked);
  box.textContent = cb.checked ? "✔" : "";
  updateRaidPreview();
}

function toggleSelectAllRaidDisks(selectAll) {
  const checkboxes = document.querySelectorAll(".raid-disk-checkbox");
  checkboxes.forEach((cb) => {
    cb.checked = selectAll;
    const idx = cb.getAttribute("data-idx");
    const row = document.getElementById(`raid-disk-row-${idx}`);
    const box = document.getElementById(`raid-cb-box-${idx}`);
    if (row) row.classList.toggle("selected", selectAll);
    if (box) box.textContent = selectAll ? "✔" : "";
  });
  updateRaidPreview();
}

function selectFsType(fs) {
  currentSelectedFsType = fs;
  const input = document.getElementById("raid-input-fstype");
  if (input) input.value = fs;
  updateFsTypePickerUI();

  const sumFs = document.getElementById("raid-sum-fs");
  if (sumFs) {
    if (fs === "btrfs") sumFs.textContent = "Btrfs (Snapshots + CoW + Checksums)";
    else if (fs === "xfs") sumFs.textContent = "XFS (Multimédia 4K & Gros Débits)";
    else sumFs.textContent = "Ext4 (Fiabilité & Empreinte Légère)";
  }
}

function updateFsTypePickerUI() {
  const cards = document.querySelectorAll(".fs-type-card");
  cards.forEach(c => {
    c.classList.toggle("active", c.getAttribute("data-fs") === currentSelectedFsType);
  });
}

function onRaidNameInput() {
  const nameInput = document.getElementById("raid-input-name");
  const hintDev = document.getElementById("raid-hint-dev");
  const mountInput = document.getElementById("raid-input-mount");
  const sumMount = document.getElementById("raid-sum-mount");

  const rawName = (nameInput?.value || "").trim().toLowerCase().replace(/[^a-z0-9_-]/g, "-");
  const cleanName = rawName || "storage-data";

  if (hintDev) hintDev.textContent = `/dev/md/${cleanName}`;
  if (mountInput && (!mountInput.value || mountInput.value.startsWith("/mnt/"))) {
    mountInput.value = `/mnt/${cleanName}`;
  }
  if (sumMount && mountInput) {
    sumMount.textContent = mountInput.value || `/mnt/${cleanName}`;
  }
}

function onRaidMountInput() {
  const mountInput = document.getElementById("raid-input-mount");
  const sumMount = document.getElementById("raid-sum-mount");
  if (sumMount && mountInput) {
    sumMount.textContent = mountInput.value || "/mnt/storage-data";
  }
}

function closeCreateRaidModal() {
  const modal = document.getElementById("create-raid-modal");
  if (modal) {
    modal.classList.remove("mini-player-mode");
    const winMpv = modal.querySelector(".file-modal-window");
    if (winMpv) {
      winMpv.style.top = "";
      winMpv.style.right = "";
      winMpv.style.left = "";
      winMpv.style.bottom = "";
      winMpv.style.margin = "";
    }
    const pipIcon = document.getElementById("mpv-pip-icon");
    if (pipIcon) pipIcon.textContent = "🗗";
    modal.style.display = "none";
  }
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
  const checkedBoxes = Array.from(document.querySelectorAll(".raid-disk-checkbox:checked"));
  const count = checkedBoxes.length;

  const countBadge = document.getElementById("raid-disks-selected-count");
  if (countBadge) countBadge.textContent = count;

  const rawEl = document.getElementById("raid-preview-raw");
  const netEl = document.getElementById("raid-preview-net");
  const parityEl = document.getElementById("raid-preview-parity");
  const resEl = document.getElementById("raid-preview-resilience");
  const btnSubmit = document.getElementById("btn-submit-raid");
  const errEl = document.getElementById("raid-submit-error");
  const guidanceEl = document.getElementById("raid-visual-guidance");

  // Estimation de taille moyenne par disque (ex: 3.6 To -> 3.64)
  let unitSize = 3.64; // To
  if (checkedBoxes.length > 0) {
    const s0 = checkedBoxes[0].getAttribute("data-size") || "3,6T";
    const num = parseFloat(s0.replace(",", ".").replace("T", "").replace("G", ""));
    if (!isNaN(num) && num > 0) {
      unitSize = s0.includes("G") ? num / 1024 : num;
    }
  }

  const rawTotal = (count * unitSize).toFixed(1);
  let netTotal = 0;
  let parityTotal = 0;
  let resilienceText = "Sélectionnez des disques";
  let resilienceBadge = "badge-secondary";
  let isValid = false;
  let minReq = 2;
  let dataPercent = 100;
  let parityPercent = 0;

  switch (currentSelectedRaidLevel) {
    case "raid5":
      minReq = 3;
      if (count >= 3) {
        netTotal = ((count - 1) * unitSize).toFixed(1);
        parityTotal = (1 * unitSize).toFixed(1);
        dataPercent = Math.round(((count - 1) / count) * 100);
        parityPercent = 100 - dataPercent;
        resilienceText = "🛡️ Tolère la panne de 1 disque sans perte (Parité simple)";
        resilienceBadge = "badge-success";
        isValid = true;
      } else {
        resilienceText = "RAID 5 requiert au moins 3 disques physiques";
        resilienceBadge = "badge-danger";
      }
      break;

    case "raid1":
      minReq = 2;
      if (count >= 2) {
        netTotal = unitSize.toFixed(1);
        parityTotal = ((count - 1) * unitSize).toFixed(1);
        dataPercent = Math.round((1 / count) * 100);
        parityPercent = 100 - dataPercent;
        resilienceText = `🛡️ Miroir 1:${count-1} (Tolère ${count-1} panne(s) simultanée(s))`;
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
        parityTotal = (2 * unitSize).toFixed(1);
        dataPercent = Math.round(((count - 2) / count) * 100);
        parityPercent = 100 - dataPercent;
        resilienceText = "🛡️🛡️ Tolère la panne simultanée de 2 disques (Double parité P+Q)";
        resilienceBadge = "badge-success";
        isValid = true;
      } else {
        resilienceText = "RAID 6 requiert au moins 4 disques physiques";
        resilienceBadge = "badge-danger";
      }
      break;

    case "raid10":
      minReq = 4;
      if (count >= 4 && count % 2 === 0) {
        netTotal = ((count / 2) * unitSize).toFixed(1);
        parityTotal = ((count / 2) * unitSize).toFixed(1);
        dataPercent = 50;
        parityPercent = 50;
        resilienceText = "⚡🛡️ Performance maximale + Tolère 1 panne par paire miroir";
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
        parityTotal = 0;
        dataPercent = 100;
        parityPercent = 0;
        resilienceText = "⚠️ 0 tolérance aux pannes (Perte totale si 1 disque flanche)";
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
        parityTotal = 0;
        dataPercent = 100;
        parityPercent = 0;
        resilienceText = "📦 Concaténation JBOD (0 redondance)";
        resilienceBadge = "badge-warning";
        isValid = true;
      }
      break;
  }

  if (rawEl) rawEl.textContent = `${rawTotal} To`;
  if (netEl) netEl.textContent = `${netTotal} To`;
  if (parityEl) parityEl.textContent = `${parityTotal} To`;
  if (resEl) {
    resEl.textContent = resilienceText;
    resEl.className = `badge ${resilienceBadge}`;
  }

  // Mise à jour de la jauge bicolore
  const splitDataSeg = document.getElementById("split-data-seg");
  const splitParitySeg = document.getElementById("split-parity-seg");
  const splitDataLabel = document.getElementById("split-data-label");
  const splitParityLabel = document.getElementById("split-parity-label");

  if (splitDataSeg && splitParitySeg) {
    if (count > 0 && isValid) {
      splitDataSeg.style.width = `${dataPercent}%`;
      splitParitySeg.style.width = `${parityPercent}%`;
      if (splitDataLabel) splitDataLabel.textContent = `Données Utiles ${dataPercent}% (${netTotal} To)`;
      if (splitParityLabel) splitParityLabel.textContent = parityPercent > 0 ? `Parité ${parityPercent}% (${parityTotal} To)` : `0% Parité`;
    } else {
      splitDataSeg.style.width = "100%";
      splitParitySeg.style.width = "0%";
      if (splitDataLabel) splitDataLabel.textContent = `Sélectionnez au moins ${minReq} disques`;
      if (splitParityLabel) splitParityLabel.textContent = "";
    }
  }

  // Rendu ultra-graphique des disques et blocs logiques
  renderRaidVisualDiagram(currentSelectedRaidLevel, checkedBoxes, minReq, unitSize);

  if (btnSubmit) {
    btnSubmit.disabled = !isValid;
    btnSubmit.classList.toggle("disabled", !isValid);
    if (isValid) {
      btnSubmit.innerHTML = `🚀 Créer et Initialiser la Grappe RAID (${count} Disques - ${netTotal} To Utiles)`;
    } else {
      btnSubmit.innerHTML = `🚀 Créer et Initialiser la Grappe RAID (Sélection incomplète)`;
    }
  }

  if (errEl) {
    errEl.textContent = isValid ? "" : `⚠️ Configuration incomplète : ${count}/${minReq} disques requis pour ${currentSelectedRaidLevel.toUpperCase()}.`;
  }

  if (guidanceEl) {
    if (!isValid && count > 0) {
      guidanceEl.style.display = "flex";
      guidanceEl.innerHTML = `<span>⚠️</span> <span>Pour assembler une grappe en <strong>${currentSelectedRaidLevel.toUpperCase()}</strong>, cochez au moins <strong>${minReq} disques</strong> physiques (actuellement : ${count}).</span>`;
    } else {
      guidanceEl.style.display = "none";
    }
  }

  // Synchronisation des métriques du cockpit de droite
  const sumMount = document.getElementById("raid-sum-mount");
  const mountInput = document.getElementById("raid-input-mount");
  if (sumMount && mountInput) sumMount.textContent = mountInput.value || "/mnt/storage-data";

  const sumFs = document.getElementById("raid-sum-fs");
  if (sumFs) {
    if (currentSelectedFsType === "btrfs") sumFs.textContent = "Btrfs (Snapshots + CoW + Checksums)";
    else if (currentSelectedFsType === "xfs") sumFs.textContent = "XFS (Multimédia 4K & Gros Débits)";
    else sumFs.textContent = "Ext4 (Fiabilité & Empreinte Légère)";
  }

  const sumRes = document.getElementById("raid-sum-resilience");
  if (sumRes) {
    sumRes.textContent = isValid ? resilienceText : "⚠️ Configuration incomplète";
    sumRes.style.color = isValid ? "var(--green)" : "var(--red)";
  }

  const onlineIndicator = document.getElementById("rack-disks-online-indicator");
  if (onlineIndicator) {
    onlineIndicator.textContent = isValid ? `● ${count} DISQUE(S) ACTIFS` : `● ATTENTE DISQUES (${count}/${minReq})`;
    onlineIndicator.style.color = isValid ? "var(--green)" : "var(--peach)";
  }
}

// RENDU GRAPHIQUE SPECTACULAIRE DU SCHÉMA DES DISQUES ET BLOCS DE PARITÉ
function renderRaidVisualDiagram(level, checkedBoxes, minReq, unitSize) {
  const rack = document.getElementById("raid-disk-rack-visual");
  if (!rack) return;

  const count = checkedBoxes.length;

  if (count === 0) {
    rack.innerHTML = `
      <div style="grid-column: 1 / -1; padding: 30px; text-align: center; color: var(--subtext0);">
        <div style="font-size: 2rem; margin-bottom: 8px;">🗄️</div>
        <strong>Aucun disque sélectionné</strong>
        <div style="font-size: 0.78rem; margin-top: 4px;">Cochez des disques dans la liste ci-dessus pour visualiser la répartition des blocs de données et de parité.</div>
      </div>
    `;
    return;
  }

  let html = "";

  // Génération des baies pour chaque disque physique coché
  checkedBoxes.forEach((cb, idx) => {
    const devPath = cb.value;
    const size = cb.getAttribute("data-size") || "4 To";

    let blocksHtml = "";
    // Génération de 4 tranches/blocs illustrant le striping et la parité
    for (let stripe = 0; stripe < 4; stripe++) {
      let blockClass = "block-data";
      let blockLabel = `DATA [D${stripe * count + idx + 1}]`;

      if (level === "raid0") {
        blockClass = "block-data";
        blockLabel = `STRIPE A${stripe + 1}`;
      } else if (level === "raid1") {
        if (idx === 0) {
          blockClass = "block-data";
          blockLabel = `DATA [D${stripe + 1}]`;
        } else {
          blockClass = "block-mirror";
          blockLabel = `MIROIR [M${stripe + 1}]`;
        }
      } else if (level === "raid5") {
        // Parité distribuée en diagonale
        const parityDiskIndex = (3 - stripe) % count;
        if (idx === parityDiskIndex) {
          blockClass = "block-parity";
          blockLabel = `PARITÉ [P${stripe + 1}]`;
        } else {
          blockClass = "block-data";
          blockLabel = `DATA [D${stripe + 1}.${idx + 1}]`;
        }
      } else if (level === "raid6") {
        // Double parité P + Q
        const pIndex = (4 - stripe) % count;
        const qIndex = (5 - stripe) % count;
        if (idx === pIndex) {
          blockClass = "block-parity";
          blockLabel = `PARITÉ [P${stripe + 1}]`;
        } else if (idx === qIndex) {
          blockClass = "block-parity";
          blockLabel = `PARITÉ [Q${stripe + 1}]`;
        } else {
          blockClass = "block-data";
          blockLabel = `DATA [D${stripe + 1}.${idx + 1}]`;
        }
      } else if (level === "raid10") {
        const pairId = Math.floor(idx / 2) + 1;
        if (idx % 2 === 0) {
          blockClass = "block-data";
          blockLabel = `DATA P${pairId}.${stripe + 1}`;
        } else {
          blockClass = "block-mirror";
          blockLabel = `MIR P${pairId}.${stripe + 1}`;
        }
      } else {
        blockClass = "block-data";
        blockLabel = `BLOC ${stripe + 1}`;
      }

      blocksHtml += `<div class="disk-block-stripe ${blockClass}">${blockLabel}</div>`;
    }

    html += `
      <div class="disk-bay-slot">
        <div class="disk-bay-header">
          <div>
            <div class="disk-bay-name">${escapeHtml(devPath)}</div>
            <div class="disk-bay-size">Baie #${idx + 1} • ${escapeHtml(size)}</div>
          </div>
          <span class="disk-bay-led" title="Disque sain et prêt"></span>
        </div>
        <div class="disk-blocks-stack">
          ${blocksHtml}
        </div>
      </div>
    `;
  });

  // Emplacements fantômes (Ghost slots) si le nombre requis n'est pas atteint
  if (count < minReq) {
    const missing = minReq - count;
    for (let m = 0; m < missing; m++) {
      html += `
        <div class="disk-bay-slot ghost-slot">
          <div class="ghost-icon">➕</div>
          <div class="ghost-text">Disque #${count + m + 1} Requis</div>
          <div style="font-size:0.68rem; color:var(--subtext0);">Cochez 1 disque supplémentaire</div>
        </div>
      `;
    }
  }

  rack.innerHTML = html;
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
      showToast("🛠️ Tâche d'assemblage RAID lancée en arrière-plan !", "success", 4000);
      closeCreateRaidModal();
      activeStorageJob = json.data;
      updateStorageJobView(json.data);
      startPollingStorageJob();
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
  if (modal) {
    modal.classList.remove("mini-player-mode");
    const winAud = modal.querySelector(".file-modal-window");
    if (winAud) {
      winAud.style.top = "";
      winAud.style.right = "";
      winAud.style.left = "";
      winAud.style.bottom = "";
      winAud.style.margin = "";
    }
    const audPipIcon = document.getElementById("audio-pip-icon");
    if (audPipIcon) audPipIcon.textContent = "🗗";
    modal.style.display = "none";
  }
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
      showSystemError("Échec du formatage", "Le formatage du disque a échoué.", json.message || "Erreur de formatage", 12000);
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
// SERVICES CRITIQUES & APERÇU SERVEURS DE JEUX (VUE D'ENSEMBLE)
// --------------------------------------------------------------------------
async function loadServices() {
  try {
    const res = await fetch("/api/services");
    const json = await res.json();
    if (json.success && json.data) {
      const data = json.data;

      // Colonne 1 : Services critiques du NAS
      const nasWrap = document.getElementById("overview-nas-services-list");
      if (nasWrap && data.services) {
        nasWrap.innerHTML = data.services.map(s => {
          const pulseClass = s.is_active ? 'online' : 'offline';
          const statusText = s.is_active ? 'En ligne' : (s.sub_state || 'Arrêté');
          const statusBadgeClass = s.is_active ? 'badge-success' : 'badge-secondary';
          const icon = s.icon || '⚡';
          const addressHtml = s.address ? `<span class="service-address-code" title="${escapeHtml(s.address)}">${escapeHtml(s.address)}</span>` : '';

          return `
            <div class="overview-service-item">
              <div class="service-item-left">
                <div class="service-item-icon">${icon}</div>
                <div class="service-item-info">
                  <div class="service-item-title-row">
                    <span class="service-pulse-dot ${pulseClass}" title="${s.is_active ? 'Service opérationnel (En ligne)' : 'Service inactif'}"></span>
                    <span class="service-item-title">${escapeHtml(s.display_name)}</span>
                    <span class="badge ${statusBadgeClass}" style="font-size:0.68rem; padding:1px 6px;">${escapeHtml(statusText)}</span>
                  </div>
                  <div class="service-item-sub">${escapeHtml(s.unit_name)}</div>
                </div>
              </div>
              <div class="service-item-right">
                ${addressHtml}
                <button type="button" class="btn btn-secondary btn-xs" onclick="restartService('${escapeHtml(s.unit_name)}')" title="Redémarrer ${escapeHtml(s.display_name)}">🔄</button>
              </div>
            </div>
          `;
        }).join("");
      }

      // Rétrocompatibilité avec ancien sélecteur quick-services-wrap si présent
      const quickWrap = document.getElementById("quick-services-wrap");
      if (quickWrap && data.services) {
        quickWrap.innerHTML = data.services.map(s => `
          <div style="background:var(--mantle); border:1px solid rgba(255,255,255,0.06); padding:8px 14px; border-radius:var(--radius-md); display:flex; align-items:center; gap:10px;">
            <span class="service-pulse-dot ${s.is_active ? 'online' : 'offline'}"></span>
            <span style="font-size:0.85rem; font-weight:600;">${escapeHtml(s.display_name)}</span>
            <span style="font-size:0.75rem; color:var(--subtext0);">(${escapeHtml(s.sub_state)})</span>
            <button type="button" class="btn btn-secondary btn-xs" onclick="restartService('${escapeHtml(s.unit_name)}')">🔄</button>
          </div>
        `).join("");
      }
    }
  } catch (err) {
    console.warn("Erreur fetch /api/services:", err);
  }

  // Alimentation de la colonne 2 : Liste minimale des serveurs de jeux
  loadOverviewGameServers();
}

async function loadOverviewGameServers() {
  const container = document.getElementById("overview-game-servers-list");
  if (!container) return;

  try {
    if (typeof gameServersData !== 'undefined' && Array.isArray(gameServersData) && gameServersData.length > 0) {
      renderOverviewGameServers(gameServersData);
      return;
    }
    const res = await fetch("/api/games/servers");
    const json = await res.json();
    if (json.success && Array.isArray(json.data)) {
      if (typeof gameServersData !== 'undefined') {
        gameServersData = json.data;
      }
      renderOverviewGameServers(json.data);
    } else {
      renderOverviewGameServers([]);
    }
  } catch (err) {
    console.warn("Erreur chargement serveurs de jeu aperçu:", err);
    renderOverviewGameServers([]);
  }
}

function renderOverviewGameServers(servers) {
  const container = document.getElementById("overview-game-servers-list");
  if (!container) return;

  if (!servers || servers.length === 0) {
    container.innerHTML = `
      <div class="game-servers-empty-card">
        <div style="font-size:2rem; margin-bottom:8px; opacity:0.6;">🎮</div>
        <div style="font-weight:700; font-size:0.9rem; color:var(--text); margin-bottom:4px;">Aucun serveur de jeu actif</div>
        <div style="font-size:0.75rem; color:var(--subtext0); margin-bottom:12px; max-width:280px;">Déployez votre premier serveur de jeu (Minecraft, Palworld, Rust, Ark...) en un clic.</div>
        <button type="button" class="btn btn-primary btn-xs" onclick="switchTab('tab-games')">
          <span>➕ Déployer un serveur</span>
        </button>
      </div>
    `;
    return;
  }

  container.innerHTML = servers.map(srv => {
    const isOnline = srv.status === 'online';
    const isStarting = srv.status === 'starting' || srv.status === 'deploying';
    let pulseClass = 'stopped';
    let statusText = 'Arrêté';
    let badgeClass = 'badge-secondary';

    if (isOnline) {
      pulseClass = 'online';
      statusText = 'En ligne';
      badgeClass = 'badge-success';
    } else if (isStarting) {
      pulseClass = 'starting';
      statusText = srv.status === 'deploying' ? 'Déploiement' : 'Démarrage';
      badgeClass = 'badge-warning';
    } else if (srv.status === 'error') {
      pulseClass = 'stopped';
      statusText = 'Erreur';
      badgeClass = 'badge-danger';
    }

    const iconHtml = srv.icon_url 
      ? `<img src="${escapeHtml(srv.icon_url)}" style="width:24px; height:24px; object-fit:contain; border-radius:4px;" alt="icon" onerror="this.outerHTML='🎮'">`
      : (srv.icon || '🎮');

    const address = srv.lan_ip && srv.port ? `${srv.lan_ip}:${srv.port}` : (srv.port ? `Port ${srv.port}` : 'N/A');

    return `
      <div class="overview-service-item" style="cursor:pointer;" onclick="switchTab('tab-games')" title="Gérer le serveur ${escapeHtml(srv.name)}">
        <div class="service-item-left">
          <div class="service-item-icon">${iconHtml}</div>
          <div class="service-item-info">
            <div class="service-item-title-row">
              <span class="service-pulse-dot ${pulseClass}" title="${statusText}"></span>
              <span class="service-item-title">${escapeHtml(srv.name)}</span>
              <span class="badge ${badgeClass}" style="font-size:0.68rem; padding:1px 6px;">${escapeHtml(statusText)}</span>
            </div>
            <div class="service-item-sub">${escapeHtml(srv.game_name || srv.egg_id || 'Serveur de jeu')}</div>
          </div>
        </div>
        <div class="service-item-right">
          <span class="service-address-code" title="${escapeHtml(address)}">${escapeHtml(address)}</span>
          <span style="font-size:0.8rem; color:var(--subtext0); opacity:0.6;">➔</span>
        </div>
      </div>
    `;
  }).join('');
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
// EXPLORATEUR DE LOGS SYSTÈME PRO
// --------------------------------------------------------------------------
let rawLogsBuffer = "";
let logsLiveInterval = null;
let isLogsLiveActive = false;

function onLogUnitChanged() {
  const select = document.getElementById("select-log-unit");
  const customInput = document.getElementById("input-custom-log-unit");
  if (!select) return;

  if (select.value === "__CUSTOM__") {
    if (customInput) {
      customInput.style.display = "inline-block";
      customInput.focus();
    }
  } else {
    if (customInput) {
      customInput.style.display = "none";
    }
    loadLogs();
  }
}

async function loadLogs() {
  const selectUnit = document.getElementById("select-log-unit");
  const customInput = document.getElementById("input-custom-log-unit");
  const selectPrio = document.getElementById("select-log-priority");
  const selectLines = document.getElementById("select-log-lines");
  const content = document.getElementById("terminal-content");
  const title = document.getElementById("terminal-unit-title");

  let unit = selectUnit ? selectUnit.value : "_SYSTEM_";
  if (unit === "__CUSTOM__") {
    unit = customInput && customInput.value.trim() ? customInput.value.trim() : "_SYSTEM_";
  }

  const priority = selectPrio ? selectPrio.value : "all";
  const lines = selectLines ? selectLines.value : "250";

  if (title) {
    let prioFlag = priority && priority !== "all" ? ` -p ${priority}` : "";
    let unitFlag = (unit === "_SYSTEM_" || unit === "all") ? "" : (unit === "_KERNEL_" ? " -k" : (unit === "_BOOT_" ? " -b" : ` -u ${unit}`));
    title.textContent = `journalctl${unitFlag}${prioFlag} -n ${lines} --no-pager`;
  }

  if (content && !rawLogsBuffer && !isLogsLiveActive) {
    content.innerHTML = `<span style="color:var(--subtext0);">Lecture du journal système...</span>`;
  }

  try {
    const params = new URLSearchParams({
      unit: unit,
      lines: lines,
    });
    if (priority && priority !== "all") {
      params.append("priority", priority);
    }

    const res = await fetch(`/api/logs?${params.toString()}`);
    const json = await res.json();
    if (json.success && json.data) {
      rawLogsBuffer = json.data;
      renderLogsContent(rawLogsBuffer);
    } else {
      rawLogsBuffer = "";
      if (content) content.innerHTML = `<span style="color:var(--subtext0);">Aucun journal disponible pour cette sélection ou service inactif.</span>`;
      updateLogsBadges(0, 0);
    }
  } catch (e) {
    if (content) content.innerHTML = `<span style="color:var(--red);">Erreur lors de la lecture des logs : ${escapeHtml(e.message || e)}</span>`;
  }
}

function renderLogsContent(text) {
  const content = document.getElementById("terminal-content");
  if (!content) return;

  if (!text) {
    content.innerHTML = `<span style="color:var(--subtext0);">Aucune entrée de journal.</span>`;
    updateLogsBadges(0, 0);
    return;
  }

  const searchInput = document.getElementById("input-log-search");
  const query = searchInput ? searchInput.value.trim().toLowerCase() : "";

  const lines = text.split("\n");
  let errorCount = 0;
  let matchingLines = 0;
  let htmlLines = [];

  for (let line of lines) {
    if (!line.trim()) continue;

    const lower = line.toLowerCase();
    const isError = lower.includes("err") || lower.includes("fail") || lower.includes("crit") || lower.includes("fatal") || lower.includes("emerg");
    const isWarning = !isError && (lower.includes("warn") || lower.includes("attention"));

    if (isError) errorCount++;

    if (query && !lower.includes(query)) {
      continue;
    }

    matchingLines++;
    let escaped = escapeHtml(line);

    if (query) {
      const regex = new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, "gi");
      escaped = escaped.replace(regex, '<mark class="log-search-match">$1</mark>');
    }

    let lineClass = isError ? "log-line-err" : (isWarning ? "log-line-warn" : "log-line-info");
    htmlLines.push(`<span class="${lineClass}">${escaped}</span>`);
  }

  content.innerHTML = htmlLines.length > 0 ? htmlLines.join("\n") : `<span style="color:var(--subtext0);">Aucune ligne ne correspond à la recherche "${escapeHtml(query)}".</span>`;
  updateLogsBadges(matchingLines, errorCount);

  if (isLogsLiveActive) {
    content.scrollTop = content.scrollHeight;
  }
}

function filterDisplayedLogs() {
  renderLogsContent(rawLogsBuffer);
}

function updateLogsBadges(totalLines, errorCount) {
  const lineBadge = document.getElementById("logs-line-count-badge");
  const errBadge = document.getElementById("logs-error-count-badge");

  if (lineBadge) {
    lineBadge.textContent = `${totalLines} ligne${totalLines > 1 ? "s" : ""}`;
  }
  if (errBadge) {
    if (errorCount > 0) {
      errBadge.style.display = "inline-block";
      errBadge.textContent = `⚠️ ${errorCount} erreur${errorCount > 1 ? "s" : ""}`;
    } else {
      errBadge.style.display = "none";
    }
  }
}

function toggleLogsLiveStream() {
  const btn = document.getElementById("btn-logs-live");
  const dot = document.getElementById("live-stream-dot");
  const text = document.getElementById("btn-logs-live-text");

  if (isLogsLiveActive) {
    clearInterval(logsLiveInterval);
    logsLiveInterval = null;
    isLogsLiveActive = false;
    if (dot) dot.classList.remove("active");
    if (text) text.textContent = "Flux Direct (Off)";
    if (btn) btn.classList.remove("btn-primary");
    showToast("Flux de logs en direct arrêté", "info");
  } else {
    isLogsLiveActive = true;
    if (dot) dot.classList.add("active");
    if (text) text.textContent = "Flux Direct (Actif)";
    if (btn) btn.classList.add("btn-primary");
    loadLogs();
    logsLiveInterval = setInterval(() => {
      loadLogs();
    }, 2000);
    showToast("Flux de logs en direct activé (actualisation 2s)", "success");
  }
}

function clearLogsView() {
  const content = document.getElementById("terminal-content");
  if (content) content.innerHTML = `<span style="color:var(--subtext0);">Vue des logs effacée. Cliquez sur Actualiser pour recharger.</span>`;
  updateLogsBadges(0, 0);
}

function scrollLogsToBottom() {
  const content = document.getElementById("terminal-content");
  if (content) {
    content.scrollTo({ top: content.scrollHeight, behavior: "smooth" });
  }
}

function toggleLogsFullscreen() {
  const frame = document.getElementById("logs-window-frame");
  if (!frame) return;
  const isFull = frame.classList.toggle("fullscreen-mode");
  if (isFull) {
    showToast("Mode Plein Écran activé. Appuyez sur Échap pour quitter.", "info");
  }
}

function exportLogsFile() {
  const selectUnit = document.getElementById("select-log-unit");
  const customInput = document.getElementById("input-custom-log-unit");
  const selectPrio = document.getElementById("select-log-priority");
  const selectLines = document.getElementById("select-log-lines");

  let unit = selectUnit ? selectUnit.value : "_SYSTEM_";
  if (unit === "__CUSTOM__") {
    unit = customInput && customInput.value.trim() ? customInput.value.trim() : "_SYSTEM_";
  }
  const priority = selectPrio ? selectPrio.value : "all";
  const lines = selectLines ? selectLines.value : "500";

  const url = `/api/logs/export?unit=${encodeURIComponent(unit)}&lines=${lines}&priority=${encodeURIComponent(priority)}`;
  window.open(url, "_blank");
  showToast("Téléchargement du fichier de logs en cours...", "success");
}

function copyLogs() {
  const content = document.getElementById("terminal-content");
  if (content) {
    copyText(content.innerText);
    showToast("Logs copiés dans le presse-papiers", "success");
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
    const host = (location.hostname && location.hostname !== "localhost" && location.hostname !== "127.0.0.1") ? location.hostname : (currentSftpData?.primary_lan_ip || "IP_LOCALE");
    const user = getCurrentDashboardUsername();
    uriEl.textContent = `sftp://${user}@${host}:22`;
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

// ==========================================================================
// BANNIÈRE / POPUP D'ERREUR SYSTÈME DÉTAILLÉE FLOTTANTE (DURÉE >= 10s)
// ==========================================================================
let systemErrorDismissTimer = null;
let systemErrorDuration = 10000;
let systemErrorRemaining = 10000;
let systemErrorStartTime = 0;
let systemErrorIsPaused = false;
let currentSystemErrorFullText = "";

function showSystemError(title, summary, technicalDetails = null, durationMs = 10000) {
  const toast = document.getElementById("system-error-toast");
  if (!toast) {
    showToast(`${title} : ${summary}`, "error");
    return;
  }

  let mainSummary = summary || "Une erreur est survenue lors de l'opération système.";
  let techDetails = technicalDetails;

  if (!techDetails && mainSummary) {
    if (mainSummary.includes("ERROR:") || mainSummary.includes("WARNING:") || mainSummary.includes("failed to") || mainSummary.includes("No such file") || mainSummary.includes("Échec du formatage")) {
      const parts = mainSummary.split(/(?=ERROR:|WARNING:|failed to|cannot open)/);
      if (parts.length > 1) {
        mainSummary = parts[0].trim().replace(/:\s*$/, "");
        techDetails = parts.slice(1).join("\n").trim();
      } else {
        techDetails = mainSummary;
        mainSummary = "L'opération a été interrompue par une erreur système.";
      }
    }
  }

  currentSystemErrorFullText = `[${title}]\n${mainSummary}\n\n${techDetails ? `Détails techniques :\n${techDetails}` : ""}`;

  const titleEl = document.getElementById("system-error-title");
  const summaryEl = document.getElementById("system-error-summary");
  const techWrap = document.getElementById("system-error-terminal-wrap");
  const techEl = document.getElementById("system-error-terminal");
  const bar = document.getElementById("system-error-countdown-bar");

  if (titleEl) titleEl.textContent = title || "Échec de l'opération";
  if (summaryEl) summaryEl.textContent = mainSummary;

  if (techDetails && techDetails.trim().length > 0) {
    if (techEl) techEl.textContent = techDetails.trim();
    if (techWrap) techWrap.style.display = "block";
  } else {
    if (techWrap) techWrap.style.display = "none";
  }

  toast.classList.remove("toast-fading-out");
  toast.style.display = "block";

  if (bar) {
    bar.style.transition = "none";
    bar.style.width = "100%";
    setTimeout(() => {
      bar.style.transition = `width ${durationMs}ms linear`;
      bar.style.width = "0%";
    }, 50);
  }

  systemErrorDuration = durationMs;
  systemErrorRemaining = durationMs;
  systemErrorStartTime = Date.now();
  systemErrorIsPaused = false;

  if (systemErrorDismissTimer) clearTimeout(systemErrorDismissTimer);
  systemErrorDismissTimer = setTimeout(() => {
    dismissSystemError();
  }, durationMs);
}

function pauseSystemErrorCountdown() {
  if (systemErrorIsPaused) return;
  systemErrorIsPaused = true;
  const elapsed = Date.now() - systemErrorStartTime;
  systemErrorRemaining = Math.max(1000, systemErrorRemaining - elapsed);
  if (systemErrorDismissTimer) clearTimeout(systemErrorDismissTimer);

  const bar = document.getElementById("system-error-countdown-bar");
  if (bar) {
    const computedWidth = window.getComputedStyle(bar).width;
    bar.style.transition = "none";
    bar.style.width = computedWidth;
  }
}

function resumeSystemErrorCountdown() {
  if (!systemErrorIsPaused) return;
  systemErrorIsPaused = false;
  systemErrorStartTime = Date.now();

  const bar = document.getElementById("system-error-countdown-bar");
  if (bar) {
    bar.style.transition = `width ${systemErrorRemaining}ms linear`;
    bar.style.width = "0%";
  }

  if (systemErrorDismissTimer) clearTimeout(systemErrorDismissTimer);
  systemErrorDismissTimer = setTimeout(() => {
    dismissSystemError();
  }, systemErrorRemaining);
}

function dismissSystemError() {
  if (systemErrorDismissTimer) {
    clearTimeout(systemErrorDismissTimer);
    systemErrorDismissTimer = null;
  }
  const toast = document.getElementById("system-error-toast");
  if (toast) {
    toast.classList.add("toast-fading-out");
    setTimeout(() => {
      toast.style.display = "none";
      toast.classList.remove("toast-fading-out");
    }, 450);
  }
}

function copySystemErrorDetails() {
  if (!currentSystemErrorFullText) return;
  navigator.clipboard.writeText(currentSystemErrorFullText).then(() => {
    const icon = document.getElementById("copy-error-btn-icon");
    const label = document.getElementById("copy-error-btn-label");
    if (icon) icon.textContent = "✅";
    if (label) label.textContent = "Copié !";
    setTimeout(() => {
      if (icon) icon.textContent = "📋";
      if (label) label.textContent = "Copier";
    }, 2000);
  }).catch(() => {});
}

function toggleErrorTerminal() {
  const term = document.getElementById("system-error-terminal");
  const btn = document.getElementById("btn-toggle-error-terminal");
  if (!term || !btn) return;
  if (term.classList.contains("expanded")) {
    term.classList.remove("expanded");
    btn.textContent = "Agrandir ↕";
  } else {
    term.classList.add("expanded");
    btn.textContent = "Réduire ↕";
  }
}

function showToast(message, type = "info") {
  if (type === "error" && message && (message.length > 70 || message.includes("ERROR:") || message.includes("\n"))) {
    showSystemError("Erreur Système", message, null, 10000);
    return;
  }

  const container = document.getElementById("toast-container");
  if (!container) return;

  const toast = document.createElement("div");
  toast.className = `toast toast-${type}`;
  const icon = type === "success" ? "✅" : type === "error" ? "❌" : "ℹ️";

  if (type === "error") {
    toast.innerHTML = `
      <div style="display:flex; align-items:center; gap:8px; flex:1;">
        <span>${icon}</span>
        <span style="flex:1;">${escapeHtml(message)}</span>
      </div>
      <button type="button" style="background:none; border:none; color:inherit; font-size:1.1rem; cursor:pointer; opacity:0.7; margin-left:8px; line-height:1;" onclick="this.closest('.toast').remove()">&times;</button>
    `;
  } else {
    toast.innerHTML = `<span>${icon}</span> <span>${escapeHtml(message)}</span>`;
  }

  container.appendChild(toast);

  // Les erreurs restent affichées au moins 10 secondes (10000ms), le reste 3.5 secondes
  const duration = type === "error" ? 10000 : 3500;

  setTimeout(() => {
    if (toast.parentNode) {
      toast.style.opacity = "0";
      toast.style.transform = "translateY(10px)";
      setTimeout(() => toast.remove(), 300);
    }
  }, duration);
}

function escapeHtml(str) {
  if (str === null || str === undefined) return "";
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
let sessionSudoPassword = null;
let terminalSudoMode = false;
let termFontSize = 13.5;
let pendingSudoCommand = null;

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

  const u = getCurrentDashboardUsername();
  const sidebarUser = document.getElementById("console-sidebar-user");
  if (sidebarUser) sidebarUser.textContent = `${u}@noos-nas`;

  if (subTab === "terminal") {
    checkTerminalSudoStatus();
    updateTerminalPrompt();
    const input = document.getElementById("bash-input");
    if (input) setTimeout(() => input.focus(), 60);
  } else {
    loadLogs();
  }
}

async function checkTerminalSudoStatus() {
  const badge = document.getElementById("sidebar-sudo-status-badge");
  const lockIcon = document.getElementById("term-sudo-lock-icon");
  const statusText = document.getElementById("term-sudo-status-text");
  const btnToggle = document.getElementById("btn-term-sudo-toggle");

  let isCached = false;
  try {
    const res = await fetch("/api/terminal/sudo-status");
    const json = await res.json();
    if (json.success && json.data) {
      isCached = json.data.cached;
    }
  } catch (e) {
    console.warn("Erreur vérification sudo:", e);
  }

  const isElevated = isCached || !!sessionSudoPassword;
  terminalSudoMode = isElevated;

  if (badge) {
    if (isElevated) {
      badge.style.color = "var(--green)";
      badge.textContent = "Sudo actif (0 mot de passe)";
    } else {
      badge.style.color = "var(--yellow)";
      badge.textContent = "Sudo disponible";
    }
  }

  if (lockIcon) lockIcon.textContent = isElevated ? "🔓" : "🔒";
  if (statusText) statusText.textContent = isElevated ? "Sudo Actif" : "Mode Sudo";
  if (btnToggle) {
    btnToggle.classList.toggle("btn-primary", isElevated);
    btnToggle.classList.toggle("btn-secondary", !isElevated);
  }

  updateTerminalPrompt();
}

function toggleTerminalSudoMode() {
  if (terminalSudoMode || sessionSudoPassword) {
    if (confirm("Souhaitez-vous révoquer l'élévation des privilèges administrateur (sudo) pour cette session ?")) {
      sessionSudoPassword = null;
      terminalSudoMode = false;
      fetch("/api/terminal/sudo-drop", { method: "POST" })
        .then(() => {
          checkTerminalSudoStatus();
          showToast("Privilèges administrateur (sudo) révoqués", "info");
        })
        .catch(() => {
          checkTerminalSudoStatus();
        });
    }
  } else {
    showTerminalSudoBanner(null);
  }
}

function showTerminalSudoBanner(cmdToRetry) {
  const banner = document.getElementById("terminal-sudo-banner");
  const desc = document.getElementById("sudo-banner-desc");
  const input = document.getElementById("terminal-sudo-password");
  if (!banner) return;

  pendingSudoCommand = cmdToRetry;

  if (cmdToRetry) {
    if (desc) desc.textContent = `La commande "${cmdToRetry}" requiert les privilèges administrateur. Saisissez votre mot de passe utilisateur Linux :`;
  } else {
    if (desc) desc.textContent = "Saisissez votre mot de passe utilisateur pour activer la session sudo sans confirmation :";
  }

  banner.style.display = "flex";
  if (input) {
    input.value = "";
    setTimeout(() => input.focus(), 50);
  }
}

function hideTerminalSudoBanner() {
  const banner = document.getElementById("terminal-sudo-banner");
  const input = document.getElementById("terminal-sudo-password");
  if (banner) banner.style.display = "none";
  if (input) input.value = "";
  pendingSudoCommand = null;

  const bashInput = document.getElementById("bash-input");
  if (bashInput) setTimeout(() => bashInput.focus(), 50);
}

function cancelTerminalSudoAuth() {
  hideTerminalSudoBanner();
  showToast("Authentification administrateur annulée", "info");
}

function toggleSudoPasswordVisibility() {
  const input = document.getElementById("terminal-sudo-password");
  if (!input) return;
  input.type = input.type === "password" ? "text" : "password";
}

async function submitTerminalSudoAuth() {
  const input = document.getElementById("terminal-sudo-password");
  const rememberCheck = document.getElementById("terminal-sudo-remember");
  if (!input) return;

  const password = input.value;
  if (!password) {
    showToast("Veuillez saisir votre mot de passe", "warning");
    input.focus();
    return;
  }

  showToast("Validation des privilèges administrateur...", "info");

  try {
    const res = await fetch("/api/terminal/sudo-auth", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ password: password })
    });
    const json = await res.json();

    if (json.success && json.data) {
      if (rememberCheck && rememberCheck.checked) {
        sessionSudoPassword = password;
      }
      terminalSudoMode = true;
      showToast(json.message || "Privilèges sudo validés avec succès !", "success");

      const cmdToRun = pendingSudoCommand;
      hideTerminalSudoBanner();
      checkTerminalSudoStatus();

      if (cmdToRun) {
        executeBashCommandWithPassword(cmdToRun, password);
      }
    } else {
      showToast(json.message || "Mot de passe incorrect ou refusé par sudo", "error");
      input.focus();
      input.select();
    }
  } catch (err) {
    showToast("Erreur lors de la validation sudo : " + err, "error");
  }
}

function toggleInputSudoPrefix() {
  const input = document.getElementById("bash-input");
  const btn = document.getElementById("btn-sudo-prefix");
  if (!input) return;

  let val = input.value.trim();
  if (val.startsWith("sudo ")) {
    input.value = val.substring(5);
    if (btn) btn.classList.remove("active");
  } else {
    input.value = "sudo " + val;
    if (btn) btn.classList.add("active");
  }
  input.focus();
}

function toggleTerminalHeight() {
  const frame = document.getElementById("term-window-frame");
  if (frame) {
    frame.classList.toggle("compact-height");
  }
}

function toggleTerminalFullscreen() {
  const frame = document.getElementById("term-window-frame");
  if (!frame) return;
  const isFull = frame.classList.toggle("fullscreen-mode");
  if (isFull) {
    showToast("Terminal plein écran activé. Appuyez sur Échap pour quitter.", "info");
  }
}

function adjustTermFontSize(delta) {
  const body = document.getElementById("bash-terminal-body");
  if (!body) return;

  termFontSize = Math.max(10, Math.min(22, termFontSize + delta));
  body.style.fontSize = `${termFontSize}px`;
}

function exportTerminalSession() {
  const body = document.getElementById("bash-terminal-body");
  if (!body) return;

  const text = body.innerText;
  const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `noos-terminal-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.txt`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
  showToast("Session terminal sauvegardée avec succès", "success");
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

  const sudoPrefixBtn = document.getElementById("btn-sudo-prefix");
  if (sudoPrefixBtn) sudoPrefixBtn.classList.remove("active");

  executeBashCommandWithPassword(cmd, sessionSudoPassword);
}

async function executeBashCommandWithPassword(cmd, password) {
  appendCommandToTerminal(cmd, terminalCwd);

  const statusPill = document.getElementById("term-status-pill");
  if (statusPill) {
    statusPill.className = "term-status-pill busy";
    statusPill.textContent = "● Exécution...";
  }

  try {
    const payload = {
      command: cmd,
      cwd: terminalCwd,
      password: password || undefined,
      run_as_root: terminalSudoMode,
    };

    const res = await fetch("/api/terminal/exec", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });
    const json = await res.json();
    const result = json.data || {};

    if (statusPill) {
      statusPill.className = "term-status-pill online";
      statusPill.textContent = "● Prêt";
    }

    if (result.needs_password) {
      appendResultToTerminal({
        success: false,
        stdout: result.stdout || "",
        stderr: result.stderr || "[sudo] Mot de passe requis pour exécuter cette commande.",
        exit_code: 1,
        duration_ms: result.duration_ms || 0
      });
      showTerminalSudoBanner(cmd);
      return;
    }

    if (result.cwd) {
      terminalCwd = result.cwd;
      updateTerminalPrompt();
    }

    appendResultToTerminal(result);
  } catch (err) {
    if (statusPill) {
      statusPill.className = "term-status-pill online";
      statusPill.textContent = "● Erreur";
    }
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

  const u = getCurrentDashboardUsername();
  const isSudo = cmd.trim().startsWith("sudo ") || terminalSudoMode;
  const suffix = isSudo ? "#" : "$";
  const prefixTag = isSudo ? `<span style="color:var(--red); font-weight:700;">[sudo] </span>` : "";

  const entry = document.createElement("div");
  entry.className = "term-history-entry";
  entry.innerHTML = `
    <div class="term-cmd-line">
      <div>
        <span class="term-cmd-prompt">${prefixTag}${escapeHtml(u)}@noos-nas:<b>${escapeHtml(formatShortCwd(cwd))}</b>${suffix}</span>
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
  const macUser = document.getElementById("term-mac-user");
  const macCwd = document.getElementById("term-mac-cwd");
  const shortCwd = formatShortCwd(terminalCwd);
  const u = getCurrentDashboardUsername();

  const isSudo = terminalSudoMode || !!sessionSudoPassword;
  const suffix = isSudo ? "#" : "$";
  const prefixTag = isSudo ? `<span style="color:var(--red); font-weight:700;">[sudo] </span>` : "";

  if (promptLabel) {
    promptLabel.className = isSudo ? "bash-prompt-label sudo-active" : "bash-prompt-label";
    promptLabel.innerHTML = `${prefixTag}${escapeHtml(u)}@noos-nas:<b>${escapeHtml(shortCwd)}</b>${suffix}`;
  }
  if (cwdBadge) {
    cwdBadge.textContent = `📁 ${shortCwd}`;
  }
  if (macUser) macUser.textContent = u;
  if (macCwd) macCwd.textContent = shortCwd;

  const sidebarUser = document.getElementById("console-sidebar-user");
  if (sidebarUser) sidebarUser.textContent = `${u}@noos-nas`;
}

function formatShortCwd(cwd) {
  if (!cwd) return "~";
  const home = getUserHome();
  if (cwd === home) return "~";
  if (home && cwd.startsWith(home + "/")) {
    return "~" + cwd.substring(home.length);
  }
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
      <span style="color:var(--mauve); font-weight:bold;">🚀 Noos Interactive Terminal Pro</span> — Écran effacé.<br>
      <span style="color:var(--subtext0); font-size:0.8rem;">
        • Touche <kbd>Tab</kbd> : Autocomplétion • Flèches <kbd>↑</kbd> / <kbd>↓</kbd> : Historique • <kbd>Ctrl+L</kbd> : Effacer<br>
        • Mode Sudo : Élévation et mémorisation du mot de passe activées
      </span>
    </div>
  `;
}

function copyBashTerminal() {
  const body = document.getElementById("bash-terminal-body");
  if (body) {
    copyText(body.innerText);
    showToast("Contenu du terminal copié dans le presse-papiers", "success");
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
      const sudoBanner = document.getElementById("terminal-sudo-banner");
      if (sudoBanner && sudoBanner.style.display !== "none") {
        cancelTerminalSudoAuth();
      }
      const termFrame = document.getElementById("term-window-frame");
      if (termFrame && termFrame.classList.contains("fullscreen-mode")) {
        termFrame.classList.remove("fullscreen-mode");
      }
      const logsFrame = document.getElementById("logs-window-frame");
      if (logsFrame && logsFrame.classList.contains("fullscreen-mode")) {
        logsFrame.classList.remove("fullscreen-mode");
      }
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
let currentFolderPath = "";
let isTrashView = false;
let trashOverview = null;
let selectedTrashItem = null;
let currentFolderParent = null;
let currentEntries = [];
let fileViewMode = localStorage.getItem("noos_file_view_mode") || "grid";
let fileSortColumn = localStorage.getItem("noos_file_sort_col") || "name";
let fileSortDirection = localStorage.getItem("noos_file_sort_dir") || "asc";
let pinnedMountsList = [];
let remoteMountsList = [];
let storageMountsList = [];
let discoveredDevicesList = [];
let fileClipboard = null; // { action: 'copy' | 'cut', path: string, name: string, paths?: string[] }
let selectedFileItem = null;
let selectedFilePaths = new Set();
let lastSelectedFilePath = null;
let currentCompressFormat = "zip";
let currentCompressLevel = "normal";

async function navigateToPath(targetPath) {
  isTrashView = false;
  isKDriveView = false;
  currentKDriveAccountId = null;
  selectedTrashItem = null;
  clearFileSelection();
  const normalTb = document.getElementById("files-toolbar-normal");
  const trashTb = document.getElementById("files-toolbar-trash");
  if (normalTb) normalTb.style.display = "flex";
  if (trashTb) trashTb.style.display = "none";
  if (!targetPath) targetPath = getUserHome();

  try {
    const res = await fetch(`/api/files/list?path=${encodeURIComponent(targetPath)}`);
    const json = await res.json();

    if (!json.success || !json.data) {
      const fallback = getUserHome();
      if (targetPath !== fallback) {
        console.warn(`Chemin inaccessible (${targetPath}), repli vers ${fallback}`);
        showToast(json.message || "Dossier introuvable, retour au dossier personnel.", "warning");
        return navigateToPath(fallback);
      }
      showToast(json.message || "Impossible d'ouvrir ce dossier", "error");
      return;
    }

    const data = json.data;
    currentFolderPath = data.current_path;
    currentFolderParent = data.parent_path;
    currentEntries = data.entries || [];
    try {
      localStorage.setItem("noos_files_path", currentFolderPath);
    } catch (e) {}

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
    document.querySelectorAll(".files-mount-item").forEach(item => item.classList.remove("active"));
    const el = document.getElementById("fnav-trash");
    if (el) el.classList.add("active");
    return;
  }
  const home = getUserHome();
  const normPath = path ? path.replace(/\/+$/, '') : '';
  const normHome = home ? home.replace(/\/+$/, '') : '';
  const mapping = {
    [home]: "fnav-home",
    [normHome]: "fnav-home",
    [`${normHome}/documents`]: "fnav-docs",
    [`${normHome}/images`]: "fnav-pics",
    [`${normHome}/videos`]: "fnav-vids",
    [`${normHome}/musique`]: "fnav-music",
    [`${normHome}/telechargements`]: "fnav-dl",
    [`${normHome}/downloads`]: "fnav-dl",
    [`${normHome}/pictures`]: "fnav-pics",
    [`${normHome}/music`]: "fnav-music",
    "/etc/nixos": "fnav-nixos"
  };

  document.querySelectorAll(".files-nav-item").forEach(item => item.classList.remove("active"));
  document.querySelectorAll(".files-kdrive-item").forEach(item => item.classList.remove("active"));
  const activeId = mapping[path] || mapping[normPath];
  if (activeId) {
    const el = document.getElementById(activeId);
    if (el) el.classList.add("active");
  }

  // Synchronisation avec les disques et volumes épinglés
  document.querySelectorAll(".files-mount-item").forEach(item => {
    const mp = item.getAttribute("data-mount-path");
    const normMp = mp ? mp.replace(/\/+$/, '') || '/' : '';
    if (mp && (mp === path || normMp === normPath || (normPath === '' && normMp === '/'))) {
      item.classList.add("active");
    } else {
      item.classList.remove("active");
    }
  });
}

function copyCurrentFolderPath() {
  if (!currentFolderPath) return;
  const p = currentFolderPath.replace(/\/+$/, '') || '/';
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(p).then(() => {
      showToast(`Chemin copié : ${p}`, "info");
    }).catch(() => {
      showToast(p, "info");
    });
  } else {
    showToast(`Chemin : ${p}`, "info");
  }
}

function updateFilesBreadcrumbs(path) {
  const container = document.getElementById("files-breadcrumbs");
  const quickDisplay = document.getElementById("files-quick-path-display");

  if (isKDriveView) {
    if (quickDisplay) {
      quickDisplay.textContent = `kDrive://${currentKDriveAccountName}${currentKDriveFolderId === 0 ? '' : ` (Dossier #${currentKDriveFolderId})`}`;
      quickDisplay.title = "Stockage Cloud kDrive Infomaniak";
    }
    if (!container) return;

    let html = `<span class="crumb-item" onclick="navigateToKDrive('${escapeHtml(currentKDriveAccountId)}', 0, '${escapeHtml(currentKDriveAccountName)}')" title="Racine de votre kDrive">
      <span style="margin-right:4px;">☁️</span>
      <span>${escapeHtml(currentKDriveAccountName || "Mon kDrive")}</span>
    </span>`;

    if (kdriveBreadcrumbsStack && kdriveBreadcrumbsStack.length > 1) {
      for (let i = 1; i < kdriveBreadcrumbsStack.length; i++) {
        const crumb = kdriveBreadcrumbsStack[i];
        const isLast = i === kdriveBreadcrumbsStack.length - 1;
        html += `<span class="crumb-separator"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg></span>`;
        html += `<span class="crumb-item ${isLast ? 'active' : ''}" onclick="navigateToKDrive('${escapeHtml(currentKDriveAccountId)}', ${crumb.id}, '${escapeHtml(crumb.name)}')" title="${escapeHtml(crumb.name)}">${escapeHtml(crumb.name)}</span>`;
      }
    }
    container.innerHTML = html;
    return;
  }

  const cleanPath = (path || "/").replace(/\/+$/, "") || "/";
  if (quickDisplay) {
    quickDisplay.textContent = cleanPath;
    quickDisplay.title = "Copier le chemin : " + cleanPath;
  }
  if (!container) return;

  const parts = cleanPath.split("/").filter(Boolean);
  let html = `<span class="crumb-item ${parts.length === 0 ? 'active' : ''}" onclick="navigateToPath('/')" title="Racine du système (/)">
    <svg class="crumb-item-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
      <rect x="2" y="2" width="20" height="8" rx="2" ry="2"></rect>
      <rect x="2" y="14" width="20" height="8" rx="2" ry="2"></rect>
      <line x1="6" y1="6" x2="6.01" y2="6"></line>
      <line x1="6" y1="18" x2="6.01" y2="18"></line>
    </svg>
    <span>Racine</span>
  </span>`;

  let accumulated = "";
  parts.forEach((part, idx) => {
    accumulated += "/" + part;
    const isLast = idx === parts.length - 1;
    const thisPath = accumulated;
    html += `<span class="crumb-separator"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg></span>`;
    html += `<span class="crumb-item ${isLast ? 'active' : ''}" onclick="navigateToPath('${escapeHtml(thisPath)}')" title="${escapeHtml(thisPath)}">${escapeHtml(part)}</span>`;
  });

  container.innerHTML = html;
}

function isArchiveFile(fileName) {
  if (!fileName) return false;
  return /\.(zip|7z|tar\.gz|tgz|tar\.bz2|tbz2|tar\.xz|txz|tar\.zst|tzst|tar|rar|iso|gz|bz2|xz|zst)$/i.test(fileName);
}

function renderFilesList(entries) {
  const gridWrap = document.getElementById("files-grid-wrap");
  const tableBody = document.getElementById("files-table-tbody");
  const gridContainer = document.getElementById("files-grid-wrap");
  const tableContainer = document.getElementById("files-table-wrap");

  if (!gridWrap || !tableBody) return;

  // Tri des éléments
  const sortedEntries = [...entries].sort((a, b) => {
    if (a.is_dir && !b.is_dir) return -1;
    if (!a.is_dir && b.is_dir) return 1;

    let res = 0;
    if (fileSortColumn === "name") {
      res = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    } else if (fileSortColumn === "size") {
      res = (a.size_bytes || 0) - (b.size_bytes || 0);
    } else if (fileSortColumn === "date") {
      res = (a.modified || "").localeCompare(b.modified || "");
    } else if (fileSortColumn === "type") {
      const typeA = getFileTypeLabel(a);
      const typeB = getFileTypeLabel(b);
      res = typeA.localeCompare(typeB);
    }
    return fileSortDirection === "asc" ? res : -res;
  });

  updateSortIndicators();

  if (sortedEntries.length === 0) {
    const emptyHtml = `<div style="grid-column:1/-1; padding:40px; text-align:center; color:var(--subtext0);">📁 Dossier vide</div>`;
    gridWrap.innerHTML = emptyHtml;
    tableBody.innerHTML = `<tr><td colspan="6" style="text-align:center; color:var(--subtext0); padding:30px;">📁 Dossier vide</td></tr>`;
    updateSelectionUI();
    return;
  }

  // Rendu Grille
  gridWrap.innerHTML = entries.map(item => {
    const isSelected = selectedFilePaths.has(item.path);
    const icon = getFileIcon(item);
    let cardPreview = `<div class="file-card-icon">${icon}</div>`;
    if (isImageFile(item.name, item.category)) {
      const thumbUrl = buildAuthenticatedUrl("/api/files/image-view", { path: item.path, thumb: "true" });
      cardPreview = `<div class="file-card-icon file-card-img-preview" style="width:100%; height:80px; max-height:80px; overflow:hidden; border-radius:6px; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,0.35);"><img src="${thumbUrl}" loading="lazy" alt="${escapeHtml(item.name)}" style="width:100%; height:100%; max-width:100%; max-height:100%; object-fit:cover; display:block; border-radius:5px;" onerror="this.onerror=null; this.parentElement.className='file-card-icon'; this.parentElement.style='width:100%; height:80px; display:flex; align-items:center; justify-content:center; font-size:2.4rem;'; this.parentElement.innerHTML='${icon}';"></div>`;
    }
    const metaHtml = item.is_mount_point
      ? `<span style="color:var(--yellow); font-weight:600; display:inline-flex; align-items:center; gap:3px;">💾 Montage</span>`
      : escapeHtml(item.size_human);
    return `
      <div class="file-card ${isSelected ? 'selected' : ''}" style="min-width:0; overflow:hidden;"
           data-path="${escapeHtml(item.path)}"
           onclick="handleFileClick(event, '${escapeHtml(item.path)}', ${item.is_dir})"
           ondblclick="handleFileDblClick('${escapeHtml(item.path)}', ${item.is_dir})"
           oncontextmenu="handleItemContextMenu(event, '${escapeHtml(item.path)}')">
        <div class="file-card-select" onclick="handleCardCheckboxClick(event, '${escapeHtml(item.path)}')" title="Sélectionner">
          <input type="checkbox" class="file-card-checkbox" ${isSelected ? 'checked' : ''} tabindex="-1">
        </div>
        ${cardPreview}
        <div class="file-card-name" title="${escapeHtml(item.name)}${item.is_mount_point ? ' (Point de montage protégé)' : ''}">
          ${escapeHtml(item.name)}${item.is_mount_point ? ' <span title="Point de montage protégé" style="font-size:0.8rem;">🔒</span>' : ''}
        </div>
        <div class="file-card-meta">${metaHtml}</div>
      </div>
    `;
  }).join("");

  // Rendu Liste / Table
  tableBody.innerHTML = entries.map(item => {
    const isSelected = selectedFilePaths.has(item.path);
    const icon = getFileIcon(item);
    return `
      <tr data-path="${escapeHtml(item.path)}"
          class="${isSelected ? 'selected' : ''}"
          onclick="handleFileClick(event, '${escapeHtml(item.path)}', ${item.is_dir})"
          ondblclick="handleFileDblClick('${escapeHtml(item.path)}', ${item.is_dir})"
          oncontextmenu="handleItemContextMenu(event, '${escapeHtml(item.path)}')">
        <td style="width:36px; text-align:center;" onclick="handleRowCheckboxClick(event, '${escapeHtml(item.path)}')" title="Sélectionner">
          <input type="checkbox" class="file-table-checkbox" ${isSelected ? 'checked' : ''} tabindex="-1">
        </td>
        <td>
          <span style="font-size:1.1rem; margin-right:8px;">${icon}</span>
          <strong style="color:var(--text);">${escapeHtml(item.name)}</strong>
          ${item.is_mount_point ? ' <span title="Point de montage protégé" style="font-size:0.8rem; margin-left:4px;">🔒</span>' : ''}
        </td>
        <td style="color:var(--subtext0); font-family:var(--font-mono); font-size:0.8rem;">${escapeHtml(item.size_human)}</td>
        <td style="color:var(--subtext0); font-size:0.8rem;">${escapeHtml(item.modified)}</td>
        <td><span class="badge ${item.is_mount_point ? "badge-warning" : (item.is_dir ? "badge-primary" : "badge-secondary")}" style="font-size:0.75rem;">${escapeHtml(getFileTypeLabel(item))}</span></td>
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

  updateSelectionUI();
}

function isDocumentFile(fileName, category) {
  if (!fileName) return false;
  // Documents texte : .pdf, .docx, .doc, .odt, .rtf
  // Tableurs : .xlsx, .xls, .ods, .csv
  // Présentations : .pptx, .ppt, .odp
  return /\.(pdf|docx|doc|odt|rtf|xlsx|xls|ods|csv|pptx|ppt|odp)$/i.test(fileName);
}

function getFileIcon(item) {
  if (item.is_mount_point) return "💾";
  if (item.is_dir) return "📁";
  const name = item.name.toLowerCase();
  if (isArchiveFile(name)) return "📦";
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

  if (e.ctrlKey || e.metaKey) {
    if (selectedFilePaths.has(path)) {
      selectedFilePaths.delete(path);
    } else {
      selectedFilePaths.add(path);
      lastSelectedFilePath = path;
    }
  } else if (e.shiftKey && lastSelectedFilePath && currentEntries.length > 0) {
    const idx1 = currentEntries.findIndex(i => i.path === lastSelectedFilePath);
    const idx2 = currentEntries.findIndex(i => i.path === path);
    if (idx1 !== -1 && idx2 !== -1) {
      const min = Math.min(idx1, idx2);
      const max = Math.max(idx1, idx2);
      for (let i = min; i <= max; i++) {
        selectedFilePaths.add(currentEntries[i].path);
      }
    } else {
      selectedFilePaths.add(path);
    }
    lastSelectedFilePath = path;
  } else {
    selectedFilePaths.clear();
    selectedFilePaths.add(path);
    lastSelectedFilePath = path;
  }

  selectedFileItem = currentEntries.find(i => i.path === path) || null;
  updateSelectionUI();
}

function handleCardCheckboxClick(e, path) {
  e.stopPropagation();
  if (selectedFilePaths.has(path)) {
    selectedFilePaths.delete(path);
  } else {
    selectedFilePaths.add(path);
    lastSelectedFilePath = path;
  }
  selectedFileItem = currentEntries.find(i => i.path === path) || null;
  updateSelectionUI();
}

function handleRowCheckboxClick(e, path) {
  e.stopPropagation();
  if (selectedFilePaths.has(path)) {
    selectedFilePaths.delete(path);
  } else {
    selectedFilePaths.add(path);
    lastSelectedFilePath = path;
  }
  selectedFileItem = currentEntries.find(i => i.path === path) || null;
  updateSelectionUI();
}

function toggleSelectAllFiles(e) {
  if (e && e.stopPropagation) e.stopPropagation();
  if (selectedFilePaths.size === currentEntries.length && currentEntries.length > 0) {
    clearFileSelection();
  } else {
    selectAllFiles();
  }
}

function selectAllFiles() {
  currentEntries.forEach(i => selectedFilePaths.add(i.path));
  updateSelectionUI();
}

function clearFileSelection() {
  selectedFilePaths.clear();
  lastSelectedFilePath = null;
  updateSelectionUI();
}

function updateSelectionUI() {
  document.querySelectorAll(".file-card").forEach(el => {
    const p = el.getAttribute("data-path");
    const isSel = selectedFilePaths.has(p);
    el.classList.toggle("selected", isSel);
    const chk = el.querySelector(".file-card-checkbox");
    if (chk) chk.checked = isSel;
  });

  document.querySelectorAll(".files-table-view tbody tr").forEach(el => {
    const p = el.getAttribute("data-path");
    const isSel = selectedFilePaths.has(p);
    el.classList.toggle("selected", isSel);
    const chk = el.querySelector(".file-table-checkbox");
    if (chk) chk.checked = isSel;
  });

  const allBox = document.getElementById("files-select-all");
  if (allBox) {
    allBox.checked = currentEntries.length > 0 && selectedFilePaths.size === currentEntries.length;
    allBox.indeterminate = selectedFilePaths.size > 0 && selectedFilePaths.size < currentEntries.length;
  }

  const bar = document.getElementById("files-selection-bar");
  const countEl = document.getElementById("files-sel-count");
  if (bar && countEl) {
    if (selectedFilePaths.size > 0) {
      bar.style.display = "flex";
      countEl.textContent = `${selectedFilePaths.size} élément${selectedFilePaths.size > 1 ? "s" : ""} sélectionné${selectedFilePaths.size > 1 ? "s" : ""}`;

      const selectedEntries = currentEntries.filter(e => selectedFilePaths.has(e.path));
      const hasOnlyMounts = selectedEntries.length > 0 && selectedEntries.every(e => e.is_mount_point);
      const delBtn = document.getElementById("btn-files-multi-delete");
      const cutBtn = document.getElementById("btn-files-multi-cut");
      if (delBtn) {
        delBtn.style.opacity = hasOnlyMounts ? "0.45" : "1";
        delBtn.title = hasOnlyMounts ? "Suppression impossible : point(s) de montage protégé(s)" : "Supprimer les éléments sélectionnés";
      }
      if (cutBtn) {
        cutBtn.style.opacity = hasOnlyMounts ? "0.45" : "1";
        cutBtn.title = hasOnlyMounts ? "Déplacement impossible : point(s) de montage protégé(s)" : "Couper les éléments sélectionnés";
      }
    } else {
      bar.style.display = "none";
    }
  }
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
  if (path && path.startsWith("kdrive://")) {
    const clean = path.replace("kdrive://", "");
    const parts = clean.split("/");
    const accountId = parts[0];
    const fileId = parseInt(parts[1], 10) || 0;
    if (isDir) {
      const item = currentEntries.find(i => i.path === path);
      navigateToKDrive(accountId, fileId, item ? item.name : "Dossier");
    } else {
      const item = currentEntries.find(i => i.path === path);
      const fileName = item ? item.name : "Fichier";
      openKDriveFileActionModal(accountId, fileId, fileName);
    }
    return;
  }

  if (isDir) {
    navigateToPath(path);
  } else {
    const item = currentEntries.find(i => i.path === path);
    const fileName = item ? item.name : path.split("/").pop();
    const cat = item ? item.category : "";

    if (isArchiveFile(fileName)) {
      showExtractModal(path, fileName);
    } else if (isImageFile(fileName, cat)) {
      openImageModal(path, fileName, item);
    } else if (isDocumentFile(fileName, cat)) {
      openDocModal(path, fileName, item);
    } else if (isVideoFile(fileName, cat)) {
      openMpvModal(path, fileName);
    } else if (isAudioFile(fileName, cat)) {
      openAudioModal(path, fileName, item ? item.size_bytes : 0);
    } else if (isNvimEditableFile(fileName, cat)) {
      openNvimModal(path, fileName);
    } else {
      showToast(`Fichier : ${fileName}`, "info");
    }
  }
}

function navigateUpFolder() {
  if (isKDriveView) {
    if (currentKDriveFolderId !== 0) {
      navigateToKDrive(currentKDriveAccountId, currentKDriveParentFolderId || 0);
    } else {
      showToast("Vous êtes déjà à la racine de votre kDrive.", "info");
    }
    return;
  }

  if (currentFolderParent) {
    navigateToPath(currentFolderParent);
  } else {
    showToast("Vous êtes déjà à la racine du système.", "info");
  }
}

function refreshCurrentFolder() {
  if (isKDriveView) {
    navigateToKDrive(currentKDriveAccountId, currentKDriveFolderId);
    return;
  }
  navigateToPath(currentFolderPath);
}

function setFileViewMode(mode) {
  fileViewMode = mode;
  try {
    localStorage.setItem("noos_file_view_mode", mode);
  } catch (e) {}

  const btnGrid = document.getElementById("btn-view-grid");
  const btnTable = document.getElementById("btn-view-table");

  if (btnGrid) btnGrid.classList.toggle("active", mode === "grid");
  if (btnTable) btnTable.classList.toggle("active", mode === "table");

  renderFilesList(currentEntries);
}

function toggleSortFiles(column) {
  if (fileSortColumn === column) {
    fileSortDirection = fileSortDirection === "asc" ? "desc" : "asc";
  } else {
    fileSortColumn = column;
    fileSortDirection = "asc";
  }
  try {
    localStorage.setItem("noos_file_sort_col", fileSortColumn);
    localStorage.setItem("noos_file_sort_dir", fileSortDirection);
  } catch (e) {}

  updateSortIndicators();
  renderFilesList(currentEntries);
}

function updateSortIndicators() {
  ["name", "size", "date", "type"].forEach(col => {
    const th = document.getElementById(`th-sort-${col}`);
    if (!th) return;
    const indicator = th.querySelector(".sort-indicator");
    if (!indicator) return;
    if (fileSortColumn === col) {
      indicator.textContent = fileSortDirection === "asc" ? "▲" : "▼";
      th.classList.add("sorted");
    } else {
      indicator.textContent = "↕️";
      th.classList.remove("sorted");
    }
  });
}

function getFileTypeLabel(item) {
  if (item.is_mount_point) return "Point de montage";
  if (item.is_dir) return "Dossier";
  const name = item.name.toLowerCase();
  const ext = name.split(".").pop() || "";
  if (isArchiveFile(name)) return `Archive (${ext.toUpperCase()})`;
  if (isImageFile(name, item.category)) return `Image (${ext.toUpperCase()})`;
  if (isVideoFile(name, item.category)) return `Vidéo (${ext.toUpperCase()})`;
  if (isAudioFile(name, item.category)) return `Audio (${ext.toUpperCase()})`;
  if (isDocumentFile(name, item.category)) return `Document (${ext.toUpperCase()})`;
  if (item.category === "code") return `Code (${ext.toUpperCase()})`;
  return ext ? ext.toUpperCase() : "Fichier";
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

  if (!selectedFilePaths.has(path)) {
    selectedFilePaths.clear();
    selectedFilePaths.add(path);
    lastSelectedFilePath = path;
    updateSelectionUI();
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
  const ctxCompress = document.getElementById("ctx-compress");
  const ctxExtractHere = document.getElementById("ctx-extract-here");
  const ctxExtractTo = document.getElementById("ctx-extract-to");

  if (ctxOpen) ctxOpen.style.display = selectedFileItem && selectedFileItem.is_dir ? "flex" : "none";
  if (ctxPaste) {
    if (selectedFileItem && selectedFileItem.is_dir) {
      ctxPaste.style.display = "flex";
      ctxPaste.innerHTML = `<span>📥</span> Coller dans ce dossier`;
      ctxPaste.classList.toggle("disabled", !fileClipboard);
    } else if (fileClipboard) {
      ctxPaste.style.display = "flex";
      ctxPaste.innerHTML = `<span>📥</span> Coller ici`;
      ctxPaste.classList.remove("disabled");
    } else {
      ctxPaste.style.display = "none";
    }
  }

  const isArchive = selectedFileItem && !selectedFileItem.is_dir && isArchiveFile(selectedFileItem.name);
  if (ctxCompress) {
    ctxCompress.style.display = "flex";
    if (selectedFilePaths.size > 1) {
      ctxCompress.innerHTML = `<span>🗜️</span> Compresser (${selectedFilePaths.size})...`;
    } else {
      ctxCompress.innerHTML = `<span>🗜️</span> Compresser...`;
    }
  }
  if (ctxExtractHere) ctxExtractHere.style.display = (isArchive && selectedFilePaths.size <= 1) ? "flex" : "none";
  if (ctxExtractTo) ctxExtractTo.style.display = (isArchive && selectedFilePaths.size <= 1) ? "flex" : "none";

  const isVideo = selectedFileItem && !selectedFileItem.is_dir &&
    isVideoFile(selectedFileItem.name, selectedFileItem.category);
  const isAudio = selectedFileItem && !selectedFileItem.is_dir &&
    isAudioFile(selectedFileItem.name, selectedFileItem.category);
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
  const ctxDeletePerm = document.getElementById("ctx-delete-perm");
  const ctxPaste = document.getElementById("ctx-paste");

  if (ctxOpen) ctxOpen.style.display = "none";
  if (ctxCopy) ctxCopy.style.display = "none";
  if (ctxCut) ctxCut.style.display = "none";
  if (ctxRename) ctxRename.style.display = "none";
  if (ctxDelete) ctxDelete.style.display = "none";
  if (ctxDeletePerm) ctxDeletePerm.style.display = "none";

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
    const hasMountInSelection = Array.from(selectedFilePaths).some(p => {
      const it = currentEntries.find(e => e.path === p);
      return it && it.is_mount_point;
    });
    const isMount = selectedFileItem.is_mount_point || hasMountInSelection;

    ["ctx-copy", "ctx-paste"].forEach(id => {
      const el = document.getElementById(id);
      if (el) el.style.display = "flex";
    });
    // Les actions destructives ou de déplacement sont formellement masquées pour les points de montage protégés
    ["ctx-cut", "ctx-rename", "ctx-delete", "ctx-delete-perm"].forEach(id => {
      const el = document.getElementById(id);
      if (el) el.style.display = isMount ? "none" : "flex";
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

    case "compress":
      showCompressModal();
      break;

    case "extract-here":
      if (selectedFileItem) {
        extractArchiveDirect(selectedFileItem.path, selectedFileItem.name);
      }
      break;

    case "extract-to":
      if (selectedFileItem) {
        showExtractModal(selectedFileItem.path, selectedFileItem.name);
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
      if (!selectedFileItem && selectedFilePaths.size === 0) return;
      {
        const paths = selectedFilePaths.size > 0 && selectedFileItem && selectedFilePaths.has(selectedFileItem.path)
          ? Array.from(selectedFilePaths)
          : (selectedFileItem ? [selectedFileItem.path] : Array.from(selectedFilePaths));

        fileClipboard = {
          action: "copy",
          paths: paths,
          path: paths[0],
          name: paths.length > 1 ? `${paths.length} élément(s)` : (selectedFileItem ? selectedFileItem.name : paths[0].split("/").pop())
        };
        showToast(`Copié dans le presse-papiers : ${fileClipboard.name}`, "info");
        updateFilesStatusBar(currentEntries.length, 0);
      }
      break;

    case "cut":
      if (!selectedFileItem && selectedFilePaths.size === 0) return;
      {
        const paths = selectedFilePaths.size > 0 && selectedFileItem && selectedFilePaths.has(selectedFileItem.path)
          ? Array.from(selectedFilePaths)
          : (selectedFileItem ? [selectedFileItem.path] : Array.from(selectedFilePaths));

        fileClipboard = {
          action: "cut",
          paths: paths,
          path: paths[0],
          name: paths.length > 1 ? `${paths.length} élément(s)` : (selectedFileItem ? selectedFileItem.name : paths[0].split("/").pop())
        };
        showToast(`Coupé dans le presse-papiers : ${fileClipboard.name}`, "info");
        updateFilesStatusBar(currentEntries.length, 0);
      }
      break;

    case "paste":
      if (!fileClipboard) return;
      {
        let targetDest = currentFolderPath;
        if (selectedFileItem && selectedFileItem.is_dir) {
          targetDest = selectedFileItem.path;
        }
        await pasteClipboardItem(targetDest);
      }
      break;

    case "rename":
      if (!selectedFileItem) return;
      if (selectedFileItem.is_mount_point) {
        showToast(`Renommage interdit : '${selectedFileItem.name}' est un point de montage de disque protégé.`, "error");
        return;
      }
      promptRename(selectedFileItem);
      break;

    case "delete":
      if (!selectedFileItem) return;
      if (selectedFileItem.is_mount_point) {
        showToast(`Suppression interdite : '${selectedFileItem.name}' est un point de montage de disque protégé.`, "error");
        return;
      }
      confirmDelete(selectedFileItem, false);
      break;

    case "delete-permanent":
      if (!selectedFileItem) return;
      if (selectedFileItem.is_mount_point) {
        showToast(`Suppression interdite : '${selectedFileItem.name}' est un point de montage de disque protégé.`, "error");
        return;
      }
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
  if (isKDriveView) {
    const name = prompt("Nom du nouveau dossier sur kDrive :", "Nouveau_Dossier");
    if (!name || !name.trim()) return;

    try {
      const res = await fetch(`/api/kdrive/accounts/${encodeURIComponent(currentKDriveAccountId)}/mkdir`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          parent_id: currentKDriveFolderId,
          name: name.trim()
        })
      });
      const json = await res.json();
      if (json.success) {
        showToast("Dossier kDrive créé avec succès !", "success");
        refreshCurrentFolder();
      } else {
        showToast(json.message || "Erreur lors de la création sur kDrive", "error");
      }
    } catch (err) {
      showToast("Erreur réseau kDrive : " + err, "error");
    }
    return;
  }

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
  if (item && item.is_mount_point) {
    showToast(`Renommage interdit : '${item.name}' est un point de montage de disque protégé.`, "error");
    return;
  }

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
  if (item && item.is_mount_point) {
    showToast(`Suppression interdite : '${item.name}' est un point de montage de disque protégé.`, "error");
    return;
  }

  if (item && item.path && item.path.startsWith("kdrive://")) {
    const parts = item.path.replace("kdrive://", "").split("/");
    const accountId = parts[0];
    const fileId = parseInt(parts[1], 10) || 0;
    if (!confirm(`Déplacer "${item.name}" vers la corbeille de votre kDrive ?`)) return;

    try {
      const res = await fetch(`/api/kdrive/accounts/${encodeURIComponent(accountId)}/files/${fileId}`, {
        method: "DELETE"
      });
      const json = await res.json();
      if (json.success) {
        showToast(json.message || `"${item.name}" déplacé dans la corbeille kDrive.`, "success");
        refreshCurrentFolder();
      } else {
        showToast(json.message || "Erreur lors de la suppression sur kDrive", "error");
      }
    } catch (err) {
      showToast("Erreur réseau kDrive : " + err, "error");
    }
    return;
  }

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

async function pasteClipboardItem(targetDir) {
  if (!fileClipboard) return;

  const dest = targetDir || currentFolderPath;
  const endpoint = fileClipboard.action === "cut" ? "/api/files/move" : "/api/files/copy";
  const paths = fileClipboard.paths || (fileClipboard.path ? [fileClipboard.path] : []);
  if (paths.length === 0) return;

  const destName = dest.split("/").pop() || dest;
  const opAction = fileClipboard.action === "cut" ? "move" : "copy";
  const actionFr = opAction === "move" ? "Déplacement" : "Copie";

  startFileTransferTray(opAction, paths.length, dest);
  showToast(`${actionFr} de ${paths.length} élément(s) vers ${destName}...`, "info");

  let successCount = 0;
  let lastError = null;

  for (let i = 0; i < paths.length; i++) {
    const src = paths[i];
    const fileName = src.split("/").pop() || src;
    updateFileTransferTrayItem(i, paths.length, fileName, actionFr, false, false);
    // Petit délai pour assurer la fluidité de rendu visuel dans le DOM
    await new Promise(r => setTimeout(r, 25));

    try {
      const res = await fetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          src_path: src,
          dest_dir: dest
        })
      });
      const json = await res.json();
      if (json.success) {
        successCount++;
        updateFileTransferTrayItem(i + 1, paths.length, fileName, "Terminé", true, false);
      } else {
        lastError = json.message || "Échec du serveur";
        updateFileTransferTrayItem(i + 1, paths.length, fileName, "Erreur", false, true, lastError);
      }
    } catch (err) {
      console.error(err);
      lastError = err.message || String(err);
      updateFileTransferTrayItem(i + 1, paths.length, fileName, "Erreur", false, true, lastError);
    }
  }

  finishFileTransferTray(successCount, paths.length, lastError);

  if (successCount > 0) {
    showToast(`${successCount}/${paths.length} élément(s) ${opAction === 'move' ? 'déplacé(s)' : 'collé(s)'} avec succès !`, "success");
    if (fileClipboard.action === "cut") {
      fileClipboard = null;
      updateFilesStatusBar(currentEntries.length, 0);
    }
    refreshCurrentFolder();
  } else {
    showToast(`Erreur lors du transfert : ${lastError || "Impossible de transférer les éléments"}`, "error");
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

    // 1. CPU
    const cpuModel = document.getElementById("hw-cpu-model");
    const cpuArch = document.getElementById("hw-cpu-arch");
    const cpuCores = document.getElementById("hw-cpu-cores");
    const cpuFreq = document.getElementById("hw-cpu-freq");
    const cpuCache = document.getElementById("hw-cpu-cache");
    const cpuVirt = document.getElementById("hw-cpu-virt");

    if (cpuModel) cpuModel.textContent = hw.cpu.model;
    if (cpuArch) cpuArch.textContent = hw.cpu.architecture;
    if (cpuCores) cpuCores.textContent = `${hw.cpu.total_cores} Cœurs / ${hw.cpu.total_threads} Threads${hw.cpu.sockets > 1 ? ` (${hw.cpu.sockets} Sockets)` : ''}`;
    if (cpuFreq) cpuFreq.textContent = hw.cpu.base_frequency_ghz;
    if (cpuCache) {
      let c = (hw.cpu.cache || "").replace("Intel® ", "").replace(" Smart Cache", " Cache").replace(" (30 Mo / socket)", "");
      cpuCache.textContent = c || "Cache L1/L2/L3";
    }
    if (cpuVirt) {
      let v = (hw.cpu.virtualization || "").replace(" (Matériel Activé)", "").replace(" (Activé)", "").replace(" (Hardware Enabled)", "");
      cpuVirt.textContent = v.includes("Actif") ? v : `${v} Actif`;
    }

    // 2. Carte Mère
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

    // 3. RAM
    const ramTotal = document.getElementById("hw-ram-total");
    const ramType = document.getElementById("hw-ram-type");
    const ramChannels = document.getElementById("hw-ram-channels");
    const ramAvail = document.getElementById("hw-ram-avail");

    if (ramTotal) ramTotal.textContent = `${hw.memory.total_gb} Go (${hw.memory.mem_type})`;
    if (ramType) ramType.textContent = hw.memory.mem_type;
    if (ramChannels) ramChannels.textContent = hw.memory.channels;
    if (ramAvail) ramAvail.textContent = `${hw.memory.available_gb} Go dispo (${hw.memory.free_gb} Go libres)`;

    // 4. GPU
    const gpuModel = document.getElementById("hw-gpu-model");
    const gpuVram = document.getElementById("hw-gpu-vram");
    const gpuDriver = document.getElementById("hw-gpu-driver");
    const gpuRole = document.getElementById("hw-gpu-role");

    if (gpuModel) gpuModel.textContent = hw.gpu.model;
    if (gpuVram) gpuVram.textContent = hw.gpu.vram;
    if (gpuDriver) gpuDriver.textContent = hw.gpu.driver;
    if (gpuRole) {
      if (hw.gpu.features && hw.gpu.features.length > 0) {
        gpuRole.textContent = hw.gpu.features[1] || hw.gpu.features[0];
      } else {
        gpuRole.textContent = "Transcodage Matériel Actif";
      }
    }

    // 5. Réseau Physique LAN (Filtrage strict des interfaces virtuelles)
    const netTitle = document.getElementById("hw-net-title");
    const netList = document.getElementById("hw-network-list");
    if (netList && hw.network_adapters && hw.network_adapters.length > 0) {
      const physicalAdapters = hw.network_adapters.filter(a => {
        if (typeof a.is_physical === "boolean") return a.is_physical;
        const name = (a.interface_name || "").toLowerCase();
        return !name.startsWith("docker") &&
               !name.startsWith("veth") &&
               !name.startsWith("virbr") &&
               !name.startsWith("br-") &&
               !name.startsWith("vnet") &&
               !name.startsWith("wg") &&
               !name.startsWith("tun") &&
               !name.startsWith("tap") &&
               !name.startsWith("dummy") &&
               name !== "lo";
      });

      const displayAdapters = physicalAdapters.length > 0
        ? physicalAdapters
        : hw.network_adapters.filter(a => a.is_up && !a.interface_name.startsWith("veth"));

      if (netTitle) {
        if (displayAdapters.length === 1) {
          const a = displayAdapters[0];
          netTitle.textContent = `${a.interface_name} — ${a.controller_model}`;
        } else {
          netTitle.textContent = `${displayAdapters.length} Interface(s) Réseau Physique(s) Détectée(s)`;
        }
      }

      netList.innerHTML = displayAdapters.map(a => {
        const isUp = a.is_up;
        const speedText = isUp
          ? (a.speed_mbps > 0
              ? (a.speed_mbps >= 1000 ? `${(a.speed_mbps / 1000).toFixed(a.speed_mbps % 1000 === 0 ? 0 : 1)} Gbps` : `${a.speed_mbps} Mbps`)
              : "Actif")
          : "Déconnecté";
        const pillClass = isUp ? "hw-spec-pill-success" : "hw-spec-pill-muted";
        const dotColor = isUp ? "green" : "gray";

        return `
          <span class="hw-spec-pill ${pillClass}">
            <span class="hw-status-dot ${dotColor}"></span>
            <strong>${displayAdapters.length > 1 ? a.interface_name + ' : ' : ''}${speedText}</strong>
          </span>
          ${a.ipv4 ? `
            <span class="hw-spec-pill hw-spec-pill-teal">
              <span class="hw-spec-tag">IPv4</span>
              <strong class="hw-mono">${a.ipv4}</strong>
            </span>
          ` : ''}
          <span class="hw-spec-pill">
            <span class="hw-spec-tag">MAC</span>
            <strong class="hw-mono" style="opacity:0.85;">${a.mac_address}</strong>
          </span>
        `;
      }).join("");
    }

    // 6. Contrôleurs & Stockage
    const storageTitle = document.getElementById("hw-storage-title");
    const storageCtrl = document.getElementById("hw-storage-ctrl");
    const osKernel = document.getElementById("hw-os-kernel");

    if (hw.storage_controllers && hw.storage_controllers.length > 0) {
      if (storageTitle) {
        const types = [...new Set(hw.storage_controllers.map(c => c.controller_type))];
        storageTitle.textContent = types.join(" & ") || "Contrôleurs SATA & NVMe";
      }
      if (storageCtrl) {
        const types = [...new Set(hw.storage_controllers.map(c => {
          let t = c.controller_type || c.name || "";
          return t.replace(" Controller", "").replace(" [AHCI]", "").replace("Series Chipset ", "").replace(" Technology ", " ");
        }))];
        storageCtrl.textContent = types.slice(0, 2).join(" + ") || "Contrôleurs SATA / NVMe";
      }
    }
    if (hw.system_summary && osKernel) {
      osKernel.textContent = `${hw.system_summary.os_name} (${hw.system_summary.kernel_version})`;
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

    // Mise à jour de la pastille de disques dans la synthèse matérielle
    const drivesCount = document.getElementById("hw-drives-count");
    if (drivesCount && smart.disks) {
      if (smart.disks.length === 0) {
        drivesCount.textContent = "Aucun disque détecté";
      } else {
        const hdds = smart.disks.filter(d => (d.disk_type || "").includes("HDD")).length;
        const ssds = smart.disks.filter(d => (d.disk_type || "").includes("SSD") || (d.disk_type || "").includes("NVMe")).length;
        const parts = [];
        if (hdds > 0) parts.push(`${hdds}x HDD`);
        if (ssds > 0) parts.push(`${ssds}x SSD/NVMe`);
        drivesCount.textContent = parts.length > 0 ? parts.join(" + ") : `${smart.disks.length}x Disques`;
      }
    }

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
  updateFloatingDockLayout();
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

function formatBytes(bytes, decimals = 1) {
  if (!bytes || isNaN(bytes) || bytes <= 0) return "0 B";
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ["B", "Ko", "Mo", "Go", "To", "Po"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  if (i < 0) return "0 B";
  if (i >= sizes.length) return (bytes / Math.pow(k, sizes.length - 1)).toFixed(dm) + " " + sizes[sizes.length - 1];
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + " " + sizes[i];
}
if (typeof window !== "undefined") {
  window.formatBytes = formatBytes;
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

// ==========================================================================
// DOCK FLOTTANT UNIFIÉ EN BAS À DROITE : SUPERPOSITION & DÉROULEMENT
// ==========================================================================
let floatingDockFolded = false;

function updateFloatingDockLayout() {
  const stack = document.getElementById("floating-dock-stack");
  const summary = document.getElementById("floating-dock-summary");
  const countBadge = document.getElementById("floating-dock-count");
  const toggleIcon = document.getElementById("floating-dock-toggle-icon");
  const toggleText = document.getElementById("floating-dock-toggle-text");
  const toastContainer = document.querySelector(".toast-container");
  const dock = document.getElementById("floating-task-dock");

  if (!stack) return;

  // Récupérer uniquement les vraies cartes de tâches ayant la classe .floating-task-card et visibles
  const visibleCards = Array.from(stack.querySelectorAll(".floating-task-card")).filter(child => {
    return child.style.display !== "none" && window.getComputedStyle(child).display !== "none";
  });

  const count = visibleCards.length;

  if (summary) {
    if (count >= 2) {
      summary.style.display = "flex";
      if (countBadge) countBadge.textContent = `${count}`;
      if (toggleIcon) toggleIcon.textContent = floatingDockFolded ? "▼" : "▲";
      if (toggleText) toggleText.textContent = floatingDockFolded ? "Dérouler" : "Tout replier";
    } else {
      summary.style.display = "none";
    }
  }

  // Ajustement dynamique de l'élévation des notifications (.toast-container)
  if (toastContainer && dock) {
    setTimeout(() => {
      const dockHeight = dock.offsetHeight;
      if (count > 0 && dockHeight > 0) {
        toastContainer.style.bottom = `${dockHeight + 36}px`;
      } else {
        toastContainer.style.bottom = "24px";
      }
    }, 50);
  }
}

function toggleFloatingDockUnfold() {
  floatingDockFolded = !floatingDockFolded;
  const stack = document.getElementById("floating-dock-stack");
  if (!stack) return;

  const cards = Array.from(stack.querySelectorAll(".floating-task-card"));
  cards.forEach(card => {
    if (floatingDockFolded) {
      card.classList.add("minimized");
    } else {
      card.classList.remove("minimized");
    }
  });

  updateFloatingDockLayout();
}

function toggleFloatingCardMinimize(cardId) {
  const card = document.getElementById(cardId);
  if (!card) return;
  card.classList.toggle("minimized");
  updateFloatingDockLayout();
}

function toggleUploadTrayMinimize() {
  toggleFloatingCardMinimize("upload-tray");
  const tray = document.getElementById("upload-tray");
  const btn = document.getElementById("upload-tray-min-btn");
  if (btn && tray) {
    btn.textContent = tray.classList.contains("minimized") ? "□" : "_";
  }
}

function closeUploadTray() {
  const tray = document.getElementById("upload-tray");
  if (tray) {
    tray.style.display = "none";
    updateFloatingDockLayout();
  }
}

// ==========================================================================
// POPUP FLOTTANTE : SUIVI DE TRANSFERT DE FICHIERS (COPIE / DÉPLACEMENT)
// ==========================================================================
let fileTransferAutoCloseTimer = null;

function startFileTransferTray(opType, totalItems, destPath) {
  if (fileTransferAutoCloseTimer) {
    clearTimeout(fileTransferAutoCloseTimer);
    fileTransferAutoCloseTimer = null;
  }

  const tray = document.getElementById("file-transfer-floating-tray");
  if (!tray) return;

  const isCut = opType === "cut" || opType === "move";
  const icon = document.getElementById("file-transfer-icon");
  const title = document.getElementById("file-transfer-title");
  const subtitle = document.getElementById("file-transfer-subtitle");
  const badge = document.getElementById("file-transfer-badge");
  const destEl = document.getElementById("file-transfer-dest");
  const countEl = document.getElementById("file-transfer-count");
  const bar = document.getElementById("file-transfer-progress-bar");
  const currentFile = document.getElementById("file-transfer-current-file");
  const listEl = document.getElementById("file-transfer-items-list");

  if (icon) icon.textContent = isCut ? "✂️" : "📋";
  if (title) title.textContent = isCut ? "Déplacement de fichiers" : "Copie de fichiers";
  if (subtitle) subtitle.textContent = `Préparation du transfert...`;
  if (badge) {
    badge.className = "badge badge-accent";
    badge.textContent = "0%";
  }
  if (destEl) {
    destEl.textContent = `📁 ${destPath}`;
    destEl.title = destPath;
  }
  if (countEl) countEl.textContent = `0/${totalItems}`;
  if (bar) {
    bar.style.width = "0%";
    bar.style.background = "linear-gradient(90deg, var(--mauve), var(--blue))";
  }
  if (currentFile) currentFile.textContent = "Démarrage...";
  if (listEl) listEl.innerHTML = "";

  tray.style.display = "block";
  tray.classList.remove("minimized");
  updateFloatingDockLayout();
}

function updateFileTransferTrayItem(currentIndex, totalItems, itemName, actionLabel, isDone, isError, errorMsg) {
  const percent = totalItems > 0 ? Math.round((currentIndex / totalItems) * 100) : 0;
  const bar = document.getElementById("file-transfer-progress-bar");
  const badge = document.getElementById("file-transfer-badge");
  const subtitle = document.getElementById("file-transfer-subtitle");
  const countEl = document.getElementById("file-transfer-count");
  const currentFile = document.getElementById("file-transfer-current-file");
  const listEl = document.getElementById("file-transfer-items-list");

  if (bar) bar.style.width = `${percent}%`;
  if (badge) badge.textContent = `${percent}%`;
  if (countEl) countEl.textContent = `${currentIndex}/${totalItems}`;
  if (subtitle) subtitle.textContent = `${actionLabel} (${currentIndex}/${totalItems})`;
  if (currentFile) currentFile.textContent = `${itemName}`;

  if (listEl && itemName) {
    const row = document.createElement("div");
    row.className = "floating-card-item-row";
    const statusText = isError ? `❌ ${errorMsg || 'Échec'}` : (isDone ? "✅ Terminé" : "⏳ En cours");
    const statusColor = isError ? "var(--red)" : (isDone ? "var(--green)" : "var(--mauve)");
    row.innerHTML = `
      <span class="item-name" title="${escapeHtml(itemName)}">📄 ${escapeHtml(itemName)}</span>
      <span class="item-status" style="color: ${statusColor};">${statusText}</span>
    `;
    listEl.prepend(row);
    while (listEl.children.length > 5) {
      listEl.removeChild(listEl.lastChild);
    }
  }
}

function finishFileTransferTray(successCount, totalItems, lastError) {
  const bar = document.getElementById("file-transfer-progress-bar");
  const badge = document.getElementById("file-transfer-badge");
  const title = document.getElementById("file-transfer-title");
  const subtitle = document.getElementById("file-transfer-subtitle");
  const currentFile = document.getElementById("file-transfer-current-file");

  if (lastError && successCount === 0) {
    if (title) title.textContent = "Transfert interrompu";
    if (subtitle) subtitle.textContent = lastError;
    if (badge) {
      badge.className = "badge badge-danger";
      badge.textContent = "❌ Échec";
    }
    if (bar) {
      bar.style.width = "100%";
      bar.style.background = "var(--red)";
    }
    if (currentFile) currentFile.textContent = "Erreur survenue";
  } else {
    if (title) title.textContent = "Transfert terminé";
    if (subtitle) subtitle.textContent = `${successCount}/${totalItems} élément(s) transféré(s)`;
    if (badge) {
      badge.className = "badge badge-success";
      badge.textContent = "✅ 100%";
    }
    if (bar) {
      bar.style.width = "100%";
      bar.style.background = "var(--green)";
    }
    if (currentFile) currentFile.textContent = "Tous les fichiers ont été copiés avec succès";

    fileTransferAutoCloseTimer = setTimeout(() => {
      closeFileTransferTray();
    }, 7000);
  }

  updateFloatingDockLayout();
}

function closeFileTransferTray() {
  if (fileTransferAutoCloseTimer) {
    clearTimeout(fileTransferAutoCloseTimer);
    fileTransferAutoCloseTimer = null;
  }
  const tray = document.getElementById("file-transfer-floating-tray");
  if (tray) {
    tray.style.display = "none";
    updateFloatingDockLayout();
  }
}

// ==========================================================================
// POPUP FLOTTANTE : SUIVI DE COMPRESSION & EXTRACTION D'ARCHIVES
// ==========================================================================
let fileArchiveTimerInterval = null;
let fileArchiveAutoCloseTimer = null;
let fileArchiveStartTime = 0;

function startFileArchiveTray(opType, targetName, destDir, detailText) {
  if (fileArchiveTimerInterval) clearInterval(fileArchiveTimerInterval);
  if (fileArchiveAutoCloseTimer) clearTimeout(fileArchiveAutoCloseTimer);

  const tray = document.getElementById("file-archive-floating-tray");
  if (!tray) return;

  const isCompress = opType === "compress";
  const icon = document.getElementById("file-archive-icon");
  const title = document.getElementById("file-archive-title");
  const subtitle = document.getElementById("file-archive-subtitle");
  const badge = document.getElementById("file-archive-badge");
  const targetPathEl = document.getElementById("file-archive-target-path");
  const timerEl = document.getElementById("file-archive-timer");
  const bar = document.getElementById("file-archive-progress-bar");
  const descEl = document.getElementById("file-archive-status-desc");

  if (icon) {
    icon.className = "floating-card-icon spin-slow";
    icon.textContent = isCompress ? "🗜️" : "📦";
  }
  if (title) title.textContent = isCompress ? "Compression d'archive" : "Extraction d'archive";
  if (subtitle) subtitle.textContent = isCompress ? "Génération en tâche de fond..." : "Décompression en cours...";
  if (badge) {
    badge.className = "badge badge-warning";
    badge.textContent = "⏳ En cours";
  }
  if (targetPathEl) {
    targetPathEl.textContent = `📦 ${targetName}`;
    targetPathEl.title = targetName;
  }
  if (descEl) descEl.textContent = detailText || (isCompress ? "Création de l'archive et compression..." : `Extraction vers ${destDir}...`);
  if (bar) {
    bar.className = "floating-card-progress-bar progress-animated";
    bar.style.width = "40%";
    bar.style.background = "";
  }
  if (timerEl) timerEl.textContent = "⏱️ 0s";

  fileArchiveStartTime = Date.now();
  fileArchiveTimerInterval = setInterval(() => {
    const elapsed = Math.round((Date.now() - fileArchiveStartTime) / 1000);
    if (timerEl) timerEl.textContent = `⏱️ ${elapsed}s`;
  }, 1000);

  tray.style.display = "block";
  tray.classList.remove("minimized");
  updateFloatingDockLayout();
}

function finishFileArchiveTray(isSuccess, message, durationSec) {
  if (fileArchiveTimerInterval) {
    clearInterval(fileArchiveTimerInterval);
    fileArchiveTimerInterval = null;
  }

  const icon = document.getElementById("file-archive-icon");
  const title = document.getElementById("file-archive-title");
  const subtitle = document.getElementById("file-archive-subtitle");
  const badge = document.getElementById("file-archive-badge");
  const bar = document.getElementById("file-archive-progress-bar");
  const timerEl = document.getElementById("file-archive-timer");
  const descEl = document.getElementById("file-archive-status-desc");

  if (timerEl) timerEl.textContent = `⏱️ ${durationSec}s`;

  if (icon) icon.className = "floating-card-icon";

  if (isSuccess) {
    if (icon) icon.textContent = "✅";
    if (title) title.textContent = "Opération réussie";
    if (subtitle) subtitle.textContent = `Terminée en ${durationSec}s`;
    if (badge) {
      badge.className = "badge badge-success";
      badge.textContent = `✅ Succès (${durationSec}s)`;
    }
    if (bar) {
      bar.className = "floating-card-progress-bar";
      bar.style.width = "100%";
      bar.style.background = "var(--green)";
    }
    if (descEl) descEl.textContent = message || "L'archive a été traitée avec succès.";

    fileArchiveAutoCloseTimer = setTimeout(() => {
      closeFileArchiveTray();
    }, 8000);
  } else {
    if (icon) icon.textContent = "❌";
    if (title) title.textContent = "Échec de l'opération";
    if (subtitle) subtitle.textContent = "Une erreur est survenue";
    if (badge) {
      badge.className = "badge badge-danger";
      badge.textContent = "❌ Échec";
    }
    if (bar) {
      bar.className = "floating-card-progress-bar";
      bar.style.width = "100%";
      bar.style.background = "var(--red)";
    }
    if (descEl) descEl.textContent = message || "Erreur lors du traitement de l'archive.";
  }

  updateFloatingDockLayout();
}

function closeFileArchiveTray() {
  if (fileArchiveTimerInterval) {
    clearInterval(fileArchiveTimerInterval);
    fileArchiveTimerInterval = null;
  }
  if (fileArchiveAutoCloseTimer) {
    clearTimeout(fileArchiveAutoCloseTimer);
    fileArchiveAutoCloseTimer = null;
  }
  const tray = document.getElementById("file-archive-floating-tray");
  if (tray) {
    tray.style.display = "none";
    updateFloatingDockLayout();
  }
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

  const streamUrl = buildAuthenticatedUrl("/api/files/stream", { path });
  if (title) title.textContent = fileName || path.split("/").pop();
  if (nativeBtn) nativeBtn.href = streamUrl;

  video.src = streamUrl;
  video.currentTime = 0;
  modal.classList.remove("mini-player-mode");
  const winMpv = modal.querySelector(".file-modal-window");
  if (winMpv) {
    winMpv.style.top = "";
    winMpv.style.right = "";
    winMpv.style.left = "";
    winMpv.style.bottom = "";
    winMpv.style.margin = "";
  }
  const pipIcon = document.getElementById("mpv-pip-icon");
  if (pipIcon) pipIcon.textContent = "🗗";
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
      e.preventDefault();
      if (video.paused) {
        video.play();
        showMpvOsd("▶ Lecture");
      } else {
        video.pause();
        showMpvOsd("⏸ Pause");
      }
      break;

    case "KeyP":
      e.preventDefault();
      toggleMediaPip("mpv-modal");
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


// ==========================================================================
// FONCTIONS DE GESTION DES ARCHIVES & MULTI-SÉLECTION
// ==========================================================================

function togglePasswordVisibility(inputId, btnEl) {
  const inp = document.getElementById(inputId);
  if (!inp) return;
  if (inp.type === "password") {
    inp.type = "text";
    if (btnEl) btnEl.textContent = "🙈";
  } else {
    inp.type = "password";
    if (btnEl) btnEl.textContent = "👁️";
  }
}

function triggerMultiCopy() {
  if (selectedFilePaths.size === 0) return;
  const paths = Array.from(selectedFilePaths);
  fileClipboard = {
    action: "copy",
    paths: paths,
    path: paths[0],
    name: `${paths.length} élément(s)`
  };
  showToast(`${paths.length} élément(s) copié(s) dans le presse-papiers`, "info");
  updateFilesStatusBar(currentEntries.length, 0);
}

function triggerMultiCut() {
  if (selectedFilePaths.size === 0) return;

  const selectedEntries = currentEntries.filter(e => selectedFilePaths.has(e.path));
  const mountPoints = selectedEntries.filter(e => e.is_mount_point);
  if (mountPoints.length > 0) {
    const mpNames = mountPoints.map(m => m.name).join(", ");
    showToast(`Impossible de couper : ${mpNames} est un point de montage de disque protégé.`, "error");
    return;
  }

  const paths = Array.from(selectedFilePaths);
  fileClipboard = {
    action: "cut",
    paths: paths,
    path: paths[0],
    name: `${paths.length} élément(s)`
  };
  showToast(`${paths.length} élément(s) coupé(s) dans le presse-papiers`, "info");
  updateFilesStatusBar(currentEntries.length, 0);
}

async function triggerMultiDelete() {
  if (selectedFilePaths.size === 0) return;

  const selectedEntries = currentEntries.filter(e => selectedFilePaths.has(e.path));
  const mountPoints = selectedEntries.filter(e => e.is_mount_point);
  const targets = selectedEntries.filter(e => !e.is_mount_point);

  if (mountPoints.length > 0) {
    const mpNames = mountPoints.map(m => m.name).join(", ");
    if (targets.length === 0) {
      showToast(`Suppression interdite : ${mpNames} est un point de montage de disque protégé.`, "error");
      return;
    } else {
      showToast(`Note : les points de montage suivants sont protégés et exclus de la suppression : ${mpNames}`, "warning");
    }
  }

  const count = targets.length;
  if (count === 0) return;

  if (!confirm(`Voulez-vous déplacer ces ${count} élément(s) vers la corbeille ?`)) {
    return;
  }

  let successCount = 0;
  for (const item of targets) {
    try {
      const res = await fetch("/api/files/delete", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: item.path, permanent: false })
      });
      const json = await res.json();
      if (json.success) successCount++;
    } catch (e) {
      console.error(e);
    }
  }

  showToast(`${successCount}/${count} élément(s) mis à la corbeille`, successCount === count ? "success" : "warning");
  clearFileSelection();
  refreshCurrentFolder();
}

function showCompressModal(itemsOverride) {
  let items = itemsOverride;
  if (!items || items.length === 0) {
    if (selectedFilePaths.size > 0) {
      items = Array.from(selectedFilePaths);
    } else if (selectedFileItem) {
      items = [selectedFileItem.path];
    }
  }

  if (!items || items.length === 0) {
    showToast("Veuillez sélectionner au moins un fichier ou dossier à compresser", "warning");
    return;
  }

  const countEl = document.getElementById("compress-summary-count");
  if (countEl) {
    countEl.textContent = `${items.length} élément${items.length > 1 ? "s" : ""} à compresser`;
  }
  const listEl = document.getElementById("compress-items-list");
  if (listEl) {
    listEl.innerHTML = items.map(p => `<div>• ${escapeHtml(p.split("/").pop())}</div>`).join("");
  }

  let defaultName = "archive.zip";
  if (items.length === 1) {
    const singleName = items[0].split("/").pop();
    const base = singleName.replace(/\.[^/.]+$/, "");
    defaultName = `${base || singleName}.zip`;
  } else {
    const folderName = currentFolderPath.split("/").filter(Boolean).pop() || "nas";
    defaultName = `${folderName}_archive.zip`;
  }

  const nameInput = document.getElementById("compress-archive-name");
  if (nameInput) nameInput.value = defaultName;

  selectCompressFormat("zip");
  selectCompressLevel("normal");

  const pwdCheck = document.getElementById("compress-enable-password");
  if (pwdCheck) pwdCheck.checked = false;
  const pwdWrap = document.getElementById("compress-password-input-wrap");
  if (pwdWrap) pwdWrap.style.display = "none";
  const pwdInput = document.getElementById("compress-password");
  if (pwdInput) pwdInput.value = "";

  const curDestCode = document.getElementById("compress-current-dest-path");
  if (curDestCode) curDestCode.textContent = currentFolderPath;
  const radCurrent = document.querySelector('input[name="compress-dest-mode"][value="current"]');
  if (radCurrent) radCurrent.checked = true;
  toggleCompressDestCustom(false);

  const modal = document.getElementById("modal-compress");
  if (modal) modal.style.display = "flex";
}

function closeCompressModal() {
  const modal = document.getElementById("modal-compress");
  if (modal) modal.style.display = "none";
}

function selectCompressFormat(fmt) {
  currentCompressFormat = fmt;
  document.querySelectorAll(".archive-format-card").forEach(c => {
    c.classList.toggle("active", c.getAttribute("data-format") === fmt);
  });

  const nameInput = document.getElementById("compress-archive-name");
  if (nameInput && nameInput.value) {
    let base = nameInput.value.replace(/\.(zip|7z|tar\.gz|tgz|tar\.bz2|tbz2|tar\.xz|txz|tar\.zst|tzst|tar)$/i, "");
    if (!base) base = "archive";
    let ext = fmt;
    if (fmt === "tar.gz") ext = "tar.gz";
    else if (fmt === "tar.xz") ext = "tar.xz";
    else if (fmt === "tar.zst") ext = "tar.zst";
    nameInput.value = `${base}.${ext}`;
  }

  const pwdGroup = document.getElementById("compress-password-group");
  if (pwdGroup) {
    if (fmt === "tar" || fmt === "tar.gz" || fmt === "tar.xz" || fmt === "tar.zst") {
      pwdGroup.style.opacity = "0.45";
      pwdGroup.title = "Le chiffrement par mot de passe natif nécessite le format ZIP ou 7-Zip";
      const chk = document.getElementById("compress-enable-password");
      if (chk) chk.disabled = true;
      toggleCompressPasswordField(false);
    } else {
      pwdGroup.style.opacity = "1";
      pwdGroup.title = "";
      const chk = document.getElementById("compress-enable-password");
      if (chk) chk.disabled = false;
    }
  }
}

function selectCompressLevel(lvl) {
  currentCompressLevel = lvl;
  ["fast", "normal", "maximum"].forEach(l => {
    const pill = document.getElementById(`pill-level-${l}`);
    if (pill) {
      pill.classList.toggle("active", l === lvl);
      const rad = pill.querySelector("input");
      if (rad) rad.checked = (l === lvl);
    }
  });
}

function toggleCompressPasswordField(force) {
  const chk = document.getElementById("compress-enable-password");
  const show = typeof force === "boolean" ? force : (chk ? chk.checked : false);
  const wrap = document.getElementById("compress-password-input-wrap");
  if (wrap) wrap.style.display = show ? "block" : "none";
}

function toggleCompressDestCustom(isCustom) {
  const wrap = document.getElementById("compress-custom-dest-wrap");
  if (wrap) {
    wrap.style.display = isCustom ? "block" : "none";
    if (isCustom) {
      const inp = document.getElementById("compress-custom-dest");
      if (inp && !inp.value) inp.value = currentFolderPath;
    }
  }
}

async function submitCompress() {
  const btn = document.getElementById("btn-submit-compress");
  const nameInput = document.getElementById("compress-archive-name");
  let archiveName = nameInput ? nameInput.value.trim() : "";
  if (!archiveName) {
    showToast("Veuillez renseigner un nom pour l'archive", "warning");
    if (nameInput) nameInput.focus();
    return;
  }

  const fmt = currentCompressFormat;
  let ext = "." + fmt;
  if (fmt === "tar.gz") ext = ".tar.gz";
  else if (fmt === "tar.xz") ext = ".tar.xz";
  else if (fmt === "tar.zst") ext = ".tar.zst";

  if (!archiveName.toLowerCase().endsWith(ext.toLowerCase())) {
    archiveName += ext;
  }

  let items = Array.from(selectedFilePaths);
  if (items.length === 0 && selectedFileItem) {
    items = [selectedFileItem.path];
  }
  if (items.length === 0) {
    showToast("Aucun fichier sélectionné", "warning");
    return;
  }

  const destMode = document.querySelector('input[name="compress-dest-mode"]:checked')?.value || "current";
  let destDir = currentFolderPath;
  if (destMode === "custom") {
    const customInput = document.getElementById("compress-custom-dest");
    const val = customInput ? customInput.value.trim() : "";
    if (val) destDir = val;
  }

  let password = null;
  const enablePwd = document.getElementById("compress-enable-password")?.checked;
  if (enablePwd) {
    const pwdInput = document.getElementById("compress-password");
    password = pwdInput ? pwdInput.value : "";
    if (!password) {
      showToast("Veuillez saisir un mot de passe ou désactiver la protection", "warning");
      if (pwdInput) pwdInput.focus();
      return;
    }
  }

  const origBtnHtml = btn ? btn.innerHTML : "";
  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span>⏳</span> Compression en cours...`;
  }

  // Fermer la modale immédiatement pour libérer l'écran et lancer la popup flottante en bas à droite
  closeCompressModal();
  startFileArchiveTray('compress', archiveName, destDir, `${items.length} élément(s) • Format .${fmt}`);
  const startTime = Date.now();
  showToast(`Création de l'archive ${archiveName}...`, "info");

  try {
    const res = await fetch("/api/files/compress", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        sources: items,
        dest_dir: destDir,
        archive_name: archiveName,
        format: fmt,
        compression_level: currentCompressLevel,
        password: password || null
      })
    });
    const rawText = await res.text();
    let json;
    try {
      json = JSON.parse(rawText);
    } catch (e) {
      const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
      finishFileArchiveTray(false, "Réponse serveur invalide", duration);
      showToast("Erreur serveur : " + (rawText || "Réponse invalide"), "error");
      return;
    }
    const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
    if (json.success) {
      finishFileArchiveTray(true, json.message || "Archive créée avec succès !", duration);
      showToast(json.message || "Archive créée avec succès !", "success");
      clearFileSelection();
      refreshCurrentFolder();
    } else {
      finishFileArchiveTray(false, json.message || "Échec de la compression", duration);
      showToast("Erreur lors de la compression : " + (json.message || "Échec"), "error");
    }
  } catch (err) {
    const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
    finishFileArchiveTray(false, "Erreur réseau : " + err, duration);
    showToast("Erreur réseau : " + err, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = origBtnHtml;
    }
  }
}

async function showExtractModal(archivePath, archiveName) {
  const path = archivePath || (selectedFileItem ? selectedFileItem.path : null);
  const name = archiveName || (selectedFileItem ? selectedFileItem.name : (path ? path.split("/").pop() : "archive"));

  if (!path) {
    showToast("Aucune archive spécifiée pour l'extraction", "warning");
    return;
  }

  document.getElementById("extract-archive-path").value = path;
  document.getElementById("extract-archive-name").textContent = name;
  document.getElementById("extract-current-dest-path").textContent = currentFolderPath;

  const infoEl = document.getElementById("extract-archive-info-text");
  if (infoEl) infoEl.textContent = "Analyse de l'archive en cours...";

  const pwdInput = document.getElementById("extract-password");
  if (pwdInput) pwdInput.value = "";

  const radCurrent = document.querySelector('input[name="extract-dest-mode"][value="current"]');
  if (radCurrent) radCurrent.checked = true;
  toggleExtractDestCustom(false);

  const subfolderChk = document.getElementById("extract-create-subfolder");
  if (subfolderChk) subfolderChk.checked = true;

  const modal = document.getElementById("modal-extract");
  if (modal) modal.style.display = "flex";

  try {
    const res = await fetch("/api/files/archive-info", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ archive_path: path })
    });
    const json = await res.json();
    if (json.success && json.data) {
      const d = json.data;
      let text = `Format : ${d.format.toUpperCase()}`;
      if (d.file_count > 0) text += ` • ${d.file_count} fichier(s) détecté(s)`;
      if (d.is_encrypted) {
        text += ` • 🔒 Archive chiffrée (mot de passe requis)`;
        if (pwdInput) pwdInput.placeholder = "Mot de passe requis pour déchiffrer";
      } else {
        if (pwdInput) pwdInput.placeholder = "Laisser vide si non chiffré";
      }
      if (infoEl) infoEl.textContent = text;
    } else {
      if (infoEl) infoEl.textContent = "Format d'archive prêt à être extrait";
    }
  } catch (e) {
    if (infoEl) infoEl.textContent = "Format d'archive prêt à être extrait";
  }
}

function closeExtractModal() {
  const modal = document.getElementById("modal-extract");
  if (modal) modal.style.display = "none";
}

function toggleExtractDestCustom(isCustom) {
  const wrap = document.getElementById("extract-custom-dest-wrap");
  if (wrap) {
    wrap.style.display = isCustom ? "block" : "none";
    if (isCustom) {
      const inp = document.getElementById("extract-custom-dest");
      if (inp && !inp.value) inp.value = currentFolderPath;
    }
  }
}

async function submitExtract() {
  const btn = document.getElementById("btn-submit-extract");
  const archivePath = document.getElementById("extract-archive-path")?.value;
  if (!archivePath) {
    showToast("Chemin d'archive introuvable", "error");
    return;
  }

  const destMode = document.querySelector('input[name="extract-dest-mode"]:checked')?.value || "current";
  let destDir = currentFolderPath;
  if (destMode === "custom") {
    const customInput = document.getElementById("extract-custom-dest");
    const val = customInput ? customInput.value.trim() : "";
    if (val) destDir = val;
  }

  const createSubfolder = document.getElementById("extract-create-subfolder")?.checked ?? true;
  const pwdInput = document.getElementById("extract-password");
  const password = (pwdInput && pwdInput.value.trim()) ? pwdInput.value.trim() : null;

  const origBtnHtml = btn ? btn.innerHTML : "";
  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span>⏳</span> Extraction en cours...`;
  }

  const archiveName = archivePath.split("/").pop() || "archive";
  closeExtractModal();
  startFileArchiveTray('extract', archiveName, destDir, `Extraction vers ${destDir}`);
  const startTime = Date.now();
  showToast(`Extraction de ${archiveName}...`, "info");

  try {
    const res = await fetch("/api/files/extract", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        archive_path: archivePath,
        dest_dir: destDir,
        create_subfolder: createSubfolder,
        password: password || null
      })
    });
    const rawText = await res.text();
    let json;
    try {
      json = JSON.parse(rawText);
    } catch (e) {
      const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
      finishFileArchiveTray(false, "Réponse serveur invalide", duration);
      showToast("Erreur serveur : " + (rawText || "Réponse invalide"), "error");
      return;
    }
    const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
    if (json.success) {
      finishFileArchiveTray(true, json.message || "Archive extraite avec succès !", duration);
      showToast(json.message || "Archive extraite avec succès !", "success");
      clearFileSelection();
      refreshCurrentFolder();
    } else {
      const errMsg = json.message || "";
      if (errMsg.toLowerCase().includes("mot de passe") || errMsg.toLowerCase().includes("password") || errMsg.toLowerCase().includes("chiffr")) {
        closeFileArchiveTray();
        openArchivePasswordModal(archivePath, archiveName, destDir, createSubfolder);
      } else {
        finishFileArchiveTray(false, errMsg || "Échec de l'extraction", duration);
        showToast("Erreur lors de l'extraction : " + errMsg, "error");
      }
    }
  } catch (err) {
    const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
    finishFileArchiveTray(false, "Erreur réseau : " + err, duration);
    showToast("Erreur réseau : " + err, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = origBtnHtml;
    }
  }
}

async function extractArchiveDirect(path, name) {
  showToast(`Vérification de l'archive ${name}...`, "info");
  try {
    const infoRes = await fetch("/api/files/archive-info", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ archive_path: path })
    });
    const infoJson = await infoRes.json();
    if (infoJson.success && infoJson.data && infoJson.data.is_encrypted) {
      openArchivePasswordModal(path, name, currentFolderPath, true);
      return;
    }
  } catch (e) {
    // Continue direct attempt
  }

  startFileArchiveTray('extract', name, currentFolderPath, `Extraction directe vers ${currentFolderPath}`);
  const startTime = Date.now();
  showToast(`Extraction de ${name} en cours... ⏳`, "info");

  try {
    const res = await fetch("/api/files/extract", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        archive_path: path,
        dest_dir: currentFolderPath,
        create_subfolder: true,
        password: null
      })
    });
    const rawText = await res.text();
    let json;
    try {
      json = JSON.parse(rawText);
    } catch (e) {
      const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
      finishFileArchiveTray(false, "Réponse serveur invalide", duration);
      showToast("Erreur serveur : " + (rawText || "Réponse invalide"), "error");
      return;
    }
    const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
    if (json.success) {
      finishFileArchiveTray(true, json.message || `Archive ${name} extraite avec succès !`, duration);
      showToast(json.message || `Archive ${name} extraite avec succès !`, "success");
      refreshCurrentFolder();
    } else {
      const errMsg = json.message || "";
      if (errMsg.toLowerCase().includes("mot de passe") || errMsg.toLowerCase().includes("password") || errMsg.toLowerCase().includes("chiffr")) {
        closeFileArchiveTray();
        openArchivePasswordModal(path, name, currentFolderPath, true);
      } else {
        finishFileArchiveTray(false, errMsg || "Échec de l'extraction", duration);
        showToast("Échec de l'extraction : " + errMsg, "error");
      }
    }
  } catch (err) {
    const duration = Math.max(1, Math.round((Date.now() - startTime) / 1000));
    finishFileArchiveTray(false, "Erreur réseau : " + err, duration);
    showToast("Erreur réseau : " + err, "error");
  }
}

function openArchivePasswordModal(archivePath, archiveName, destDir, createSubfolder) {
  document.getElementById("pwd-prompt-archive-path").value = archivePath;
  document.getElementById("pwd-prompt-dest-path").value = destDir || currentFolderPath;
  document.getElementById("pwd-prompt-subfolder").value = createSubfolder ? "true" : "false";
  document.getElementById("pwd-prompt-archive-name").textContent = archiveName || archivePath.split("/").pop();
  const input = document.getElementById("pwd-prompt-input");
  if (input) {
    input.value = "";
    setTimeout(() => input.focus(), 150);
  }
  document.getElementById("modal-archive-password").style.display = "flex";
}

function closeArchivePasswordModal() {
  document.getElementById("modal-archive-password").style.display = "none";
}

async function submitArchivePasswordPrompt() {
  const btn = document.getElementById("btn-submit-pwd-prompt");
  const archivePath = document.getElementById("pwd-prompt-archive-path").value;
  const destDir = document.getElementById("pwd-prompt-dest-path").value || currentFolderPath;
  const createSubfolder = document.getElementById("pwd-prompt-subfolder").value === "true";
  const pwdInput = document.getElementById("pwd-prompt-input");
  const password = pwdInput ? pwdInput.value : "";

  if (!password) {
    showToast("Veuillez saisir le mot de passe de l'archive", "warning");
    if (pwdInput) pwdInput.focus();
    return;
  }

  const origBtnHtml = btn ? btn.innerHTML : "";
  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span>⏳</span> Déchiffrement & Extraction...`;
  }

  try {
    const res = await fetch("/api/files/extract", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        archive_path: archivePath,
        dest_dir: destDir,
        create_subfolder: createSubfolder,
        password: password || null
      })
    });
    const rawText = await res.text();
    let json;
    try {
      json = JSON.parse(rawText);
    } catch (e) {
      showToast("Erreur serveur : " + (rawText || "Réponse invalide"), "error");
      return;
    }
    if (json.success) {
      showToast(json.message || "Archive déchiffrée et extraite avec succès !", "success");
      closeArchivePasswordModal();
      clearFileSelection();
      refreshCurrentFolder();
    } else {
      showToast("Échec : " + (json.message || "Mot de passe incorrect"), "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = origBtnHtml;
    }
  }
}

// ==========================================================================
// FONCTIONS DE GESTION DU MINI-LECTEUR FLOTTANT (PICTURE-IN-PICTURE)
// ==========================================================================

function toggleMediaPip(modalId, forceMini) {
  const modal = document.getElementById(modalId);
  if (!modal) return;

  const isMini = modal.classList.contains("mini-player-mode");
  const shouldBeMini = forceMini !== undefined ? forceMini : !isMini;

  const iconId = modalId === "mpv-modal" ? "mpv-pip-icon" : "audio-pip-icon";
  const btnId = modalId === "mpv-modal" ? "mpv-pip-btn" : "audio-pip-btn";
  const icon = document.getElementById(iconId);
  const btn = document.getElementById(btnId);
  const win = modal.querySelector(".file-modal-window");

  const header = modal.querySelector(".file-modal-header");

  if (shouldBeMini) {
    modal.classList.add("mini-player-mode");
    if (icon) icon.textContent = "⤢";
    if (btn) btn.title = "Agrandir le lecteur au centre (P)";

    if (win) {
      win.style.removeProperty("left");
      win.style.removeProperty("bottom");
      win.style.top = "122px";
      win.style.right = "24px";
      win.style.margin = "0";
    }

    if (header) {
      header.title = "Glisser pour déplacer le lecteur flottant";
    }

    if (modalId === "audio-modal") {
      const canvas = document.getElementById("audio-visualizer-canvas");
      if (canvas) {
        canvas.width = 480;
        canvas.height = 70;
      }
    }

    initMediaPipDrag(modalId);
  } else {
    modal.classList.remove("mini-player-mode");
    modal.classList.remove("is-dragging");
    if (icon) icon.textContent = "🗗";
    if (btn) btn.title = "Réduire en vignette flottante (P)";

    if (win) {
      win.style.top = "";
      win.style.right = "";
      win.style.left = "";
      win.style.bottom = "";
      win.style.margin = "";
    }

    if (header) {
      header.title = "";
    }

    if (modalId === "audio-modal") {
      const canvas = document.getElementById("audio-visualizer-canvas");
      if (canvas) {
        canvas.width = 680;
        canvas.height = 170;
      }
    }
  }
}

function initMediaPipDrag(modalId) {
  const modal = document.getElementById(modalId);
  if (!modal) return;
  const header = modal.querySelector(".file-modal-header");
  const win = modal.querySelector(".file-modal-window");
  if (!header || !win || header._pipDragInitialized) return;

  header._pipDragInitialized = true;
  let isDragging = false;
  let startX = 0, startY = 0, startLeft = 0, startTop = 0;

  const onDragStart = (e) => {
    if (!modal.classList.contains("mini-player-mode")) return;
    if (e.target.closest("button") || e.target.closest("a") || e.target.closest("input")) return;

    isDragging = true;
    modal.classList.add("is-dragging");
    const clientX = e.clientX !== undefined ? e.clientX : (e.touches && e.touches[0] ? e.touches[0].clientX : null);
    const clientY = e.clientY !== undefined ? e.clientY : (e.touches && e.touches[0] ? e.touches[0].clientY : null);
    if (clientX === null || clientY === null) return;

    const rect = win.getBoundingClientRect();
    startX = clientX;
    startY = clientY;
    startLeft = rect.left;
    startTop = rect.top;

    win.style.setProperty("right", "auto", "important");
    win.style.setProperty("bottom", "auto", "important");
    win.style.setProperty("left", `${startLeft}px`, "important");
    win.style.setProperty("top", `${startTop}px`, "important");
    win.style.transition = "none";
    document.body.style.userSelect = "none";

    const onDragMove = (ev) => {
      if (!isDragging) return;
      const curX = ev.clientX !== undefined ? ev.clientX : (ev.touches && ev.touches[0] ? ev.touches[0].clientX : null);
      const curY = ev.clientY !== undefined ? ev.clientY : (ev.touches && ev.touches[0] ? ev.touches[0].clientY : null);
      if (curX === null || curY === null) return;

      const dx = curX - startX;
      const dy = curY - startY;
      const newLeft = Math.max(8, Math.min(window.innerWidth - rect.width - 8, startLeft + dx));
      const newTop = Math.max(54, Math.min(window.innerHeight - rect.height - 8, startTop + dy));
      win.style.setProperty("left", `${newLeft}px`, "important");
      win.style.setProperty("top", `${newTop}px`, "important");
    };

    const onDragEnd = () => {
      if (!isDragging) return;
      isDragging = false;
      modal.classList.remove("is-dragging");
      win.style.transition = "";
      document.body.style.userSelect = "";
      window.removeEventListener("mousemove", onDragMove);
      window.removeEventListener("mouseup", onDragEnd);
      window.removeEventListener("touchmove", onDragMove);
      window.removeEventListener("touchend", onDragEnd);
    };

    window.addEventListener("mousemove", onDragMove, { passive: false });
    window.addEventListener("mouseup", onDragEnd);
    window.addEventListener("touchmove", onDragMove, { passive: false });
    window.addEventListener("touchend", onDragEnd);
  };

  header.addEventListener("mousedown", onDragStart);
  header.addEventListener("touchstart", onDragStart, { passive: false });
}

function handleModalOverlayClick(e, modalId) {
  if (e.target.id === modalId) {
    // Si clic dans le vide autour de la fenêtre de suivi de déploiement de jeu, on la réduit en popup flottante
    if (modalId === "modal-game-deploy-progress") {
      minimizeGameDeployModal();
      return;
    }
    // Si clic dans le vide autour du lecteur vidéo ou audio en mode centré, on le réduit en vignette flottante
    if (modalId === "mpv-modal" || modalId === "audio-modal") {
      const modal = document.getElementById(modalId);
      if (modal && !modal.classList.contains("mini-player-mode")) {
        toggleMediaPip(modalId, true);
        return;
      }
    }
    if (modalId === "image-modal") closeImageModal();
    if (modalId === "nvim-modal") closeNvimModal();
    if (modalId === "mpv-modal") closeMpvModal();
    if (modalId === "audio-modal") closeAudioModal();
    if (modalId === "create-raid-modal") closeCreateRaidModal();
    if (modalId === "format-disk-modal") closeFormatDiskModal();
    if (modalId === "doc-modal") closeDocModal();
    if (modalId === "modal-compress") closeCompressModal();
    if (modalId === "modal-extract") closeExtractModal();
    if (modalId === "modal-archive-password") closeArchivePasswordModal();
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

  modal.classList.remove("mini-player-mode");
  const winAud = modal.querySelector(".file-modal-window");
  if (winAud) {
    winAud.style.top = "";
    winAud.style.right = "";
    winAud.style.left = "";
    winAud.style.bottom = "";
    winAud.style.margin = "";
  }
  const audPipIcon = document.getElementById("audio-pip-icon");
  if (audPipIcon) audPipIcon.textContent = "🗗";
  const canvas = document.getElementById("audio-visualizer-canvas");
  if (canvas) {
    canvas.width = 680;
    canvas.height = 170;
  }

  const streamUrl = buildAuthenticatedUrl("/api/files/stream", { path });
  audioEl.src = streamUrl;

  if (!audioCtx) {
    try {
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
    } catch (e) {
      console.warn("Échec d'initialisation AudioContext :", e);
    }
  }

  modal.style.display = "flex";

  try {
    if (audioCtx && audioCtx.state === "suspended") {
      audioCtx.resume();
    }
  } catch (e) {}

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
      e.preventDefault();
      toggleAudioPlay();
      break;

    case "KeyP":
      e.preventDefault();
      toggleMediaPip("audio-modal");
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

function onMountPresetChanged() {
  const preset = document.getElementById("mount-flags-preset")?.value;
  const nofail = document.getElementById("flag-nofail");
  const noatime = document.getElementById("flag-noatime");
  const defaults = document.getElementById("flag-defaults");
  const compress = document.getElementById("flag-compress");
  const custom = document.getElementById("mount-custom-options");
  const fsType = document.getElementById("mount-select-fstype")?.value || "btrfs";

  if (preset === "nas-optimal") {
    if (nofail) nofail.checked = true;
    if (noatime) noatime.checked = true;
    if (defaults) defaults.checked = true;
    if (compress) compress.checked = (fsType === "btrfs");
    if (custom) custom.value = "";
  } else if (preset === "readonly") {
    if (nofail) nofail.checked = true;
    if (noatime) noatime.checked = false;
    if (defaults) defaults.checked = true;
    if (compress) compress.checked = false;
    if (custom) custom.value = "ro";
  }
}

function onMountFlagManualChange() {
  const preset = document.getElementById("mount-flags-preset");
  if (preset) preset.value = "custom";
}

function openMountVolumeModal(name, device, level, forceFormat = false, detectedFs = "") {
  const modal = document.getElementById("mount-volume-modal");
  const title = document.getElementById("mount-modal-title");
  const badge = document.getElementById("mount-modal-badge");
  const targetName = document.getElementById("mount-target-name");
  const targetDevice = document.getElementById("mount-target-device");
  const targetLevel = document.getElementById("mount-target-level");
  const targetFormat = document.getElementById("mount-target-format");
  const targetDetectedFs = document.getElementById("mount-target-detected-fs");
  const infoText = document.getElementById("mount-modal-info-text");
  const vgOptions = document.getElementById("mount-vg-options-wrap");
  const btn = document.getElementById("btn-submit-mount");
  const pathInput = document.getElementById("mount-input-path");
  const warningBox = document.getElementById("mount-format-warning-box");
  const preserveBox = document.getElementById("mount-preserve-info-box");
  const detectedFsLabel = document.getElementById("mount-detected-fs-label");
  const fstypeGroup = document.getElementById("mount-fstype-group");
  const confirmGroup = document.getElementById("mount-format-confirm-group");
  const confirmCheckbox = document.getElementById("mount-confirm-format-checkbox");

  if (!modal) return;

  if (targetName) targetName.value = name || "";
  if (targetDevice) targetDevice.value = device || "";
  if (targetLevel) targetLevel.value = level || "";
  if (targetFormat) targetFormat.value = forceFormat ? "true" : "false";
  if (targetDetectedFs) targetDetectedFs.value = detectedFs || "";

  const isVg = (level || "").toLowerCase().includes("grappe") || (level || "").toLowerCase().includes("pool");
  if (vgOptions) vgOptions.style.display = isVg ? "block" : "none";

  const devWrap = document.getElementById("mount-device-input-wrap");
  const customDevInput = document.getElementById("mount-custom-device");
  if (!device) {
    if (devWrap) devWrap.style.display = "block";
    if (customDevInput && !customDevInput.value) customDevInput.value = "/dev/sdb1";
  } else {
    if (devWrap) devWrap.style.display = "none";
  }

  // Suggérer un nom propre dans /mnt/
  if (pathInput) {
    const cleanPool = (name || "storage").toLowerCase().replace(/[^a-z0-9_-]/g, "");
    pathInput.value = `/mnt/${cleanPool || "storage"}`;
    const footerSummary = document.getElementById("mount-footer-summary");
    if (footerSummary) footerSummary.textContent = pathInput.value;
    pathInput.oninput = () => {
      if (footerSummary) footerSummary.textContent = pathInput.value || "/mnt/storage";
    };
  }

  // Réinitialiser les drapeaux sur NAS Optimal
  const presetSel = document.getElementById("mount-flags-preset");
  if (presetSel) presetSel.value = "nas-optimal";
  onMountPresetChanged();

  const persistCheck = document.getElementById("mount-checkbox-persist");
  if (persistCheck) persistCheck.checked = true;

  if (forceFormat) {
    if (badge) {
      badge.textContent = "FORMATAGE & DESTRUCTION";
      badge.className = "badge badge-danger";
    }
    if (title) title.textContent = `Formater & Monter le volume : ${name}`;
    if (warningBox) warningBox.style.display = "block";
    if (preserveBox) preserveBox.style.display = "none";
    if (fstypeGroup) fstypeGroup.style.display = "block";
    if (confirmGroup) confirmGroup.style.display = "block";
    if (confirmCheckbox) confirmCheckbox.checked = false;

    if (infoText) {
      infoText.innerHTML = `Le volume <strong>${escapeHtml(device)}</strong> va être <strong>intégralement formaté</strong>. Toutes les données seront effacées.`;
    }

    if (btn) {
      btn.textContent = "⚠️ Formater & Monter dans /mnt";
      btn.className = "btn btn-danger";
      btn.disabled = false;
    }
  } else {
    if (badge) {
      badge.textContent = "MONTAGE SANS FORMATAGE";
      badge.className = "badge badge-success";
    }
    if (title) title.textContent = `Monter le volume existant : ${name}`;
    if (warningBox) warningBox.style.display = "none";
    if (preserveBox) preserveBox.style.display = "block";
    if (detectedFsLabel) detectedFsLabel.textContent = (detectedFs || "Détecté").toUpperCase();
    if (fstypeGroup) fstypeGroup.style.display = "none";
    if (confirmGroup) confirmGroup.style.display = "none";

    if (infoText) {
      infoText.innerHTML = `Les données existantes sur <strong>${escapeHtml(device)}</strong> seront <strong>intégralement préservées</strong> (aucun formatage).`;
    }

    if (btn) {
      btn.textContent = "⚡ Monter sans formater dans /mnt";
      btn.className = "btn btn-primary";
      btn.disabled = false;
    }
  }

  modal.style.display = "flex";
}

function toggleMountSubmitBtn() {
  const cb = document.getElementById("mount-confirm-format-checkbox");
  const btn = document.getElementById("btn-submit-mount");
  if (!btn) return;
  const isStillFormat = document.getElementById("mount-target-format")?.value === "true";
  if (isStillFormat) {
    if (cb && cb.checked) {
      btn.textContent = "⚠️ Formater & Monter dans /mnt";
      btn.className = "btn btn-danger";
      btn.style.boxShadow = "0 0 16px rgba(243, 139, 168, 0.4)";
    } else {
      btn.textContent = "⚠️ Formater & Monter dans /mnt";
      btn.className = "btn btn-danger";
      btn.style.boxShadow = "none";
    }
  }
}

function closeMountVolumeModal() {
  const modal = document.getElementById("mount-volume-modal");
  if (modal) modal.style.display = "none";
}

async function submitMountVolume() {
  const name = document.getElementById("mount-target-name")?.value;
  const device = (document.getElementById("mount-target-device")?.value || document.getElementById("mount-custom-device")?.value || "").trim();
  const raidSelect = document.getElementById("mount-select-raid");
  const lvInput = document.getElementById("mount-input-lvname");
  const fstypeSelect = document.getElementById("mount-select-fstype");
  const pathInput = document.getElementById("mount-input-path");
  const btn = document.getElementById("btn-submit-mount");
  const isForceFormat = document.getElementById("mount-target-format")?.value === "true";
  const confirmCb = document.getElementById("mount-confirm-format-checkbox");
  const confirmBox = document.getElementById("mount-format-confirm-group");

  if (!device) {
    showToast("⚠️ Périphérique bloc introuvable. Veuillez vérifier ou renseigner un chemin valide.", "warning", 6000);
    return;
  }

  const mountpoint = (pathInput?.value || "").trim();
  if (!mountpoint || !mountpoint.startsWith("/")) {
    showToast("⚠️ Veuillez renseigner un point de montage valide commençant par '/' (ex: /mnt/storage).", "warning", 6000);
    if (pathInput) pathInput.focus();
    return;
  }

  // Contrôle interactif de sécurité si formatage destructif
  if (isForceFormat && (!confirmCb || !confirmCb.checked)) {
    showToast("⚠️ Action requise : Veuillez cocher la case de confirmation pour autoriser le formatage destructif !", "warning", 7000);
    if (confirmBox) {
      confirmBox.scrollIntoView({ behavior: "smooth", block: "center" });
      confirmBox.classList.add("shake-alert");
      setTimeout(() => confirmBox.classList.remove("shake-alert"), 1200);
    }
    if (confirmCb) confirmCb.focus();
    return;
  }

  // Récupérer les drapeaux sélectionnés
  const options = [];
  if (document.getElementById("flag-defaults")?.checked) options.push("defaults");
  if (document.getElementById("flag-noatime")?.checked) options.push("noatime");
  if (document.getElementById("flag-nofail")?.checked) options.push("nofail");

  const fsTypeVal = fstypeSelect?.value || "btrfs";
  if (fsTypeVal === "btrfs" && document.getElementById("flag-compress")?.checked) {
    options.push("compress=zstd");
  }

  const customOpts = document.getElementById("mount-custom-options")?.value.trim();
  if (customOpts) {
    customOpts.split(",").forEach(opt => {
      const clean = opt.trim();
      if (clean && !options.includes(clean)) options.push(clean);
    });
  }

  const persist = document.getElementById("mount-checkbox-persist")?.checked ?? true;

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span class="spinner-border spinner-border-sm" role="status" style="display:inline-block; width:14px; height:14px; border:2px solid currentColor; border-right-color:transparent; border-radius:50%; animation:spin 0.75s linear infinite; margin-right:8px;"></span> Formatage & Montage en cours...`;
  }

  try {
    const res = await fetch("/api/storage/mount", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: name || "storage",
        device: device,
        mountpoint: mountpoint,
        fs_type: fsTypeVal,
        raid_type: raidSelect?.value || "raid5",
        lv_name: lvInput?.value || "storage",
        options: options,
        persist: persist,
        force_format: isForceFormat
      })
    });

    const json = await res.json();
    if (json.success) {
      showToast(json.data || "Volume monté avec succès !", "success");
      closeMountVolumeModal();
      loadStorage();
    } else {
      showSystemError("Échec du montage du volume", "Le montage du périphérique a été interrompu par le système.", json.message || "Erreur inconnue", 12000);
    }
  } catch (e) {
    showToast("Erreur de connexion : " + e, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = isForceFormat ? "⚠️ Formater & Monter dans /mnt" : "⚡ Monter sans formater dans /mnt";
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
// MODALE ET GESTION DE LA DISSOLUTION / DESTRUCTION D'UNE GRAPPE RAID
// --------------------------------------------------------------------------
function openDestroyRaidModal(name, device) {
  const modal = document.getElementById("modal-destroy-raid");
  if (!modal) return;

  const r = (cachedLogicalRaids || []).find(x => x.name === name || x.device === device) || {
    name: name,
    device: device,
    level: "Grappe RAID",
    size_human: "--",
    mountpoint: null,
    members: []
  };

  const nameInput = document.getElementById("destroy-raid-name");
  const devInput = document.getElementById("destroy-raid-device");
  const mntInput = document.getElementById("destroy-raid-mountpoint");
  const labelEl = document.getElementById("destroy-raid-label");
  const levelBadge = document.getElementById("destroy-raid-level-badge");
  const sizeBadge = document.getElementById("destroy-raid-size-badge");
  const devLabel = document.getElementById("destroy-raid-dev-label");
  const mntWarn = document.getElementById("destroy-raid-mount-warning");
  const mntLabel = document.getElementById("destroy-raid-mount-label");
  const membersList = document.getElementById("destroy-raid-members-list");
  const confirmCb = document.getElementById("destroy-raid-confirm-checkbox");
  const btn = document.getElementById("btn-submit-destroy-raid");

  if (nameInput) nameInput.value = r.name || name || "";
  if (devInput) devInput.value = r.device || device || "";
  if (mntInput) mntInput.value = r.mountpoint || "";

  if (labelEl) labelEl.textContent = r.name || name;
  if (levelBadge) levelBadge.textContent = r.level || "RAID";
  if (sizeBadge) sizeBadge.textContent = r.size_human || "--";
  if (devLabel) devLabel.textContent = r.device || device;

  if (mntWarn && mntLabel) {
    if (r.mountpoint) {
      mntWarn.style.display = "block";
      mntLabel.textContent = r.mountpoint;
    } else {
      mntWarn.style.display = "none";
    }
  }

  if (membersList) {
    const mems = r.members || [];
    if (mems.length > 0) {
      membersList.innerHTML = mems.map(m => `
        <span class="member-disk-pill" style="padding:4px 10px; font-size:0.8rem; background:rgba(137,180,250,0.12); border:1px solid rgba(137,180,250,0.25); border-radius:6px; color:var(--blue); font-family:var(--font-mono); font-weight:600;">
          💿 ${escapeHtml(m)}
        </span>
      `).join("");
    } else {
      membersList.innerHTML = `<span style="font-size:0.8rem; color:var(--subtext0);">Disques physiques sous-jacents de la grappe</span>`;
    }
  }

  if (confirmCb) confirmCb.checked = false;
  if (btn) {
    btn.disabled = false;
    btn.innerHTML = "🧨 Casser Définitivement la Grappe";
  }

  modal.style.display = "flex";
}

function closeDestroyRaidModal() {
  const modal = document.getElementById("modal-destroy-raid");
  if (modal) modal.style.display = "none";
}

async function submitDestroyRaid() {
  const name = document.getElementById("destroy-raid-name")?.value;
  const device = document.getElementById("destroy-raid-device")?.value;
  const confirmCb = document.getElementById("destroy-raid-confirm-checkbox");
  const confirmBox = document.getElementById("destroy-raid-confirm-box");
  const wipeCheck = document.getElementById("destroy-raid-wipe-check");
  const btn = document.getElementById("btn-submit-destroy-raid");

  if (!name && !device) {
    showToast("⚠️ Identifiant de grappe introuvable.", "warning");
    return;
  }

  // Contrôle de confirmation obligatoire
  if (!confirmCb || !confirmCb.checked) {
    showToast("⚠️ Action requise : Veuillez cocher la case de confirmation pour autoriser la dissolution de la grappe !", "warning", 7000);
    if (confirmBox) {
      confirmBox.scrollIntoView({ behavior: "smooth", block: "center" });
      confirmBox.classList.add("shake-alert");
      setTimeout(() => confirmBox.classList.remove("shake-alert"), 1200);
    }
    if (confirmCb) confirmCb.focus();
    return;
  }

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span class="spinner-border spinner-border-sm" role="status" style="display:inline-block; width:14px; height:14px; border:2px solid currentColor; border-right-color:transparent; border-radius:50%; animation:spin 0.75s linear infinite; margin-right:8px;"></span> Dissolution en cours...`;
  }

  try {
    const res = await fetch("/api/storage/raids/destroy", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: name,
        device: device,
        wipe_members: wipeCheck?.checked ?? true
      })
    });

    const json = await res.json();
    if (json.success) {
      closeDestroyRaidModal();
      showToast("🧨 Tâche de dissolution RAID lancée en arrière-plan !", "success", 4000);
      activeStorageJob = json.data;
      updateStorageJobView(json.data);
      startPollingStorageJob();
      loadStorage();
    } else {
      showSystemError("Échec de la dissolution du RAID", "Le système n'a pas pu dissoudre la grappe RAID.", json.message || "Erreur inconnue", 12000);
    }
  } catch (err) {
    showToast("Erreur de connexion : " + err, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = "🧨 Casser Définitivement la Grappe";
    }
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
  if (userInput) userInput.value = getCurrentDashboardUsername();
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
  const user = document.getElementById("repair-perm-user")?.value || getCurrentDashboardUsername();
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
    downloadBtn.href = buildAuthenticatedUrl("/api/files/stream", { path: file.path });
    downloadBtn.download = file.name;
  }

  if (loadingOverlay) loadingOverlay.style.display = "flex";
  resetImageTransformState();

  const previewUrl = buildAuthenticatedUrl("/api/files/image-view", { path: file.path });
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
    const thumbUrl = buildAuthenticatedUrl("/api/files/image-view", { path: file.path, thumb: "true" });
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
    const compressModal = document.getElementById("modal-compress");
    if (compressModal && compressModal.style.display !== "none") {
      closeCompressModal();
      return;
    }
    const extractModal = document.getElementById("modal-extract");
    if (extractModal && extractModal.style.display !== "none") {
      closeExtractModal();
      return;
    }
    const pwdModal = document.getElementById("modal-archive-password");
    if (pwdModal && pwdModal.style.display !== "none") {
      closeArchivePasswordModal();
      return;
    }
    const powerModal = document.getElementById("power-modal");
    if (powerModal && powerModal.style.display !== "none") {
      closePowerModal();
      return;
    }
    const docModal = document.getElementById("doc-modal");
    if (docModal && docModal.style.display !== "none") {
      closeDocModal();
      return;
    }
    if (selectedFilePaths && selectedFilePaths.size > 0) {
      clearFileSelection();
    }
  }

  // Ctrl+A / Cmd+A dans la vue Fichiers
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
    const activeTab = document.querySelector(".tab-pane.active");
    const activeEl = document.activeElement;
    const isInput = activeEl && (activeEl.tagName === "INPUT" || activeEl.tagName === "TEXTAREA" || activeEl.isContentEditable);
    if (!isInput && activeTab && activeTab.id === "tab-files") {
      e.preventDefault();
      selectAllFiles();
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
    breadcrumb.innerHTML = `
      <span class="crumb-item active" style="color:var(--mauve); display:inline-flex; align-items:center; gap:6px;">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--mauve)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="3 6 5 6 21 6"></polyline>
          <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
        </svg>
        <span>Corbeille (Rétention 30 jours)</span>
      </span>
    `;
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
      const thumbUrl = buildAuthenticatedUrl("/api/files/image-view", { path: item.trash_path, thumb: "true" });
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
    if (fmt === "mp3" && destInput.value === (getUserHome() + "/videos")) {
      destInput.value = getUserHome() + "/musique";
    } else if (fmt === "mp4" && destInput.value === (getUserHome() + "/musique")) {
      destInput.value = getUserHome() + "/videos";
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
    if (destInput && (!destInput.value || destInput.value === (getUserHome() + "/videos") || destInput.value === (getUserHome() + "/musique"))) {
      destInput.value = currentYoutubeFormat === "mp3" ? (getUserHome() + "/musique") : (getUserHome() + "/videos");
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

  const dest_dir = destInput ? destInput.value.trim() : (currentYoutubeFormat === "mp3" ? (getUserHome() + "/musique") : (getUserHome() + "/videos"));
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
      const targetDir = job.output_dir || getUserHome();
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
    const displayDir = job.output_dir || job.output_file || getUserHome();
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

  const directPreviewUrl = buildAuthenticatedUrl("/api/documents/preview", { path });
  const directDownloadUrl = buildAuthenticatedUrl("/api/files/stream", { path });

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
let storeRenderLimit = 48;
let currentFirewallData = null;

function switchDockerSubTab(subTab, updateHash = true) {
  activeDockerSubTab = subTab;
  try {
    localStorage.setItem("noos_subtab_containers", subTab);
  } catch (e) {}
  if (updateHash && activeTab === "tab-containers") {
    updateUrlHash("tab-containers", subTab);
  }
  const btnContainers = document.getElementById("btn-subtab-containers");
  const btnStore = document.getElementById("btn-subtab-store");
  const btnImages = document.getElementById("btn-subtab-images");
  const paneContainers = document.getElementById("docker-pane-containers");
  const paneStore = document.getElementById("docker-pane-store");
  const paneImages = document.getElementById("docker-pane-images");

  if (btnContainers) btnContainers.classList.toggle("active", subTab === "containers");
  if (btnStore) btnStore.classList.toggle("active", subTab === "store");
  if (btnImages) btnImages.classList.toggle("active", subTab === "images");

  if (paneContainers) paneContainers.style.display = subTab === "containers" ? "block" : "none";
  if (paneStore) paneStore.style.display = subTab === "store" ? "block" : "none";
  if (paneImages) paneImages.style.display = subTab === "images" ? "block" : "none";

  if (subTab === "store" && !currentStoreCatalog) {
    loadDockerStore();
  } else if (subTab === "images") {
    loadDockerImages();
  }
}

function openDockerImagesCleanup() {
  switchTab("tab-containers");
  switchDockerSubTab("images");
}

async function refreshContainersAndStore(showFeedback = false) {
  try {
    const promises = [loadDockerContainers(), loadDockerStore()];
    if (activeDockerSubTab === "images") {
      promises.push(loadDockerImages());
    }
    await Promise.all(promises);
    if (showFeedback) {
      showToast("Conteneurs, boutique et images actualisés !", "success");
    }
  } catch (err) {
    console.error("Erreur actualisation conteneurs/store:", err);
  }
}

// --------------------------------------------------------------------------
// ÉTAT & GESTION DES CONTENEURS DOCKER (TRI, FILTRES, PAGINATION & MULTI-SÉLECTION)
// --------------------------------------------------------------------------
let allDockerContainers = [];
let dockerContainersStatusFilter = "all"; // "all" | "running" | "unhealthy" | "stopped"
let dockerContainersSort = "name_asc";    // "name_asc" | "name_desc" | "status_online" | "status_offline"
let dockerContainersSearch = "";
let dockerContainersPerPage = 25;
let dockerContainersCurrentPage = 1;
let selectedDockerContainers = new Set();

async function loadDockerContainers() {
  const container = document.getElementById("containers-container");
  const countBadge = document.getElementById("running-containers-count");
  if (!container) return;

  try {
    const res = await fetch("/api/docker/containers");
    const json = await res.json();
    if (!json.success || !json.data) {
      container.innerHTML = `<p style="color:var(--red); font-size:0.9rem; padding: 20px;">Erreur lors de la récupération des conteneurs.</p>`;
      return;
    }

    const rawContainers = Array.isArray(json.data) ? json.data : [];
    // Règle Noos : Masquer impérativement les conteneurs correspondant aux serveurs de jeux (noos-game* et legacy steveos-game*)
    allDockerContainers = rawContainers.filter(c => {
      const name = (c.name || "").replace(/^\//, "").toLowerCase();
      return !name.startsWith("noos-game") && !name.startsWith("steveos-game");
    });

    const runningCount = allDockerContainers.filter(c => c.is_running).length;
    if (countBadge) countBadge.textContent = runningCount;

    // Nettoyer la sélection des conteneurs qui n'existent plus
    const existingNames = new Set(allDockerContainers.map(c => (c.name || "").replace(/^\//, "")));
    for (const name of selectedDockerContainers) {
      if (!existingNames.has(name)) {
        selectedDockerContainers.delete(name);
      }
    }

    // Mettre à jour les compteurs des filtres pills
    updateContainersFilterCounts();

    // Rendre la vue avec filtres, tri et pagination
    renderDockerContainersView();

  } catch (err) {
    console.error("Erreur fetch /api/docker/containers:", err);
    container.innerHTML = `<p style="color:var(--red); font-size:0.9rem; padding:20px;">Erreur de communication avec le démon Docker.</p>`;
  }
}

function updateContainersFilterCounts() {
  const total = allDockerContainers.length;
  const running = allDockerContainers.filter(c => c.is_running).length;
  const unhealthy = allDockerContainers.filter(c => c.status && c.status.toLowerCase().includes("unhealthy")).length;
  const stopped = allDockerContainers.filter(c => !c.is_running).length;

  const countAll = document.getElementById("count-filter-all");
  const countRunning = document.getElementById("count-filter-running");
  const countUnhealthy = document.getElementById("count-filter-unhealthy");
  const countStopped = document.getElementById("count-filter-stopped");

  if (countAll) countAll.textContent = total;
  if (countRunning) countRunning.textContent = running;
  if (countUnhealthy) countUnhealthy.textContent = unhealthy;
  if (countStopped) countStopped.textContent = stopped;
}

function setContainersStatusFilter(filterKey) {
  dockerContainersStatusFilter = filterKey;
  dockerContainersCurrentPage = 1;

  document.querySelectorAll(".docker-status-filter-pills .docker-pill-btn").forEach(btn => {
    btn.classList.toggle("active", btn.id === `pill-filter-${filterKey}`);
  });

  renderDockerContainersView();
}

function onContainersFilterChange() {
  const input = document.getElementById("containers-search-input");
  const clearBtn = document.getElementById("containers-search-clear");
  dockerContainersSearch = (input?.value || "").trim().toLowerCase();
  dockerContainersCurrentPage = 1;

  if (clearBtn) {
    clearBtn.style.display = dockerContainersSearch ? "block" : "none";
  }

  renderDockerContainersView();
}

function clearContainersSearch() {
  const input = document.getElementById("containers-search-input");
  const clearBtn = document.getElementById("containers-search-clear");
  if (input) input.value = "";
  if (clearBtn) clearBtn.style.display = "none";
  dockerContainersSearch = "";
  dockerContainersCurrentPage = 1;
  renderDockerContainersView();
}

function onContainersSortChange() {
  const select = document.getElementById("containers-sort-select");
  if (select) dockerContainersSort = select.value;
  renderDockerContainersView();
}

function onContainersPerPageChange() {
  const select = document.getElementById("containers-per-page-select");
  if (select) {
    dockerContainersPerPage = parseInt(select.value, 10) || 25;
    dockerContainersCurrentPage = 1;
  }
  renderDockerContainersView();
}

function renderDockerContainersView() {
  const container = document.getElementById("containers-container");
  const paginationBar = document.getElementById("containers-pagination-bar");
  const paginationInfo = document.getElementById("containers-pagination-info");
  const paginationControls = document.getElementById("containers-pagination-controls");
  if (!container) return;

  if (allDockerContainers.length === 0) {
    container.innerHTML = `
      <div style="background: var(--surface0); border: 1px dashed var(--surface1); border-radius: 14px; padding: 36px 20px; text-align: center;">
        <div style="font-size: 2.4rem; margin-bottom: 8px;">🐳</div>
        <div style="font-weight: 700; color: var(--text); font-size: 1.05rem;">Aucun conteneur applicatif actif</div>
        <div style="font-size: 0.85rem; color: var(--subtext0); margin: 6px 0 18px 0;">Découvrez et déployez vos applications en 1-clic depuis le Store officiel STEvE_OS.</div>
        <button type="button" class="btn btn-primary btn-sm" onclick="switchDockerSubTab('store')">🛍️ Parcourir la Boutique d'Applications</button>
      </div>
    `;
    if (paginationBar) paginationBar.style.display = "none";
    updateBulkActionsBar([]);
    return;
  }

  // 1. Filtrage
  let filtered = allDockerContainers.filter(c => {
    // Filtre d'état
    if (dockerContainersStatusFilter === "running" && !c.is_running) return false;
    if (dockerContainersStatusFilter === "unhealthy" && (!c.status || !c.status.toLowerCase().includes("unhealthy"))) return false;
    if (dockerContainersStatusFilter === "stopped" && c.is_running) return false;

    // Filtre de recherche
    if (dockerContainersSearch) {
      const cleanName = (c.name || "").replace(/^\//, "").toLowerCase();
      const appName = (c.store_app_name || "").toLowerCase();
      const image = (c.image || "").toLowerCase();
      const ports = (c.ports || "").toLowerCase();
      const status = (c.status || "").toLowerCase();
      const match = cleanName.includes(dockerContainersSearch) ||
                    appName.includes(dockerContainersSearch) ||
                    image.includes(dockerContainersSearch) ||
                    ports.includes(dockerContainersSearch) ||
                    status.includes(dockerContainersSearch);
      if (!match) return false;
    }
    return true;
  });

  // 2. Tri
  filtered.sort((a, b) => {
    const nameA = (a.store_app_name || a.name || "").replace(/^\//, "");
    const nameB = (b.store_app_name || b.name || "").replace(/^\//, "");

    switch (dockerContainersSort) {
      case "name_asc":
        return nameA.localeCompare(nameB, undefined, { numeric: true, sensitivity: "base" });
      case "name_desc":
        return nameB.localeCompare(nameA, undefined, { numeric: true, sensitivity: "base" });
      case "status_online":
        return (b.is_running ? 1 : 0) - (a.is_running ? 1 : 0) || nameA.localeCompare(nameB);
      case "status_offline":
        return (a.is_running ? 1 : 0) - (b.is_running ? 1 : 0) || nameA.localeCompare(nameB);
      default:
        return 0;
    }
  });

  // 3. Pagination
  const total = filtered.length;
  const totalPages = Math.max(1, Math.ceil(total / dockerContainersPerPage));
  if (dockerContainersCurrentPage > totalPages) {
    dockerContainersCurrentPage = totalPages;
  }
  if (dockerContainersCurrentPage < 1) {
    dockerContainersCurrentPage = 1;
  }

  const startIndex = (dockerContainersCurrentPage - 1) * dockerContainersPerPage;
  const endIndex = Math.min(startIndex + dockerContainersPerPage, total);
  const pagedItems = filtered.slice(startIndex, endIndex);

  // 4. Rendu de la liste
  if (filtered.length === 0) {
    container.innerHTML = `
      <div style="background: var(--surface0); border: 1px dashed var(--surface1); border-radius: 12px; padding: 30px; text-align: center; color: var(--subtext0);">
        <div style="font-size: 2rem; margin-bottom: 6px;">🔍</div>
        <div style="font-weight: 600; color: var(--text);">Aucun conteneur ne correspond à vos critères</div>
        <p style="font-size: 0.85rem; margin: 4px 0 14px 0;">Modifiez votre recherche ou réinitialisez le filtre d'état.</p>
        <button type="button" class="btn btn-secondary btn-xs" onclick="clearContainersSearch(); setContainersStatusFilter('all');">Réinitialiser les filtres</button>
      </div>
    `;
    if (paginationBar) paginationBar.style.display = "none";
    updateBulkActionsBar([]);
    return;
  }

  const host = window.location.hostname;
  container.innerHTML = pagedItems.map(c => {
    const cleanName = (c.name || "").replace(/^\//, "");
    const shortId = (c.id || "").substring(0, 10);
    const isRunning = Boolean(c.is_running);
    const isUnhealthy = Boolean(c.status && c.status.toLowerCase().includes("unhealthy"));
    const isStore = Boolean(c.is_store_app);
    const appName = c.store_app_name || cleanName;
    const appIcon = c.store_icon || "";
    const storeAppId = (c.store_app_id || "").toLowerCase();
    const isSelected = selectedDockerContainers.has(cleanName);

    const openLink = c.web_port
      ? `<a href="http://${host}:${c.web_port}" target="_blank" class="btn-docker-open" title="Ouvrir l'application Web (Port ${c.web_port})">
           <span>🚀</span> Ouvrir :${c.web_port} ↗
         </a>`
      : "";

    const portsBadge = c.ports && !c.web_port
      ? `<span class="docker-line-ports" title="${escapeHtml(c.ports)}">🔌 ${escapeHtml(c.ports)}</span>`
      : "";

    const avatarHtml = (isStore && appIcon)
      ? `<img src="${escapeHtml(appIcon)}" alt="${escapeHtml(appName)}" class="docker-avatar-img" onerror="this.outerHTML='<div class=\\'docker-avatar-icon\\'>🐳</div>'">`
      : `<div class="docker-avatar-icon">🐳</div>`;

    const storeBadge = isStore
      ? `<span class="badge badge-primary" style="font-size:0.68rem; margin-left:6px; padding:2px 7px; vertical-align:middle; cursor:pointer;" onclick="openStoreAppModal('${escapeHtml(storeAppId)}')" title="Cliquer pour voir la fiche dans l'App Store">🛍️ ${escapeHtml(appName)}</span>`
      : "";

    const storeBtn = (isStore && storeAppId)
      ? `<button type="button" class="btn-docker-action" onclick="openStoreAppModal('${escapeHtml(storeAppId)}')" title="Voir la fiche dans l'App Store">
           <span>🛍️</span> Fiche Store
         </button>`
      : "";

    const stateBadgeText = isUnhealthy ? "⚠️ Dégradé (Unhealthy)" : (isRunning ? "🟢 En cours" : "🟡 Arrêté");
    const stateBadgeClass = isUnhealthy ? "state-unhealthy" : (isRunning ? "state-running" : "state-stopped");
    const dotClass = isUnhealthy ? "dot-unhealthy" : (isRunning ? "dot-running" : "dot-stopped");

    return `
      <div class="docker-container-row ${isRunning ? 'is-running' : 'is-stopped'} ${isUnhealthy ? 'is-unhealthy' : ''} ${isSelected ? 'is-selected' : ''}" id="docker-container-row-${escapeHtml(cleanName)}" data-store-app-id="${escapeHtml(storeAppId)}" data-container-id="${escapeHtml(c.id || '')}">
        <div class="docker-row-select">
          <input type="checkbox" class="docker-container-select-cb" value="${escapeHtml(cleanName)}" ${isSelected ? 'checked' : ''} onchange="toggleSelectContainer('${escapeHtml(cleanName)}', this.checked)">
        </div>

        <div class="docker-row-left">
          <span class="docker-status-dot ${dotClass}" title="${isUnhealthy ? 'État dégradé / Unhealthy' : (isRunning ? 'En cours d\'exécution' : 'Arrêté')}"></span>
          ${avatarHtml}
          <div class="docker-row-identity">
            <div class="docker-row-name" title="${escapeHtml(cleanName)}">
              ${escapeHtml(cleanName)}
              ${storeBadge}
            </div>
            <div class="docker-row-sub">
              <span class="docker-row-id font-mono">${escapeHtml(shortId)}</span>
              <span class="docker-badge-state ${stateBadgeClass}">
                ${stateBadgeText}
              </span>
            </div>
          </div>
        </div>

        <div class="docker-row-middle">
          <span class="docker-row-image font-mono" title="Image: ${escapeHtml(c.image)}">
            📦 ${escapeHtml(c.image)}
          </span>
          ${openLink}
          ${portsBadge}
          <span class="docker-row-uptime" title="${escapeHtml(c.status)}">${escapeHtml(c.status)}</span>
        </div>

        <div class="docker-row-actions">
          ${isRunning ? `
            <button type="button" class="btn-docker-action btn-docker-restart" onclick="dockerContainerAction('${escapeHtml(cleanName)}', 'restart')" title="Redémarrer le conteneur">
              <span>🔄</span> Redémarrer
            </button>
            <button type="button" class="btn-docker-action btn-docker-stop" onclick="dockerContainerAction('${escapeHtml(cleanName)}', 'stop')" title="Arrêter le conteneur">
              <span>⏹</span> Arrêter
            </button>
          ` : `
            <button type="button" class="btn-docker-action btn-docker-start" onclick="dockerContainerAction('${escapeHtml(cleanName)}', 'start')" title="Démarrer le conteneur">
              <span>▶</span> Démarrer
            </button>
          `}
          <button type="button" class="btn-docker-action" onclick="openDockerConfigModalForContainer('${escapeHtml(cleanName)}')" title="Modifier les variables d'environnement">
            <span>⚙️</span> Variables
          </button>
          ${storeBtn}
          <button type="button" class="btn-docker-action" onclick="openDockerLogsModal('${escapeHtml(cleanName)}')" title="Consulter les journaux Docker">
            <span>📜</span> Logs
          </button>
          <button type="button" class="btn-docker-action btn-docker-delete" onclick="openDeleteDockerModal('${escapeHtml(cleanName)}', '${escapeHtml(c.image || '')}', '${escapeHtml(storeAppId)}', '${escapeHtml(appName)}', '${escapeHtml(appIcon)}')" title="Supprimer le conteneur">
            <span>🗑️</span> Supprimer
          </button>
        </div>
      </div>
    `;
  }).join("");

  // 5. Rendu de la pagination
  if (paginationBar) {
    if (total > dockerContainersPerPage) {
      paginationBar.style.display = "flex";
      if (paginationInfo) {
        paginationInfo.textContent = `Affichage de ${startIndex + 1} à ${endIndex} sur ${total} conteneur${total > 1 ? "s" : ""}`;
      }
      if (paginationControls) {
        let controlsHtml = `
          <button type="button" class="docker-page-btn" onclick="changeContainersPage(${dockerContainersCurrentPage - 1})" ${dockerContainersCurrentPage === 1 ? "disabled" : ""}>
            ‹ Précédent
          </button>
        `;

        for (let p = 1; p <= totalPages; p++) {
          if (p === 1 || p === totalPages || (p >= dockerContainersCurrentPage - 1 && p <= dockerContainersCurrentPage + 1)) {
            controlsHtml += `
              <button type="button" class="docker-page-btn ${p === dockerContainersCurrentPage ? 'active' : ''}" onclick="changeContainersPage(${p})">
                ${p}
              </button>
            `;
          } else if (p === dockerContainersCurrentPage - 2 || p === dockerContainersCurrentPage + 2) {
            controlsHtml += `<span style="color:var(--subtext0); padding: 0 3px;">…</span>`;
          }
        }

        controlsHtml += `
          <button type="button" class="docker-page-btn" onclick="changeContainersPage(${dockerContainersCurrentPage + 1})" ${dockerContainersCurrentPage === totalPages ? "disabled" : ""}>
            Suivant ›
          </button>
        `;
        paginationControls.innerHTML = controlsHtml;
      }
    } else {
      paginationBar.style.display = "none";
    }
  }

  // 6. Mise à jour de la barre d'actions groupées
  updateBulkActionsBar(pagedItems);
}

function changeContainersPage(page) {
  dockerContainersCurrentPage = page;
  renderDockerContainersView();
  const pane = document.getElementById("docker-pane-containers");
  if (pane) pane.scrollIntoView({ behavior: "smooth", block: "start" });
}

function toggleSelectContainer(cleanName, isChecked) {
  if (isChecked) {
    selectedDockerContainers.add(cleanName);
  } else {
    selectedDockerContainers.delete(cleanName);
  }

  const row = document.getElementById(`docker-container-row-${cleanName}`);
  if (row) {
    row.classList.toggle("is-selected", isChecked);
  }

  const pagedCheckboxes = document.querySelectorAll(".docker-container-select-cb");
  const visibleNames = Array.from(pagedCheckboxes).map(cb => cb.value);
  updateBulkActionsBarVisibleCheck(visibleNames);
}

function toggleSelectAllContainers(isChecked) {
  const pagedCheckboxes = document.querySelectorAll(".docker-container-select-cb");
  pagedCheckboxes.forEach(cb => {
    cb.checked = isChecked;
    const name = cb.value;
    if (isChecked) {
      selectedDockerContainers.add(name);
    } else {
      selectedDockerContainers.delete(name);
    }
    const row = document.getElementById(`docker-container-row-${name}`);
    if (row) row.classList.toggle("is-selected", isChecked);
  });

  const visibleNames = Array.from(pagedCheckboxes).map(cb => cb.value);
  updateBulkActionsBarVisibleCheck(visibleNames);
}

function clearSelectedContainers() {
  selectedDockerContainers.clear();
  document.querySelectorAll(".docker-container-select-cb").forEach(cb => {
    cb.checked = false;
  });
  document.querySelectorAll(".docker-container-row.is-selected").forEach(r => {
    r.classList.remove("is-selected");
  });
  updateBulkActionsBar([]);
}

function updateBulkActionsBarVisibleCheck(visibleNames) {
  const bar = document.getElementById("docker-bulk-actions-bar");
  const badge = document.getElementById("docker-bulk-count-badge");
  const selectAllCb = document.getElementById("docker-select-all-cb");
  const btnCounts = document.querySelectorAll(".bulk-btn-count");

  const count = selectedDockerContainers.size;
  if (count > 0) {
    if (bar) bar.style.display = "flex";
    if (badge) badge.textContent = `${count} sélectionné${count > 1 ? "s" : ""}`;
    btnCounts.forEach(el => { el.textContent = count; });

    if (selectAllCb && visibleNames.length > 0) {
      const allVisibleSelected = visibleNames.every(name => selectedDockerContainers.has(name));
      const someVisibleSelected = visibleNames.some(name => selectedDockerContainers.has(name));
      selectAllCb.checked = allVisibleSelected;
      selectAllCb.indeterminate = !allVisibleSelected && someVisibleSelected;
    }
  } else {
    if (bar) bar.style.display = "none";
    if (selectAllCb) {
      selectAllCb.checked = false;
      selectAllCb.indeterminate = false;
    }
  }
}

function updateBulkActionsBar(pagedItems) {
  const visibleNames = pagedItems.map(c => (c.name || "").replace(/^\//, ""));
  updateBulkActionsBarVisibleCheck(visibleNames);
}

// --------------------------------------------------------------------------
// ACTIONS GROUPÉES SUR LES CONTENEURS DOCKER SÉLECTIONNÉS
// --------------------------------------------------------------------------
async function bulkRestartContainers() {
  const names = Array.from(selectedDockerContainers);
  if (names.length === 0) return;
  if (!confirm(`Confirmez-vous le redémarrage des ${names.length} conteneur(s) sélectionné(s) ?`)) return;

  showToast(`Redémarrage de ${names.length} conteneur(s) en cours...`, "info");
  let successes = 0;
  let failures = 0;

  await Promise.all(names.map(async (name) => {
    try {
      const res = await fetch(`/api/docker/containers/${encodeURIComponent(name)}/action`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: "restart" })
      });
      const json = await res.json();
      if (json.success) successes++;
      else failures++;
    } catch {
      failures++;
    }
  }));

  if (failures === 0) {
    showToast(`${successes} conteneur(s) redémarré(s) avec succès !`, "success");
  } else {
    showToast(`Redémarrage groupé : ${successes} réussi(s), ${failures} échec(s)`, "warning");
  }
  clearSelectedContainers();
  await loadDockerContainers();
}

async function bulkStopContainers() {
  const names = Array.from(selectedDockerContainers);
  if (names.length === 0) return;
  if (!confirm(`Confirmez-vous l'arrêt des ${names.length} conteneur(s) sélectionné(s) ?`)) return;

  showToast(`Arrêt de ${names.length} conteneur(s) en cours...`, "info");
  let successes = 0;
  let failures = 0;

  await Promise.all(names.map(async (name) => {
    try {
      const res = await fetch(`/api/docker/containers/${encodeURIComponent(name)}/action`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: "stop" })
      });
      const json = await res.json();
      if (json.success) successes++;
      else failures++;
    } catch {
      failures++;
    }
  }));

  if (failures === 0) {
    showToast(`${successes} conteneur(s) arrêté(s) avec succès !`, "success");
  } else {
    showToast(`Arrêt groupé : ${successes} réussi(s), ${failures} échec(s)`, "warning");
  }
  clearSelectedContainers();
  await loadDockerContainers();
}

async function bulkStartContainers() {
  const names = Array.from(selectedDockerContainers);
  if (names.length === 0) return;
  if (!confirm(`Confirmez-vous le démarrage des ${names.length} conteneur(s) sélectionné(s) ?`)) return;

  showToast(`Démarrage de ${names.length} conteneur(s) en cours...`, "info");
  let successes = 0;
  let failures = 0;

  await Promise.all(names.map(async (name) => {
    try {
      const res = await fetch(`/api/docker/containers/${encodeURIComponent(name)}/action`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: "start" })
      });
      const json = await res.json();
      if (json.success) successes++;
      else failures++;
    } catch {
      failures++;
    }
  }));

  if (failures === 0) {
    showToast(`${successes} conteneur(s) démarré(s) avec succès !`, "success");
  } else {
    showToast(`Démarrage groupé : ${successes} réussi(s), ${failures} échec(s)`, "warning");
  }
  clearSelectedContainers();
  await loadDockerContainers();
}

function openBulkDeleteContainersModal() {
  const names = Array.from(selectedDockerContainers);
  if (names.length === 0) return;

  const modal = document.getElementById("modal-bulk-delete-docker");
  const countEl = document.getElementById("bulk-delete-count");
  const previewEl = document.getElementById("bulk-delete-list-preview");
  const checkImg = document.getElementById("bulk-delete-image-check");
  const checkData = document.getElementById("bulk-delete-data-check");

  if (countEl) countEl.textContent = names.length;
  if (checkImg) checkImg.checked = false;
  if (checkData) checkData.checked = false;

  if (previewEl) {
    previewEl.innerHTML = names.map(n => {
      const containerObj = allDockerContainers.find(c => (c.name || "").replace(/^\//, "") === n);
      const appName = containerObj?.store_app_name || n;
      const isStore = containerObj?.is_store_app;
      return `<div style="display:flex; justify-content:space-between; align-items:center; padding: 3px 0; border-bottom: 1px solid rgba(255,255,255,0.05);">
        <span>🐳 <strong>${escapeHtml(n)}</strong></span>
        ${isStore ? `<span class="badge badge-primary" style="font-size:0.7rem;">🛍️ ${escapeHtml(appName)}</span>` : ''}
      </div>`;
    }).join("");
  }

  if (modal) modal.style.display = "flex";
}

function closeBulkDeleteDockerModal() {
  const modal = document.getElementById("modal-bulk-delete-docker");
  if (modal) modal.style.display = "none";
}

async function confirmBulkDeleteDockerContainers() {
  const names = Array.from(selectedDockerContainers);
  if (names.length === 0) return;

  const checkImg = document.getElementById("bulk-delete-image-check");
  const deleteImage = checkImg ? Boolean(checkImg.checked) : false;
  const checkData = document.getElementById("bulk-delete-data-check");
  const deleteData = checkData ? Boolean(checkData.checked) : false;
  const btn = document.getElementById("btn-confirm-bulk-delete-docker");

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span>⏳</span> Suppression en cours...`;
  }

  showToast(`Suppression de ${names.length} conteneur(s)...`, "info");
  let successes = 0;
  let failures = 0;

  await Promise.all(names.map(async (name) => {
    try {
      const containerObj = allDockerContainers.find(c => (c.name || "").replace(/^\//, "") === name);
      const storeAppId = containerObj?.store_app_id;

      const res = await fetch(`/api/docker/containers/${encodeURIComponent(name)}?delete_image=${deleteImage}&delete_data=${deleteData}`, {
        method: "DELETE"
      });
      const json = await res.json();
      if (json.success) {
        successes++;
        if (storeAppId) dismissDockerDeployToast(storeAppId);
        dismissDockerDeployToast(name);
      } else {
        failures++;
      }
    } catch {
      failures++;
    }
  }));

  if (btn) {
    btn.disabled = false;
    btn.innerHTML = `<span>🗑️</span> Confirmer la suppression groupée`;
  }

  closeBulkDeleteDockerModal();
  clearSelectedContainers();

  if (failures === 0) {
    showToast(`${successes} conteneur(s) supprimé(s) avec succès !`, "success");
  } else {
    showToast(`Suppression groupée terminée : ${successes} réussi(s), ${failures} échec(s)`, "warning");
  }

  await refreshContainersAndStore();
  if (typeof loadDockerImages === "function") {
    loadDockerImages();
  }
}

// --------------------------------------------------------------------------
// MODALE SUPPRESSION DOCKER CONTENEUR (AVEC LIEN STORE & OPTION PURGE)
// --------------------------------------------------------------------------
let pendingDeleteDockerName = null;
let pendingDeleteDockerImage = null;
let pendingDeleteDockerStoreAppId = null;
let pendingDeleteDockerStoreAppName = null;

function openDeleteDockerModal(name, image, storeAppId, storeAppName, storeAppIcon) {
  pendingDeleteDockerName = name;
  pendingDeleteDockerImage = image || "";
  pendingDeleteDockerStoreAppId = storeAppId || null;
  pendingDeleteDockerStoreAppName = storeAppName || null;

  const modal = document.getElementById("modal-delete-docker");
  const nameEl = document.getElementById("delete-docker-name-display");
  const imgEl = document.getElementById("delete-docker-image-preview");
  const checkEl = document.getElementById("delete-docker-image-check");

  const storeBanner = document.getElementById("delete-docker-store-banner");
  const storeIconEl = document.getElementById("delete-docker-store-icon");
  const storeAppNameEl = document.getElementById("delete-docker-store-appname");

  const dataLabel = document.getElementById("delete-docker-data-label");
  const dataCheck = document.getElementById("delete-docker-data-check");
  const dataDirPreview = document.getElementById("delete-docker-data-dir-preview");

  if (nameEl) nameEl.textContent = name;
  if (imgEl) imgEl.textContent = image ? `Image : ${image}` : "Aucune image identifiée";
  if (checkEl) checkEl.checked = false;

  const targetAppId = (storeAppId || name).toLowerCase();
  if (storeBanner) {
    if (storeAppId) {
      storeBanner.style.display = "flex";
      if (storeIconEl) storeIconEl.src = storeAppIcon || "/favicon.ico";
      if (storeAppNameEl) storeAppNameEl.textContent = storeAppName || storeAppId;
    } else {
      storeBanner.style.display = "none";
    }
  }

  if (dataLabel) {
    dataLabel.style.display = "flex";
    if (dataCheck) dataCheck.checked = false;
    if (dataDirPreview) dataDirPreview.textContent = `~/docker/${targetAppId}`;
  }

  if (modal) modal.style.display = "flex";
}

function closeDeleteDockerModal() {
  pendingDeleteDockerName = null;
  pendingDeleteDockerImage = null;
  pendingDeleteDockerStoreAppId = null;
  pendingDeleteDockerStoreAppName = null;
  const modal = document.getElementById("modal-delete-docker");
  if (modal) modal.style.display = "none";
}

async function confirmDeleteDockerContainer() {
  if (!pendingDeleteDockerName) return;
  const name = pendingDeleteDockerName;
  const storeAppId = pendingDeleteDockerStoreAppId;
  const checkEl = document.getElementById("delete-docker-image-check");
  const deleteImage = checkEl ? Boolean(checkEl.checked) : false;
  const dataCheck = document.getElementById("delete-docker-data-check");
  const deleteData = dataCheck ? Boolean(dataCheck.checked) : false;
  const btn = document.getElementById("btn-confirm-delete-docker");

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span>⏳</span> Suppression en cours...`;
  }

  try {
    const res = await fetch(`/api/docker/containers/${encodeURIComponent(name)}?delete_image=${deleteImage}&delete_data=${deleteData}`, {
      method: "DELETE"
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || `Conteneur '${name}' supprimé avec succès.`, "success");
      closeDeleteDockerModal();

      // Nettoyer toute popup de déploiement en cours ou résiduelle
      if (storeAppId) {
        dismissDockerDeployToast(storeAppId);
      }
      dismissDockerDeployToast(name);

      // Synchronisation croisée immédiate Conteneurs + Store
      await refreshContainersAndStore();
      if (typeof loadDockerImages === "function") {
        loadDockerImages();
      }
    } else {
      showToast(`Erreur : ${json.message || "Échec de la suppression"}`, "error");
    }
  } catch (err) {
    showToast(`Erreur réseau : ${err.message}`, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span>🗑️</span> Confirmer la suppression`;
    }
  }
}

function scrollToDockerContainer(appId, containerId) {
  switchTab("tab-containers");
  switchDockerSubTab("containers");

  setTimeout(() => {
    const cleanAppId = (appId || "").toLowerCase();
    const cleanCid = (containerId || "").toLowerCase();

    let targetRow = null;
    if (cleanAppId) {
      targetRow = document.querySelector(`.docker-container-row[data-store-app-id="${cleanAppId}"]`)
        || document.getElementById(`docker-container-row-${cleanAppId}`);
    }
    if (!targetRow && cleanCid) {
      targetRow = document.querySelector(`.docker-container-row[data-container-id="${cleanCid}"]`);
    }
    if (!targetRow && cleanAppId) {
      const allRows = document.querySelectorAll(".docker-container-row");
      for (const r of allRows) {
        if ((r.textContent || "").toLowerCase().includes(cleanAppId)) {
          targetRow = r;
          break;
        }
      }
    }

    if (targetRow) {
      targetRow.scrollIntoView({ behavior: "smooth", block: "center" });
      targetRow.classList.remove("row-highlight-pulse");
      void targetRow.offsetWidth; // Forcer le reflow CSS
      targetRow.classList.add("row-highlight-pulse");
      setTimeout(() => targetRow.classList.remove("row-highlight-pulse"), 2600);
    } else {
      showToast("Conteneur non trouvé dans la liste des conteneurs actifs.", "info");
    }
  }, 220);
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

    // Rendu dynamique des pastilles de catégories avec compteurs
    if (catContainer && currentStoreCatalog.categories) {
      const catCounts = {};
      currentStoreCatalog.apps.forEach(a => {
        const cat = a.category || "Outils & Utilitaires";
        catCounts[cat] = (catCounts[cat] || 0) + 1;
      });

      catContainer.innerHTML = currentStoreCatalog.categories.map(cat => {
        const count = (cat === "Tous") ? currentStoreCatalog.apps.length : (catCounts[cat] || 0);
        return `
          <button type="button" class="store-cat-pill ${cat === activeStoreCategory ? 'active' : ''}" onclick="selectStoreCategory('${escapeHtml(cat)}')">
            <span>${escapeHtml(cat)}</span>
            <span class="store-cat-count">${count}</span>
          </button>
        `;
      }).join("");
    }

    filterStoreApps(true);
    if (typeof syncActiveDockerDeploymentsWithBackend === "function") {
      syncActiveDockerDeploymentsWithBackend();
    }
  } catch (err) {
    console.error("Erreur fetch /api/docker/store:", err);
    grid.innerHTML = `<p style="color:var(--red); font-size:0.9rem;">Erreur de chargement du catalogue store.</p>`;
  }
}

function selectStoreCategory(cat) {
  activeStoreCategory = cat;
  document.querySelectorAll(".store-cat-pill").forEach(pill => {
    const span = pill.querySelector("span");
    const pillCat = span ? span.textContent.trim() : pill.textContent.trim();
    pill.classList.toggle("active", pillCat === cat);
  });
  filterStoreApps(true);
}

function clearStoreSearch() {
  const input = document.getElementById("store-search-input");
  const clearBtn = document.getElementById("store-search-clear");
  if (input) input.value = "";
  if (clearBtn) clearBtn.style.display = "none";
  filterStoreApps(true);
}

function loadMoreStoreApps() {
  storeRenderLimit += 48;
  filterStoreApps(false);
}

function filterStoreApps(resetLimit = false) {
  const grid = document.getElementById("store-apps-grid");
  const searchInput = document.getElementById("store-search-input");
  const clearBtn = document.getElementById("store-search-clear");
  const sortSelect = document.getElementById("store-sort-select");
  const statusSelect = document.getElementById("store-status-select");
  const countIndicator = document.getElementById("store-count-val");
  const paginationContainer = document.getElementById("store-pagination-container");
  const remainingCountEl = document.getElementById("store-remaining-count");

  if (!grid || !currentStoreCatalog) return;

  if (resetLimit) {
    storeRenderLimit = 48;
  }

  const query = (searchInput ? searchInput.value.trim().toLowerCase() : "");
  if (clearBtn) {
    clearBtn.style.display = query ? "block" : "none";
  }

  const sortVal = sortSelect ? sortSelect.value : "recommended";
  const statusVal = statusSelect ? statusSelect.value : "all";

  // 1. Filtrage dynamique
  let filtered = currentStoreCatalog.apps.filter(app => {
    const matchCat = (activeStoreCategory === "Tous" || app.category === activeStoreCategory);

    let matchQuery = true;
    if (query) {
      const q = query;
      const portMatch = app.default_port ? String(app.default_port).includes(q) : false;
      matchQuery = app.name.toLowerCase().includes(q) ||
                   (app.tagline && app.tagline.toLowerCase().includes(q)) ||
                   (app.description && app.description.toLowerCase().includes(q)) ||
                   (app.category && app.category.toLowerCase().includes(q)) ||
                   (app.id && app.id.toLowerCase().includes(q)) ||
                   portMatch;
    }

    let matchStatus = true;
    if (statusVal === "installed") {
      matchStatus = !!app.is_installed;
    } else if (statusVal === "available") {
      matchStatus = !app.is_installed;
    }

    return matchCat && matchQuery && matchStatus;
  });

  // 2. Tri
  if (sortVal === "name_asc") {
    filtered.sort((a, b) => a.name.localeCompare(b.name));
  } else if (sortVal === "name_desc") {
    filtered.sort((a, b) => b.name.localeCompare(a.name));
  } else if (sortVal === "port") {
    filtered.sort((a, b) => (a.default_port || 99999) - (b.default_port || 99999));
  } else {
    // Par défaut : Recommandés en premier, puis alphabétique
    filtered.sort((a, b) => {
      if (a.recommended && !b.recommended) return -1;
      if (!a.recommended && b.recommended) return 1;
      return a.name.localeCompare(b.name);
    });
  }

  // 3. Mise à jour de l'indicateur de volume
  if (countIndicator) {
    countIndicator.textContent = filtered.length;
  }

  // 4. État vide
  if (filtered.length === 0) {
    grid.innerHTML = `
      <div style="grid-column: 1 / -1; text-align: center; padding: 48px 20px; color: var(--subtext0);">
        <div style="font-size: 2.5rem; margin-bottom: 12px;">🔍</div>
        <h4 style="color: var(--text); margin-bottom: 6px;">Aucune application trouvée</h4>
        <p style="font-size: 0.88rem;">Aucune application ne correspond à vos filtres ou terme de recherche.</p>
        <button type="button" class="btn btn-secondary btn-sm" onclick="clearStoreSearch(); selectStoreCategory('Tous');" style="margin-top: 14px;">
          Réinitialiser la recherche
        </button>
      </div>
    `;
    if (paginationContainer) paginationContainer.style.display = "none";
    return;
  }

  // 5. Pagination / Découpage
  const visibleApps = filtered.slice(0, storeRenderLimit);
  const remaining = filtered.length - visibleApps.length;

  if (paginationContainer && remainingCountEl) {
    if (remaining > 0) {
      paginationContainer.style.display = "block";
      remainingCountEl.textContent = remaining;
    } else {
      paginationContainer.style.display = "none";
    }
  }

  // 6. Rendu des cartes d'applications
  const host = window.location.hostname;
  grid.innerHTML = visibleApps.map(app => {
    const isInstalled = app.is_installed;
    const isRunning = app.is_running;
    const cleanId = (app.id || "").toLowerCase();
    const isDeploying = activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "active";
    const isQueued = dockerDeployQueue.some(q => q.appId.toLowerCase() === cleanId);

    const openLink = (isInstalled && isRunning && app.default_port && !isDeploying)
      ? `<a href="http://${host}:${app.default_port}" target="_blank" class="store-open-link"><span>🚀</span> Ouvrir (Port ${app.default_port}) ↗</a>`
      : "";

    let statusPillHtml = "";
    if (isDeploying) {
      statusPillHtml = `<div class="store-status-pill status-warning" style="background:rgba(250, 179, 135, 0.2); color:var(--peach); border-color:rgba(250, 179, 135, 0.4);">⏳ Déploiement (${activeDockerDeployments[cleanId].progressPercent || 20}%)</div>`;
    } else if (isQueued) {
      statusPillHtml = `<div class="store-status-pill status-secondary" style="background:rgba(180, 190, 254, 0.2); color:var(--blue); border-color:rgba(180, 190, 254, 0.4);">🕒 En file d'attente</div>`;
    } else {
      statusPillHtml = `<div class="store-status-pill ${isInstalled ? (isRunning ? 'status-active' : 'status-stopped') : 'status-available'}">${isInstalled ? (isRunning ? '🟢 Active' : '🟡 Arrêtée') : '⚪ Non installée'}</div>`;
    }

    let actionsRowHtml = "";
    if (isDeploying) {
      actionsRowHtml = `
        <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openStoreAppModal('${escapeHtml(app.id)}')">
          <span>ℹ️</span> Détails
        </button>
        <button type="button" class="btn btn-warning btn-xs store-btn-action" onclick="openDockerConfigModal('${escapeHtml(app.id)}')">
          <span>⏳</span> En cours
        </button>
      `;
    } else if (isQueued) {
      actionsRowHtml = `
        <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openStoreAppModal('${escapeHtml(app.id)}')">
          <span>ℹ️</span> Détails
        </button>
        <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openDockerConfigModal('${escapeHtml(app.id)}')">
          <span>🕒</span> En attente
        </button>
      `;
    } else if (isInstalled) {
      actionsRowHtml = `
        <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="scrollToDockerContainer('${escapeHtml(app.id)}', '${escapeHtml(app.container_id || '')}')" title="Voir dans Mes Conteneurs">
          <span>🐳</span> Conteneur
        </button>
        <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openStoreAppModal('${escapeHtml(app.id)}')">
          <span>ℹ️</span> Détails
        </button>
        <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openDockerConfigModal('${escapeHtml(app.id)}')">
          <span>⚙️</span> Variables
        </button>
        <button type="button" class="btn btn-danger btn-xs store-btn-action" onclick="uninstallStoreApp('${escapeHtml(app.id)}', '${escapeHtml(app.name)}')">
          <span>🗑️</span> Désinstaller
        </button>
      `;
    } else {
      actionsRowHtml = `
        <button type="button" class="btn btn-secondary btn-xs store-btn-action" onclick="openStoreAppModal('${escapeHtml(app.id)}')">
          <span>ℹ️</span> Détails
        </button>
        <button type="button" class="btn btn-success btn-xs store-btn-action store-btn-install" onclick="openDockerConfigModal('${escapeHtml(app.id)}')">
          <span>📥</span> Installer
        </button>
      `;
    }

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
          <span>Données : <code style="color:var(--mauve); font-size:0.75rem;">${getUserHome()}/docker/${escapeHtml(app.id)}</code></span>
        </div>

        <div class="store-app-bottom">
          <div class="store-bottom-status-row">
            ${statusPillHtml}
            ${openLink}
          </div>

          <div class="store-bottom-actions-row">
            ${actionsRowHtml}
          </div>
        </div>
      </div>
    `;
  }).join("");
}

async function checkModalFirewallStatus(justOpened = false) {
  const portInput = document.getElementById("config-app-port");
  const container = document.getElementById("modal-fw-alert-container");
  const alertBox = document.getElementById("modal-fw-alert");
  const icon = document.getElementById("fw-shield-icon");
  const title = document.getElementById("fw-alert-title");
  const desc = document.getElementById("fw-alert-desc");
  const btn = document.getElementById("btn-quick-open-fw");

  if (!container || !portInput) return;

  const portVal = portInput.value.trim();
  const port = parseInt(portVal, 10);

  if (!portVal || isNaN(port) || port < 1 || port > 65535) {
    container.style.display = "none";
    return;
  }

  container.style.display = "block";

  try {
    if (!currentFirewallData || justOpened) {
      const res = await fetch("/api/firewall");
      const json = await res.json();
      if (json.success && json.data) {
        currentFirewallData = json.data;
      }
    }

    let isOpen = false;
    if (currentFirewallData) {
      if (!currentFirewallData.is_enabled) {
        isOpen = true;
      } else {
        // 1. Contrôle des règles unifiées (système ET personnalisées)
        const rules = currentFirewallData.rules || [];
        isOpen = rules.some(r => r.port === port && r.enabled !== false && (r.protocol === "TCP" || r.protocol === "BOTH"));

        // 2. Contrôle de repli sur les ports TCP système
        if (!isOpen) {
          const tcpPorts = currentFirewallData.tcp_ports || [];
          isOpen = tcpPorts.some(p => p.port === port);
        }
      }
    }

    if (isOpen) {
      if (alertBox) {
        alertBox.className = "modal-fw-alert fw-open" + (justOpened ? " fw-just-verified" : "");
      }
      if (icon) icon.textContent = justOpened ? "✅" : "🛡️";

      if (justOpened) {
        if (title) title.innerHTML = `<span style="color:var(--green); font-weight:700;">✅ Port ${port} (TCP) ouvert et vérifié avec succès !</span>`;
        if (desc) desc.textContent = `Le pare-feu STEvE_OS autorise désormais le trafic sur le port ${port}. L'application sera accessible immédiatement sur votre réseau local.`;
      } else {
        if (title) title.innerHTML = `<span style="color:var(--green); font-weight:700;">Pare-feu Noos : Port ${port} (TCP) Ouvert</span>`;
        if (desc) desc.textContent = `Ce port est déjà autorisé dans le pare-feu. Vos appareils du réseau local pourront y accéder sans blocage.`;
      }

      if (btn) {
        btn.style.display = "inline-flex";
        btn.disabled = true;
        btn.className = "btn btn-success btn-xs fw-quick-btn";
        btn.innerHTML = `<span>✓</span> Port Ouvert & Vérifié`;
        btn.style.cursor = "default";
      }
    } else {
      if (alertBox) alertBox.className = "modal-fw-alert fw-closed";
      if (icon) icon.textContent = "⚠️";
      if (title) title.innerHTML = `<span>Pare-feu Noos : Port ${port} (TCP) Non Ouvert</span>`;
      if (desc) desc.textContent = `Le pare-feu bloque actuellement ce port. Cliquez ci-contre pour l'autoriser immédiatement sur le réseau local.`;
      if (btn) {
        btn.style.display = "inline-flex";
        btn.disabled = false;
        btn.className = "btn btn-warning btn-xs fw-quick-btn";
        btn.innerHTML = `<span>🛡️</span> Ouvrir le port ${port} en 1 clic`;
        btn.style.cursor = "pointer";
      }
    }
  } catch (err) {
    console.warn("Erreur checkModalFirewallStatus:", err);
  }
}

async function quickOpenModalFirewallPort() {
  const portInput = document.getElementById("config-app-port");
  const appIdInput = document.getElementById("config-app-id");
  const btn = document.getElementById("btn-quick-open-fw");
  const title = document.getElementById("fw-alert-title");
  const desc = document.getElementById("fw-alert-desc");
  const icon = document.getElementById("fw-shield-icon");

  if (!portInput) return;
  const port = parseInt(portInput.value.trim(), 10);
  if (isNaN(port) || port < 1 || port > 65535) {
    showToast("Numéro de port invalide pour le pare-feu", "warning");
    return;
  }

  const appId = appIdInput ? appIdInput.value.trim() : "App";
  const app = currentStoreCatalog && currentStoreCatalog.apps ? currentStoreCatalog.apps.find(a => a.id === appId) : null;
  const label = app ? app.name : appId;

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span>⏳</span> Vérification & Ouverture...`;
  }
  if (title) {
    title.innerHTML = `<span>⏳ Ouverture du port ${port} en cours...</span>`;
  }
  if (desc) {
    desc.textContent = `Application de la règle pare-feu et vérification de la bonne ouverture...`;
  }

  try {
    const payload = {
      port: port,
      protocol: "TCP",
      label: `${label} (Docker)`,
      category: "Conteneurs"
    };

    const res = await fetch("/api/firewall/rules", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    const json = await res.json();
    const isAlreadyOpen = !json.success && json.message && json.message.includes("existe déjà");

    if (json.success || isAlreadyOpen) {
      showToast(`🛡️ Port ${port} (TCP) autorisé et vérifié dans le pare-feu !`, "success");
      currentFirewallData = null; // Invalider le cache pour forcer une ré-interrogation fraîche
      await new Promise(r => setTimeout(r, 300));
      await checkModalFirewallStatus(true); // Passer l'alerte en vert avec confirmation
      loadFirewall(); // Actualiser l'onglet Pare-feu en tâche de fond
    } else {
      showToast(`Erreur ouverture pare-feu : ${json.message || "Échec"}`, "error");
      if (btn) {
        btn.disabled = false;
        btn.className = "btn btn-warning btn-xs fw-quick-btn";
        btn.innerHTML = `<span>🛡️</span> Réessayer d'ouvrir le port ${port}`;
      }
      await checkModalFirewallStatus(false);
    }
  } catch (err) {
    showToast(`Erreur réseau lors de l'ouverture du port : ${err}`, "error");
    if (btn) {
      btn.disabled = false;
      btn.className = "btn btn-warning btn-xs fw-quick-btn";
      btn.innerHTML = `<span>🛡️</span> Réessayer d'ouvrir le port ${port}`;
    }
    await checkModalFirewallStatus(false);
  }
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
  if (dataPathEl) dataPathEl.textContent = `${getUserHome()}/docker/${app.id}`;
  if (nixPathEl) nixPathEl.textContent = `/etc/nixos/docker/${app.id}.nix`;
  if (websiteEl) {
    websiteEl.href = app.website || "#";
    websiteEl.style.display = app.website ? "inline-flex" : "none";
  }

  if (actionsEl) {
    const isInstalled = app.is_installed;
    const cleanId = (app.id || "").toLowerCase();
    const isDeploying = activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "active";
    const isQueued = dockerDeployQueue.some(q => q.appId.toLowerCase() === cleanId);

    if (isDeploying) {
      actionsEl.innerHTML = `
        <button type="button" class="btn btn-warning btn-sm" onclick="openDockerConfigModal('${app.id}'); closeStoreAppModal();">
          <span>⏳</span> Déploiement en cours (${activeDockerDeployments[cleanId].progressPercent || 20}%)...
        </button>
      `;
    } else if (isQueued) {
      actionsEl.innerHTML = `
        <button type="button" class="btn btn-secondary btn-sm" onclick="openDockerConfigModal('${app.id}'); closeStoreAppModal();">
          <span>🕒</span> En file d'attente...
        </button>
      `;
    } else if (isInstalled) {
      actionsEl.innerHTML = `
        <button type="button" class="btn btn-secondary btn-sm" onclick="scrollToDockerContainer('${app.id}', '${escapeHtml(app.container_id || '')}'); closeStoreAppModal();" title="Voir le conteneur dans Mes Conteneurs">
          <span>🐳</span> Voir le conteneur
        </button>
        <button type="button" class="btn btn-secondary btn-sm" onclick="openDockerConfigModal('${app.id}'); closeStoreAppModal();">
          ⚙️ Modifier les variables
        </button>
        <button type="button" class="btn btn-danger btn-sm" onclick="uninstallStoreApp('${app.id}', '${escapeHtml(app.name)}'); closeStoreAppModal();">
          🗑️ Désinstaller du NAS
        </button>
      `;
    } else {
      actionsEl.innerHTML = `
        <button type="button" class="btn btn-success btn-sm" onclick="openDockerConfigModal('${app.id}'); closeStoreAppModal();">
          📥 Installer l'application
        </button>
      `;
    }
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
  const dataDir = `${getUserHome()}/docker/${appId}`;

  document.getElementById("config-app-id").value = appId;
  document.getElementById("config-app-title").textContent = `Configuration : ${title}`;
  document.getElementById("config-app-subtitle").textContent = app && app.tagline ? app.tagline : "Variables d'environnement, port et volumes";
  const portInput = document.getElementById("config-app-port");
  if (portInput) {
    portInput.value = defaultPort;
    portInput.oninput = () => {
      updateConfigUrlPreview();
      checkModalFirewallStatus();
    };
  }
  document.getElementById("config-app-icon").src = icon;
  document.getElementById("config-app-data-dir").value = dataDir;
  
  const nixTarget = document.getElementById("config-app-nix-target");
  if (nixTarget) nixTarget.textContent = `/etc/nixos/docker/${appId}.nix`;

  updateConfigUrlPreview();
  checkModalFirewallStatus();

  // Remplir les variables d'environnement
  const container = document.getElementById("config-env-rows-container");
  if (container) {
    container.innerHTML = "";
    let envs = DEFAULT_DOCKER_ENVS[appId];
    if (!envs && app && Array.isArray(app.env) && app.env.length > 0) {
      envs = app.env.map(e => ({ key: e.name, value: e.default || "" }));
    }
    if (!envs) {
      envs = [
        { key: "TZ", value: "Europe/Paris" },
        { key: "PUID", value: "1000" },
        { key: "PGID", value: "100" }
      ];
    }
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
        mediaInput.value = getUserHome() + "/videos";
        mediaInput.dataset.app = appId;
      }
      selectMediaPreset(mediaInput ? mediaInput.value : (getUserHome() + "/videos"), false);
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

  updateDockerConfigModalDeployButton(appId);
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
  const base = (mediaInput && mediaInput.value.trim()) ? mediaInput.value.trim().replace(/\/+$/, "") : (getUserHome() + "/videos");

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
  const dataDir = document.getElementById("config-app-data-dir").value.trim() || `${getUserHome()}/docker/${appId}`;

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
  else if (appId === "adguard" || appId.includes("adguard")) image = "adguard/adguardhome:latest";
  else if (appId === "pihole") image = "pihole/pihole:latest";
  else if (appId === "nextcloud") image = "lscr.io/linuxserver/nextcloud:latest";
  else if (appId === "home-assistant") image = "ghcr.io/home-assistant/home-assistant:stable";

  let composeYaml = `# =========================================================================\n`;
  composeYaml += `# 🐳 Noos NAS Edition — Configuration Docker Compose\n`;
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

  if (appId === "adguard" || appId.includes("adguard")) {
    composeYaml += `    ports:\n`;
    composeYaml += `      - "53:53/tcp"\n`;
    composeYaml += `      - "53:53/udp"\n`;
    composeYaml += `      - "3000:3000/tcp" # WebUI initiale (Assistant d installation)\n`;
    composeYaml += `      - "80:80/tcp"     # WebUI finale administration\n`;
    composeYaml += `      - "853:853/tcp"   # DNS over TLS\n`;
  } else {
    composeYaml += `    ports:\n`;
    composeYaml += `      - "${port}:${port}"\n`;
  }

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
  } else if (appId === "adguard" || appId.includes("adguard")) {
    composeYaml += `      - ${dataDir}/work:/opt/adguardhome/work\n`;
    composeYaml += `      - ${dataDir}/conf:/opt/adguardhome/conf\n`;
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
    `${getUserHome()}/docker/${appId}/docker-compose.yml`
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

// ============================================================================
// GESTIONNAIRE D'ORCHESTRATION & FILE D'ATTENTE DOCKER STORE (MULTI-POPUPS & PERSISTANCE)
// ============================================================================
const MAX_CONCURRENT_DOCKER_DEPLOYS = 2;
const DOCKER_DEPLOY_STORAGE_KEY = "noos_docker_deployments_v1";
const activeDockerDeployments = {}; // appId -> { appId, appName, icon, port, payload, status, step, progressPercent, subtitle, badgeText, badgeClass, errorMessage, startedAt, ... }
const dockerDeployQueue = [];        // [ { appId, appName, icon, port, payload }, ... ]
let dockerDeployTickerInterval = null;
let isDockerPageUnloading = false;
if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", () => {
    isDockerPageUnloading = true;
  });
}

function saveDockerDeployStateToStorage() {
  try {
    const cleanedActive = {};
    const now = Date.now();
    for (const [key, val] of Object.entries(activeDockerDeployments)) {
      if (!val) continue;
      const copy = { ...val };
      delete copy.stepTimer1;
      delete copy.stepTimer2;
      delete copy.dismissTimer;

      // Conserver les déploiements actifs, ou récents (< 30 minutes si terminés/échoués)
      if (copy.status === "active" || (copy.completedAt && (now - copy.completedAt < 1800000)) || (copy.startedAt && (now - copy.startedAt < 1800000))) {
        cleanedActive[key] = copy;
      }
    }

    const payload = {
      active: cleanedActive,
      queue: dockerDeployQueue,
      updatedAt: now
    };
    localStorage.setItem(DOCKER_DEPLOY_STORAGE_KEY, JSON.stringify(payload));
  } catch (e) {
    console.warn("[Docker Deploy] Erreur sauvegarde localStorage:", e);
  }
}

function restoreDockerDeployStateFromStorage() {
  try {
    const raw = (localStorage.getItem(DOCKER_DEPLOY_STORAGE_KEY) );
    if (!raw) return;
    const data = JSON.parse(raw);
    if (!data) return;

    let hasChanges = false;
    const now = Date.now();

    // 1. Restaurer la file d'attente
    if (Array.isArray(data.queue)) {
      data.queue.forEach(item => {
        const cleanId = (item.appId || "").toLowerCase();
        if (cleanId && !dockerDeployQueue.some(q => q.appId.toLowerCase() === cleanId)) {
          dockerDeployQueue.push(item);
          hasChanges = true;
        }
      });
    }

    // 2. Restaurer les déploiements actifs ou récents
    if (data.active && typeof data.active === "object") {
      for (const [key, val] of Object.entries(data.active)) {
        const cleanId = (key || "").toLowerCase();
        if (cleanId && !activeDockerDeployments[cleanId]) {
          activeDockerDeployments[cleanId] = {
            ...val,
            stepTimer1: null,
            stepTimer2: null
          };
          hasChanges = true;
        }
      }
    }

    if (hasChanges || Object.keys(activeDockerDeployments).length > 0 || dockerDeployQueue.length > 0) {
      renderAllDockerDeployToasts();
      const currentConfigAppId = document.getElementById("config-app-id") ? document.getElementById("config-app-id").value.trim() : null;
      if (currentConfigAppId) {
        updateDockerConfigModalDeployButton(currentConfigAppId);
      }
      ensureDockerDeployTicker();
      syncActiveDockerDeploymentsWithBackend();
    }
  } catch (e) {
    console.warn("[Docker Deploy] Erreur restauration localStorage:", e);
  }
}

async function syncActiveDockerDeploymentsWithBackend() {
  try {
    const res = await fetch("/api/docker/store/deployments");
    const json = await res.json();
    let hasUpdated = false;

    if (json.success && Array.isArray(json.data)) {
      const backendDeployments = json.data; // [{ app_id, status, step, progress_percent, message, started_at, is_running, error }]

      backendDeployments.forEach(bDep => {
        const cleanId = (bDep.app_id || "").toLowerCase();
        let clientDep = activeDockerDeployments[cleanId];

        if (clientDep) {
          if (bDep.status === "ready" || bDep.is_running) {
            if (clientDep.status !== "success") {
              clientDep.status = "success";
              clientDep.step = 4;
              clientDep.progressPercent = 100;
              clientDep.subtitle = clientDep.port ? `Conteneur actif sur le port ${clientDep.port}` : "Conteneur actif sur votre NAS STEvE_OS";
              clientDep.badgeText = "🟢 Prêt (100%)";
              clientDep.badgeClass = "badge-success";
              clientDep.errorMessage = null;
              clientDep.completedAt = Date.now();
              hasUpdated = true;

              setTimeout(() => {
                if (activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "success") {
                  dismissDockerDeployToast(cleanId);
                }
              }, 12000);
            }
          } else if (bDep.status === "installing") {
            // RÈGLE PRIORITAIRE : Si le backend est en cours d'installation, le client doit être 'active'
            // (corrige le faux statut d'erreur provoqué par un rechargement de page F5)
            if (clientDep.status !== "active") {
              clientDep.status = "active";
              clientDep.badgeClass = "badge-warning";
              clientDep.errorMessage = null;
              hasUpdated = true;
            }
            const newStep = Math.max(clientDep.step || 1, bDep.step || 1);
            const newProgress = Math.max(clientDep.progressPercent || 20, bDep.progress_percent || 20);
            const newMsg = bDep.message || clientDep.subtitle;
            if (newStep !== clientDep.step || newProgress !== clientDep.progressPercent || newMsg !== clientDep.subtitle) {
              clientDep.step = newStep;
              clientDep.progressPercent = newProgress;
              clientDep.subtitle = newMsg;
              clientDep.badgeText = `⏳ En cours (${newProgress}%)`;
              hasUpdated = true;
            }
          } else if (bDep.status === "failed") {
            if (clientDep.status !== "error") {
              clientDep.status = "error";
              clientDep.step = bDep.step || 3;
              clientDep.errorMessage = bDep.error || bDep.message || "Erreur de démarrage Docker Compose";
              clientDep.subtitle = "Erreur de démarrage Docker Compose";
              clientDep.badgeText = "🔴 Erreur";
              clientDep.badgeClass = "badge-danger";
              clientDep.completedAt = Date.now();
              hasUpdated = true;
            }
          }
        } else if (bDep.status === "installing") {
          // Un déploiement en cours sur le serveur n'était pas dans le client local
          activeDockerDeployments[cleanId] = {
            appId: cleanId,
            appName: bDep.app_id.charAt(0).toUpperCase() + bDep.app_id.slice(1),
            icon: "/favicon.ico",
            port: null,
            payload: { app_id: cleanId },
            status: "active",
            step: bDep.step || 2,
            progressPercent: bDep.progress_percent || 45,
            subtitle: bDep.message || "Installation en cours sur le NAS...",
            badgeText: `⏳ En cours (${bDep.progress_percent || 45}%)`,
            badgeClass: "badge-warning",
            errorMessage: null,
            startedAt: bDep.started_at ? (bDep.started_at * 1000) : Date.now()
          };
          hasUpdated = true;
        }
      });
    }

    // Réconciliation supplémentaire avec /api/docker/containers pour valider les conteneurs actifs
    // (Vérifier à la fois 'active' et 'error' pour rattraper les faux échecs lors d'un F5)
    const pendingKeys = Object.keys(activeDockerDeployments).filter(k => 
      activeDockerDeployments[k].status === "active" || 
      (activeDockerDeployments[k].status === "error" && (!activeDockerDeployments[k].completedAt || (Date.now() - activeDockerDeployments[k].completedAt < 300000)))
    );

    if (pendingKeys.length > 0) {
      try {
        const cRes = await fetch("/api/docker/containers");
        const cJson = await cRes.json();
        if (cJson.success && Array.isArray(cJson.data)) {
          const runningNames = cJson.data
            .filter(c => (c.status || "").toLowerCase().includes("up") || (c.state || "").toLowerCase() === "running")
            .map(c => (c.name || "").toLowerCase().replace(/^\//, ""));

          pendingKeys.forEach(cleanId => {
            const dep = activeDockerDeployments[cleanId];
            if (!dep) return;
            const isRunning = runningNames.some(n => n === cleanId || n.includes(cleanId) || cleanId.includes(n));
            if (isRunning) {
              dep.status = "success";
              dep.step = 4;
              dep.progressPercent = 100;
              dep.subtitle = dep.port ? `Conteneur actif sur le port ${dep.port}` : "Conteneur actif sur votre NAS STEvE_OS";
              dep.badgeText = "🟢 Prêt (100%)";
              dep.badgeClass = "badge-success";
              dep.errorMessage = null;
              dep.completedAt = Date.now();
              hasUpdated = true;

              setTimeout(() => {
                if (activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "success") {
                  dismissDockerDeployToast(cleanId);
                }
              }, 12000);
            } else if (dep.status === "active" && dep.startedAt && (Date.now() - dep.startedAt > 300000)) {
              dep.status = "error";
              dep.step = 3;
              dep.errorMessage = "Délai d'attente dépassé (timeout 5 min). Veuillez vérifier les logs du conteneur.";
              dep.subtitle = "Délai de déploiement dépassé";
              dep.badgeText = "🔴 Timeout";
              dep.badgeClass = "badge-danger";
              dep.completedAt = Date.now();
              hasUpdated = true;
            }
          });
        }
      } catch (errContainers) {
        console.warn("[Docker Deploy] Erreur vérification conteneurs:", errContainers);
      }
    }

    if (hasUpdated) {
      renderAllDockerDeployToasts();
      saveDockerDeployStateToStorage();
      processNextInDockerDeployQueue();
      const currentConfigAppId = document.getElementById("config-app-id") ? document.getElementById("config-app-id").value.trim() : null;
      if (currentConfigAppId) {
        updateDockerConfigModalDeployButton(currentConfigAppId);
      }
    }
  } catch (err) {
    console.warn("[Docker Deploy] Erreur synchronisation déploiements backend:", err);
  }
}

function ensureDockerDeployTicker() {
  if (dockerDeployTickerInterval) return;

  dockerDeployTickerInterval = setInterval(() => {
    const hasActive = Object.values(activeDockerDeployments).some(d => d.status === "active");
    const hasQueue = dockerDeployQueue.length > 0;

    if (!hasActive && !hasQueue) {
      clearInterval(dockerDeployTickerInterval);
      dockerDeployTickerInterval = null;
      return;
    }

    syncActiveDockerDeploymentsWithBackend();
  }, 2000);
}

function updateDockerConfigModalDeployButton(appId) {
  const btn = document.getElementById("btn-submit-docker-deploy");
  if (!btn) return;

  const currentAppId = (appId || (document.getElementById("config-app-id") ? document.getElementById("config-app-id").value.trim() : "")).toLowerCase();
  if (!currentAppId) {
    btn.disabled = false;
    btn.className = "btn btn-primary btn-sm";
    btn.innerHTML = "🚀 Déployer l'application";
    return;
  }

  const activeDep = activeDockerDeployments[currentAppId];
  const queueIdx = dockerDeployQueue.findIndex(q => q.appId.toLowerCase() === currentAppId);

  if (activeDep && (activeDep.status === "active" || activeDep.status === "running")) {
    btn.disabled = true;
    btn.className = "btn btn-warning btn-sm";
    btn.innerHTML = `<span>⏳</span> Déploiement en cours (${activeDep.progressPercent || 20}%)...`;
    btn.title = "Cette application est en cours de déploiement.";
    return;
  }

  if (queueIdx !== -1) {
    btn.disabled = true;
    btn.className = "btn btn-secondary btn-sm";
    btn.innerHTML = `<span>🕒</span> En file d'attente (Position #${queueIdx + 1})`;
    btn.title = "Cette application est déjà en file d'attente.";
    return;
  }

  // L'application peut être déployée ou mise en file d'attente
  btn.disabled = false;
  const runningCount = Object.values(activeDockerDeployments).filter(d => d.status === "active").length;

  if (runningCount >= MAX_CONCURRENT_DOCKER_DEPLOYS) {
    btn.className = "btn btn-primary btn-sm";
    btn.innerHTML = `<span>📥</span> Ajouter à la file d'attente (${runningCount} en cours)`;
    btn.title = `${runningCount} applications sont déjà en cours de déploiement. Cette application démarrera automatiquement dès qu'un déploiement se libère.`;
  } else if (runningCount === 1) {
    btn.className = "btn btn-primary btn-sm";
    btn.innerHTML = `<span>🚀</span> Déployer l'application (Simultané 2/2)`;
    btn.title = "Déploiement simultané avec l'autre application en cours d'installation.";
  } else {
    btn.className = "btn btn-primary btn-sm";
    btn.innerHTML = `<span>🚀</span> Déployer l'application`;
    btn.title = "Déployer immédiatement cette application.";
  }
}

function processNextInDockerDeployQueue() {
  const runningCount = Object.values(activeDockerDeployments).filter(d => d.status === "active").length;
  if (runningCount < MAX_CONCURRENT_DOCKER_DEPLOYS && dockerDeployQueue.length > 0) {
    const nextItem = dockerDeployQueue.shift();
    saveDockerDeployStateToStorage();
    renderAllDockerDeployToasts();
    startDockerAppDeploy(nextItem);
  }
}

function removeDockerFromDeployQueue(appId) {
  const cleanId = (appId || "").toLowerCase();
  const qIdx = dockerDeployQueue.findIndex(q => q.appId.toLowerCase() === cleanId);
  if (qIdx !== -1) {
    const removed = dockerDeployQueue.splice(qIdx, 1)[0];
    showToast(`Application ${removed.appName || cleanId} retirée de la file d'attente.`, "info");
    dismissDockerDeployToast(cleanId);
    saveDockerDeployStateToStorage();
    renderAllDockerDeployToasts();
    updateDockerConfigModalDeployButton(cleanId);
  }
}

let currentDockerDeployError = {
  appName: "",
  appId: "",
  message: ""
};

function openDockerDeployErrorModal(appName, message) {
  if (appName) currentDockerDeployError.appName = appName;
  if (message) currentDockerDeployError.message = message;

  const modal = document.getElementById("modal-docker-deploy-error");
  const title = document.getElementById("docker-deploy-error-modal-title");
  const textarea = document.getElementById("docker-deploy-error-textarea");
  const hintBox = document.getElementById("docker-deploy-error-hint");
  const hintTitle = document.getElementById("docker-deploy-error-hint-title");
  const hintDesc = document.getElementById("docker-deploy-error-hint-desc");
  const feedback = document.getElementById("docker-error-copied-feedback");

  if (feedback) feedback.style.display = "none";

  const appLabel = currentDockerDeployError.appName || currentDockerDeployError.appId || "Application";
  if (title) title.textContent = `Échec du Déploiement : ${appLabel}`;

  const msg = currentDockerDeployError.message || "Aucun détail d'erreur disponible.";
  if (textarea) {
    textarea.value = msg;
    setTimeout(() => { textarea.scrollTop = textarea.scrollHeight; }, 100);
  }

  // Diagnostic intelligent et conseils ergonomiques
  if (hintBox) {
    const isPort53 = msg.includes("0.0.0.0:53") || msg.includes(":53/tcp") || msg.includes(":53/udp") || msg.includes("port 53");
    const isPortConflict = msg.includes("address already in use") || msg.includes("failed to bind host port");

    if (isPort53) {
      hintBox.style.display = "block";
      if (hintTitle) hintTitle.innerHTML = "⚠️ Conflit sur le port 53 (DNS système déjà actif)";
      if (hintDesc) hintDesc.innerHTML = "Le port <strong>53 (TCP/UDP)</strong> est déjà utilisé par un service de résolution DNS local sur l'hôte (ex: <code>systemd-resolved</code> ou <code>dnsmasq</code>).<br>Pour déployer un résolveur DNS comme AdGuard Home ou Pi-hole sans conflit :<br>• <strong>Option recommandée</strong> : Désactiver le résolveur stub local dans NixOS (<code>services.resolved.extraConfig = \"DNSStubListener=no\\n\";</code>) tout en conservant les serveurs DNS amont dans <code>networking.nameservers</code>.<br>• <strong>Alternative</strong> : Lier le conteneur sur l'adresse IP locale spécifique du NAS au lieu de <code>0.0.0.0</code> ou utiliser un réseau Macvlan.";
    } else if (isPortConflict) {
      const portMatch = msg.match(/:(\d+)\/(tcp|udp): address already in use/i) || msg.match(/bind.*:(\d+): address already in use/i);
      const portUsed = portMatch ? portMatch[1] : "demandé";
      hintBox.style.display = "block";
      if (hintTitle) hintTitle.innerHTML = `⚠️ Conflit de port réseau (Port ${portUsed} déjà réservé)`;
      if (hintDesc) hintDesc.innerHTML = `Le port hôte <strong>${portUsed}</strong> est déjà occupé par un autre conteneur ou un démon système du NAS.<br>Modifiez le port d'écoute externe de l'application dans la modale d'installation pour choisir un port libre.`;
    } else {
      hintBox.style.display = "none";
    }
  }

  if (modal) modal.style.display = "flex";
}

function closeDockerDeployErrorModal() {
  const modal = document.getElementById("modal-docker-deploy-error");
  if (modal) modal.style.display = "none";
}

function copyDockerDeployError() {
  const textarea = document.getElementById("docker-deploy-error-textarea");
  const feedback = document.getElementById("docker-error-copied-feedback");
  if (!textarea) return;

  const text = textarea.value || "";
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(() => {
      if (feedback) {
        feedback.style.display = "inline";
        setTimeout(() => { feedback.style.display = "none"; }, 3500);
      }
    }).catch(() => fallbackCopyDeployError(textarea, feedback));
  } else {
    fallbackCopyDeployError(textarea, feedback);
  }
}

function fallbackCopyDeployError(textarea, feedback) {
  textarea.select();
  try {
    document.execCommand("copy");
    if (feedback) {
      feedback.style.display = "inline";
      setTimeout(() => { feedback.style.display = "none"; }, 3500);
    }
  } catch (e) {
    showToast("Impossible de copier automatiquement, veuillez sélectionner le texte manuellement.", "warning");
  }
}

function dismissDockerDeployToast(appId) {
  if (!appId) {
    const cards = document.querySelectorAll(".docker-deploy-floating-toast");
    cards.forEach(c => {
      const aId = c.getAttribute("data-app-id");
      if (aId) dismissDockerDeployToast(aId);
    });
    return;
  }

  const cleanId = String(appId).toLowerCase();
  const card = document.getElementById(`docker-deploy-floating-toast-${cleanId}`);
  if (card) {
    card.style.animation = "slideOutBottomRight 0.3s cubic-bezier(0.16, 1, 0.3, 1)";
    setTimeout(() => {
      card.remove();
      if (activeDockerDeployments[cleanId]) {
        delete activeDockerDeployments[cleanId];
      }
      const qIdx = dockerDeployQueue.findIndex(q => q.appId.toLowerCase() === cleanId);
      if (qIdx !== -1) {
        dockerDeployQueue.splice(qIdx, 1);
      }
      saveDockerDeployStateToStorage();
      updateDockerConfigModalDeployButton(cleanId);
      updateFloatingDockLayout();
    }, 280);
  } else {
    if (activeDockerDeployments[cleanId]) {
      delete activeDockerDeployments[cleanId];
    }
    const qIdx = dockerDeployQueue.findIndex(q => q.appId.toLowerCase() === cleanId);
    if (qIdx !== -1) {
      dockerDeployQueue.splice(qIdx, 1);
    }
    saveDockerDeployStateToStorage();
    updateDockerConfigModalDeployButton(cleanId);
    updateFloatingDockLayout();
  }
}

function renderAllDockerDeployToasts() {
  const stack = document.getElementById("floating-dock-stack");
  if (!stack) return;

  const activeList = Object.values(activeDockerDeployments);
  const queueList = dockerDeployQueue.map((item, idx) => ({
    ...item,
    status: "queued",
    queuePos: idx + 1,
    step: 0,
    progressPercent: 0,
    subtitle: `🕒 En file d'attente (Position #${idx + 1})`,
    badgeText: `🕒 File d'attente (#${idx + 1})`,
    badgeClass: "badge-secondary"
  }));

  const allToasts = [...activeList, ...queueList];

  // Nettoyer les cartes du DOM qui n'existent plus
  const currentCardElements = document.querySelectorAll(".docker-deploy-floating-toast");
  currentCardElements.forEach(el => {
    const aId = el.getAttribute("data-app-id");
    if (!allToasts.some(t => t.appId.toLowerCase() === (aId || "").toLowerCase())) {
      el.remove();
    }
  });

  allToasts.forEach(t => {
    const cleanId = t.appId.toLowerCase();
    let card = document.getElementById(`docker-deploy-floating-toast-${cleanId}`);
    if (!card) {
      card = document.createElement("div");
      card.className = "floating-task-card docker-deploy-floating-toast";
      card.id = `docker-deploy-floating-toast-${cleanId}`;
      card.setAttribute("data-app-id", cleanId);
      stack.appendChild(card);
    }

    card.style.display = "block";

    const isSuccess = t.status === "success";
    const isError = t.status === "error";
    const isQueued = t.status === "queued";
    const isActive = t.status === "active";

    let cardStatusClass = "";
    if (isSuccess) cardStatusClass = "status-success";
    else if (isError) cardStatusClass = "status-error";
    else if (isQueued) cardStatusClass = "status-queued";

    const step1Class = (t.step > 1 || isSuccess) ? "completed" : (t.step === 1 ? "active" : "");
    const step1Icon = (t.step > 1 || isSuccess) ? "✓" : (t.step === 1 ? "⏳" : "⚪");

    const step2Class = (t.step > 2 || isSuccess) ? "completed" : (t.step === 2 ? "active" : "");
    const step2Icon = (t.step > 2 || isSuccess) ? "✓" : (t.step === 2 ? "⏳" : "⚪");

    const step3Class = isSuccess ? "completed" : (isError ? "error is-clickable" : (t.step === 3 ? "active" : ""));
    const step3Icon = isSuccess ? "✓" : (isError ? "❌" : (t.step === 3 ? "⏳" : "⚪"));

    const step4Class = isSuccess ? "completed" : (isError ? "error is-clickable" : (t.step === 4 ? "active" : ""));
    const step4Icon = isSuccess ? "✓" : (isError ? "❌" : (t.step === 4 ? "⏳" : "⚪"));

    const barBg = isError
      ? "background: var(--red); width: 80%;"
      : (isSuccess
        ? "background: linear-gradient(90deg, var(--green), #a6e3a1); width: 100%;"
        : (isQueued
          ? "background: var(--surface2); width: 100%; opacity: 0.6;"
          : "background: linear-gradient(90deg, var(--mauve), var(--blue)); width: " + (t.progressPercent || 20) + "%;"));

    const actionsDisplay = (isSuccess || isError || isQueued) ? "flex" : "none";

    let openBtnHtml = "";
    if (isSuccess && t.port) {
      openBtnHtml = `
        <a href="http://${window.location.hostname}:${t.port}" target="_blank" class="btn btn-success btn-xs" style="text-decoration:none;">
          <span>🚀</span> Ouvrir (Port ${t.port}) ↗
        </a>
      `;
    }

    let errorBtnHtml = "";
    if (isError) {
      errorBtnHtml = `
        <button type="button" class="btn btn-danger btn-xs" onclick="openDockerDeployErrorModal('${escapeHtml(t.appName)}', '${escapeHtml(t.errorMessage || '')}')">
          <span>🔍</span> Voir le rapport d'erreur
        </button>
      `;
    }

    let queueCancelBtnHtml = "";
    if (isQueued) {
      queueCancelBtnHtml = `
        <button type="button" class="btn btn-secondary btn-xs" onclick="removeDockerFromDeployQueue('${escapeHtml(cleanId)}')">
          <span>✕</span> Retirer de la file
        </button>
      `;
    }

    card.innerHTML = `
      <div class="docker-deploy-toast-card ${cardStatusClass}" id="docker-deploy-toast-card-${cleanId}">
        <div class="docker-deploy-header">
          <div class="docker-deploy-icon-wrap">
            <img src="${t.icon || '/favicon.ico'}" alt="${escapeHtml(t.appName)}" class="docker-deploy-img-icon" onerror="this.src='/favicon.ico';">
            <span class="docker-deploy-pulse-dot" style="${isQueued ? 'background:var(--blue); animation:none;' : (isSuccess ? 'background:var(--green); animation:none;' : (isError ? 'background:var(--red); animation:none;' : ''))}"></span>
          </div>
          <div class="docker-deploy-titles">
            <div class="docker-deploy-title">${escapeHtml(t.appName)}</div>
            <div class="docker-deploy-subtitle">${escapeHtml(t.subtitle)}</div>
          </div>
          <div class="docker-deploy-header-right">
            <span class="badge ${t.badgeClass || 'badge-warning'}" style="font-size:0.7rem;">${t.badgeText}</span>
            <button type="button" class="floating-card-action-btn" onclick="toggleFloatingCardMinimize('docker-deploy-floating-toast-${cleanId}')" title="Réduire / Dérouler">_</button>
            <button type="button" class="docker-deploy-close-btn" onclick="dismissDockerDeployToast('${cleanId}')" title="Masquer la notification">&times;</button>
          </div>
        </div>

        <div class="floating-card-body docker-deploy-toast-body">
          <div class="docker-deploy-progress-wrap">
            <div class="docker-deploy-progress-bar ${isQueued ? 'progress-animated' : ''}" style="${barBg}"></div>
          </div>

          <div class="docker-deploy-steps">
            <div class="deploy-step-item ${step1Class}">
              <span class="step-status-icon">${step1Icon}</span>
              <span class="step-text">Préparation du dossier persistant (~/docker/${escapeHtml(cleanId)})</span>
            </div>
            <div class="deploy-step-item ${step2Class}">
              <span class="step-status-icon">${step2Icon}</span>
              <span class="step-text">Configuration des variables & ports réseau</span>
            </div>
            <div class="deploy-step-item ${step3Class}" ${isError ? `onclick="openDockerDeployErrorModal('${escapeHtml(t.appName)}', '${escapeHtml(t.errorMessage || '')}')" style="cursor:pointer;" title="Cliquer pour voir l'erreur"` : ''}>
              <span class="step-status-icon">${step3Icon}</span>
              <span class="step-text">Lancement Docker Compose (docker compose up -d)</span>
            </div>
            <div class="deploy-step-item ${step4Class}" ${isError ? `onclick="openDockerDeployErrorModal('${escapeHtml(t.appName)}', '${escapeHtml(t.errorMessage || '')}')" style="cursor:pointer;" title="Cliquer pour voir l'erreur"` : ''}>
              <span class="step-status-icon">${step4Icon}</span>
              <span class="step-text">${isError ? 'Échec d\'exécution Docker Compose (cliquez pour inspecter)' : 'Vérification de l\'état actif & mise en ligne'}</span>
            </div>
          </div>

          <div class="docker-deploy-actions" style="display: ${actionsDisplay};">
            ${openBtnHtml}
            ${errorBtnHtml}
            ${queueCancelBtnHtml}
            <button type="button" class="btn btn-secondary btn-xs" onclick="switchTab('tab-containers'); switchDockerSubTab('containers');">
              <span>📦</span> Mes Conteneurs
            </button>
          </div>
        </div>
      </div>
    `;
  });

  updateFloatingDockLayout();
}

async function startDockerAppDeploy(item) {
  const cleanId = (item.appId || "").toLowerCase();
  const { appName, icon, port, payload } = item;

  const dep = {
    appId: cleanId,
    appName: appName,
    icon: icon,
    port: port,
    payload: payload,
    status: "active",
    step: 1,
    progressPercent: 20,
    subtitle: "Docker Compose v2 • Initialisation...",
    badgeText: "⏳ En cours (20%)",
    badgeClass: "badge-warning",
    errorMessage: null,
    startedAt: Date.now()
  };
  activeDockerDeployments[cleanId] = dep;

  saveDockerDeployStateToStorage();
  ensureDockerDeployTicker();
  renderAllDockerDeployToasts();
  updateDockerConfigModalDeployButton(cleanId);

  // Étape 2 à 450ms
  dep.stepTimer1 = setTimeout(() => {
    if (activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "active") {
      activeDockerDeployments[cleanId].step = 2;
      activeDockerDeployments[cleanId].progressPercent = 45;
      activeDockerDeployments[cleanId].subtitle = "Configuration des volumes persistants...";
      activeDockerDeployments[cleanId].badgeText = "⏳ En cours (45%)";
      saveDockerDeployStateToStorage();
      renderAllDockerDeployToasts();
    }
  }, 450);

  // Étape 3 à 1000ms
  dep.stepTimer2 = setTimeout(() => {
    if (activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "active") {
      activeDockerDeployments[cleanId].step = 3;
      activeDockerDeployments[cleanId].progressPercent = 75;
      activeDockerDeployments[cleanId].subtitle = "Lancement conteneur (docker compose up -d)...";
      activeDockerDeployments[cleanId].badgeText = "⏳ Compose up (75%)";
      saveDockerDeployStateToStorage();
      renderAllDockerDeployToasts();
    }
  }, 1000);

  try {
    const res = await fetch("/api/docker/store/install", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    const json = await res.json();

    if (json.success) {
      if (activeDockerDeployments[cleanId]) {
        activeDockerDeployments[cleanId].status = "success";
        activeDockerDeployments[cleanId].step = 4;
        activeDockerDeployments[cleanId].progressPercent = 100;
        activeDockerDeployments[cleanId].subtitle = port ? `Conteneur actif sur le port ${port}` : "Conteneur actif sur votre NAS STEvE_OS";
        activeDockerDeployments[cleanId].badgeText = "🟢 Prêt (100%)";
        activeDockerDeployments[cleanId].badgeClass = "badge-success";
        activeDockerDeployments[cleanId].completedAt = Date.now();
        saveDockerDeployStateToStorage();
      }
      showToast(json.data || `Application ${appName} configurée et démarrée avec succès !`, "success");
      setTimeout(() => refreshContainersAndStore(), 2000);

      // Auto-fermeture après 12 secondes
      setTimeout(() => {
        if (activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "success") {
          dismissDockerDeployToast(cleanId);
        }
      }, 12000);

    } else {
      if (activeDockerDeployments[cleanId]) {
        activeDockerDeployments[cleanId].status = "error";
        activeDockerDeployments[cleanId].step = 3;
        activeDockerDeployments[cleanId].errorMessage = json.message || "Erreur de démarrage Docker Compose";
        activeDockerDeployments[cleanId].subtitle = "Erreur de démarrage Docker Compose";
        activeDockerDeployments[cleanId].badgeText = "🔴 Erreur";
        activeDockerDeployments[cleanId].badgeClass = "badge-danger";
        activeDockerDeployments[cleanId].completedAt = Date.now();
        saveDockerDeployStateToStorage();
      }
      showToast(`Erreur déploiement ${appName} : ${json.message}`, "error");
      openDockerDeployErrorModal(appName, json.message);
    }
  } catch (err) {
    if (isDockerPageUnloading) {
      console.log(`[Docker Deploy] Requête interrompue lors du rechargement de page pour ${appName}, déploiement maintenu actif.`);
      return;
    }

    // Avant de déclarer une erreur fatale, vérifier si le serveur est en train de déployer
    try {
      const checkRes = await fetch("/api/docker/store/deployments");
      const checkJson = await checkRes.json();
      if (checkJson.success && Array.isArray(checkJson.data)) {
        const serverDep = checkJson.data.find(d => (d.app_id || "").toLowerCase() === cleanId);
        if (serverDep && serverDep.status === "installing") {
          console.log(`[Docker Deploy] Déploiement ${appName} toujours actif sur le serveur, polling maintenu.`);
          ensureDockerDeployTicker();
          return;
        } else if (serverDep && (serverDep.status === "ready" || serverDep.is_running)) {
          if (activeDockerDeployments[cleanId]) {
            activeDockerDeployments[cleanId].status = "success";
            activeDockerDeployments[cleanId].step = 4;
            activeDockerDeployments[cleanId].progressPercent = 100;
            activeDockerDeployments[cleanId].subtitle = port ? `Conteneur actif sur le port ${port}` : "Conteneur actif sur votre NAS STEvE_OS";
            activeDockerDeployments[cleanId].badgeText = "🟢 Prêt (100%)";
            activeDockerDeployments[cleanId].badgeClass = "badge-success";
            activeDockerDeployments[cleanId].errorMessage = null;
            activeDockerDeployments[cleanId].completedAt = Date.now();
            saveDockerDeployStateToStorage();
            renderAllDockerDeployToasts();
            return;
          }
        }
      }
    } catch (_) {}

    if (activeDockerDeployments[cleanId]) {
      activeDockerDeployments[cleanId].status = "error";
      activeDockerDeployments[cleanId].step = 3;
      activeDockerDeployments[cleanId].errorMessage = String(err);
      activeDockerDeployments[cleanId].subtitle = "Erreur de communication avec le NAS";
      activeDockerDeployments[cleanId].badgeText = "🔴 Erreur";
      activeDockerDeployments[cleanId].badgeClass = "badge-danger";
      activeDockerDeployments[cleanId].completedAt = Date.now();
      saveDockerDeployStateToStorage();
    }
    showToast(`Erreur requête ${appName} : ${err}`, "error");
    openDockerDeployErrorModal(appName, String(err));
  } finally {
    saveDockerDeployStateToStorage();
    renderAllDockerDeployToasts();
    updateDockerConfigModalDeployButton(cleanId);
    processNextInDockerDeployQueue();
  }
}

// Rétrocompatibilité si appelée de l'extérieur
function startDockerDeployToast(appId, appName, icon, port) {
  startDockerAppDeploy({
    appId,
    appName: appName || appId,
    icon,
    port,
    payload: { app_id: appId, port }
  });
}

function completeDockerDeployToast(success, message, port, appId, appName) {
  const cleanId = (appId || "").toLowerCase();
  if (activeDockerDeployments[cleanId]) {
    activeDockerDeployments[cleanId].status = success ? "success" : "error";
    saveDockerDeployStateToStorage();
    renderAllDockerDeployToasts();
  }
}

async function submitDockerDeploy() {
  const appIdInput = document.getElementById("config-app-id");
  const rawId = appIdInput ? appIdInput.value.trim() : "";
  const cleanId = rawId.toLowerCase();

  if (!cleanId) {
    showToast("Identifiant d'application manquant", "error");
    return;
  }

  const titleEl = document.getElementById("config-app-title");
  const appTitle = titleEl ? titleEl.textContent.replace("Configuration : ", "").trim() : cleanId;
  const iconEl = document.getElementById("config-app-icon");
  const appIcon = iconEl ? iconEl.src : "/favicon.ico";

  // Contrôle anti-doublon si déjà actif ou en attente
  if (activeDockerDeployments[cleanId] && activeDockerDeployments[cleanId].status === "active") {
    showToast(`L'application ${appTitle} est déjà en cours de déploiement.`, "warning");
    return;
  }
  if (dockerDeployQueue.some(q => q.appId.toLowerCase() === cleanId)) {
    showToast(`L'application ${appTitle} est déjà dans la file d'attente.`, "warning");
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
    app_id: cleanId,
    port: portVal ? parseInt(portVal, 10) : null,
    data_dir: dataDir || null,
    media_dir: mediaDirVal || null,
    gpu_device: gpuDeviceVal || null,
    env_vars: envVars
  };

  const item = {
    appId: cleanId,
    appName: appTitle,
    icon: appIcon,
    port: portVal ? parseInt(portVal, 10) : null,
    payload: payload
  };

  // Fermer la fenêtre de configuration
  closeDockerConfigModal();

  const runningCount = Object.values(activeDockerDeployments).filter(d => d.status === "active").length;
  if (runningCount < MAX_CONCURRENT_DOCKER_DEPLOYS) {
    // Démarrage immédiat en parallèle
    startDockerAppDeploy(item);
  } else {
    // Ajout dans la file d'attente FIFO
    dockerDeployQueue.push(item);
    saveDockerDeployStateToStorage();
    ensureDockerDeployTicker();
    showToast(`Application ${appTitle} ajoutée à la file d'attente (Position #${dockerDeployQueue.length})`, "info");
    renderAllDockerDeployToasts();
    updateDockerConfigModalDeployButton(cleanId);
  }
}

// Initialisation immédiate de la restauration des déploiements
if (typeof document !== "undefined") {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => restoreDockerDeployStateFromStorage());
  } else {
    restoreDockerDeployStateFromStorage();
  }
}

async function uninstallStoreApp(appId, appName) {
  const deleteData = confirm(`Désinstaller l'application '${appName}' ?\n\nCliquez sur OK pour désinstaller.\n(Vous pourrez choisir à l'étape suivante si vous souhaitez conserver ou effacer les données dans ${getUserHome()}/docker/${appId}).`);
  if (!deleteData) return;

  const purge = confirm(`Voulez-vous également SUPPRIMER définitivement les données de ${getUserHome()}/docker/${appId} ?\n\n- Cliquez sur OK pour SUPPRIMER les fichiers.\n- Cliquez sur Annuler pour CONSERVER les données de configuration.`);

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
      dismissDockerDeployToast(appId);
      await refreshContainersAndStore();
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

  try {
    const headers = {};
    if (token) {
      headers["Authorization"] = `Bearer ${token}`;
    }

    const res = await originalFetch("/api/auth/me", { headers });
    if (res.ok) {
      const data = await res.json();
      if (data.success || data.authenticated) {
        currentUserSession = data;
        if (data.home_dir) currentUserHome = data.home_dir;
        updateUserSessionUI(data);
        document.body.classList.remove("not-authenticated");
        document.body.classList.add("authenticated");
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
  document.body.classList.remove("authenticated");
  document.body.classList.add("not-authenticated");
  showLoginModal();
}

function showLoginModal() {
  document.body.classList.remove("authenticated");
  document.body.classList.add("not-authenticated");
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
  const homeTextEl = document.getElementById("fnav-home-text");

  if (session && session.username) {
    if (userPill) userPill.style.display = "flex";
    if (usernameEl) usernameEl.textContent = session.username;
    if (userRoleEl) userRoleEl.textContent = session.is_admin ? "(Admin)" : "(Utilisateur)";
    if (homeTextEl) homeTextEl.textContent = `personnel (~/${session.username})`;
    updateSftpQuickUrisWithUser(session.username);

    if (!currentFolderPath || currentFolderPath === "/home/chomiam" || currentFolderPath.includes("/chomiam")) {
      currentFolderPath = getUserHome();
    }
  } else {
    if (userPill) userPill.style.display = "none";
    if (homeTextEl) homeTextEl.textContent = "personnel";
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
        remember: remember
      })
    });

    const data = await res.json();

    if (res.ok && data.success && data.token) {
      setAuthToken(data.token, remember);
      currentUserSession = {
        username: data.username,
        is_admin: data.is_admin,
        home_dir: data.home_dir || `/home/${data.username}`
      };
      if (data.home_dir) currentUserHome = data.home_dir;
      updateUserSessionUI(currentUserSession);
      document.body.classList.remove("not-authenticated");
      document.body.classList.add("authenticated");
      hideLoginModal();

      if (!isAppInitialized) {
        isAppInitialized = true;
        initApp();
        setupPolling();
      }

      showToast(`Bienvenue sur Noos, ${data.username} !`, "success");
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

  // 1. Masquer immédiatement le dashboard et afficher l'écran de connexion
  document.body.classList.remove("authenticated");
  document.body.classList.add("not-authenticated");
  showLoginModal();

  // 2. Appel serveur de révocation
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

  // 3. Purge complète du stockage client et des cookies
  clearAuthToken();
  updateUserSessionUI(null);

  // 4. Nettoyage de l'URL (?token=...)
  if (window.location.search) {
    try {
      window.history.replaceState({}, document.title, window.location.pathname);
    } catch (e) {}
  }

  // 5. Redirection / Rechargement propre à la racine pour purger la mémoire, les intervals et le DOM
  window.location.replace("/");
}

function toggleLoginPasswordVisibility() {
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

function switchNetworkSubtab(subtabId, updateHash = true) {
  activeNetworkSubtab = subtabId;
  try {
    localStorage.setItem("noos_subtab_network", subtabId);
  } catch (e) {}
  if (updateHash && activeTab === "tab-network") {
    updateUrlHash("tab-network", subtabId);
  }
  document.querySelectorAll(".network-subtab-btn").forEach(btn => {
    btn.classList.toggle("active", btn.getAttribute("data-subtab") === subtabId);
  });
  document.querySelectorAll(".network-subpane").forEach(pane => {
    pane.classList.toggle("active", pane.id === subtabId);
  });
  if (subtabId === "subtab-vpn") {
    loadWireguardClients();
  }
  if (subtabId === "subtab-dns") {
    loadDnsSettings();
  }
  if (subtabId === "subtab-samba") {
    loadSambaData();
  }
  if (subtabId === "subtab-sftp") {
    loadSftpData();
  }
  if (subtabId === "subtab-firewall") {
    try {
      const savedFwState = localStorage.getItem("noos_firewall_state");
      if (savedFwState === "disabled") {
        const toggle = document.getElementById("firewall-global-toggle");
        if (toggle) toggle.checked = false;
        const toggleText = document.getElementById("firewall-toggle-text");
        if (toggleText) toggleText.textContent = "Protection Inactive";
        const heroCard = document.getElementById("firewall-hero-card");
        if (heroCard) heroCard.classList.add("disabled-state");
      }
    } catch (e) {}
    startTrafficPolling();
    loadTrafficHistory();
  } else {
    stopTrafficPolling();
  }
}

async function loadNetwork(showFeedback = false) {
  try {
    const res = await fetch("/api/network");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const net = json.data;
    cachedNetworkData = net;

    // Précharger le badge DNS
    fetch("/api/network/dns").then(r => r.ok ? r.json() : null).then(d => {
      if (d && d.success && d.data) {
        const badge = document.getElementById("badge-subtab-dns");
        if (badge) {
          const mode = d.data.mode;
          badge.textContent = mode === "custom" ? "Personnalisé" : (d.data.active_servers[0] || mode);
        }
      }
    }).catch(() => {});

    // --- 1. Adresses IP & Hôte ---
    const lanIp = net.primary_lan_ip || "127.0.0.1";
    const user = (currentUserSession && currentUserSession.username) || "noos";

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
    window.currentFirewallOverview = fw;

    // Sauvegarder dans localStorage pour persistance UI immédiate
    try {
      localStorage.setItem("noos_firewall_state", fw.is_enabled ? "enabled" : "disabled");
    } catch (e) {}

    // Mise à jour de la carte Héro Pare-feu
    const heroCard = document.getElementById("firewall-hero-card");
    if (heroCard) {
      heroCard.classList.toggle("disabled-state", !fw.is_enabled);
    }

    const shieldIcon = document.getElementById("firewall-shield-icon");
    if (shieldIcon) {
      shieldIcon.textContent = fw.is_enabled ? "🛡️" : "⚠️";
    }

    const fwBadge = document.getElementById("firewall-status-badge");
    if (fwBadge) {
      fwBadge.textContent = fw.is_enabled ? "🟢 Protection Active" : "🔴 Protection Désactivée";
      fwBadge.className = `badge ${fw.is_enabled ? "badge-success" : "badge-danger"}`;
    }

    const fwDesc = document.getElementById("firewall-status-desc");
    if (fwDesc) {
      fwDesc.textContent = fw.is_enabled
        ? "Le pare-feu NixOS filtre les flux réseau entrants. Seuls les services autorisés et les règles déclarées sont accessibles."
        : "⚠️ Attention : le pare-feu est désactivé. Tous les ports d'écoute de la machine sont exposés et accessibles sans restriction.";
    }

    const badgeFw = document.getElementById("badge-subtab-firewall");
    if (badgeFw) {
      badgeFw.textContent = fw.is_enabled ? "Actif" : "Désactivé";
      badgeFw.className = `subtab-pill-badge ${fw.is_enabled ? "badge-success" : "badge-danger"}`;
    }

    const fwToggle = document.getElementById("firewall-global-toggle");
    if (fwToggle) {
      fwToggle.checked = !!fw.is_enabled;
    }
    const fwToggleText = document.getElementById("firewall-toggle-text");
    if (fwToggleText) {
      fwToggleText.textContent = fw.is_enabled ? "Protection Active" : "Protection Inactive";
    }

    // Compteurs métriques
    const openPortsCountEl = document.getElementById("fw-open-ports-count");
    if (openPortsCountEl) openPortsCountEl.textContent = fw.total_open_ports || (fw.rules ? fw.rules.length : 0);

    const customRulesCountEl = document.getElementById("fw-custom-rules-count");
    if (customRulesCountEl) customRulesCountEl.textContent = fw.custom_rules_count || 0;

    const fail2banCountEl = document.getElementById("fw-fail2ban-count");
    if (fail2banCountEl) fail2banCountEl.textContent = (fw.banned_ips || []).length;

    // Règles de ports unifiées
    window.allFirewallRules = fw.rules || [];
    renderFirewallPorts(window.allFirewallRules);

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

    // Charger sFTP si le sous-onglet sFTP est actif
    const sftpSubpane = document.getElementById("subtab-sftp");
    if (activeNetworkSubtab === "subtab-sftp" || (sftpSubpane && sftpSubpane.classList.contains("active"))) {
      loadSftpData();
    }

    // Charger Samba si le sous-onglet Samba est actif
    const sambaSubpane = document.getElementById("subtab-samba");
    if (activeNetworkSubtab === "subtab-samba" || (sambaSubpane && sambaSubpane.classList.contains("active"))) {
      loadSambaData();
    }

    // Charger les profils clients WireGuard si le sous-onglet VPN est actif
    const vpnSubpane = document.getElementById("subtab-vpn");
    if (activeNetworkSubtab === "subtab-vpn" || (vpnSubpane && vpnSubpane.classList.contains("active"))) {
      loadWireguardClients();
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

// =========================================================================
// 🛡️ CONTRÔLEUR DE PORTS & PARE-FEU STEvE_OS
// =========================================================================

window.firewallFilter = "all";
window.allFirewallRules = [];
window.selectedTrafficIface = "all";
window.trafficPollingTimer = null;
window.cachedTrafficData = null;

function setFirewallFilter(filter) {
  window.firewallFilter = filter;
  document.querySelectorAll("#firewall-filter-pills .btn-filter-pill").forEach(btn => {
    btn.classList.toggle("active", btn.textContent.toLowerCase().includes(filter) || (filter === "all" && btn.textContent === "Tous"));
  });
  filterFirewallPorts();
}

window.isSystemPortsCollapsed = (localStorage.getItem("noos_fw_system_collapsed") === "true");

function toggleSystemPortsCollapse() {
  window.isSystemPortsCollapsed = !window.isSystemPortsCollapsed;
  try {
    localStorage.setItem("noos_fw_system_collapsed", window.isSystemPortsCollapsed ? "true" : "false");
  } catch (_) {}
  filterFirewallPorts();
}

function renderFirewallPorts(rules) {
  const tbody = document.getElementById("firewall-tbody");
  if (!tbody) return;

  if (!rules || rules.length === 0) {
    tbody.innerHTML = `<tr><td colspan="7" style="text-align:center; color:var(--subtext0); padding:28px;">Aucun port ou règle correspondant.</td></tr>`;
    return;
  }

  const queryInput = document.getElementById("firewall-search-input");
  const query = queryInput ? queryInput.value.trim() : "";

  // Si on filtre par pilule spécifique ("custom" ou "system")
  if (window.firewallFilter === "custom") {
    tbody.innerHTML = renderRuleRows(rules.filter(r => !r.is_system));
    return;
  }
  if (window.firewallFilter === "system") {
    tbody.innerHTML = renderRuleRows(rules.filter(r => r.is_system));
    return;
  }

  const customRules = rules.filter(r => !r.is_system);
  const systemRules = rules.filter(r => r.is_system);

  // L'accordéon n'est replié que si pas de recherche textuelle active
  const isCollapsed = window.isSystemPortsCollapsed && !query;

  let html = "";

  // 1. Groupe Utilisateur
  if (customRules.length > 0 || window.firewallFilter === "all") {
    html += `
      <tr class="fw-group-header fw-group-user">
        <td colspan="7">
          <div class="fw-group-title">
            <span>👤 Ports Personnalisés (Utilisateur)</span>
            <span class="badge badge-accent">${customRules.length} règle${customRules.length > 1 ? 's' : ''}</span>
          </div>
        </td>
      </tr>
    `;
    if (customRules.length === 0) {
      html += `
        <tr class="fw-empty-row">
          <td colspan="7" style="text-align:center; padding:16px; color:var(--subtext0); font-size:0.85rem;">
            Aucun port personnalisé ouvert. Cliquez sur <b>➕ Ouvrir un Port</b> pour autoriser un nouveau service.
          </td>
        </tr>
      `;
    } else {
      html += renderRuleRows(customRules);
    }
  }

  // 2. Groupe Système NixOS
  if (systemRules.length > 0) {
    html += `
      <tr class="fw-group-header fw-group-system">
        <td colspan="7">
          <div class="fw-group-title-collapsible" onclick="toggleSystemPortsCollapse()">
            <div class="fw-group-title-left">
              <span>🔒 Ports Système Déclaratifs (NixOS)</span>
              <span class="badge badge-success">${systemRules.length} port${systemRules.length > 1 ? 's' : ''}</span>
              <span class="fw-group-hint">${isCollapsed ? "(Cliquez pour afficher les ports)" : "(Cliquez pour réduire la liste)"}</span>
            </div>
            <button type="button" class="btn-toggle-fw-group">
              <span>${isCollapsed ? "Afficher les ports" : "Réduire"}</span>
              <span class="fw-chevron ${isCollapsed ? 'collapsed' : ''}">▼</span>
            </button>
          </div>
        </td>
      </tr>
    `;

    if (!isCollapsed) {
      html += renderRuleRows(systemRules);
    }
  }

  tbody.innerHTML = html;
}

function renderRuleRows(ruleList) {
  if (ruleList.length === 0) {
    return `<tr><td colspan="7" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun port dans cette catégorie.</td></tr>`;
  }

  return ruleList.map(r => {
    const isTcp = r.protocol === "TCP";
    const isUdp = r.protocol === "UDP";
    const isBoth = r.protocol === "BOTH";

    const protoBadgeClass = isTcp ? "proto-badge-tcp" : (isUdp ? "proto-badge-udp" : "proto-badge-both");
    const protoLabel = isBoth ? "TCP / UDP" : escapeHtml(r.protocol);

    const originBadge = r.is_system
      ? `<span class="origin-badge-system" title="Déclaré nativement par les modules NixOS"><span class="badge-dot"></span>NixOS Système</span>`
      : `<span class="origin-badge-custom" title="Règle personnalisée persistée dans firewall-rules.json"><span class="badge-dot-user"></span>Utilisateur</span>`;

    const statusBadge = r.enabled
      ? `<span class="badge badge-success">🟢 ${escapeHtml(r.status || "Autorisé")}</span>`
      : `<span class="badge badge-secondary">⏸ ${escapeHtml(r.status || "Désactivé")}</span>`;

    const categoryIcon = getCategoryIcon(r.category);

    const actionsHtml = r.is_system
      ? `<span title="Ce port est géré par la configuration déclarative NixOS" style="font-size:0.75rem; color:var(--subtext0); cursor:help;">🔒 Immuable</span>`
      : `
        <button type="button" class="btn btn-secondary btn-xs" onclick="openEditPortModal('${escapeHtml(r.id)}')" title="Modifier la règle" style="padding:4px 8px;">✏️ Modifier</button>
        <button type="button" class="btn btn-danger btn-xs" onclick="deletePortRule('${escapeHtml(r.id)}', ${r.port})" title="Supprimer la règle" style="padding:4px 8px;">🗑️</button>
      `;

    return `
      <tr class="${r.is_system ? 'row-origin-system' : 'row-origin-user'}">
        <td><strong class="port-number-cell">${r.port}</strong></td>
        <td><span class="badge ${protoBadgeClass}">${protoLabel}</span></td>
        <td>
          <div style="font-weight:600; color:var(--text);">${escapeHtml(r.label || "Service")}</div>
        </td>
        <td><span class="category-pill">${categoryIcon} ${escapeHtml(r.category || "Autre")}</span></td>
        <td>${originBadge}</td>
        <td>${statusBadge}</td>
        <td style="text-align:right;"><div class="action-btns-cell">${actionsHtml}</div></td>
      </tr>
    `;
  }).join("");
}

function getCategoryIcon(cat) {
  switch ((cat || "").toLowerCase()) {
    case "jeux": return "🎮";
    case "web": return "🌐";
    case "multimédia": return "🍿";
    case "partage": return "📁";
    case "vpn": return "🔒";
    default: return "⚙️";
  }
}

function filterFirewallPorts() {
  const input = document.getElementById("firewall-search-input");
  const query = input ? input.value.trim().toLowerCase() : "";

  let list = window.allFirewallRules || [];

  // Filtrage par pilule
  if (window.firewallFilter === "custom") {
    list = list.filter(r => !r.is_system);
  } else if (window.firewallFilter === "system") {
    list = list.filter(r => r.is_system);
  } else if (window.firewallFilter === "tcp") {
    list = list.filter(r => r.protocol === "TCP" || r.protocol === "BOTH");
  } else if (window.firewallFilter === "udp") {
    list = list.filter(r => r.protocol === "UDP" || r.protocol === "BOTH");
  }

  // Filtrage par recherche
  if (query) {
    list = list.filter(r => {
      return r.port.toString().includes(query) ||
             (r.protocol || "").toLowerCase().includes(query) ||
             (r.label || "").toLowerCase().includes(query) ||
             (r.category || "").toLowerCase().includes(query);
    });
  }

  renderFirewallPorts(list);
}

// --- Bascule Globale du Pare-feu ---
function onFirewallToggleChange(checked) {
  if (!checked) {
    // Demander confirmation sécurisée avant de désactiver
    const modal = document.getElementById("modal-firewall-confirm");
    if (modal) modal.style.display = "flex";
  } else {
    executeFirewallToggle(true);
  }
}

function cancelFirewallToggle() {
  const modal = document.getElementById("modal-firewall-confirm");
  if (modal) modal.style.display = "none";
  const toggle = document.getElementById("firewall-global-toggle");
  if (toggle) toggle.checked = true;
}

function confirmFirewallDisable() {
  const modal = document.getElementById("modal-firewall-confirm");
  if (modal) modal.style.display = "none";
  executeFirewallToggle(false);
}

async function executeFirewallToggle(enable) {
  showToast(enable ? "Activation du pare-feu NixOS..." : "Désactivation du pare-feu...", "info");

  // Persistance préemptive immédiate côté client pour éviter tout clignotement ou rebond
  try {
    localStorage.setItem("noos_firewall_state", enable ? "enabled" : "disabled");
  } catch (e) {}

  const fwToggle = document.getElementById("firewall-global-toggle");
  if (fwToggle) fwToggle.checked = enable;
  const fwToggleText = document.getElementById("firewall-toggle-text");
  if (fwToggleText) fwToggleText.textContent = enable ? "Protection Active" : "Protection Inactive";
  const heroCard = document.getElementById("firewall-hero-card");
  if (heroCard) heroCard.classList.toggle("disabled-state", !enable);
  const fwBadge = document.getElementById("firewall-status-badge");
  if (fwBadge) {
    fwBadge.textContent = enable ? "🟢 Protection Active" : "🔴 Protection Désactivée";
    fwBadge.className = `badge ${enable ? "badge-success" : "badge-danger"}`;
  }

  try {
    const res = await fetch("/api/firewall/toggle", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enable })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Statut du pare-feu mis à jour avec succès", "success");
      loadNetwork();
    } else {
      showToast("Erreur : " + (json.message || "Échec de l'opération"), "error");
      if (fwToggle) fwToggle.checked = !enable;
      try {
        localStorage.setItem("noos_firewall_state", (!enable) ? "enabled" : "disabled");
      } catch (e) {}
      loadNetwork();
    }
  } catch (e) {
    showToast("Erreur de communication : " + e, "error");
  }
}

// --- Modale d'ouverture & édition de port ---
function openCreatePortModal() {
  const modal = document.getElementById("modal-port-rule");
  if (!modal) return;

  document.getElementById("modal-port-rule-title").textContent = "Ouvrir un Port Réseau";
  document.getElementById("port-rule-id").value = "";
  document.getElementById("port-rule-number").value = "";
  document.getElementById("port-rule-label").value = "";
  document.getElementById("port-rule-category").value = "Autre";
  document.getElementById("port-rule-enabled").checked = true;
  selectPortProtocol("TCP");

  modal.style.display = "flex";
}

function openEditPortModal(ruleId) {
  const rule = (window.allFirewallRules || []).find(r => r.id === ruleId);
  if (!rule) {
    showToast("Règle introuvable", "error");
    return;
  }

  const modal = document.getElementById("modal-port-rule");
  if (!modal) return;

  document.getElementById("modal-port-rule-title").textContent = `Modifier le Port ${rule.port}`;
  document.getElementById("port-rule-id").value = rule.id;
  document.getElementById("port-rule-number").value = rule.port;
  document.getElementById("port-rule-label").value = rule.label;
  document.getElementById("port-rule-category").value = rule.category || "Autre";
  document.getElementById("port-rule-enabled").checked = !!rule.enabled;
  selectPortProtocol(rule.protocol || "TCP");

  modal.style.display = "flex";
}

function closePortRuleModal() {
  const modal = document.getElementById("modal-port-rule");
  if (modal) modal.style.display = "none";
}

function selectPortProtocol(proto) {
  document.getElementById("port-rule-protocol").value = proto;
  document.querySelectorAll("#port-protocol-segmented .btn-segment").forEach(btn => {
    btn.classList.toggle("active", btn.getAttribute("data-proto") === proto);
  });
}

function applyPortPreset(port, proto, label, cat) {
  document.getElementById("port-rule-number").value = port;
  selectPortProtocol(proto);
  document.getElementById("port-rule-label").value = label;
  document.getElementById("port-rule-category").value = cat;
  showToast(`Préréglage appliqué : ${label} (${port} ${proto})`, "info");
}

async function submitPortRule() {
  const id = document.getElementById("port-rule-id").value;
  const port = parseInt(document.getElementById("port-rule-number").value, 10);
  const protocol = document.getElementById("port-rule-protocol").value || "TCP";
  const label = document.getElementById("port-rule-label").value.trim();
  const category = document.getElementById("port-rule-category").value;
  const enabled = document.getElementById("port-rule-enabled").checked;

  if (isNaN(port) || port < 1 || port > 65535) {
    showToast("Veuillez renseigner un numéro de port valide (1 - 65535)", "warning");
    return;
  }
  if (!label) {
    showToast("Veuillez saisir un libellé pour ce port", "warning");
    return;
  }

  const isEdit = !!id;
  const url = isEdit ? `/api/firewall/rules/${encodeURIComponent(id)}` : "/api/firewall/rules";
  const method = isEdit ? "PUT" : "POST";
  const payload = isEdit
    ? { port, protocol, label, category, enabled }
    : { port, protocol, label, category };

  try {
    const res = await fetch(url, {
      method,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });
    const json = await res.json();
    if (json.success) {
      showToast(isEdit ? `Port ${port} mis à jour avec succès !` : `Port ${port} ouvert avec succès !`, "success");
      closePortRuleModal();
      loadNetwork();
    } else {
      showToast("Erreur : " + (json.message || "Impossible d'enregistrer la règle"), "error");
    }
  } catch (e) {
    showToast("Erreur lors de l'enregistrement : " + e, "error");
  }
}

async function deletePortRule(id, port) {
  if (!confirm(`Confirmer la fermeture et la suppression de la règle pour le port ${port} ?`)) {
    return;
  }

  try {
    const res = await fetch(`/api/firewall/rules/${encodeURIComponent(id)}`, {
      method: "DELETE"
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || `Port ${port} supprimé avec succès !`, "success");
      loadNetwork();
    } else {
      showToast("Erreur lors de la suppression : " + (json.message || "Échec"), "error");
    }
  } catch (e) {
    showToast("Erreur : " + e, "error");
  }
}

// =========================================================================
// 📊 SURVEILLANCE DU TRAFIC RÉSEAU EN TEMPS RÉEL
// =========================================================================

function startTrafficPolling() {
  if (window.trafficPollingTimer) return;
  fetchLiveTraffic();
  window.trafficPollingTimer = setInterval(fetchLiveTraffic, 2500);
}

function stopTrafficPolling() {
  if (window.trafficPollingTimer) {
    clearInterval(window.trafficPollingTimer);
    window.trafficPollingTimer = null;
  }
}

async function fetchLiveTraffic() {
  try {
    const res = await fetch("/api/network/traffic/live");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const data = json.data;
    window.cachedTrafficData = data;
    renderTrafficMetrics(data);
  } catch (e) {
    console.warn("Erreur fetch /api/network/traffic/live:", e);
  }
}

function selectTrafficInterface(iface) {
  window.selectedTrafficIface = iface;
  document.querySelectorAll("#traffic-iface-pills .btn-iface-pill").forEach(btn => {
    btn.classList.toggle("active", btn.textContent === iface || (iface === "all" && btn.textContent === "Toutes"));
  });
  if (window.cachedTrafficData) {
    renderTrafficMetrics(window.cachedTrafficData);
  }
}

function renderTrafficMetrics(data) {
  // Mettre à jour les pilules d'interfaces si nécessaire
  const pillsWrap = document.getElementById("traffic-iface-pills");
  if (pillsWrap && data.interfaces) {
    const currentPills = Array.from(pillsWrap.querySelectorAll(".btn-iface-pill")).map(b => b.textContent);
    const needed = ["Toutes", ...data.interfaces.map(i => i.name)];
    if (currentPills.join(",") !== needed.join(",")) {
      pillsWrap.innerHTML = needed.map(name => {
        const val = name === "Toutes" ? "all" : name;
        const isActive = window.selectedTrafficIface === val;
        return `<button type="button" class="btn-iface-pill ${isActive ? "active" : ""}" onclick="selectTrafficInterface('${val}')">${escapeHtml(name)}</button>`;
      }).join("");
    }
  }

  let rxSpeed = data.total_rx_speed_human;
  let txSpeed = data.total_tx_speed_human;
  let rxBytesSec = data.total_rx_sec;
  let txBytesSec = data.total_tx_sec;
  let rxTotalHuman = "--";
  let txTotalHuman = "--";

  if (window.selectedTrafficIface !== "all") {
    const target = (data.interfaces || []).find(i => i.name === window.selectedTrafficIface);
    if (target) {
      rxSpeed = target.rx_speed_human;
      txSpeed = target.tx_speed_human;
      rxBytesSec = target.rx_bytes_sec;
      txBytesSec = target.tx_bytes_sec;
      rxTotalHuman = target.total_rx_human;
      txTotalHuman = target.total_tx_human;
    }
  } else if (data.interfaces && data.interfaces.length > 0) {
    let totRx = 0;
    let totTx = 0;
    data.interfaces.forEach(i => {
      totRx += (i.total_rx_bytes || 0);
      totTx += (i.total_tx_bytes || 0);
    });
    rxTotalHuman = formatBytesJs(totRx);
    txTotalHuman = formatBytesJs(totTx);
  }

  // Mettre à jour les compteurs
  const rxSpeedEl = document.getElementById("traffic-rx-speed");
  if (rxSpeedEl) rxSpeedEl.textContent = rxSpeed;

  const txSpeedEl = document.getElementById("traffic-tx-speed");
  if (txSpeedEl) txSpeedEl.textContent = txSpeed;

  const rxTotalEl = document.getElementById("traffic-rx-total");
  if (rxTotalEl) rxTotalEl.textContent = rxTotalHuman;

  const txTotalEl = document.getElementById("traffic-tx-total");
  if (txTotalEl) txTotalEl.textContent = txTotalHuman;

  // Jauges visuelles (échelle relative max 50 Mo/s)
  const maxRef = 50 * 1024 * 1024;
  const rxPct = Math.min(100, Math.max(3, Math.round((rxBytesSec / maxRef) * 100)));
  const txPct = Math.min(100, Math.max(3, Math.round((txBytesSec / maxRef) * 100)));

  const rxMeter = document.getElementById("traffic-rx-meter");
  if (rxMeter) rxMeter.style.width = `${rxPct}%`;

  const txMeter = document.getElementById("traffic-tx-meter");
  if (txMeter) txMeter.style.width = `${txPct}%`;
}

function formatBytesJs(bytes) {
  return formatBytes(bytes);
}

async function loadTrafficHistory() {
  try {
    const res = await fetch("/api/network/traffic/history");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const hist = json.data;
    const todayEl = document.getElementById("traffic-today-total");
    if (todayEl) todayEl.textContent = hist.today_total_human || "--";

    const todayDetailEl = document.getElementById("traffic-today-detail");
    if (todayDetailEl) todayDetailEl.textContent = `(RX: ${hist.today_rx_human || "--"} | TX: ${hist.today_tx_human || "--"})`;

    const monthEl = document.getElementById("traffic-month-total");
    if (monthEl) monthEl.textContent = hist.month_total_human || "--";

    const monthDetailEl = document.getElementById("traffic-month-detail");
    if (monthDetailEl) monthDetailEl.textContent = `(RX: ${hist.month_rx_human || "--"} | TX: ${hist.month_tx_human || "--"})`;

    const vnstatBadge = document.getElementById("traffic-vnstat-badge");
    if (vnstatBadge) {
      vnstatBadge.textContent = hist.has_vnstat ? "Actif (vnStat)" : "Noyau Linux";
      vnstatBadge.className = `badge ${hist.has_vnstat ? "badge-success" : "badge-secondary"}`;
    }
  } catch (e) {
    console.warn("Erreur fetch /api/network/traffic/history:", e);
  }
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


// ================= GESTION DES PROFILS CLIENTS WIREGUARD =================

let cachedWgClients = [];
let currentViewingWgClient = null;

function renderWireguardClients(clients, tbody) {
  if (!tbody) tbody = document.getElementById("wg-clients-tbody");
  if (!tbody) return;

  if (!clients || clients.length === 0) {
    tbody.innerHTML = `
      <tr>
        <td colspan="5" style="text-align:center; color:var(--subtext0); padding:30px;">
          <div style="font-size:1.8rem; margin-bottom:8px;">🔒</div>
          <div style="font-weight:600; color:var(--text); margin-bottom:4px;">Aucun profil client WireGuard créé</div>
          <div style="font-size:0.82rem; margin-bottom:14px;">Générez un profil pour votre smartphone, PC portable ou tablette pour accéder au NAS en toute sécurité.</div>
          <button type="button" class="btn btn-primary btn-sm" onclick="openCreateWgClientModal()">
            <span>➕</span> Créer le premier profil
          </button>
        </td>
      </tr>
    `;
    return;
  }

  tbody.innerHTML = clients.map(c => {
    const dateStr = c.created_at ? new Date(c.created_at * 1000).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" }) : "—";
    return `
      <tr>
        <td>
          <div style="font-weight:600; color:var(--text);">${escapeHtml(c.username)}</div>
          <div style="font-size:0.75rem; color:var(--subtext0);">ID: ${escapeHtml(c.id)}</div>
        </td>
        <td>
          <span class="badge badge-info" style="font-family:var(--font-mono);">${escapeHtml(c.client_ip)}/32</span>
        </td>
        <td>
          <code style="font-size:0.75rem; color:var(--teal); background:rgba(0,0,0,0.2); padding:3px 6px; border-radius:4px;" title="${escapeHtml(c.public_key)}">
            ${escapeHtml(c.public_key.substring(0, 16))}...
          </code>
        </td>
        <td style="font-size:0.82rem; color:var(--subtext1);">${dateStr}</td>
        <td style="text-align:right;">
          <div style="display:inline-flex; gap:6px; flex-wrap:wrap; justify-content:flex-end;">
            <button type="button" class="btn btn-secondary btn-xs" onclick="openViewWgClientModal('${c.id}')" title="Afficher le QR code et la configuration">
              <span>📱</span> QR Code &amp; Config
            </button>
            <button type="button" class="btn btn-secondary btn-xs" onclick="downloadWgClientConfig('${c.id}')" title="Télécharger le fichier .conf">
              <span>📥</span> .conf
            </button>
            <button type="button" class="btn btn-secondary btn-xs" onclick="copyWgClientConfigById('${c.id}')" title="Copier la configuration dans le presse-papiers">
              <span>📋</span> Copier
            </button>
            <button type="button" class="btn btn-danger btn-xs" onclick="deleteWgClient('${c.id}', '${escapeHtml(c.username)}')" title="Révoquer l'accès de cet appareil">
              <span>🗑️</span>
            </button>
          </div>
        </td>
      </tr>
    `;
  }).join("");
}

async function loadWireguardClients() {
  const tbody = document.getElementById("wg-clients-tbody");
  if (!tbody) return;

  // 1. Rendu optimiste immédiat depuis le cache local (0 ms dès le chargement de la page)
  if (!cachedWgClients || cachedWgClients.length === 0) {
    try {
      const saved = localStorage.getItem("noos_cached_wg_clients");
      if (saved) {
        const parsed = JSON.parse(saved);
        if (Array.isArray(parsed) && parsed.length > 0) {
          cachedWgClients = parsed;
          renderWireguardClients(cachedWgClients, tbody);
        }
      }
    } catch (e) {}
  }

  // 2. Récupération fraîche en arrière-plan
  try {
    const res = await fetch("/api/wireguard/clients");
    const json = await res.json();
    if (!json.success || !json.data) {
      if (!cachedWgClients || cachedWgClients.length === 0) {
        tbody.innerHTML = `<tr><td colspan="5" style="text-align:center; color:var(--red); padding:16px;">Impossible de charger les profils : ${escapeHtml(json.message || "Erreur serveur")}</td></tr>`;
      }
      return;
    }

    cachedWgClients = json.data;
    try {
      localStorage.setItem("noos_cached_wg_clients", JSON.stringify(cachedWgClients));
    } catch (e) {}

    renderWireguardClients(cachedWgClients, tbody);

  } catch (err) {
    if (!cachedWgClients || cachedWgClients.length === 0) {
      tbody.innerHTML = `<tr><td colspan="5" style="text-align:center; color:var(--red); padding:16px;">Erreur de connexion : ${escapeHtml(err.message)}</td></tr>`;
    }
  }
}

async function openCreateWgClientModal() {
  const modal = document.getElementById("modal-create-wg-client");
  if (!modal) return;

  const usernameInput = document.getElementById("wg-input-username");
  const ipInput = document.getElementById("wg-input-ip");
  const allowedIpsInput = document.getElementById("wg-input-allowed-ips");
  const dnsInput = document.getElementById("wg-input-dns");
  const endpointInput = document.getElementById("wg-input-endpoint");

  if (usernameInput) usernameInput.value = "";
  if (allowedIpsInput) allowedIpsInput.value = "10.100.0.1/32"; // Règle stricte machine hôte uniquement
  if (dnsInput) dnsInput.value = "1.1.1.1, 8.8.8.8";

  try {
    const res = await fetch("/api/wireguard/server");
    const json = await res.json();
    if (json.success && json.data) {
      if (ipInput) ipInput.value = json.data.next_client_ip || "10.100.0.2";
      if (endpointInput) endpointInput.value = json.data.endpoint || "";
    }
  } catch (e) {
    if (ipInput) ipInput.value = "10.100.0.2";
  }

  modal.style.display = "flex";
  if (usernameInput) setTimeout(() => usernameInput.focus(), 50);
}

function closeCreateWgClientModal() {
  const modal = document.getElementById("modal-create-wg-client");
  if (modal) modal.style.display = "none";
}

async function submitCreateWgClient() {
  const usernameInput = document.getElementById("wg-input-username");
  const ipInput = document.getElementById("wg-input-ip");
  const allowedIpsInput = document.getElementById("wg-input-allowed-ips");
  const dnsInput = document.getElementById("wg-input-dns");
  const endpointInput = document.getElementById("wg-input-endpoint");
  const submitBtn = document.getElementById("btn-submit-create-wg");

  const username = usernameInput ? usernameInput.value.trim() : "";
  if (!username) {
    showToast("Veuillez saisir un nom d'utilisateur ou d'appareil.", "warning");
    if (usernameInput) usernameInput.focus();
    return;
  }

  const payload = {
    username: username,
    client_ip: ipInput && ipInput.value.trim() ? ipInput.value.trim() : null,
    allowed_ips: allowedIpsInput && allowedIpsInput.value.trim() ? allowedIpsInput.value.trim() : "10.100.0.1/32",
    dns: dnsInput && dnsInput.value.trim() ? dnsInput.value.trim() : null,
    endpoint: endpointInput && endpointInput.value.trim() ? endpointInput.value.trim() : null,
  };

  try {
    if (submitBtn) {
      submitBtn.disabled = true;
      submitBtn.textContent = "Génération en cours...";
    }

    const res = await fetch("/api/wireguard/clients", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    const json = await res.json();
    if (!json.success || !json.data) {
      showToast("Échec : " + (json.message || "Erreur lors de la génération"), "error");
      return;
    }

    showToast(`Profil WireGuard créé pour '${username}' !`, "success");
    closeCreateWgClientModal();
    await loadWireguardClients();

    // Ouvrir immédiatement la modale d'affichage / QR Code pour le nouvel utilisateur !
    openViewWgClientModal(json.data.id);

  } catch (err) {
    showToast("Erreur réseau : " + err.message, "error");
  } finally {
    if (submitBtn) {
      submitBtn.disabled = false;
      submitBtn.textContent = "✨ Générer le Profil Client";
    }
  }
}

function cleanWireguardConfigForQr(text) {
  if (!text) return "";
  return text
    .split("\n")
    .map(line => line.trim())
    .filter(line => line && !line.startsWith("#"))
    .join("\n");
}

function openViewWgClientModal(clientId) {
  const client = cachedWgClients.find(c => c.id === clientId);
  if (!client) {
    showToast("Profil client introuvable.", "error");
    return;
  }

  currentViewingWgClient = client;

  const modal = document.getElementById("modal-view-wg-client");
  const usernameEl = document.getElementById("wg-view-modal-username");
  const ipEl = document.getElementById("wg-view-modal-ip");
  const configEl = document.getElementById("wg-view-modal-config");
  const qrContainer = document.getElementById("wg-qrcode-canvas");

  if (usernameEl) usernameEl.textContent = client.username;
  if (ipEl) ipEl.textContent = client.client_ip + "/32";
  if (configEl) configEl.textContent = client.config_text;

  // Nettoyage de la configuration pour un QR Code haute lisibilité (syntaxe INI pure sans commentaires)
  const qrPayload = cleanWireguardConfigForQr(client.config_text);

  // Rendu du QR Code
  if (qrContainer && typeof QRCode !== "undefined") {
    qrContainer.innerHTML = "";
    try {
      new QRCode(qrContainer, {
        text: qrPayload,
        width: 210,
        height: 210,
        colorDark: "#000000",
        colorLight: "#ffffff",
        correctLevel: QRCode.CorrectLevel.M
      });
    } catch (e) {
      console.error("Erreur génération QR Code:", e);
      qrContainer.innerHTML = `<span style="color:var(--red); font-size:0.75rem;">Erreur QR Code</span>`;
    }
  }

  if (modal) modal.style.display = "flex";
}

function closeViewWgClientModal() {
  const modal = document.getElementById("modal-view-wg-client");
  if (modal) modal.style.display = "none";
}

function copyCurrentWgConfig() {
  if (!currentViewingWgClient) return;
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(currentViewingWgClient.config_text).then(() => {
      showToast(`Configuration de '${currentViewingWgClient.username}' copiée !`, "success");
    }).catch(() => {
      prompt("Copiez manuellement la configuration :", currentViewingWgClient.config_text);
    });
  } else {
    prompt("Copiez manuellement la configuration :", currentViewingWgClient.config_text);
  }
}

function copyWgClientConfigById(clientId) {
  const client = cachedWgClients.find(c => c.id === clientId);
  if (!client) return;
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(client.config_text).then(() => {
      showToast(`Configuration de '${client.username}' copiée !`, "success");
    }).catch(() => {
      prompt("Copiez manuellement la configuration :", client.config_text);
    });
  } else {
    prompt("Copiez manuellement la configuration :", client.config_text);
  }
}

function downloadCurrentWgConfig() {
  if (!currentViewingWgClient) return;
  downloadConfigFile(currentViewingWgClient.username, currentViewingWgClient.config_text);
}

function downloadWgClientConfig(clientId) {
  const client = cachedWgClients.find(c => c.id === clientId);
  if (!client) return;
  downloadConfigFile(client.username, client.config_text);
}

function downloadConfigFile(username, content) {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${username}-wg0.conf`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
  showToast(`Fichier ${username}-wg0.conf téléchargé !`, "success");
}

async function deleteWgClient(clientId, username) {
  if (!confirm(`Voulez-vous vraiment révoquer et supprimer l'accès WireGuard pour '${username}' ?\n\nCet appareil ne pourra plus se connecter au NAS.`)) {
    return;
  }

  try {
    const res = await fetch(`/api/wireguard/clients/${encodeURIComponent(clientId)}`, {
      method: "DELETE"
    });
    const json = await res.json();
    if (json.success) {
      showToast(`Profil de '${username}' révoqué avec succès.`, "success");
      await loadWireguardClients();
    } else {
      showToast("Échec : " + (json.message || "Erreur de suppression"), "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err.message, "error");
  }
}


// =========================================================================
// 💻 MODULE MACHINES VIRTUELLES (KVM / QEMU)
// =========================================================================

let cachedVms = [];
let cachedIsos = [];
let cachedGpus = [];
let isoPollInterval = null;
let activeConsoleVm = null;

async function loadVms(showToastFeedback = false) {
  try {
    const res = await fetch("/api/vms");
    const json = await res.json();
    if (!json.success || !json.data) return;

    cachedVms = json.data;
    renderVmsOverview(cachedVms);

    if (showToastFeedback) {
      showToast("État des machines virtuelles actualisé !", "success");
    }
  } catch (err) {
    console.error("Erreur chargement VMs :", err);
  }
}

function renderVmsOverview(vms) {
  // 1. Calcul des statistiques globales
  const runningCount = vms.filter(v => v.state === "running").length;
  const totalVcpus = vms.reduce((acc, v) => acc + (v.vcpus || 0), 0);
  const totalRamMb = vms.reduce((acc, v) => acc + (v.memory_mb || 0), 0);
  const totalDiskGb = vms.reduce((acc, v) => acc + (v.disk_size_gb || 0), 0);

  const statRunning = document.getElementById("vms-stat-running");
  if (statRunning) statRunning.textContent = runningCount;

  const statVcpus = document.getElementById("vms-stat-vcpus");
  if (statVcpus) statVcpus.textContent = totalVcpus;

  const statRam = document.getElementById("vms-stat-ram");
  if (statRam) {
    statRam.textContent = totalRamMb >= 1024 
      ? (totalRamMb / 1024).toFixed(1) + " Go" 
      : totalRamMb + " Mo";
  }

  const statDisk = document.getElementById("vms-stat-disk");
  if (statDisk) statDisk.textContent = totalDiskGb.toFixed(1) + " Go";

  // Badge navigation
  const badge = document.getElementById("vms-count-badge");
  if (badge) {
    if (runningCount > 0) {
      badge.textContent = runningCount;
      badge.style.display = "inline-block";
    } else {
      badge.style.display = "none";
    }
  }

  // 2. Rendu des cartes de machines virtuelles
  const grid = document.getElementById("vms-cards-grid");
  if (!grid) return;

  if (vms.length === 0) {
    grid.innerHTML = `
      <div style="grid-column: 1 / -1; text-align:center; padding: 48px 20px; background:var(--surface0); border-radius:var(--radius-lg); border:1px dashed rgba(255,255,255,0.1);">
        <div style="font-size:2.8rem; margin-bottom:12px;">💻</div>
        <h3 style="margin:0 0 8px 0; color:var(--text);">Aucune Machine Virtuelle Détectée</h3>
        <p style="color:var(--subtext0); max-width:480px; margin:0 auto 20px auto; font-size:0.9rem;">
          Créez votre première machine virtuelle en quelques clics (Linux, Windows 11, BSD) avec firmware UEFI et émulation TPM 2.0.
        </p>
        <button type="button" class="btn btn-primary btn-sm" onclick="showCreateVmModal()">
          ✨ Déployer une Machine Virtuelle
        </button>
      </div>
    `;
    return;
  }

  grid.innerHTML = vms.map(vm => {
    const isRunning = vm.state === "running";
    const isPaused = vm.state === "paused";

    let statusBadge = '';
    if (isRunning) {
      statusBadge = '<span class="badge" style="background:rgba(166,227,161,0.18); color:var(--green); border:1px solid rgba(166,227,161,0.3); padding:4px 8px; border-radius:6px; font-size:0.75rem; font-weight:700;">🟢 En ligne</span>';
    } else if (isPaused) {
      statusBadge = '<span class="badge" style="background:rgba(249,226,175,0.18); color:var(--yellow); border:1px solid rgba(249,226,175,0.3); padding:4px 8px; border-radius:6px; font-size:0.75rem; font-weight:700;">🟡 En pause</span>';
    } else {
      statusBadge = '<span class="badge" style="background:rgba(255,255,255,0.08); color:var(--subtext0); border:1px solid rgba(255,255,255,0.1); padding:4px 8px; border-radius:6px; font-size:0.75rem; font-weight:600;">⚪ Éteinte</span>';
    }

    let osIcon = '🐧';
    if (vm.os_type === 'windows' || vm.name.toLowerCase().includes('win')) {
      osIcon = '🪟';
    } else if (vm.os_type === 'other') {
      osIcon = '📦';
    }

    const netLabel = vm.network_type === 'bridge' ? 'Pont LAN (br0)' : 'NAT Isolé (virbr0)';

    return `
      <div class="card vm-card" style="background:var(--surface0); border:1px solid rgba(255,255,255,0.08); border-radius:var(--radius-lg); padding:20px; display:flex; flex-direction:column; justify-content:space-between; box-shadow:0 8px 24px rgba(0,0,0,0.25); transition:transform 0.15s ease, border-color 0.15s ease;">
        <div>
          <!-- Header de la carte -->
          <div style="display:flex; justify-content:space-between; align-items:flex-start; margin-bottom:14px;">
            <div style="display:flex; align-items:center; gap:12px;">
              <div style="font-size:2rem; width:46px; height:46px; background:var(--mantle); border:1px solid rgba(255,255,255,0.08); border-radius:12px; display:flex; align-items:center; justify-content:center;">
                ${osIcon}
              </div>
              <div>
                <h3 style="margin:0; font-size:1.05rem; font-weight:700; color:var(--text);">${escapeHtml(vm.name)}</h3>
                <div style="font-size:0.78rem; color:var(--subtext0); margin-top:2px;">${escapeHtml(netLabel)}</div>
              </div>
            </div>
            <div>${statusBadge}</div>
          </div>

          <!-- Spécifications matérielles -->
          <div style="display:grid; grid-template-columns: 1fr 1fr; gap:8px; background:var(--mantle); border-radius:8px; padding:12px; margin-bottom:16px; font-size:0.8rem;">
            <div>
              <span style="color:var(--subtext0);">Processeurs : </span>
              <strong style="color:var(--text);">${vm.vcpus} vCPU</strong>
            </div>
            <div>
              <span style="color:var(--subtext0);">Mémoire : </span>
              <strong style="color:var(--mauve);">${(vm.memory_mb / 1024).toFixed(1)} Go</strong>
            </div>
            <div>
              <span style="color:var(--subtext0);">Stockage : </span>
              <strong style="color:var(--peach);">${vm.disk_size_gb.toFixed(1)} Go</strong>
            </div>
            <div>
              <span style="color:var(--subtext0);">Console : </span>
              <strong style="color:var(--teal);">${vm.vnc_port ? 'Port ' + vm.vnc_port : 'Inactive'}</strong>
            </div>
          </div>

          ${vm.gpu_passthrough ? `
            <div style="background:rgba(203,166,247,0.12); border:1px solid rgba(203,166,247,0.25); border-radius:6px; padding:6px 10px; font-size:0.75rem; color:var(--mauve); margin-bottom:14px; display:flex; align-items:center; gap:6px;">
              <span>🎮</span> <strong>GPU Passthrough actif</strong>
            </div>
          ` : ''}
        </div>

        <!-- Boutons d'action -->
        <div style="display:flex; flex-direction:column; gap:8px; border-top:1px solid rgba(255,255,255,0.06); padding-top:14px;">
          ${isRunning ? `
            <button type="button" class="btn btn-primary btn-sm" style="width:100%; font-weight:700;" onclick="openVmConsole('${escapeHtml(vm.name)}')">
              <span>🖥️</span> Ouvrir la Console Web noVNC
            </button>
            <div style="display:grid; grid-template-columns: 1fr 1fr 1fr; gap:6px;">
              <button type="button" class="btn btn-secondary btn-xs" onclick="vmAction('${escapeHtml(vm.name)}', 'pause')" title="Mettre en pause">
                ⏸ Pause
              </button>
              <button type="button" class="btn btn-secondary btn-xs" onclick="vmAction('${escapeHtml(vm.name)}', 'shutdown')" title="Arrêt propre ACPI">
                ⏹ Éteindre
              </button>
              <button type="button" class="btn btn-secondary btn-xs" onclick="vmAction('${escapeHtml(vm.name)}', 'destroy')" title="Arrêt forcé immédiat">
                ⚡ Forcer Off
              </button>
            </div>
          ` : isPaused ? `
            <button type="button" class="btn btn-primary btn-sm" style="width:100%;" onclick="vmAction('${escapeHtml(vm.name)}', 'resume')">
              <span>▶</span> Reprendre la Machine
            </button>
            <button type="button" class="btn btn-secondary btn-xs" onclick="vmAction('${escapeHtml(vm.name)}', 'destroy')" title="Arrêt forcé">
              ⚡ Forcer l'Arrêt
            </button>
          ` : `
            <button type="button" class="btn btn-primary btn-sm" style="width:100%;" onclick="vmAction('${escapeHtml(vm.name)}', 'start')">
              <span>▶</span> Démarrer la Machine
            </button>
            <div style="display:flex; justify-content:flex-end;">
              <button type="button" class="btn btn-secondary btn-xs" style="color:var(--red);" onclick="deleteVm('${escapeHtml(vm.name)}')">
                <span>🗑</span> Supprimer la VM
              </button>
            </div>
          `}
        </div>
      </div>
    `;
  }).join('');
}

async function vmAction(vmName, action) {
  try {
    const res = await fetch(`/api/vms/${encodeURIComponent(vmName)}/action`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || `Action '${action}' réussie sur ${vmName}`, "success");
      await loadVms();
    } else {
      showToast("Échec de l'action : " + (json.message || "Erreur inconnue"), "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err.message, "error");
  }
}

async function deleteVm(vmName) {
  if (!confirm(`Attention : Voulez-vous vraiment détruire et supprimer la machine virtuelle '${vmName}' ainsi que son disque dur virtuel ?\n\nCette action est irréversible.`)) {
    return;
  }
  await vmAction(vmName, 'delete');
}

// --------------------------------------------------------------------------
// MODALE CRÉATION MACHINE VIRTUELLE
// --------------------------------------------------------------------------

async function showCreateVmModal() {
  const modal = document.getElementById("modal-create-vm");
  if (!modal) return;

  // Réinitialisation des inputs
  document.getElementById("create-vm-name").value = "";
  document.getElementById("create-vm-vcpus").value = 2;
  document.getElementById("create-vm-vcpus-range").value = 2;
  document.getElementById("create-vm-ram").value = 2048;
  document.getElementById("create-vm-ram-range").value = 2048;
  document.getElementById("create-vm-ram-human").textContent = "2.0 Go";
  document.getElementById("create-vm-disk").value = 25;
  document.getElementById("create-vm-disk-range").value = 25;
  document.getElementById("create-vm-uefi").checked = true;
  document.getElementById("create-vm-tpm").checked = false;

  modal.style.display = "flex";

  // Charger la liste des ISOs
  try {
    const res = await fetch("/api/vms/isos");
    const json = await res.json();
    const select = document.getElementById("create-vm-iso-select");
    if (select) {
      if (json.success && json.data && json.data.length > 0) {
        cachedIsos = json.data;
        select.innerHTML = json.data.map(iso => 
          `<option value="${escapeHtml(iso.path)}">${escapeHtml(iso.name)} (${iso.size_human})</option>`
        ).join('');
      } else {
        select.innerHTML = '<option value="">Aucune ISO trouvée dans /mnt/storage/isos</option>';
      }
    }
  } catch (e) {
    console.error("Erreur chargement ISOs :", e);
  }

  // Charger la liste des GPUs
  try {
    const res = await fetch("/api/vms/gpus");
    const json = await res.json();
    const select = document.getElementById("create-vm-gpu-select");
    if (select) {
      select.innerHTML = '<option value="">Émulation Standard VirtIO / QXL (Console Web VNC)</option>';
      if (json.success && json.data && json.data.length > 0) {
        cachedGpus = json.data;
        json.data.forEach(gpu => {
          const opt = document.createElement("option");
          opt.value = gpu.pci_address;
          opt.textContent = `🎮 ${gpu.name} (${gpu.pci_address}${gpu.iommu_group !== null ? ' - IOMMU Grp ' + gpu.iommu_group : ''})`;
          select.appendChild(opt);
        });
      }
    }
  } catch (e) {
    console.error("Erreur chargement GPUs :", e);
  }

  onVmGpuChange();
}

function closeCreateVmModal() {
  const modal = document.getElementById("modal-create-vm");
  if (modal) modal.style.display = "none";
}

function onVmOsChange(osType) {
  const uefiCheck = document.getElementById("create-vm-uefi");
  const tpmCheck = document.getElementById("create-vm-tpm");
  const ramInput = document.getElementById("create-vm-ram");
  const ramRange = document.getElementById("create-vm-ram-range");
  const ramHuman = document.getElementById("create-vm-ram-human");
  const diskInput = document.getElementById("create-vm-disk");
  const diskRange = document.getElementById("create-vm-disk-range");

  if (osType === 'windows') {
    if (uefiCheck) uefiCheck.checked = true;
    if (tpmCheck) tpmCheck.checked = true;
    if (ramInput && ramRange) {
      ramInput.value = 4096;
      ramRange.value = 4096;
      if (ramHuman) ramHuman.textContent = "4.0 Go";
    }
    if (diskInput && diskRange) {
      diskInput.value = 60;
      diskRange.value = 60;
    }
  } else {
    if (tpmCheck) tpmCheck.checked = false;
    if (ramInput && ramRange) {
      ramInput.value = 2048;
      ramRange.value = 2048;
      if (ramHuman) ramHuman.textContent = "2.0 Go";
    }
    if (diskInput && diskRange) {
      diskInput.value = 25;
      diskRange.value = 25;
    }
  }
}

function onVmGpuChange() {
  const select = document.getElementById("create-vm-gpu-select");
  const banner = document.getElementById("vm-gpu-conflict-banner");
  const text = document.getElementById("vm-gpu-conflict-text");
  if (!select || !banner || !text) return;

  const chosenPci = select.value;
  if (!chosenPci) {
    banner.style.display = "none";
    return;
  }

  const gpu = cachedGpus.find(g => g.pci_address === chosenPci);
  if (gpu && gpu.conflict_warning) {
    text.textContent = gpu.conflict_warning;
    banner.style.display = "block";
  } else {
    banner.style.display = "none";
  }
}

function toggleIsoSourceChoice(source) {
  const localDiv = document.getElementById("vm-iso-local-choice");
  const urlDiv = document.getElementById("vm-iso-url-choice");
  if (localDiv && urlDiv) {
    localDiv.style.display = source === 'local' ? 'block' : 'none';
    urlDiv.style.display = source === 'url' ? 'block' : 'none';
  }
}

function quickFillIsoUrl(preset) {
  const input = document.getElementById("create-vm-iso-url");
  if (!input) return;

  if (preset === 'ubuntu') {
    input.value = "https://releases.ubuntu.com/24.04/ubuntu-24.04-live-server-amd64.iso";
  } else if (preset === 'debian') {
    input.value = "https://cdimage.debian.org/debian-cd/current/amd64/iso-cd/debian-12.7.0-amd64-netinst.iso";
  } else if (preset === 'alpine') {
    input.value = "https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/x86_64/alpine-standard-3.20.3-x86_64.iso";
  }
}

async function submitCreateVm() {
  const name = document.getElementById("create-vm-name").value.trim();
  if (!name) {
    showToast("Veuillez saisir un nom pour la machine virtuelle.", "error");
    return;
  }

  const vcpus = parseInt(document.getElementById("create-vm-vcpus").value, 10) || 2;
  const memory_mb = parseInt(document.getElementById("create-vm-ram").value, 10) || 2048;
  const disk_size_gb = parseInt(document.getElementById("create-vm-disk").value, 10) || 25;
  const os_type = document.getElementById("create-vm-os").value;

  const isoSource = document.querySelector('input[name="vm-iso-source"]:checked')?.value || 'local';
  let iso_path = null;

  if (isoSource === 'local') {
    iso_path = document.getElementById("create-vm-iso-select").value || null;
  } else if (isoSource === 'url') {
    const url = document.getElementById("create-vm-iso-url").value.trim();
    if (url) {
      showToast("Lancement du téléchargement de l'image ISO...", "info");
      try {
        await fetch("/api/vms/isos/download", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ url })
        });
      } catch (e) {}
    }
  }

  const network_type = document.querySelector('input[name="vm-network-type"]:checked')?.value || 'nat';
  const gpu_pci = document.getElementById("create-vm-gpu-select").value || null;
  const enable_uefi = document.getElementById("create-vm-uefi").checked;
  const enable_tpm = document.getElementById("create-vm-tpm").checked;

  const btn = document.getElementById("btn-submit-create-vm");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "⏳ Création en cours...";
  }

  try {
    const res = await fetch("/api/vms", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name,
        vcpus,
        memory_mb,
        disk_size_gb,
        os_type,
        iso_path,
        network_type,
        gpu_pci,
        enable_uefi,
        enable_tpm,
      })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || "Machine virtuelle créée avec succès !", "success");
      closeCreateVmModal();
      await loadVms();
    } else {
      showToast("Échec : " + (json.message || "Erreur lors de la création"), "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err.message, "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "✨ Déployer la Machine Virtuelle";
    }
  }
}

// --------------------------------------------------------------------------
// CONSOLE WEB NOVNC
// --------------------------------------------------------------------------

function openVmConsole(vmName) {
  activeConsoleVm = vmName;
  const modal = document.getElementById("modal-vm-console");
  const title = document.getElementById("vnc-modal-vm-name");
  const iframe = document.getElementById("vm-console-iframe");
  if (!modal || !iframe) return;

  if (title) title.textContent = vmName;

  const vncUrl = `/novnc/vnc.html?path=api/vms/${encodeURIComponent(vmName)}/vnc&autoconnect=true&resize=scale&show_dot=true`;
  iframe.src = vncUrl;
  modal.style.display = "flex";
}

function closeVmConsole() {
  const modal = document.getElementById("modal-vm-console");
  const iframe = document.getElementById("vm-console-iframe");
  if (iframe) iframe.src = "about:blank";
  if (modal) modal.style.display = "none";
  activeConsoleVm = null;
}

function sendConsoleCtrlAltDel() {
  const iframe = document.getElementById("vm-console-iframe");
  if (iframe && iframe.contentWindow) {
    iframe.contentWindow.postMessage({ action: 'ctrlaltdel' }, '*');
    showToast("Commande Ctrl+Alt+Suppr envoyée à la VM", "info");
  }
}

function toggleConsoleFullscreen() {
  const modal = document.getElementById("modal-vm-console");
  if (!modal) return;

  if (!document.fullscreenElement) {
    modal.requestFullscreen().catch(err => {
      console.warn("Fullscreen error:", err);
    });
  } else {
    document.exitFullscreen().catch(err => {
      console.warn("Exit fullscreen error:", err);
    });
  }
}

async function triggerConsolePowerAction(action) {
  if (!activeConsoleVm) return;
  await vmAction(activeConsoleVm, action);
}

// --------------------------------------------------------------------------
// GESTIONNAIRE D'IMAGES ISO
// --------------------------------------------------------------------------

async function showIsoManagerModal() {
  const modal = document.getElementById("modal-iso-manager");
  if (!modal) return;

  modal.style.display = "flex";
  await refreshIsoManager();

  if (isoPollInterval) clearInterval(isoPollInterval);
  isoPollInterval = setInterval(refreshIsoDownloadsOnly, 2000);
}

function closeIsoManagerModal() {
  const modal = document.getElementById("modal-iso-manager");
  if (modal) modal.style.display = "none";
  if (isoPollInterval) {
    clearInterval(isoPollInterval);
    isoPollInterval = null;
  }
}

async function refreshIsoManager() {
  try {
    const [resIsos, resDl] = await Promise.all([
      fetch("/api/vms/isos"),
      fetch("/api/vms/isos/downloads")
    ]);

    const jsonIsos = await resIsos.json();
    const jsonDl = await resDl.json();

    renderStoredIsos(jsonIsos.data || []);
    renderActiveIsoDownloads(jsonDl.data || []);
  } catch (e) {
    console.error("Erreur actualisation ISOs :", e);
  }
}

async function refreshIsoDownloadsOnly() {
  try {
    const res = await fetch("/api/vms/isos/downloads");
    const json = await res.json();
    renderActiveIsoDownloads(json.data || []);
  } catch (e) {}
}

function renderStoredIsos(isos) {
  const container = document.getElementById("iso-stored-list");
  if (!container) return;

  if (isos.length === 0) {
    container.innerHTML = '<div style="padding:16px; text-align:center; color:var(--subtext0); font-size:0.85rem;">Aucune image ISO présente dans /mnt/storage/isos. Téléchargez-en une ci-dessus !</div>';
    return;
  }

  container.innerHTML = isos.map(iso => `
    <div style="display:flex; justify-content:space-between; align-items:center; padding:10px 14px; border-bottom:1px solid rgba(255,255,255,0.06); font-size:0.84rem;">
      <div style="display:flex; align-items:center; gap:8px;">
        <span>💿</span>
        <div>
          <strong style="color:var(--text);">${escapeHtml(iso.name)}</strong>
          <div style="font-size:0.75rem; color:var(--subtext0);">Taille : ${iso.size_human}</div>
        </div>
      </div>
      <span class="badge" style="background:rgba(166,227,161,0.15); color:var(--green); font-size:0.75rem; padding:3px 8px; border-radius:4px;">Prête</span>
    </div>
  `).join('');
}

function renderActiveIsoDownloads(downloads) {
  const wrap = document.getElementById("iso-active-downloads-wrap");
  const list = document.getElementById("iso-active-downloads-list");
  if (!wrap || !list) return;

  const active = downloads.filter(d => d.status === 'downloading');
  if (active.length === 0) {
    wrap.style.display = "none";
    return;
  }

  wrap.style.display = "block";
  list.innerHTML = active.map(dl => `
    <div style="background:var(--mantle); border:1px solid rgba(137,180,250,0.3); border-radius:8px; padding:12px;">
      <div style="display:flex; justify-content:space-between; align-items:center; font-size:0.84rem; margin-bottom:6px;">
        <strong style="color:var(--text);">${escapeHtml(dl.filename)}</strong>
        <span style="color:var(--blue); font-weight:700;">${dl.progress.toFixed(1)}% (${dl.speed_mbps} Mo/s)</span>
      </div>
      <div style="height:6px; background:rgba(255,255,255,0.1); border-radius:3px; overflow:hidden;">
        <div style="height:100%; width:${Math.min(dl.progress, 100)}%; background:var(--blue); transition:width 0.3s ease;"></div>
      </div>
    </div>
  `).join('');
}

async function startIsoDownloadFromModal() {
  const input = document.getElementById("iso-dl-url");
  if (!input) return;
  const url = input.value.trim();
  if (!url) {
    showToast("Veuillez coller une URL de téléchargement ISO valide.", "error");
    return;
  }

  try {
    const res = await fetch("/api/vms/isos/download", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url })
    });
    const json = await res.json();
    if (json.success) {
      showToast("Téléchargement de l'image ISO lancé en tâche de fond !", "success");
      input.value = "";
      await refreshIsoManager();
    } else {
      showToast("Échec : " + (json.message || "Erreur de téléchargement"), "error");
    }
  } catch (e) {
    showToast("Erreur réseau : " + e.message, "error");
  }
}

function quickDownloadIso(url, filename) {
  const input = document.getElementById("iso-dl-url");
  if (input) input.value = url;
  startIsoDownloadFromModal();
}

function uploadIsoFromInput(event) {
  const file = event.target.files && event.target.files[0];
  if (!file) return;

  const progressWrap = document.getElementById("iso-upload-progress");
  const progressBar = document.getElementById("iso-upload-bar");
  const progressText = document.getElementById("iso-upload-text");

  if (progressWrap) progressWrap.style.display = "block";

  const formData = new FormData();
  formData.append("file", file);

  const xhr = new XMLHttpRequest();
  xhr.open("POST", "/api/vms/isos/upload", true);

  xhr.upload.onprogress = function(e) {
    if (e.lengthComputable) {
      const percent = ((e.loaded / e.total) * 100).toFixed(1);
      if (progressBar) progressBar.style.width = percent + "%";
      if (progressText) progressText.textContent = `Téléversement : ${percent}% (${formatBytes(e.loaded)} / ${formatBytes(e.total)})`;
    }
  };

  xhr.onload = function() {
    if (progressWrap) progressWrap.style.display = "none";
    if (xhr.status === 200) {
      showToast(`Image ISO '${file.name}' téléversée avec succès !`, "success");
      refreshIsoManager();
    } else {
      showToast("Erreur lors du téléversement de l'ISO.", "error");
    }
  };

  xhr.onerror = function() {
    if (progressWrap) progressWrap.style.display = "none";
    showToast("Erreur réseau pendant le téléversement.", "error");
  };

  xhr.send(formData);
}

// --------------------------------------------------------------------------
// DIAGNOSTIC GPU & IOMMU
// --------------------------------------------------------------------------

async function showGpuInfoModal() {
  const modal = document.getElementById("modal-gpu-info");
  const body = document.getElementById("gpu-diag-body");
  if (!modal || !body) return;

  modal.style.display = "flex";
  body.innerHTML = '<div style="text-align:center; padding:20px; color:var(--subtext0);">Analyse du bus PCI et des groupes IOMMU...</div>';

  try {
    const res = await fetch("/api/vms/gpus");
    const json = await res.json();
    if (!json.success || !json.data || json.data.length === 0) {
      body.innerHTML = '<div style="padding:20px; text-align:center; color:var(--subtext0);">Aucun contrôleur graphique PCI détecté sur ce système.</div>';
      return;
    }

    body.innerHTML = json.data.map(gpu => `
      <div style="background:var(--mantle); border:1px solid rgba(255,255,255,0.08); border-radius:var(--radius-md); padding:16px; margin-bottom:12px;">
        <div style="display:flex; justify-content:space-between; align-items:flex-start; margin-bottom:10px;">
          <div>
            <strong style="color:var(--text); font-size:1rem;">🎮 ${escapeHtml(gpu.name)}</strong>
            <div style="font-size:0.78rem; color:var(--subtext0); font-family:var(--font-mono);">Adresse PCI : ${gpu.pci_address}</div>
          </div>
          <span class="badge" style="background:rgba(203,166,247,0.18); color:var(--mauve); padding:4px 8px; border-radius:6px; font-size:0.75rem;">
            Pilote : ${escapeHtml(gpu.driver)}
          </span>
        </div>

        <div style="display:grid; grid-template-columns: 1fr 1fr; gap:8px; font-size:0.82rem; margin-bottom:10px;">
          <div>Groupe IOMMU : <strong style="color:var(--teal);">${gpu.iommu_group !== null ? 'Groupe ' + gpu.iommu_group + ' (Isolé)' : 'Non isolé'}</strong></div>
          <div>Statut DRM : <strong style="color:var(--green);">Actif (/dev/dri)</strong></div>
        </div>

        ${gpu.conflict_warning ? `
          <div style="background:rgba(250,179,135,0.12); border:1px solid rgba(250,179,135,0.3); border-radius:6px; padding:10px; font-size:0.8rem; color:var(--peach); line-height:1.4;">
            ⚠️ <strong>Impact Détecté :</strong> ${escapeHtml(gpu.conflict_warning)}
          </div>
        ` : `
          <div style="color:var(--green); font-size:0.8rem;">
            ✓ Ce GPU est prêt pour le Passthrough VFIO direct vers une machine virtuelle.
          </div>
        `}
      </div>
    `).join('');
  } catch (e) {
    body.innerHTML = '<div style="color:var(--red); padding:20px;">Erreur lors de la récupération des données GPU : ' + e.message + '</div>';
  }
}

function closeGpuInfoModal() {
  const modal = document.getElementById("modal-gpu-info");
  if (modal) modal.style.display = "none";
}

function toggleBuildLogAccordion() {
  const accordion = document.getElementById("build-log-accordion");
  const arrow = document.getElementById("build-log-arrow");
  if (!accordion) return;
  if (accordion.style.display === "none") {
    accordion.style.display = "block";
    if (arrow) arrow.textContent = "▲";
    accordion.scrollTop = accordion.scrollHeight;
  } else {
    accordion.style.display = "none";
    if (arrow) arrow.textContent = "▼";
  }
}


// =========================================================================
// 🧬 GESTION DES GÉNÉRATIONS NIXOS & DU CHARGEUR DE DÉMARRAGE
// =========================================================================
let generationsData = null;
let selectedGenerationIds = new Set();
let pendingCleanupMode = null;
let pendingCleanupArg = null;

async function loadGenerations(force = false) {
  const refreshBtn = document.getElementById("btn-gen-refresh");
  if (force && refreshBtn) {
    refreshBtn.disabled = true;
    refreshBtn.innerHTML = `<span>⏳</span> Chargement...`;
  }

  const tbody = document.getElementById("generations-tbody");

  try {
    const res = await fetch("/api/generations/list");
    const json = await res.json();

    if (!json.success || !json.data) {
      if (tbody) {
        tbody.innerHTML = `<tr><td colspan="7" style="text-align:center; padding:24px; color:var(--red);">
          ⚠️ Erreur de chargement : ${escapeHtml(json.message || "Impossible de récupérer les générations")}
        </td></tr>`;
      }
      return;
    }

    generationsData = json.data;
    const { generations, current_id, boot_default_id, total_count, store_free_human } = generationsData;

    // Mise à jour des compteurs et statistiques
    const statCurrent = document.getElementById("stat-gen-current");
    const statBoot = document.getElementById("stat-gen-boot");
    const statTotal = document.getElementById("stat-gen-total");
    const statFree = document.getElementById("stat-gen-free");
    const badgeCount = document.getElementById("count-system-generations");

    if (statCurrent) statCurrent.textContent = `#${current_id || "--"}`;
    if (statBoot) statBoot.textContent = `#${boot_default_id || "--"}`;
    if (statTotal) statTotal.textContent = total_count;
    if (statFree) statFree.textContent = store_free_human;
    if (badgeCount) badgeCount.textContent = total_count;

    if (!generations || generations.length === 0) {
      if (tbody) {
        tbody.innerHTML = `<tr><td colspan="7" style="text-align:center; padding:24px; color:var(--subtext0);">
          ✨ Aucune génération enregistrée.
        </td></tr>`;
      }
      return;
    }

    if (tbody) {
      tbody.innerHTML = generations.map(gen => {
        const isCurrent = gen.id === current_id || gen.is_current;
        const isBootDefault = gen.id === boot_default_id || gen.is_boot_default;
        const isChecked = selectedGenerationIds.has(gen.id);

        let rowClasses = [];
        if (isCurrent) rowClasses.push("gen-row-current");
        if (isBootDefault) rowClasses.push("gen-row-boot");

        return `
          <tr class="${rowClasses.join(' ')}">
            <td style="text-align:center;">
              <input type="checkbox" class="gen-checkbox" data-id="${gen.id}" 
                ${isChecked ? 'checked' : ''} 
                ${isCurrent ? 'disabled title="La génération active ne peut pas être supprimée"' : ''} 
                onchange="toggleSelectGeneration(${gen.id}, this.checked)">
            </td>
            <td>
              <span class="badge-gen-id">#${gen.id}</span>
            </td>
            <td>
              <span style="font-weight:600; color:var(--text);">${escapeHtml(gen.build_date)}</span>
            </td>
            <td>
              <code style="font-family:var(--font-mono); font-size:0.75rem; background:rgba(0,0,0,0.25); padding:2px 6px; border-radius:4px; color:var(--subtext0);">${escapeHtml(gen.nixos_version)}</code>
            </td>
            <td>
              <code style="font-family:var(--font-mono); font-size:0.75rem; color:var(--subtext0);">${escapeHtml(gen.kernel_version)}</code>
            </td>
            <td>
              <div style="display:flex; flex-wrap:wrap; gap:4px; align-items:center;">
                ${isCurrent ? '<span class="badge badge-success" title="Génération active en mémoire vive"><span class="pulse-dot"></span> Active</span>' : ''}
                ${isBootDefault ? '<span class="badge badge-accent" title="Génération par défaut configurée dans systemd-boot">🚀 Boot par défaut</span>' : ''}
                ${!isCurrent && !isBootDefault ? '<span class="badge badge-secondary" style="opacity:0.75;">📦 Archivée</span>' : ''}
              </div>
            </td>
            <td style="text-align:right;">
              <div style="display:inline-flex; gap:6px; align-items:center;">
                ${!isBootDefault ? `
                  <button type="button" class="btn btn-secondary btn-xs" onclick="setBootGeneration(${gen.id})" title="Sélectionner pour démarrer dessus au prochain redémarrage">
                    <span>🚀</span> Booter dessus
                  </button>
                ` : `
                  <span style="font-size:0.75rem; color:var(--mauve); font-weight:700; padding:4px 8px;">✓ Prêt au boot</span>
                `}
                ${!isCurrent ? `
                  <button type="button" class="btn btn-danger btn-xs" onclick="confirmCleanupGenerations('custom', [${gen.id}])" title="Supprimer définitivement la génération #${gen.id}">
                    <span>🗑️</span>
                  </button>
                ` : `
                  <span title="Génération active protégée" style="opacity:0.4; font-size:0.9rem; padding:0 4px; cursor:not-allowed;">🔒</span>
                `}
              </div>
            </td>
          </tr>
        `;
      }).join('');
    }

    updateSelectionBar();
  } catch (e) {
    if (tbody) {
      tbody.innerHTML = `<tr><td colspan="7" style="text-align:center; padding:24px; color:var(--red);">
        Erreur réseau : ${e.message}
      </td></tr>`;
    }
  } finally {
    if (refreshBtn) {
      refreshBtn.disabled = false;
      refreshBtn.innerHTML = `<span>🔄</span> Actualiser`;
    }
  }
}

function toggleSelectGeneration(id, checked) {
  if (checked) {
    selectedGenerationIds.add(id);
  } else {
    selectedGenerationIds.delete(id);
  }
  updateSelectionBar();
}

function toggleSelectAllGenerations(checked) {
  const checkboxes = document.querySelectorAll(".gen-checkbox");
  checkboxes.forEach(cb => {
    if (!cb.disabled) {
      cb.checked = checked;
      const id = parseInt(cb.getAttribute("data-id"), 10);
      if (checked) {
        selectedGenerationIds.add(id);
      } else {
        selectedGenerationIds.delete(id);
      }
    }
  });
  updateSelectionBar();
}

function deselectAllGenerations() {
  selectedGenerationIds.clear();
  const selectAll = document.getElementById("gen-select-all");
  if (selectAll) selectAll.checked = false;
  const checkboxes = document.querySelectorAll(".gen-checkbox");
  checkboxes.forEach(cb => {
    cb.checked = false;
  });
  updateSelectionBar();
}

function updateSelectionBar() {
  const bar = document.getElementById("gen-selection-bar");
  const countEl = document.getElementById("gen-selected-count");
  const selectAll = document.getElementById("gen-select-all");

  if (countEl) countEl.textContent = selectedGenerationIds.size;

  if (bar) {
    bar.style.display = selectedGenerationIds.size > 0 ? "flex" : "none";
  }

  if (selectAll && generationsData && generationsData.generations) {
    const selectableCount = generationsData.generations.filter(g => g.id !== generationsData.current_id).length;
    selectAll.checked = selectableCount > 0 && selectedGenerationIds.size === selectableCount;
  }
}

function confirmCustomCleanup() {
  if (selectedGenerationIds.size === 0) return;
  confirmCleanupGenerations('custom', Array.from(selectedGenerationIds));
}

function confirmCleanupGenerations(mode, arg) {
  if (!generationsData || !generationsData.generations) {
    showToast("error", "Données des générations indisponibles.");
    return;
  }

  const currentId = generationsData.current_id;
  const allIds = generationsData.generations.map(g => g.id);

  let targetIds = [];
  if (mode === 'keep_last') {
    const keepCount = arg || 3;
    targetIds = allIds.slice(keepCount).filter(id => id !== currentId);
  } else if (mode === 'only_current') {
    targetIds = allIds.filter(id => id !== currentId);
  } else if (mode === 'custom') {
    targetIds = (arg || []).filter(id => id !== currentId && allIds.includes(id));
  }

  if (targetIds.length === 0) {
    showToast("info", "Aucune génération obsolète à supprimer (les versions actives et récentes sont déjà protégées).");
    return;
  }

  pendingCleanupMode = mode;
  pendingCleanupArg = targetIds;

  const modal = document.getElementById("modal-gen-cleanup");
  const summaryEl = document.getElementById("gen-cleanup-summary-text");
  const chipsList = document.getElementById("gen-cleanup-chips-list");
  const confirmBtn = document.getElementById("btn-confirm-gen-cleanup");

  if (summaryEl) {
    summaryEl.textContent = `${targetIds.length} génération(s) vont être supprimées et purgées du store`;
  }

  if (chipsList) {
    chipsList.innerHTML = targetIds.map(id => `<span class="gen-chip-id">#${id}</span>`).join('');
  }

  if (confirmBtn) {
    confirmBtn.disabled = false;
    confirmBtn.innerHTML = `<span>🗑️</span> Confirmer la suppression (${targetIds.length})`;
  }

  if (modal) modal.style.display = "flex";
}

function closeGenCleanupModal() {
  const modal = document.getElementById("modal-gen-cleanup");
  if (modal) modal.style.display = "none";
  pendingCleanupMode = null;
  pendingCleanupArg = null;
}

let isGenCleanupRunning = false;
let genCleanupToastInterval = null;
let genCleanupToastTimeout = null;

function showGenCleanupToast(count) {
  if (genCleanupToastInterval) clearInterval(genCleanupToastInterval);
  if (genCleanupToastTimeout) clearTimeout(genCleanupToastTimeout);

  const toast = document.getElementById("gen-cleanup-floating-toast");
  if (!toast) return;

  const card = toast.querySelector(".gen-cleanup-toast-card");
  const icon = document.getElementById("gen-cleanup-toast-icon");
  const title = document.getElementById("gen-cleanup-toast-title");
  const status = document.getElementById("gen-cleanup-toast-status");
  const desc = document.getElementById("gen-cleanup-toast-desc");
  const bar = document.getElementById("gen-cleanup-toast-progress-bar");
  const eta = document.getElementById("gen-cleanup-toast-eta");
  const closeBtn = document.getElementById("btn-close-gen-cleanup-toast");

  if (card) {
    card.classList.remove("status-success", "status-error");
  }

  if (icon) icon.innerHTML = `<span class="gen-cleanup-spin">🧹</span>`;
  if (title) title.textContent = "Purge NixOS & Garbage Collection";
  if (status) {
    status.className = "gen-cleanup-toast-status status-badge-running";
    status.textContent = "En cours...";
  }
  if (desc) desc.innerHTML = `Suppression de <strong>${count}</strong> génération(s) et optimisation du store...`;
  if (bar) {
    bar.className = "gen-cleanup-toast-progress-bar progress-animated";
    bar.style.width = "20%";
  }
  if (eta) eta.textContent = "Suppression des profils de démarrage...";
  if (closeBtn) closeBtn.style.display = "block";

  toast.style.display = "block";
  toast.classList.remove("minimized");
  updateFloatingDockLayout();

  // Animation des étapes pendant le traitement en arrière-plan
  let step = 0;
  const steps = [
    { p: 35, text: "Suppression des liens dans /nix/var/nix/profiles..." },
    { p: 60, text: "Exécution de nix-collect-garbage sur /nix/store..." },
    { p: 80, text: "Libération des blocs orphelins et calcul d'espace..." },
    { p: 92, text: "Mise à jour du bootloader et finalisation..." }
  ];

  genCleanupToastInterval = setInterval(() => {
    if (step < steps.length) {
      if (bar) bar.style.width = `${steps[step].p}%`;
      if (eta) eta.textContent = steps[step].text;
      step++;
    }
  }, 2200);
}

function completeGenCleanupToast(success, dataOrMessage) {
  if (genCleanupToastInterval) {
    clearInterval(genCleanupToastInterval);
    genCleanupToastInterval = null;
  }

  const toast = document.getElementById("gen-cleanup-floating-toast");
  if (!toast) return;

  const card = toast.querySelector(".gen-cleanup-toast-card");
  const icon = document.getElementById("gen-cleanup-toast-icon");
  const title = document.getElementById("gen-cleanup-toast-title");
  const status = document.getElementById("gen-cleanup-toast-status");
  const desc = document.getElementById("gen-cleanup-toast-desc");
  const bar = document.getElementById("gen-cleanup-toast-progress-bar");
  const eta = document.getElementById("gen-cleanup-toast-eta");

  if (success) {
    if (card) card.classList.add("status-success");
    if (icon) icon.innerHTML = `<span>✨</span>`;
    if (title) title.textContent = "Purge terminée avec succès";
    if (status) {
      status.className = "gen-cleanup-toast-status status-badge-success";
      status.textContent = `${dataOrMessage.deleted_count || 0} purgée(s)`;
    }
    if (desc) desc.innerHTML = `Espace disque récupéré : <strong style="color:var(--green);">${dataOrMessage.freed_space_human || '0 B'}</strong>`;
    if (bar) {
      bar.className = "gen-cleanup-toast-progress-bar progress-success";
      bar.style.width = "100%";
    }
    if (eta) eta.innerHTML = `✅ Générations d'images Nix actualisées`;

    // Auto-fermeture douce après 6 secondes
    genCleanupToastTimeout = setTimeout(() => {
      dismissGenCleanupToast();
    }, 6000);
  } else {
    if (card) card.classList.add("status-error");
    if (icon) icon.innerHTML = `<span>❌</span>`;
    if (title) title.textContent = "Échec de la purge";
    if (status) {
      status.className = "gen-cleanup-toast-status status-badge-error";
      status.textContent = "Erreur";
    }
    if (desc) desc.textContent = typeof dataOrMessage === 'string' ? dataOrMessage : (dataOrMessage.message || "Une erreur est survenue lors de l'exécution.");
    if (bar) {
      bar.className = "gen-cleanup-toast-progress-bar progress-error";
      bar.style.width = "100%";
    }
    if (eta) eta.textContent = "Consultez les journaux système";
  }
}

function dismissGenCleanupToast() {
  if (genCleanupToastInterval) {
    clearInterval(genCleanupToastInterval);
    genCleanupToastInterval = null;
  }
  if (genCleanupToastTimeout) {
    clearTimeout(genCleanupToastTimeout);
    genCleanupToastTimeout = null;
  }
  const toast = document.getElementById("gen-cleanup-floating-toast");
  if (toast) {
    toast.style.animation = "slideOutBottomRight 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards";
    setTimeout(() => {
      toast.style.display = "none";
      toast.style.animation = "";
      updateFloatingDockLayout();
    }, 300);
  }
}

async function executePendingCleanup() {
  if (isGenCleanupRunning) {
    showToast("warning", "Une opération de purge est déjà en cours d'exécution.");
    return;
  }
  if (!pendingCleanupMode || !pendingCleanupArg || pendingCleanupArg.length === 0) return;

  const mode = pendingCleanupMode;
  const targetIds = [...pendingCleanupArg];
  const count = targetIds.length;

  // 1. Fermer immédiatement la modale pour libérer l'écran et permettre la navigation
  closeGenCleanupModal();

  // 2. Décocher les cases pour une interface nette
  deselectAllGenerations();

  // 3. Afficher la popup de progression en bas à droite
  showGenCleanupToast(count);

  isGenCleanupRunning = true;

  // Verrouiller les boutons de purge pendant l'exécution
  const btnKeep3 = document.getElementById("btn-gen-keep3");
  const btnPurge = document.getElementById("btn-gen-purge");
  const btnBatch = document.getElementById("btn-gen-batch-delete");
  if (btnKeep3) btnKeep3.disabled = true;
  if (btnPurge) btnPurge.disabled = true;
  if (btnBatch) btnBatch.disabled = true;

  try {
    let payload = {};
    if (mode === 'keep_last') {
      payload = { mode: "keep_last", count: 3 };
    } else if (mode === 'only_current') {
      payload = { mode: "only_current" };
    } else {
      payload = { mode: "custom", generation_ids: targetIds };
    }

    const res = await fetch("/api/generations/cleanup", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    const json = await res.json();
    if (json.success && json.data) {
      completeGenCleanupToast(true, json.data);
      showToast("success", `✨ Purge terminée : ${json.data.deleted_count} génération(s) supprimée(s), ${json.data.freed_space_human} libérés`);
    } else {
      completeGenCleanupToast(false, json.message || "Échec de la suppression");
      showToast("error", `Erreur : ${json.message || "Échec de la suppression"}`);
    }
  } catch (e) {
    completeGenCleanupToast(false, `Erreur réseau : ${e.message}`);
    showToast("error", `Erreur réseau : ${e.message}`);
  } finally {
    isGenCleanupRunning = false;
    if (btnKeep3) btnKeep3.disabled = false;
    if (btnPurge) btnPurge.disabled = false;
    if (btnBatch) btnBatch.disabled = false;

    // 4. Une fois terminé, actualiser automatiquement les générations d'images Nix
    await loadGenerations(true);
  }
}

async function setBootGeneration(id) {
  showToast("info", `Configuration de la génération #${id} pour le démarrage...`);
  try {
    const res = await fetch("/api/generations/boot", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ generation_id: id })
    });

    const json = await res.json();
    if (json.success) {
      const modal = document.getElementById("modal-gen-boot");
      const titleEl = document.getElementById("gen-boot-modal-title");
      if (titleEl) titleEl.textContent = `Génération #${id} configurée avec succès`;
      if (modal) modal.style.display = "flex";
      showToast("success", `🚀 Génération #${id} définie pour le prochain démarrage.`);
      await loadGenerations(true);
    } else {
      showToast("error", `Erreur : ${json.message || "Impossible de configurer le boot"}`);
    }
  } catch (e) {
    showToast("error", `Erreur réseau : ${e.message}`);
  }
}

function closeGenBootModal() {
  const modal = document.getElementById("modal-gen-boot");
  if (modal) modal.style.display = "none";
}

async function rebootNasFromGenModal() {
  closeGenBootModal();
  showToast("info", "Envoi de la commande de redémarrage immédiat au serveur...");
  try {
    const res = await fetch("/api/system/power/immediate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action: "reboot" })
    });
    const json = await res.json();
    if (json.success) {
      showToast("success", "🔄 Le NAS redémarre... Reconnexion automatique dès le retour en ligne.");
    } else {
      showToast("error", `Échec du redémarrage : ${json.message || "Erreur système"}`);
    }
  } catch (e) {
    showToast("error", `Erreur réseau : ${e.message}`);
  }
}

// ==========================================================================
// SERVEURS DE JEUX & MOTEUR D'EGGS PTERODACTYL / PELICAN
// ==========================================================================
let gameServersData = [];
let hasFetchedGameServersOnce = false;
try {
  const cachedServers = localStorage.getItem("noos_cached_game_servers");
  if (cachedServers) {
    const parsed = JSON.parse(cachedServers);
    if (Array.isArray(parsed) && parsed.length > 0) {
      gameServersData = parsed;
    }
  }
} catch (_) {}
let gameCatalogData = [];
let activeConsoleServerId = null;
let gameConsoleRefreshInterval = null;
let selectedEggForCreate = null;

let currentGamesSubtab = "servers";

function switchGamesSubtab(subtab, updateHash = true) {
  currentGamesSubtab = subtab;
  try {
    localStorage.setItem("noos_subtab_games", subtab);
  } catch (e) {}
  if (updateHash && activeTab === "tab-games") {
    updateUrlHash("tab-games", subtab);
  }
  const subtabs = ["servers", "catalog", "console"];
  subtabs.forEach(s => {
    const btn = document.getElementById(`subtab-btn-games-${s}`);
    const content = document.getElementById(`games-subtab-${s}`);
    if (btn) btn.classList.toggle("active", s === subtab);
    if (content) content.style.display = s === subtab ? "block" : "none";
  });

  if (subtab === "servers") {
    renderGameServers();
    loadGameServers();
    stopGameConsoleStream();
  } else if (subtab === "catalog") {
    loadEggCatalog();
    stopGameConsoleStream();
  } else if (subtab === "console") {
    loadGameServers();
    startGameConsoleStream();
  }
}

async function loadGameServers(forceToast = false) {
  try {
    const res = await fetch("/api/games/servers");
    const json = await res.json();
    hasFetchedGameServersOnce = true;
    if (json.success && json.data) {
      gameServersData = json.data;
      syncDeployingServersFromList(gameServersData);
      try {
        localStorage.setItem("noos_cached_game_servers", JSON.stringify(gameServersData));
      } catch (_) {}
      renderGameServers();
      updateGameConsoleSelectOptions();
      const countEl = document.getElementById("count-game-servers") || document.getElementById("games-count-badge");
      if (countEl) {
        countEl.textContent = gameServersData.length;
        countEl.style.display = gameServersData.length > 0 ? "inline-block" : "none";
      }
      if (forceToast) showToast("Serveurs de jeu actualisés", "info");
    }
  } catch (e) {
    hasFetchedGameServersOnce = true;
    console.error("Échec du chargement des serveurs de jeu :", e);
  }
}

let currentGamesViewMode = localStorage.getItem('noos_games_view_mode') || 'grid';

function setGamesViewMode(mode) {
  currentGamesViewMode = mode;
  try { localStorage.setItem('noos_games_view_mode', mode); } catch (_) {}
  const btnGrid = document.getElementById("btn-games-view-grid") || document.getElementById("btn-view-grid");
  const btnList = document.getElementById("btn-games-view-list") || document.getElementById("btn-view-list");
  if (btnGrid) btnGrid.classList.toggle("active", mode === "grid");
  if (btnList) btnList.classList.toggle("active", mode === "list");
  renderGameServers();
}

function renderGameServers() {
  if (typeof renderOverviewGameServers === 'function' && typeof gameServersData !== 'undefined') {
    renderOverviewGameServers(gameServersData);
  }
  const grid = document.getElementById("game-servers-grid") || document.getElementById("games-servers-grid");
  if (!grid) return;

  const btnGrid = document.getElementById("btn-games-view-grid") || document.getElementById("btn-view-grid");
  const btnList = document.getElementById("btn-games-view-list") || document.getElementById("btn-view-list");
  if (btnGrid) btnGrid.classList.toggle("active", currentGamesViewMode === "grid");
  if (btnList) btnList.classList.toggle("active", currentGamesViewMode === "list");

  grid.className = currentGamesViewMode === "list" ? "games-grid mode-list" : "games-grid mode-grid";

  if (gameServersData.length === 0) {
    if (!hasFetchedGameServersOnce) {
      grid.innerHTML = `
        <div class="game-skeleton-card">
          <div style="display:flex; align-items:center; gap:12px;">
            <div class="skeleton-line" style="width:48px; height:48px; border-radius:12px;"></div>
            <div style="flex:1;">
              <div class="skeleton-line" style="width:50%; height:18px; margin-bottom:8px;"></div>
              <div class="skeleton-line" style="width:30%; height:12px;"></div>
            </div>
          </div>
          <div class="skeleton-line" style="width:100%; height:75px; margin-top:14px; border-radius:8px;"></div>
          <div class="skeleton-line" style="width:100%; height:40px; margin-top:auto; border-radius:8px;"></div>
        </div>
        <div class="game-skeleton-card">
          <div style="display:flex; align-items:center; gap:12px;">
            <div class="skeleton-line" style="width:48px; height:48px; border-radius:12px;"></div>
            <div style="flex:1;">
              <div class="skeleton-line" style="width:60%; height:18px; margin-bottom:8px;"></div>
              <div class="skeleton-line" style="width:35%; height:12px;"></div>
            </div>
          </div>
          <div class="skeleton-line" style="width:100%; height:75px; margin-top:14px; border-radius:8px;"></div>
          <div class="skeleton-line" style="width:100%; height:40px; margin-top:auto; border-radius:8px;"></div>
        </div>
      `;
      return;
    }
    grid.innerHTML = `
      <div class="card" style="grid-column: 1 / -1; text-align:center; padding: 60px 20px;">
        <span style="font-size:3.5rem; display:block; margin-bottom:14px;">🎮</span>
        <div style="font-weight:700; font-size:1.2rem; color:var(--text); margin-bottom:6px;">Aucun serveur de jeu actif</div>
        <p style="color:var(--subtext0); font-size:0.9rem; max-width:500px; margin:0 auto 20px auto;">
          Déployez votre premier serveur Minecraft, Palworld ou Valheim en 1-clic depuis le catalogue d'Eggs.
        </p>
        <button type="button" class="btn btn-primary" onclick="switchGamesSubtab('catalog')">
          <span>📦</span> Parcourir le catalogue d'Eggs
        </button>
      </div>
    `;
    return;
  }

  grid.innerHTML = gameServersData.map(s => {
    const isDeploying = s.status === "deploying";
    const isStarting = s.status === "starting";
    const isOnline = s.status === "online";
    const isError = s.status === "error";
    const isStopped = s.status === "stopped" || s.status === "offline";

    let statusClass = "status-stopped";
    let statusLabel = "Arrêté";
    let statusTitle = s.status_detail || "Serveur arrêté";

    if (isDeploying) {
      statusClass = "status-deploying";
      statusLabel = "Déploiement";
    } else if (isStarting) {
      statusClass = "status-starting";
      statusLabel = "Démarrage...";
    } else if (isOnline) {
      statusClass = "status-online";
      statusLabel = "En ligne";
    } else if (isError) {
      statusClass = "status-error";
      statusLabel = s.exit_code ? `Erreur (${s.exit_code})` : "En erreur";
    }

    const rowStatusClass = isOnline
      ? "row-status-online"
      : (isStarting ? "row-status-starting" : (isDeploying ? "row-status-deploying" : (isError ? "row-status-error" : "row-status-stopped")));

    const cardStatusClass = isOnline
      ? "card-status-online"
      : (isStarting ? "card-status-starting" : (isDeploying ? "card-status-deploying" : (isError ? "card-status-error" : "card-status-stopped")));
    
    // Calcul RAM et jauge
    const totalMem = s.memory_mb || 1024;
    const usedMem = (isOnline || isStarting) ? (s.memory_used_mb || 0) : 0;
    const ramPercent = Math.min(100, Math.max(0, Math.round((usedMem / totalMem) * 100)));
    const ramUsageStr = (isOnline || isStarting) ? `${usedMem} Mo / ${totalMem} Mo` : (isDeploying ? `Installation...` : `0 / ${totalMem} Mo`);

    // CPU & Joueurs
    const cpuUsageStr = (isOnline || isStarting) ? `${(s.cpu_percent || 0).toFixed(1)}%` : `0%`;
    const cpuTitleStr = (isOnline || isStarting) && s.cpu_cores_used ? `Charge CPU normalisée : ${s.cpu_percent.toFixed(1)}% de la machine hôte (~ ${s.cpu_cores_used.toFixed(1)} cœurs)` : ((isOnline || isStarting) ? `Charge CPU : ${s.cpu_percent.toFixed(1)}%` : "Serveur arrêté");
    
    const onlineCount = s.online_players || 0;
    const playersDisplay = isOnline ? (s.max_players ? `${onlineCount} / ${s.max_players}` : `${onlineCount}`) : "--";

    // Adresses
    let lanHost = s.lan_ip || s.ip_address;
    if (!lanHost || lanHost === "127.0.0.1" || lanHost === "0.0.0.0") {
      if (window.location.hostname && window.location.hostname !== "localhost" && window.location.hostname !== "127.0.0.1") {
        lanHost = window.location.hostname;
      } else {
        lanHost = "127.0.0.1";
      }
    }
    const fullAddress = `${lanHost}:${s.port}`;
    const protoUpper = (s.port_protocol || "tcp").toUpperCase();

    // Résolution de l'icône et bannière depuis le serveur ou le catalogue
    const eggRef = Array.isArray(gameCatalogData) ? gameCatalogData.find(e => e.id === s.egg_id) : null;
    const iconUrl = s.icon_url || (eggRef ? eggRef.icon_url : null);
    const bannerUrl = s.banner_url || (eggRef ? eggRef.banner_url : null);

    const cardIconHtml = iconUrl
      ? `<img src="${iconUrl}" class="game-server-img-icon" alt="${escapeHtml(s.name)}" onerror="this.style.display='none'; this.nextElementSibling.style.display='inline-block';"><span class="game-server-emoji-fallback" style="display:none;">${s.icon || '🎮'}</span>`
      : `<span class="game-server-emoji-fallback">${s.icon || '🎮'}</span>`;

    const rowIconHtml = iconUrl
      ? `<img src="${iconUrl}" class="row-server-img-icon" alt="${escapeHtml(s.name)}" onerror="this.style.display='none'; this.nextElementSibling.style.display='inline-block';"><span class="row-server-emoji-fallback" style="display:none;">${s.icon || '🎮'}</span>`
      : `<span class="row-server-emoji-fallback">${s.icon || '🎮'}</span>`;

    const cardBannerStyle = bannerUrl
      ? `background: linear-gradient(180deg, rgba(17, 17, 27, 0.40) 0%, rgba(17, 17, 27, 0.92) 100%), url('${bannerUrl}') center/cover no-repeat;`
      : ``;

    if (currentGamesViewMode === "list") {
      // MODE LIGNE / LISTE COMPACTE
      return `
        <div class="game-server-row ${rowStatusClass}">
          ${bannerUrl ? `<div class="row-banner-backdrop" style="background-image: url('${bannerUrl}');"></div>` : ""}
          
          <!-- Identité : Icône, Nom, Jeu, Port -->
          <div class="row-col-identity">
            <div class="row-icon-badge">${rowIconHtml}</div>
            <div class="row-identity-meta">
              <div class="row-server-name" title="${escapeHtml(s.name)}">${escapeHtml(s.name)}</div>
              <div class="row-server-sub">
                <span>${escapeHtml(s.game_name)}</span>
                <span class="row-dot-sep">•</span>
                <span class="row-port-tag">${s.port} / ${protoUpper}</span>
                ${s.status_detail && (isError || isStarting) ? `
                  <span class="row-dot-sep">•</span>
                  <span class="row-status-subtag ${isError ? 'subtag-error' : 'subtag-starting'}">${escapeHtml(s.status_detail)}</span>
                ` : ''}
              </div>
            </div>
          </div>

          <!-- Statut -->
          <div class="row-col-status">
            <div class="game-server-status-pill ${statusClass}" title="${escapeHtml(statusTitle)}">
              <span class="status-beacon"></span>
              <span class="status-label">${statusLabel}</span>
            </div>
          </div>

          <!-- Adresses de connexion -->
          <div class="row-col-network">
            <div class="row-endpoints">
              <div class="endpoint-chip endpoint-lan" title="Adresse locale (LAN)">
                <span class="endpoint-tag tag-lan">🏠 Local</span>
                <span class="endpoint-ip">${fullAddress}</span>
                <button type="button" class="endpoint-copy-btn" onclick="copyGameServerAddress('${fullAddress}')" title="Copier l'adresse locale">📋</button>
              </div>

              ${s.wireguard_ip ? `
                <div class="endpoint-chip endpoint-wg" title="Adresse WireGuard VPN">
                  <span class="endpoint-tag tag-wg">🛡️ VPN</span>
                  <span class="endpoint-ip">${s.wireguard_ip}:${s.port}</span>
                  <button type="button" class="endpoint-copy-btn" onclick="copyGameServerAddress('${s.wireguard_ip}:${s.port}')" title="Copier l'adresse WireGuard">📋</button>
                </div>
              ` : ""}

              ${s.public_ip ? `
                <div class="endpoint-chip endpoint-wan" title="Adresse IPv4 publique WAN">
                  <span class="endpoint-tag tag-public">🌍 WAN</span>
                  <span class="endpoint-ip">${s.public_ip}:${s.port}</span>
                  <button type="button" class="endpoint-copy-btn" onclick="copyGameServerAddress('${s.public_ip}:${s.port}')" title="Copier l'adresse publique">📋</button>
                </div>
              ` : ""}
            </div>
          </div>

          <!-- Télémétrie compacte (RAM jauge, CPU, Joueurs) -->
          <div class="row-col-stats">
            <div class="row-stat-box" title="Consommation RAM : ${ramUsageStr}">
              <span class="row-stat-label">RAM</span>
              <div class="row-ram-bar-wrap">
                <div class="metric-bar-track" style="width: 50px; height: 5px;">
                  <div class="metric-bar-fill fill-ram" style="width: ${ramPercent}%;"></div>
                </div>
                <span class="row-stat-val font-mono">${ramPercent}%</span>
              </div>
            </div>

            <div class="row-stat-box" title="${cpuTitleStr}">
              <span class="row-stat-label">CPU</span>
              <span class="row-stat-val font-mono">⚡ ${cpuUsageStr}</span>
            </div>

            <div class="row-stat-box">
              <span class="row-stat-label">JOUEURS</span>
              <span class="row-stat-val ${isOnline && onlineCount > 0 ? "text-green" : ""}">👥 ${playersDisplay}</span>
            </div>
          </div>

          <!-- Actions en ligne -->
          <div class="row-col-actions">
            ${isDeploying ? `
              <button type="button" class="btn btn-warning btn-sm" onclick="openGameDeployProgressModal('${s.id}', '${escapeHtml(s.name)}', '${escapeHtml(s.game_name)}', '${s.icon}', '${iconUrl || ""}')" title="Suivre le déploiement">
                <span class="spinner-inline">⏳</span> Suivre
              </button>
              <button type="button" class="btn btn-secondary btn-sm" onclick="openServerConsoleView('${s.id}')" title="Console">
                <span>🖥️</span>
              </button>
              <button type="button" class="btn-card-delete" onclick="confirmDeleteGameServer('${s.id}', '${escapeHtml(s.name)}')" title="Supprimer ce serveur">
                <span>🗑️</span>
              </button>
            ` : (isStarting ? `
              <button type="button" class="btn-action-primary btn-action-console btn-row-console" onclick="openServerConsoleView('${s.id}')" title="Suivre l'initialisation dans la console">
                <span class="spinner-inline">⏳</span> Boot Console
              </button>
              <button type="button" class="btn-action-power btn-power-stop btn-row-power" onclick="controlGameServerAction('${s.id}', 'stop')" title="Interrompre le serveur">
                <span>⏹️</span>
              </button>
              <button type="button" class="btn-row-files" onclick="openServerFolderInFiles('${escapeHtml(s.data_dir)}')" title="Parcourir les fichiers">
                <span>📁</span>
              </button>
              <button type="button" class="btn-card-delete" onclick="confirmDeleteGameServer('${s.id}', '${escapeHtml(s.name)}')" title="Supprimer ce serveur">
                <span>🗑️</span>
              </button>
            ` : (isError ? `
              <button type="button" class="btn-action-primary btn-action-console btn-row-console" style="background:rgba(243,139,168,0.2); border-color:rgba(243,139,168,0.4); color:var(--red);" onclick="openServerConsoleView('${s.id}')" title="Consulter les logs de crash">
                <span>🖥️</span> Crash Log
              </button>
              <button type="button" class="btn-action-power btn-power-restart btn-row-power" onclick="controlGameServerAction('${s.id}', 'restart')" title="Relancer le serveur">
                <span>🔄</span>
              </button>
              <button type="button" class="btn-row-files" onclick="openServerFolderInFiles('${escapeHtml(s.data_dir)}')" title="Parcourir les fichiers">
                <span>📁</span>
              </button>
              <button type="button" class="btn-card-delete" onclick="confirmDeleteGameServer('${s.id}', '${escapeHtml(s.name)}')" title="Supprimer ce serveur">
                <span>🗑️</span>
              </button>
            ` : (isOnline ? `
              <button type="button" class="btn-action-primary btn-action-console btn-row-console" onclick="openServerConsoleView('${s.id}')" title="Ouvrir la console en direct">
                <span>🖥️</span> Console
              </button>
              <button type="button" class="btn-action-power btn-power-restart btn-row-power" onclick="controlGameServerAction('${s.id}', 'restart')" title="Redémarrer le serveur">
                <span>🔄</span>
              </button>
              <button type="button" class="btn-action-power btn-power-stop btn-row-power" onclick="controlGameServerAction('${s.id}', 'stop')" title="Arrêter le serveur">
                <span>⏹️</span>
              </button>
              <button type="button" class="btn-row-files" onclick="openServerFolderInFiles('${escapeHtml(s.data_dir)}')" title="Parcourir les fichiers">
                <span>📁</span>
              </button>
              <button type="button" class="btn-card-delete" onclick="confirmDeleteGameServer('${s.id}', '${escapeHtml(s.name)}')" title="Supprimer ce serveur">
                <span>🗑️</span>
              </button>
            ` : `
              <button type="button" class="btn-action-primary btn-power-start btn-row-start" onclick="controlGameServerAction('${s.id}', 'start')" title="Démarrer le serveur">
                <span>▶️</span> Démarrer
              </button>
              <button type="button" class="btn-action-secondary btn-row-console-sec" onclick="openServerConsoleView('${s.id}')" title="Voir la console">
                <span>🖥️</span> Console
              </button>
              <button type="button" class="btn-row-files" onclick="openServerFolderInFiles('${escapeHtml(s.data_dir)}')" title="Parcourir les fichiers">
                <span>📁</span>
              </button>
              <button type="button" class="btn-card-delete" onclick="confirmDeleteGameServer('${s.id}', '${escapeHtml(s.name)}')" title="Supprimer ce serveur">
                <span>🗑️</span>
              </button>
            `)))}
          </div>

        </div>
      `;
    }

    // MODE GRILLE (CARTES MODERNES)
    return `
      <div class="game-server-card ${cardStatusClass}">
        
        <!-- En-tête de carte avec Statut, Nom, Type et Suppression sécurisée -->
        <div class="game-server-banner" style="${cardBannerStyle}">
          <div class="game-server-title-box">
            <div class="game-server-icon-badge">${cardIconHtml}</div>
            <div class="game-server-meta-info">
              <div class="game-server-name" title="${escapeHtml(s.name)}">${escapeHtml(s.name)}</div>
              <div class="game-server-type">
                <span>${escapeHtml(s.game_name)}</span>
                <span class="game-server-dot-sep">•</span>
                <span class="game-server-port-tag">${s.port} / ${protoUpper}</span>
              </div>
            </div>
          </div>

          <div class="game-server-banner-right">
            <div class="game-server-status-pill ${statusClass}" title="${escapeHtml(statusTitle)}">
              <span class="status-beacon"></span>
              <span class="status-label">${statusLabel}</span>
            </div>
            <button type="button" class="btn-card-delete" onclick="confirmDeleteGameServer('${s.id}', '${escapeHtml(s.name)}')" title="Supprimer définitivement ce serveur">
              <span>🗑️</span>
            </button>
          </div>
        </div>

        <!-- Corps de la carte -->
        <div class="game-server-body">
          
          <!-- Adresses de connexion rapides -->
          <div class="game-server-endpoints">
            <div class="endpoint-chip endpoint-lan" title="Adresse sur votre réseau local (LAN)">
              <span class="endpoint-tag tag-lan">🏠 Local</span>
              <span class="endpoint-ip">${fullAddress}</span>
              <button type="button" class="endpoint-copy-btn" onclick="copyGameServerAddress('${fullAddress}')" title="Copier l'adresse locale">📋</button>
            </div>

            ${s.wireguard_ip ? `
              <div class="endpoint-chip endpoint-wg" title="Adresse sécurisée VPN WireGuard">
                <span class="endpoint-tag tag-wg">🛡️ VPN</span>
                <span class="endpoint-ip">${s.wireguard_ip}:${s.port}</span>
                <button type="button" class="endpoint-copy-btn" onclick="copyGameServerAddress('${s.wireguard_ip}:${s.port}')" title="Copier l'adresse WireGuard">📋</button>
              </div>
            ` : ""}

            ${s.public_ip ? `
              <div class="endpoint-chip endpoint-wan" title="Adresse IPv4 publique WAN">
                <span class="endpoint-tag tag-public">🌍 WAN</span>
                <span class="endpoint-ip">${s.public_ip}:${s.port}</span>
                <button type="button" class="endpoint-copy-btn" onclick="copyGameServerAddress('${s.public_ip}:${s.port}')" title="Copier l'adresse publique">📋</button>
              </div>
            ` : ""}
          </div>

          <!-- Télémétrie avec jauge de RAM & métriques -->
          <div class="game-server-metrics-box">
            <div class="metric-item">
              <div class="metric-header">
                <span class="metric-label">MÉMOIRE VIVE (${ramPercent}%)</span>
                <span class="metric-value font-mono">${ramUsageStr}</span>
              </div>
              <div class="metric-bar-track">
                <div class="metric-bar-fill fill-ram" style="width: ${ramPercent}%;"></div>
              </div>
            </div>

            <div class="metric-row-duo">
              <div class="metric-sub-item">
                <span class="metric-label">CHARGE CPU</span>
                <span class="metric-sub-val font-mono" title="${cpuTitleStr}">⚡ ${cpuUsageStr}</span>
              </div>
              <div class="metric-sub-item">
                <span class="metric-label">JOUEURS</span>
                <span class="metric-sub-val ${isOnline && onlineCount > 0 ? "text-green" : ""}">👥 ${playersDisplay}</span>
              </div>
            </div>
          </div>

          <!-- Notice explicative détaillée si erreur ou démarrage -->
          ${s.status_detail && (isError || isStarting) ? `
            <div class="game-server-notice-box ${isError ? 'notice-error' : 'notice-starting'}">
              <span>${isError ? '⚠️' : '🚀'}</span>
              <span>${escapeHtml(s.status_detail)}</span>
            </div>
          ` : ''}

          <!-- Raccourcis et métadonnées secondaires -->
          <div class="game-server-footer-info">
            <span class="text-subtext">📅 ${s.created_at || "Actif"}</span>
            <button type="button" class="btn-link-files" onclick="openServerFolderInFiles('${escapeHtml(s.data_dir)}')" title="Explorer les fichiers du serveur">
              <span>📁</span> Parcourir les fichiers
            </button>
          </div>
        </div>

        <!-- Panneau d'actions réaménagé, proéminent et ergonomique -->
        <div class="game-server-action-panel">
          ${isDeploying ? `
            <button type="button" class="btn-action-primary btn-action-progress" onclick="openGameDeployProgressModal('${s.id}', '${escapeHtml(s.name)}', '${escapeHtml(s.game_name)}', '${s.icon}', '${iconUrl || ""}')">
              <span class="spinner-inline">⏳</span> Suivre le déploiement
            </button>
            <button type="button" class="btn-action-secondary" onclick="openServerConsoleView('${s.id}')" title="Ouvrir la console">
              <span>🖥️</span> Console
            </button>
          ` : (isStarting ? `
            <div class="action-btn-group">
              <button type="button" class="btn-action-primary btn-action-console" onclick="openServerConsoleView('${s.id}')">
                <span class="spinner-inline">⏳</span> Console & Boot
              </button>
              <button type="button" class="btn-action-power btn-power-stop" onclick="controlGameServerAction('${s.id}', 'stop')" title="Interrompre le serveur">
                <span>⏹️</span> Arrêter
              </button>
            </div>
          ` : (isError ? `
            <div class="action-btn-group">
              <button type="button" class="btn-action-primary btn-action-console" style="background:rgba(243,139,168,0.2); border-color:rgba(243,139,168,0.4); color:var(--red);" onclick="openServerConsoleView('${s.id}')">
                <span>🖥️</span> Voir Crash Log
              </button>
              <button type="button" class="btn-action-power btn-power-restart" onclick="controlGameServerAction('${s.id}', 'restart')" title="Relancer le serveur">
                <span>🔄</span> Relancer
              </button>
            </div>
          ` : (isOnline ? `
            <div class="action-btn-group">
              <button type="button" class="btn-action-primary btn-action-console" onclick="openServerConsoleView('${s.id}')">
                <span>🖥️</span> Console & Logs
              </button>
              <div class="action-power-group">
                <button type="button" class="btn-action-power btn-power-restart" onclick="controlGameServerAction('${s.id}', 'restart')" title="Redémarrer le serveur de jeu">
                  <span>🔄</span> Redémarrer
                </button>
                <button type="button" class="btn-action-power btn-power-stop" onclick="controlGameServerAction('${s.id}', 'stop')" title="Arrêter le serveur de jeu">
                  <span>⏹️</span> Arrêter
                </button>
              </div>
            </div>
          ` : `
            <div class="action-btn-group">
              <button type="button" class="btn-action-primary btn-power-start" onclick="controlGameServerAction('${s.id}', 'start')">
                <span>▶️</span> Démarrer le serveur
              </button>
              <button type="button" class="btn-action-secondary" onclick="openServerConsoleView('${s.id}')" title="Voir la console et les logs">
                <span>🖥️</span> Console
              </button>
            </div>
          `)))}
        </div>

      </div>
    `;
  }).join("");
}

function copyGameServerAddress(addr) {
  navigator.clipboard.writeText(addr).then(() => {
    showToast(`Adresse ${addr} copiée dans le presse-papiers !`, "success");
  }).catch(() => {
    showToast(`Adresse : ${addr}`, "info");
  });
}

function openServerFolderInFiles(dir) {
  if (typeof navigateToPath === "function") {
    switchTab("tab-files");
    navigateToPath(dir);
  }
}

async function loadEggCatalog() {
  try {
    const res = await fetch("/api/games/catalog");
    const json = await res.json();
    if (json.success && json.data) {
      gameCatalogData = json.data;
      renderEggCatalog();
    }
  } catch (e) {
    console.error("Échec du chargement du catalogue :", e);
  }
}

async function syncEggCatalog(showToastNotice = true) {
  const spin = document.getElementById("sync-eggs-spinner");
  if (spin) spin.classList.add("fa-spin");
  try {
    const res = await fetch("/api/games/catalog/sync", { method: "POST" });
    const json = await res.json();
    if (json.success && json.data) {
      gameCatalogData = json.data;
      renderEggCatalog();
      if (showToastNotice && typeof showToast === "function") {
        showToast("✓ Catalogue d'Eggs synchronisé avec succès depuis GitHub !", "success");
      }
    }
  } catch (e) {
    console.error("Erreur de synchronisation du catalogue :", e);
    if (typeof showToast === "function") {
      showToast("❌ Erreur lors de la synchronisation du catalogue.", "error");
    }
  } finally {
    if (spin) spin.classList.remove("fa-spin");
  }
}

let eggCatalogSearchQuery = '';
let eggCatalogViewMode = localStorage.getItem('noos_game_store_view') || 'grid';

function setEggCatalogView(mode) {
  eggCatalogViewMode = mode === 'list' ? 'list' : 'grid';
  localStorage.setItem('noos_game_store_view', eggCatalogViewMode);
  updateEggCatalogViewButtons();
  renderEggCatalog();
}

function updateEggCatalogViewButtons() {
  const gridBtn = document.getElementById('egg-view-grid-btn');
  const listBtn = document.getElementById('egg-view-list-btn');
  const grid = document.getElementById('egg-catalog-grid') || document.getElementById('games-catalog-grid');

  if (gridBtn) gridBtn.classList.toggle('active', eggCatalogViewMode === 'grid');
  if (listBtn) listBtn.classList.toggle('active', eggCatalogViewMode === 'list');
  if (grid) {
    grid.classList.toggle('list-view', eggCatalogViewMode === 'list');
  }
}

function onEggCatalogSearch(val) {
  eggCatalogSearchQuery = val || '';
  const clearBtn = document.getElementById('games-catalog-search-clear');
  if (clearBtn) {
    clearBtn.style.display = eggCatalogSearchQuery.trim() ? 'flex' : 'none';
  }
  renderEggCatalog();
}

function clearEggCatalogSearch() {
  eggCatalogSearchQuery = '';
  const input = document.getElementById('games-catalog-search-input');
  const clearBtn = document.getElementById('games-catalog-search-clear');
  if (input) {
    input.value = '';
    input.focus();
  }
  if (clearBtn) {
    clearBtn.style.display = 'none';
  }
  renderEggCatalog();
}

function renderEggCatalog() {
  const grid = document.getElementById("egg-catalog-grid") || document.getElementById("games-catalog-grid");
  if (!grid) return;

  updateEggCatalogViewButtons();

  if (!gameCatalogData || gameCatalogData.length === 0) {
    grid.innerHTML = `
      <div class="card" style="grid-column: 1 / -1; width:100%; text-align:center; padding: 40px 20px;">
        <span style="font-size:3rem; display:block; margin-bottom:12px;">📦</span>
        <div style="font-weight:700; font-size:1.1rem; color:var(--text); margin-bottom:6px;">Aucun Egg dans le catalogue</div>
        <p style="color:var(--subtext0); font-size:0.85rem; max-width:400px; margin:0 auto 16px auto;">
          Cliquez sur 'Importer un Egg' pour ajouter un serveur communautaire Pterodactyl ou Pelican.
        </p>
        <button type="button" class="btn btn-primary btn-sm" onclick="openImportEggModal()">
          <span>📥</span> Importer un Egg
        </button>
      </div>
    `;
    const counter = document.getElementById('egg-catalog-counter');
    if (counter) counter.textContent = '0 jeu';
    return;
  }

  const query = eggCatalogSearchQuery.trim().toLowerCase();
  const filtered = gameCatalogData.filter(egg => {
    if (!query) return true;
    const matchName = (egg.name || '').toLowerCase().includes(query);
    const matchTagline = (egg.tagline || '').toLowerCase().includes(query);
    const matchDesc = (egg.description || '').toLowerCase().includes(query);
    const matchCat = (egg.category || '').toLowerCase().includes(query);
    const matchAuthor = (egg.author || '').toLowerCase().includes(query);
    const matchId = (egg.id || '').toLowerCase().includes(query);
    return matchName || matchTagline || matchDesc || matchCat || matchAuthor || matchId;
  });

  const counter = document.getElementById('egg-catalog-counter');
  if (counter) {
    if (query) {
      counter.textContent = `${filtered.length} / ${gameCatalogData.length} trouvé${filtered.length > 1 ? 's' : ''}`;
    } else {
      counter.textContent = `${filtered.length} jeu${filtered.length > 1 ? 'x' : ''}`;
    }
  }

  if (filtered.length === 0) {
    grid.innerHTML = `
      <div class="card" style="grid-column: 1 / -1; width:100%; text-align: center; padding: 48px 20px; background: var(--surface0); border: 1px dashed rgba(255,255,255,0.12); border-radius: var(--radius-md);">
        <span style="font-size: 2.8rem; display: block; margin-bottom: 12px; filter: grayscale(0.4);">🔍</span>
        <div style="font-weight: 700; font-size: 1.15rem; color: var(--text); margin-bottom: 6px;">Aucun serveur de jeu ne correspond à "${escapeHtml(eggCatalogSearchQuery)}"</div>
        <p style="color: var(--subtext0); font-size: 0.85rem; max-width: 420px; margin: 0 auto 16px auto;">
          Vérifiez l'orthographe ou essayez un mot-clé plus générique (ex: Minecraft, Zombie, Survie, FPS...).
        </p>
        <button type="button" class="btn btn-secondary btn-sm" onclick="clearEggCatalogSearch()">
          <span>✕</span> Effacer la recherche
        </button>
      </div>
    `;
    return;
  }

  grid.innerHTML = filtered.map(egg => {
    const bannerStyle = egg.banner_url
      ? `background: linear-gradient(180deg, rgba(17, 17, 27, 0.45) 0%, rgba(17, 17, 27, 0.90) 100%), url('${egg.banner_url}') center/cover no-repeat;`
      : `background:${egg.banner_color || 'var(--surface1)'};`;

    const iconHtml = egg.icon_url
      ? `<img src="${egg.icon_url}" class="egg-card-img-icon" alt="${escapeHtml(egg.name)}" onerror="this.style.display='none'; this.nextElementSibling.style.display='inline-block';"><span class="egg-card-icon" style="display:none;">${egg.icon || '🎮'}</span>`
      : `<span class="egg-card-icon">${egg.icon || '🎮'}</span>`;

    const taglineHtml = egg.tagline
      ? `<div class="egg-card-tagline">${escapeHtml(egg.tagline)}</div>`
      : '';

    const authorHtml = egg.author
      ? `<span class="egg-spec-pill" title="Auteur de l'Egg">👤 ${escapeHtml(egg.author.split('&')[0].trim())}</span>`
      : '';

    return `
      <div class="egg-card">
        <div class="egg-card-banner" style="${bannerStyle}">
          ${iconHtml}
          <div class="egg-card-banner-meta">
            <div class="egg-card-title">${escapeHtml(egg.name)}</div>
            <div class="egg-card-category">${escapeHtml(egg.category)}</div>
          </div>
        </div>

        <div class="egg-card-body">
          ${taglineHtml}
          <div class="egg-card-desc">${escapeHtml(egg.description)}</div>

          <div class="egg-card-specs">
            <span class="egg-spec-pill">Port : ${egg.default_port} (${egg.port_protocol.toUpperCase()})</span>
            <span class="egg-spec-pill">RAM min : ${egg.min_memory_mb / 1024} Go</span>
            ${authorHtml}
          </div>
        </div>

        <div class="egg-card-footer" style="display:flex; gap:8px; align-items:center;">
          <button type="button" class="btn btn-primary btn-sm" style="flex:1;" onclick="openCreateGameModal('${egg.id}')">
            <span>🚀</span> Déployer en 1-clic
          </button>
          ${egg.is_custom ? `
          <button type="button" class="btn btn-secondary btn-sm" onclick="deleteCustomEgg('${egg.id}', '${escapeHtml(egg.name)}')" title="Supprimer cet Egg personnalisé" style="color:var(--red); border-color:rgba(243,139,168,0.3); padding:6px 10px;">
            <span>🗑️</span>
          </button>
          ` : ''}
        </div>
      </div>
    `;
  }).join('');
}

async function deleteCustomEgg(eggId, eggName) {
  if (!confirm(`Supprimer définitivement l'Egg personnalisé "${eggName}" ?`)) {
    return;
  }
  try {
    const res = await fetch(`/api/games/eggs/${encodeURIComponent(eggId)}`, {
      method: 'DELETE',
    });
    const json = await res.json();
    if (json.success) {
      if (typeof showToast === 'function') {
        showToast('✓ Egg personnalisé supprimé avec succès', 'success');
      }
      await fetchGameCatalog();
    } else {
      if (typeof showToast === 'function') {
        showToast(json.message || 'Erreur lors de la suppression', 'error');
      }
    }
  } catch (err) {
    if (typeof showToast === 'function') {
      showToast('Erreur réseau lors de la suppression de l\'Egg', 'error');
    }
  }
}


// ==========================================================================
// ASSISTANT MINECRAFT MULTI-LOADER & CONFIGURATION SERVER.PROPERTIES
// ==========================================================================
let selectedMinecraftLoader = 'paper';
const mcVersionsCache = {};

function initMinecraftWizard() {
  selectMinecraftLoader('paper');
  switchMinecraftConfigTab('general');
}

function selectMinecraftLoader(loaderId) {
  selectedMinecraftLoader = loaderId;
  document.querySelectorAll('.mc-loader-card').forEach(card => {
    card.classList.toggle('active', card.getAttribute('data-loader') === loaderId);
  });

  const ramSlider = document.getElementById('create-game-slider-ram');
  const ramHint = document.getElementById('create-game-ram-hint');

  if (loaderId === 'forge' || loaderId === 'neoforge') {
    if (ramSlider && parseInt(ramSlider.value, 10) < 6144) {
      ramSlider.value = 6144;
      updateRamDisplay(6144);
    }
    if (ramHint) ramHint.textContent = "Recommandé pour modpacks : 6 à 8 Go";
  } else if (loaderId === 'vanilla') {
    if (ramHint) ramHint.textContent = "Recommandé Vanilla : 3 à 4 Go";
  } else {
    if (ramHint) ramHint.textContent = "Recommandé : 4 Go (Haute performance)";
  }

  loadMinecraftVersions(loaderId);
}

async function loadMinecraftVersions(loaderId) {
  const select = document.getElementById('mc-version-select');
  const hint = document.getElementById('mc-version-status-hint');
  if (!select) return;

  if (mcVersionsCache[loaderId] && mcVersionsCache[loaderId].length > 0) {
    populateMinecraftVersionDropdown(mcVersionsCache[loaderId], loaderId);
    return;
  }

  select.innerHTML = '<option value="">Chargement des versions scrapées...</option>';
  if (hint) hint.textContent = 'Interrogation de l\'API en cours...';

  try {
    const res = await fetch(`/api/games/minecraft/versions?loader=${loaderId}`);
    const json = await res.json();
    if (json.success && json.data && json.data.length > 0) {
      mcVersionsCache[loaderId] = json.data;
      populateMinecraftVersionDropdown(json.data, loaderId);
    } else {
      select.innerHTML = '<option value="1.21.1">1.21.1 (Dernière version)</option>';
      if (hint) hint.textContent = 'Versions de repli activées';
    }
  } catch (e) {
    console.error("Échec du scraping des versions Minecraft :", e);
    select.innerHTML = '<option value="1.21.1">1.21.1</option><option value="1.20.4">1.20.4</option><option value="1.20.1">1.20.1</option>';
    if (hint) hint.textContent = 'Mode hors-ligne';
  }
}

function populateMinecraftVersionDropdown(versions, loaderId) {
  const select = document.getElementById('mc-version-select');
  const hint = document.getElementById('mc-version-status-hint');
  if (!select) return;

  select.innerHTML = versions.map((v, i) => {
    const isRecommended = (loaderId === 'forge' && v === '1.20.1') || (loaderId !== 'forge' && v === '1.21.1');
    const label = isRecommended ? `${v} ★ (Version la plus stable)` : v;
    return `<option value="${v}" ${isRecommended || i === 0 ? 'selected' : ''}>${label}</option>`;
  }).join('');

  if (hint) hint.textContent = `${versions.length} versions disponibles`;
}

function switchMinecraftConfigTab(tabName) {
  const tabs = ['general', 'gameplay', 'network', 'perf'];
  tabs.forEach(t => {
    const btn = document.getElementById(`btn-mc-tab-${t}`);
    const pane = document.getElementById(`mc-tab-pane-${t}`);
    if (btn) btn.classList.toggle('active', t === tabName);
    if (pane) pane.style.display = t === tabName ? 'block' : 'none';
  });
}

function openCreateGameModal(eggId) {
  const egg = gameCatalogData.find(e => e.id === eggId);
  if (!egg) return;

  selectedEggForCreate = egg;

  document.getElementById("create-game-modal-title").textContent = `Déployer un Serveur : ${egg.name}`;
  document.getElementById("create-game-egg-badge").textContent = egg.category.toUpperCase();
  document.getElementById("create-game-icon").textContent = egg.icon || '🎮';
  document.getElementById("create-game-name").textContent = egg.name;
  document.getElementById("create-game-desc").textContent = egg.description;

  document.getElementById("create-game-input-name").value = `Mon Serveur ${egg.name.split(':')[0]}`;
  document.getElementById("create-game-input-port").value = egg.default_port;

  const ramSlider = document.getElementById("create-game-slider-ram");
  ramSlider.min = egg.min_memory_mb;
  ramSlider.value = egg.default_memory_mb;
  updateRamDisplay(egg.default_memory_mb);
  document.getElementById("create-game-ram-hint").textContent = `Recommandé : ${egg.default_memory_mb / 1024} Go (Min : ${egg.min_memory_mb / 1024} Go)`;

  const isMinecraft = (egg.id === "minecraft-java");
  const mcWrap = document.getElementById("create-game-minecraft-custom-wrap");
  const dynWrap = document.getElementById("create-game-dynamic-vars-wrap");

  if (isMinecraft) {
    if (mcWrap) mcWrap.style.display = "block";
    if (dynWrap) dynWrap.style.display = "none";
    initMinecraftWizard();
  } else {
    if (mcWrap) mcWrap.style.display = "none";
    if (dynWrap) dynWrap.style.display = "block";
  }

  // Génération des variables dynamiques
  const varsContainer = document.getElementById("create-game-dynamic-vars-container");
  if (varsContainer && !isMinecraft) {
    if (egg.variables && egg.variables.length > 0) {
      document.getElementById("create-game-dynamic-vars-wrap").style.display = "block";
      varsContainer.innerHTML = egg.variables.map(v => {
        let inputHtml = '';
        if (v.input_type === "select" && v.options) {
          inputHtml = `
            <select class="form-select dynamic-egg-var" data-var="${v.env_variable}">
              ${v.options.map(opt => `<option value="${opt}" ${opt === v.default_value ? 'selected' : ''}>${opt}</option>`).join('')}
            </select>
          `;
        } else if (v.input_type === "boolean") {
          inputHtml = `
            <select class="form-select dynamic-egg-var" data-var="${v.env_variable}">
              <option value="true" ${v.default_value === 'true' ? 'selected' : ''}>Activé (true)</option>
              <option value="false" ${v.default_value === 'false' ? 'selected' : ''}>Désactivé (false)</option>
            </select>
          `;
        } else if (v.input_type === "password") {
          inputHtml = `<input type="password" class="form-input dynamic-egg-var" data-var="${v.env_variable}" value="${escapeHtml(v.default_value)}">`;
        } else if (v.input_type === "number") {
          inputHtml = `<input type="number" class="form-input dynamic-egg-var" data-var="${v.env_variable}" value="${escapeHtml(v.default_value)}">`;
        } else {
          inputHtml = `<input type="text" class="form-input dynamic-egg-var" data-var="${v.env_variable}" value="${escapeHtml(v.default_value)}">`;
        }

        return `
          <div class="form-group" style="margin:0;">
            <label class="form-label" style="font-weight:600; font-size:0.85rem;">${escapeHtml(v.name)} :</label>
            ${inputHtml}
            <span style="font-size:0.72rem; color:var(--subtext0);">${escapeHtml(v.description)}</span>
          </div>
        `;
      }).join('');
    } else {
      document.getElementById("create-game-dynamic-vars-wrap").style.display = "none";
    }
  }

  document.getElementById("modal-create-game").style.display = "flex";
}

function updateRamDisplay(mb) {
  const gb = (mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1);
  const el = document.getElementById("create-game-ram-val");
  if (el) el.textContent = `${gb} Go`;
}

function closeCreateGameModal() {
  const modal = document.getElementById("modal-create-game");
  if (modal) modal.style.display = "none";
  selectedEggForCreate = null;
}

async function submitCreateGameServer() {
  if (!selectedEggForCreate) return;

  const nameInput = document.getElementById("create-game-input-name");
  const portInput = document.getElementById("create-game-input-port");
  const ramSlider = document.getElementById("create-game-slider-ram");
  const submitBtn = document.getElementById("btn-submit-create-game");

  const name = nameInput ? nameInput.value.trim() : "";
  if (!name) {
    showToast("Veuillez saisir un nom pour le serveur.", "warning");
    return;
  }

  const port = portInput ? parseInt(portInput.value, 10) : selectedEggForCreate.default_port;
  const memory_mb = ramSlider ? parseInt(ramSlider.value, 10) : selectedEggForCreate.default_memory_mb;

  // Récupérer les variables dynamiques ou spécifiques Minecraft
  const variables = {};

  if (selectedEggForCreate.id === "minecraft-java") {
    variables["LOADER"] = selectedMinecraftLoader;
    variables["MINECRAFT_VERSION"] = (document.getElementById("mc-version-select") || {}).value || "1.21.1";
    variables["MOTD"] = (document.getElementById("mc-cfg-motd") || {}).value || "Noos Minecraft";
    variables["LEVEL_NAME"] = (document.getElementById("mc-cfg-level-name") || {}).value || "world";
    variables["LEVEL_SEED"] = (document.getElementById("mc-cfg-seed") || {}).value || "";
    variables["GAMEMODE"] = (document.getElementById("mc-cfg-gamemode") || {}).value || "survival";
    variables["DIFFICULTY"] = (document.getElementById("mc-cfg-difficulty") || {}).value || "normal";
    variables["PVP"] = (document.getElementById("mc-cfg-pvp") || {}).value || "true";
    variables["HARDCORE"] = (document.getElementById("mc-cfg-hardcore") || {}).value || "false";
    variables["ONLINE_MODE"] = (document.getElementById("mc-cfg-online-mode") || {}).value || "true";
    variables["MAX_PLAYERS"] = (document.getElementById("mc-cfg-max-players") || {}).value || "20";
    variables["WHITELIST"] = (document.getElementById("mc-cfg-whitelist") || {}).value || "false";
    variables["COMMAND_BLOCKS"] = (document.getElementById("mc-cfg-cmd-blocks") || {}).value || "true";
    variables["VIEW_DISTANCE"] = (document.getElementById("mc-cfg-view-distance") || {}).value || "10";
    variables["SIMULATION_DISTANCE"] = (document.getElementById("mc-cfg-sim-distance") || {}).value || "8";
    variables["ALLOW_FLIGHT"] = (document.getElementById("mc-cfg-allow-flight") || {}).value || "false";
    variables["SPAWN_PROTECTION"] = (document.getElementById("mc-cfg-spawn-prot") || {}).value || "16";
  } else {
    document.querySelectorAll(".dynamic-egg-var").forEach(el => {
      const varName = el.getAttribute("data-var");
      if (varName) {
        variables[varName] = el.value;
      }
    });
  }

  const origBtn = submitBtn ? submitBtn.innerHTML : "";
  if (submitBtn) {
    submitBtn.disabled = true;
    submitBtn.innerHTML = `<span>⏳</span> Déploiement en cours...`;
  }

  showToast(`Déploiement du serveur ${name}... Cela peut prendre un instant`, "info");

  try {
    const res = await fetch("/api/games/create", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name,
        egg_id: selectedEggForCreate.id,
        memory_mb,
        port,
        variables
      })
    });
    const json = await res.json();
    if (json.success && json.data) {
      closeCreateGameModal();
      switchGamesSubtab("servers");
      loadGameServers();
      openGameDeployProgressModal(json.data.id, json.data.name, json.data.game_name, json.data.icon, json.data.icon_url || "");
    } else {
      showToast(`Échec du déploiement : ${json.message || "Erreur serveur"}`, "error");
    }
  } catch (e) {
    showToast(`Erreur réseau : ${e.message}`, "error");
  } finally {
    if (submitBtn) {
      submitBtn.disabled = false;
      submitBtn.innerHTML = origBtn;
    }
  }
}

async function controlGameServerAction(id, action) {
  showToast(`Action '${action}' envoyée au serveur...`, "info");
  const target = Array.isArray(gameServersData) ? gameServersData.find(s => s.id === id) : null;
  if (target && (action === "start" || action === "restart")) {
    target.status = "starting";
    target.status_detail = "Initialisation du conteneur...";
    renderGameServers();
  }
  try {
    const res = await fetch(`/api/games/${id}/action`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action })
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.message || "Action effectuée avec succès", "success");
      await loadGameServers();
    } else {
      showToast(`Erreur : ${json.message || "Impossible d'effectuer l'action"}`, "error");
    }
  } catch (e) {
    showToast(`Erreur réseau : ${e.message}`, "error");
  }
}

async function confirmDeleteGameServer(id, name) {
  const deleteData = confirm(`Êtes-vous sûr de vouloir supprimer définitivement le serveur '${name}' ?\n\nCliquez sur OK pour supprimer le conteneur.\nUne seconde confirmation vous demandera si vous souhaitez aussi supprimer les fichiers de sauvegarde.`);
  if (!deleteData) return;

  const purgeFiles = confirm(`Voulez-vous aussi EFFACER le dossier des mondes et sauvegardes (/mnt/storage/games/${id}) ?\n\n- Annuler = Conserver les fichiers\n- OK = Tout supprimer définitivement`);

  try {
    const res = await fetch(`/api/games/${id}/delete?delete_data=${purgeFiles}`, {
      method: "POST"
    });
    const json = await res.json();
    if (json.success) {
      showToast(`Serveur '${name}' supprimé.`, "success");
      await loadGameServers();
    } else {
      showToast(`Erreur : ${json.message}`, "error");
    }
  } catch (e) {
    showToast(`Erreur réseau : ${e.message}`, "error");
  }
}

// --------------------------------------------------------------------------
// CONSOLE TERMINALE HAUTE FIDÉLITÉ EN DIRECT
// --------------------------------------------------------------------------
let gameConsoleAutoScrollEnabled = true;
let isGameConsoleExpanded = false;
let isGameConsoleProgrammaticScrolling = false;
let lastGameConsoleRawMap = {};
let currentlyRenderedConsoleServerId = null;

function isMinecraftServer(server) {
  if (!server) return false;
  const egg = (server.egg_id || "").toLowerCase();
  const game = (server.game_name || "").toLowerCase();
  const name = (server.name || "").toLowerCase();
  if (egg.includes("minecraft") || game.includes("minecraft") || name.includes("minecraft")) return true;
  if (server.env && (server.env.MINECRAFT_VERSION || server.env.MINECRAFT_LOADER)) return true;
  return false;
}

function updateGameConsoleSelectOptions() {
  const sel = document.getElementById("game-console-server-select");
  if (!sel) return;

  if (gameServersData.length === 0) {
    sel.innerHTML = '<option value="">Aucun serveur déployé</option>';
    activeConsoleServerId = null;
    currentlyRenderedConsoleServerId = null;
    updateConsoleHeaderStats(null);
    return;
  }

  // Vérifier si activeConsoleServerId est toujours valide
  if (!activeConsoleServerId || !gameServersData.some(s => s.id === activeConsoleServerId)) {
    activeConsoleServerId = gameServersData[0].id;
  }

  sel.innerHTML = gameServersData.map(s => {
    const gameTitle = s.game_name || s.name;
    const statusLabel = s.status === 'online' ? '🟢 En ligne' : (s.status === 'starting' ? '🚀 Démarrage...' : (s.status === 'error' ? '⚠️ Erreur' : '⏹️ Arrêté'));
    return `
      <option value="${s.id}" ${s.id === activeConsoleServerId ? 'selected' : ''}>
        ${escapeHtml(gameTitle)} (${statusLabel})
      </option>
    `;
  }).join('');

  // Verrouillage impératif de la sélection sur l'élément DOM
  sel.value = activeConsoleServerId;

  const current = gameServersData.find(s => s.id === activeConsoleServerId) || gameServersData[0];
  updateConsoleHeaderStats(current);
}

function updateConsoleHeaderStats(server) {
  const statusPill = document.getElementById("game-console-status-pill");
  const statusText = document.getElementById("game-console-status-text");
  const cpuVal = document.getElementById("game-console-cpu-val");
  const ramVal = document.getElementById("game-console-ram-val");
  const titleEl = document.getElementById("game-term-title");
  const metaAddr = document.getElementById("console-meta-address");
  const metaPlayers = document.getElementById("console-meta-players");
  const metaPath = document.getElementById("console-meta-path");

  const protoLabel = document.getElementById("console-meta-proto-label");
  const portBadge = document.getElementById("console-meta-port-badge");
  const chipLan = document.getElementById("chip-ip-lan");
  const valLan = document.getElementById("console-ip-lan");
  const chipWg = document.getElementById("chip-ip-wg");
  const valWg = document.getElementById("console-ip-wg");
  const chipPub = document.getElementById("chip-ip-public");
  const valPub = document.getElementById("console-ip-public");



  if (!server) {
    if (statusPill) statusPill.className = "game-console-status badge-stopped";
    if (statusText) statusText.textContent = "AUCUN SERVEUR";
    if (cpuVal) cpuVal.textContent = "0.0%";
    if (ramVal) ramVal.textContent = "0 Mo / 0 Mo";
    if (portBadge) portBadge.textContent = "PORT --";
    if (valLan) valLan.textContent = "--:--";
    if (chipWg) chipWg.style.display = "none";
    if (chipPub) chipPub.style.display = "none";
    if (metaAddr) metaAddr.value = "--:--";
    if (metaPlayers) metaPlayers.textContent = "0 joueur(s)";
    if (metaPath) { metaPath.textContent = "--"; metaPath.title = ""; }
    if (titleEl) titleEl.textContent = "Console : Aucun serveur sélectionné";
    return;
  }

  const isOnline = server.status === "online";
  if (statusPill) {
    statusPill.className = `game-console-status ${isOnline ? "badge-online" : "badge-stopped"}`;
  }
  if (statusText) {
    statusText.textContent = isOnline ? "EN LIGNE" : "ARRÊTÉ";
  }
  if (cpuVal) {
    const pct = (server.cpu_percent || 0).toFixed(1);
    const cores = server.cpu_cores_used ? ` (~ ${server.cpu_cores_used.toFixed(1)} cœurs)` : "";
    cpuVal.textContent = `${pct}%`;
    const cpuChip = document.getElementById("chip-console-cpu");
    if (cpuChip) {
      cpuChip.title = `Utilisation CPU normalisée : ${pct}% de la machine hôte${cores}`;
    }
  }
  if (ramVal) {
    ramVal.textContent = `${server.memory_used_mb || 0} Mo / ${server.memory_mb || 0} Mo`;
  }

  // Protocole et Port
  const proto = (server.port_protocol || "tcp").toUpperCase();
  if (protoLabel) protoLabel.textContent = `CONNEXIONS (${proto})`;
  if (portBadge) portBadge.textContent = `PORT ${server.port} / ${proto}`;

  // 1. IP Réseau Local (LAN)
  let lanHost = server.lan_ip || server.ip_address;
  if (!lanHost || lanHost === "127.0.0.1" || lanHost === "0.0.0.0") {
    if (window.location.hostname && window.location.hostname !== "localhost" && window.location.hostname !== "127.0.0.1") {
      lanHost = window.location.hostname;
    } else {
      lanHost = "127.0.0.1";
    }
  }
  const fullLanAddr = `${lanHost}:${server.port}`;
  if (valLan) {
    valLan.textContent = fullLanAddr;
    valLan.dataset.addr = fullLanAddr;
  }
  if (metaAddr) metaAddr.value = fullLanAddr;

  // 2. IP WireGuard VPN (si interface active)
  if (server.wireguard_ip) {
    const fullWgAddr = `${server.wireguard_ip}:${server.port}`;
    if (chipWg) chipWg.style.display = "inline-flex";
    if (valWg) {
      valWg.textContent = fullWgAddr;
      valWg.dataset.addr = fullWgAddr;
    }
  } else {
    if (chipWg) chipWg.style.display = "none";
  }

  // 3. IPv4 Publique (WAN)
  if (server.public_ip) {
    const fullPubAddr = `${server.public_ip}:${server.port}`;
    if (chipPub) chipPub.style.display = "inline-flex";
    if (valPub) {
      valPub.textContent = fullPubAddr;
      valPub.dataset.addr = fullPubAddr;
    }
  } else {
    if (chipPub) chipPub.style.display = "none";
  }

  // Joueurs connectés
  if (metaPlayers) {
    const online = server.online_players || 0;
    const max = server.max_players;
    const playersText = max ? `${online} / ${max} joueur${online > 1 ? "s" : ""}` : `${online} joueur${online > 1 ? "s" : ""} en ligne`;
    metaPlayers.textContent = playersText;
    if (server.player_list && server.player_list.length > 0) {
      metaPlayers.title = `Joueurs connectés (${online}) :\n• ` + server.player_list.join("\n• ");
    } else {
      metaPlayers.title = isOnline ? "Aucun joueur connecté actuellement" : "Serveur arrêté";
    }
  }

  // Dossier
  if (metaPath) {
    const p = server.data_dir || "--";
    metaPath.textContent = p;
    metaPath.title = p;
  }

  if (titleEl) {
    const onlineCount = server.online_players || 0;
    const onlineInfo = isOnline ? ` • ${onlineCount} joueur${onlineCount > 1 ? "s" : ""}` : "";
    titleEl.textContent = `container@noos-nas:~/games/${server.id} (${server.name} • Port ${server.port}/${proto}${onlineInfo})`;
  }
}

function copyGameServerIp(type) {
  const current = gameServersData.find(s => s.id === activeConsoleServerId);
  if (!current) {
    showToast("Aucun serveur sélectionné.", "warning");
    return;
  }

  let el = null;
  let label = "IP";
  if (type === "lan") {
    el = document.getElementById("console-ip-lan");
    label = "Locale (LAN)";
  } else if (type === "wg") {
    el = document.getElementById("console-ip-wg");
    label = "WireGuard";
  } else if (type === "public") {
    el = document.getElementById("console-ip-public");
    label = "Publique";
  }

  const addr = el && el.dataset.addr ? el.dataset.addr : (el ? el.textContent.trim() : "");
  if (!addr || addr.includes("--")) {
    showToast(`Adresse ${label} non disponible.`, "warning");
    return;
  }

  navigator.clipboard.writeText(addr).then(() => {
    showToast(`Adresse ${label} copiée : ${addr}`, "success");
  }).catch(() => {
    showToast(`Adresse : ${addr}`, "info");
  });
}

function copyConsoleServerAddress() {
  copyGameServerIp("lan");
}

function copyConsoleServerPath() {
  const current = gameServersData.find(s => s.id === activeConsoleServerId);
  if (!current || !current.data_dir) {
    showToast("Aucun dossier disponible pour ce serveur.", "warning");
    return;
  }
  navigator.clipboard.writeText(current.data_dir).then(() => {
    showToast(`Chemin copié dans le presse-papiers : ${current.data_dir}`, "success");
  }).catch(() => {
    showToast(`Chemin : ${current.data_dir}`, "info");
  });
}

function openConsoleServerFolder() {
  const current = gameServersData.find(s => s.id === activeConsoleServerId);
  if (!current || !current.data_dir) {
    showToast("Dossier non trouvé pour ce serveur.", "warning");
    return;
  }
  openServerFolderInFiles(current.data_dir);
}

function switchConsoleActiveServer(serverId) {
  if (!serverId) return;
  activeConsoleServerId = serverId;
  const sel = document.getElementById("game-console-server-select");
  if (sel) sel.value = serverId;

  const current = gameServersData.find(s => s.id === serverId);
  updateConsoleHeaderStats(current);

  // Vider visuellement et afficher un indicateur de chargement pour le nouveau serveur
  const outputEl = document.getElementById("game-terminal-output");
  if (outputEl && currentlyRenderedConsoleServerId !== serverId) {
    const serverName = current ? (current.game_name || current.name) : serverId;
    outputEl.innerHTML = `<div class="game-terminal-welcome" style="padding:40px 20px;">` +
      `<div class="spinner-sm" style="margin: 0 auto 12px auto;"></div>` +
      `<div style="font-weight:600; color:var(--text);">Connexion à la console de ${escapeHtml(serverName)}...</div>` +
      `<div style="font-size:0.85rem; color:var(--subtext0); margin-top:4px;">Chargement des flux de logs en direct...</div>` +
    `</div>`;
  }

  currentlyRenderedConsoleServerId = null; // Force le rendu du nouveau serveur
  const dot = document.getElementById("game-console-autoscroll-dot");
  if (dot) dot.className = "terminal-status-dot active";

  fetchGameConsoleLogs();
}


function openServerConsoleView(id) {
  switchConsoleActiveServer(id);
  switchGamesSubtab("console");
}

function onGameConsoleServerChange() {
  const sel = document.getElementById("game-console-server-select");
  if (sel && sel.value) {
    switchConsoleActiveServer(sel.value);
  }
}

function toggleGameConsoleAutoScroll() {
  gameConsoleAutoScrollEnabled = !gameConsoleAutoScrollEnabled;
  const dot = document.getElementById("game-console-autoscroll-dot");
  if (dot) dot.className = `terminal-status-dot ${gameConsoleAutoScrollEnabled ? 'active' : ''}`;
  if (gameConsoleAutoScrollEnabled) {
    const box = document.getElementById("game-terminal-output");
    if (box) {
      isGameConsoleProgrammaticScrolling = true;
      box.scrollTop = box.scrollHeight;
      setTimeout(() => { isGameConsoleProgrammaticScrolling = false; }, 80);
    }
  }
}

function handleGameConsoleTerminalScroll() {
  if (isGameConsoleProgrammaticScrolling) return;
  const box = document.getElementById("game-terminal-output");
  if (!box) return;

  const isNearBottom = (box.scrollHeight - box.scrollTop - box.clientHeight) <= 60;
  const dot = document.getElementById("game-console-autoscroll-dot");

  if (!isNearBottom && gameConsoleAutoScrollEnabled) {
    gameConsoleAutoScrollEnabled = false;
    if (dot) dot.className = "terminal-status-dot";
  } else if (isNearBottom && !gameConsoleAutoScrollEnabled) {
    gameConsoleAutoScrollEnabled = true;
    if (dot) dot.className = "terminal-status-dot active";
  }
}

function copyGameConsoleLogs() {
  const box = document.getElementById("game-terminal-output");
  if (!box) return;
  const text = box.innerText || box.textContent;
  if (!text || text.includes("Sélectionnez un serveur")) {
    showToast("Aucun log à copier.", "warning");
    return;
  }
  navigator.clipboard.writeText(text).then(() => {
    showToast("Logs de la console copiés dans le presse-papiers !", "success");
  }).catch(() => {
    showToast("Impossible d'accéder au presse-papiers.", "error");
  });
}

function clearGameTerminal() {
  const box = document.getElementById("game-terminal-output");
  if (box) {
    box.innerHTML = `<div class="game-term-line"><span class="game-term-num">1</span><span class="game-term-text" style="color:var(--subtext0);">Console effacée. En attente de nouveaux logs...</span></div><div class="game-term-line"><span class="game-term-num"></span><span class="game-term-text"><span class="terminal-cursor"></span></span></div>`;
  }
}

function toggleExpandGameConsoleTerminal() {
  const container = document.getElementById("game-terminal-container");
  const icon = document.getElementById("icon-expand-console");
  const btn = document.getElementById("btn-toggle-expand-console");

  isGameConsoleExpanded = !isGameConsoleExpanded;

  if (container) {
    if (isGameConsoleExpanded) {
      container.classList.add("expanded");
    } else {
      container.classList.remove("expanded");
    }
  }

  if (icon) icon.textContent = isGameConsoleExpanded ? "🗗" : "⛶";
  if (btn) btn.title = isGameConsoleExpanded ? "Rétablir la taille normale" : "Mode plein écran";

  const box = document.getElementById("game-terminal-output");
  if (box && gameConsoleAutoScrollEnabled) {
    isGameConsoleProgrammaticScrolling = true;
    setTimeout(() => {
      box.scrollTop = box.scrollHeight;
      setTimeout(() => { isGameConsoleProgrammaticScrolling = false; }, 80);
    }, 120);
  }
}

function formatGameConsoleLogLine(line) {
  if (!line && line !== "") return null;
  let clean = line.replace(/\[[0-9;]*[a-zA-Z]/g, "")
                  .replace(/\[\]/g, "")
                  .replace(/\[[0-9;]+m/g, "")
                  .replace(/\[0m/g, "")
                  .replace(/\[m/g, "")
                  .trimEnd();
  if (!clean) return null;

  let cls = "";
  const lower = clean.toLowerCase();
  if (lower.includes("warn") || lower.includes("warning") || clean.includes("⚠️")) {
    cls = "log-term-warn";
  } else if (lower.includes("error") || lower.includes("severe") || lower.includes("fatal") || lower.includes("exception") || clean.includes("❌")) {
    cls = "log-term-error";
  } else if (lower.includes("info") || clean.includes("⚡") || clean.includes("🚀")) {
    cls = "log-term-info";
  } else if (lower.includes("debug") || lower.includes("trace")) {
    cls = "log-term-debug";
  } else if (lower.includes("done (") || lower.includes("started") || lower.includes("listening on") || clean.includes("✓") || clean.includes("🎉") || lower.includes("success")) {
    cls = "log-term-success";
  } else if (lower.includes("downloading, progress:") || lower.includes("preallocating")) {
    cls = "log-term-progress";
  }

  return { text: clean, cls };
}

let gameConsoleCycleCounter = 0;
function startGameConsoleStream() {
  stopGameConsoleStream();
  gameConsoleCycleCounter = 0;
  fetchGameConsoleLogs();
  gameConsoleRefreshInterval = setInterval(() => {
    fetchGameConsoleLogs();
    gameConsoleCycleCounter++;
    if (gameConsoleCycleCounter % 4 === 0) {
      loadGameServers();
    }
  }, 2500);
}

function stopGameConsoleStream() {
  if (gameConsoleRefreshInterval) {
    clearInterval(gameConsoleRefreshInterval);
    gameConsoleRefreshInterval = null;
  }
}

async function fetchGameConsoleLogs() {
  if (!activeConsoleServerId) {
    const sel = document.getElementById("game-console-server-select");
    if (sel && sel.value) {
      activeConsoleServerId = sel.value;
    } else if (gameServersData.length > 0) {
      activeConsoleServerId = gameServersData[0].id;
    }
  }

  const server = gameServersData.find(s => s.id === activeConsoleServerId);
  updateConsoleHeaderStats(server);

  if (!activeConsoleServerId) return;

  const requestedServerId = activeConsoleServerId;
  const linesSelect = document.getElementById("game-console-lines-select");
  const linesCount = linesSelect ? linesSelect.value : "200";

  try {
    const res = await fetch(`/api/games/${encodeURIComponent(requestedServerId)}/logs?lines=${linesCount}`);
    const json = await res.json();

    // Si l'utilisateur a basculé sur un autre serveur pendant la requête HTTP, ignorer cette réponse obsolète
    if (requestedServerId !== activeConsoleServerId) return;

    const outputEl = document.getElementById("game-terminal-output");
    if (!outputEl) return;

    if (json.success && json.data !== undefined) {
      const rawText = json.data || "";
      const cacheKey = `${requestedServerId}_${linesCount}`;
      const isSameServerAlreadyRendered = (currentlyRenderedConsoleServerId === requestedServerId);

      // Ne court-circuiter QUE si on affiche déjà ce serveur et que le contenu n'a pas bougé
      if (isSameServerAlreadyRendered && lastGameConsoleRawMap[cacheKey] === rawText && outputEl.children.length > 0) {
        if (gameConsoleAutoScrollEnabled) {
          isGameConsoleProgrammaticScrolling = true;
          outputEl.scrollTop = outputEl.scrollHeight;
          setTimeout(() => { isGameConsoleProgrammaticScrolling = false; }, 80);
        }
        return;
      }

      lastGameConsoleRawMap[cacheKey] = rawText;
      currentlyRenderedConsoleServerId = requestedServerId;

      const rawLines = rawText.split("\n");
      let html = "";
      let lineIdx = 1;
      for (const rawLine of rawLines) {
        const parsed = formatGameConsoleLogLine(rawLine);
        if (parsed) {
          html += `<div class="game-term-line">` +
            `<span class="game-term-num">${lineIdx}</span>` +
            `<span class="game-term-text ${parsed.cls}">${escapeHtml(parsed.text)}</span>` +
          `</div>`;
          lineIdx++;
        }
      }
      if (lineIdx === 1) {
        const gameTitle = server ? (server.game_name || server.name) : requestedServerId;
        const statusDesc = server && (server.status === 'starting' || server.status === 'deploying')
          ? "Initialisation ou téléchargement en cours..."
          : "Aucun log disponible pour ce serveur pour l'instant.";
        html = `<div class="game-terminal-welcome" style="padding:40px 20px;">` +
          `<div style="font-size:2rem; margin-bottom:8px;">${server && server.status === 'starting' ? '🚀' : '💤'}</div>` +
          `<div style="font-weight:600; color:var(--text);">${escapeHtml(gameTitle)}</div>` +
          `<div style="font-size:0.85rem; color:var(--subtext0); margin-top:4px;">${statusDesc}</div>` +
        `</div>`;
      } else {
        html += `<div class="game-term-line"><span class="game-term-num"></span><span class="game-term-text"><span class="terminal-cursor"></span></span></div>`;
      }

      isGameConsoleProgrammaticScrolling = true;
      outputEl.innerHTML = html;

      if (gameConsoleAutoScrollEnabled) {
        outputEl.scrollTop = outputEl.scrollHeight;
      }

      requestAnimationFrame(() => {
        if (gameConsoleAutoScrollEnabled) {
          outputEl.scrollTop = outputEl.scrollHeight;
        }
        setTimeout(() => {
          isGameConsoleProgrammaticScrolling = false;
        }, 80);
      });
    } else {
      // En cas d'erreur API, afficher un message explicite si ce serveur n'est pas encore rendu
      if (currentlyRenderedConsoleServerId !== requestedServerId) {
        currentlyRenderedConsoleServerId = requestedServerId;
        const gameTitle = server ? (server.game_name || server.name) : requestedServerId;
        outputEl.innerHTML = `<div class="game-terminal-welcome" style="padding:40px 20px;">` +
          `<div style="font-size:2rem; margin-bottom:8px;">⏹️</div>` +
          `<div style="font-weight:600; color:var(--text);">${escapeHtml(gameTitle)}</div>` +
          `<div style="font-size:0.85rem; color:var(--subtext0); margin-top:4px;">${json.message || "Serveur en cours d'initialisation ou arrêté."}</div>` +
        `</div>`;
      }
    }
  } catch (e) {
    console.error("Échec de la récupération des logs console :", e);
    if (currentlyRenderedConsoleServerId !== requestedServerId) {
      currentlyRenderedConsoleServerId = requestedServerId;
      const outputEl = document.getElementById("game-terminal-output");
      if (outputEl) {
        const gameTitle = server ? (server.game_name || server.name) : requestedServerId;
        outputEl.innerHTML = `<div class="game-terminal-welcome" style="padding:40px 20px;">` +
          `<div style="font-size:2rem; margin-bottom:8px;">⚠️</div>` +
          `<div style="font-weight:600; color:var(--text);">${escapeHtml(gameTitle)}</div>` +
          `<div style="font-size:0.85rem; color:var(--subtext0); margin-top:4px;">Impossible de contacter le conteneur Docker.</div>` +
        `</div>`;
      }
    }
  }
}

function handleGameConsoleInputKey(e) {
  if (e.key === "Enter") {
    e.preventDefault();
    sendGameTerminalCommand();
  }
}

async function sendGameTerminalCommand() {
  if (!activeConsoleServerId) {
    showToast("Veuillez sélectionner un serveur de jeu d'abord.", "warning");
    return;
  }
  const input = document.getElementById("game-terminal-input");
  if (!input) return;

  const cmd = input.value.trim();
  if (!cmd) return;

  input.value = "";
  try {
    const res = await fetch(`/api/games/${encodeURIComponent(activeConsoleServerId)}/command`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: cmd })
    });
    const json = await res.json();
    if (!json.success) {
      showToast(`Erreur commande : ${json.message || 'inconnue'}`, "error");
    }
    setTimeout(fetchGameConsoleLogs, 350);
  } catch (e) {
    showToast(`Erreur réseau : ${e.message}`, "error");
  }
}

function actionCurrentConsoleServer(action) {
  if (activeConsoleServerId) {
    controlGameServerAction(activeConsoleServerId, action);
  } else {
    showToast("Veuillez sélectionner un serveur de jeu.", "warning");
  }
}

// --------------------------------------------------------------------------
// IMPORT D'EGG
// --------------------------------------------------------------------------
function openImportEggModal() {
  document.getElementById("import-egg-url").value = "";
  document.getElementById("import-egg-json").value = "";
  document.getElementById("modal-import-egg").style.display = "flex";
}

function closeImportEggModal() {
  document.getElementById("modal-import-egg").style.display = "none";
}

async function submitImportEgg() {
  const urlInput = document.getElementById("import-egg-url");
  const jsonInput = document.getElementById("import-egg-json");
  const submitBtn = document.getElementById("btn-submit-import-egg");

  const url = urlInput ? urlInput.value.trim() : "";
  const content = jsonInput ? jsonInput.value.trim() : "";

  if (!url && !content) {
    showToast("Veuillez saisir une URL GitHub ou coller le contenu JSON d'un Egg.", "warning");
    return;
  }

  const origBtn = submitBtn ? submitBtn.innerHTML : "";
  if (submitBtn) {
    submitBtn.disabled = true;
    submitBtn.innerHTML = `<span>⏳</span> Import en cours...`;
  }

  try {
    const res = await fetch("/api/games/eggs/import", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: url ? url : null,
        content: content ? content : null
      })
    });
    const json = await res.json();
    if (json.success && json.data) {
      showToast(`✨ Egg '${json.data.name}' importé avec succès dans le catalogue !`, "success");
      closeImportEggModal();
      await loadEggCatalog();
      switchGamesSubtab("catalog");
    } else {
      showToast(`Échec de l'import : ${json.message || "Fichier invalide"}`, "error");
    }
  } catch (e) {
    showToast(`Erreur réseau : ${e.message}`, "error");
  } finally {
    if (submitBtn) {
      submitBtn.disabled = false;
      submitBtn.innerHTML = origBtn;
    }
  }
}

// ================= PROGRESSION DU DÉPLOIEMENT DU SERVEUR DE JEU =================
let deployAutoScrollEnabled = true;

function toggleDeployAutoScroll() {
  deployAutoScrollEnabled = !deployAutoScrollEnabled;
  const ind = document.getElementById("deploy-autoscroll-indicator");
  if (ind) {
    if (deployAutoScrollEnabled) {
      ind.classList.add("active");
    } else {
      ind.classList.remove("active");
    }
  }
}

function handleDeployTerminalScroll() {
  const box = document.getElementById("game-deploy-logs-box");
  if (!box) return;
  const isAtBottom = box.scrollHeight - box.clientHeight <= box.scrollTop + 30;
  const ind = document.getElementById("deploy-autoscroll-indicator");
  if (isAtBottom) {
    deployAutoScrollEnabled = true;
    if (ind) ind.classList.add("active");
  } else {
    deployAutoScrollEnabled = false;
    if (ind) ind.classList.remove("active");
  }
}

function copyDeployLogs() {
  const box = document.getElementById("game-deploy-logs-box");
  if (!box) return;
  const text = box.innerText;
  if (!text) return;
  navigator.clipboard.writeText(text).then(() => {
    showToast("Logs de déploiement copiés dans le presse-papiers !", "success");
  });
}

function formatDeployLogLine(line) {
  if (!line) return null;
  let clean = line.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "")
                  .replace(/\[\x1b\]/g, "")
                  .replace(/\[[0-9;]+m/g, "")
                  .replace(/\[0m/g, "")
                  .replace(/\[m/g, "")
                  .replace(/^[0-9]+\)\s*/, "")
                  .trim();
  if (!clean) return null;

  let cls = "";
  if (clean.includes("downloading, progress:") || clean.includes("preallocating, progress:")) {
    cls = "log-highlight-progress";
  } else if (clean.includes("ERROR") || clean.includes("Failed") || clean.includes("Erreur") || clean.includes("❌")) {
    cls = "log-highlight-error";
  } else if (clean.includes("OK") || clean.includes("fully installed") || clean.includes("Success") || clean.includes("validé") || clean.includes("✓") || clean.includes("🎉")) {
    cls = "log-highlight-success";
  } else if (clean.includes("Starting") || clean.includes("Connecting") || clean.includes("Steam Console") || clean.includes("steamcmd")) {
    cls = "log-highlight-steam";
  } else if (clean.includes("⚡") || clean.includes("🚀")) {
    cls = "log-highlight-info";
  }

  return { text: clean, cls };
}

// ==========================================================================
// REGISTRE MULTI-DÉPLOIEMENTS DE SERVEURS DE JEUX & POPUPS FLOTTANTES EN PARALLÈLE
// ==========================================================================
let activeGameDeployments = {}; // Key: serverId -> Object de déploiement
let activeGameDeployModalServerId = null; // serverId affiché en plein écran dans la modale (ou null si réduite)
let gameDeployTickerInterval = null;

// Démarre la boucle de rafraîchissement temps réel si nécessaire
function ensureGameDeployTicker() {
  if (gameDeployTickerInterval) return;
  gameDeployTickerInterval = setInterval(pollAllActiveGameDeployments, 1500);
}

// Arrête la boucle si plus aucun déploiement actif
function checkStopGameDeployTicker() {
  const activeKeys = Object.keys(activeGameDeployments);
  if (activeKeys.length === 0 && gameDeployTickerInterval) {
    clearInterval(gameDeployTickerInterval);
    gameDeployTickerInterval = null;
  }
}

// Synchronise les serveurs en cours de déploiement lors du rechargement de la liste
function syncDeployingServersFromList(servers) {
  if (!Array.isArray(servers)) return;
  let hasNew = false;
  for (const s of servers) {
    if (s.status === "deploying") {
      if (!activeGameDeployments[s.id]) {
        activeGameDeployments[s.id] = {
          id: s.id,
          name: s.name || "Serveur de jeu",
          eggName: s.game_name || "SERVEUR",
          icon: s.icon || "🎮",
          iconUrl: s.icon_url || "",
          progressPercent: 15,
          stepIndex: 2,
          statusMessage: s.status_detail || "Déploiement en cours...",
          detail: "Initialisation du conteneur...",
          logs: [],
          isComplete: false,
          isError: false,
          minimized: false
        };
        hasNew = true;
      }
    }
  }
  if (hasNew) {
    renderAllGameDeployToasts();
    ensureGameDeployTicker();
    pollAllActiveGameDeployments();
  }
}

// Ouvre la grande fenêtre de suivi pour un serveur précis en mode plein écran
function openGameDeployProgressModal(serverId, serverName, eggName, eggIcon, eggIconUrl) {
  if (!serverId) return;

  if (!activeGameDeployments[serverId]) {
    activeGameDeployments[serverId] = {
      id: serverId,
      name: serverName || "Serveur de jeu",
      eggName: eggName || "SERVEUR",
      icon: eggIcon || "🎮",
      iconUrl: eggIconUrl || "",
      progressPercent: 15,
      stepIndex: 2,
      statusMessage: "Initialisation du serveur...",
      detail: "Préparation des volumes et configurations...",
      logs: [],
      isComplete: false,
      isError: false,
      minimized: false
    };
  } else {
    if (serverName) activeGameDeployments[serverId].name = serverName;
    if (eggName) activeGameDeployments[serverId].eggName = eggName;
    if (eggIcon) activeGameDeployments[serverId].icon = eggIcon;
    if (eggIconUrl) activeGameDeployments[serverId].iconUrl = eggIconUrl;
  }

  activeGameDeployModalServerId = serverId;
  const dep = activeGameDeployments[serverId];

  const modal = document.getElementById("modal-game-deploy-progress");
  const win = document.getElementById("game-deploy-modal-window");
  if (win) {
    win.classList.add("deploy-expanded");
  }

  // Hydratation de la vue modale
  const badge = document.getElementById("game-deploy-modal-badge");
  const title = document.getElementById("game-deploy-modal-title");
  const termTitle = document.getElementById("game-deploy-terminal-title");
  const heroIcon = document.getElementById("game-deploy-hero-icon");
  const heroName = document.getElementById("game-deploy-hero-name");
  const heroStatus = document.getElementById("game-deploy-hero-status");
  const heroPercent = document.getElementById("game-deploy-hero-percent-badge");
  const barFill = document.getElementById("game-deploy-bar-fill");
  const percentLabel = document.getElementById("game-deploy-percent-label");
  const detailSubtext = document.getElementById("game-deploy-detail-subtext");
  const logsBox = document.getElementById("game-deploy-logs-box");
  const finishBtn = document.getElementById("btn-game-deploy-finish");
  const toConsoleBtn = document.getElementById("btn-game-deploy-to-console");
  const cancelBtn = document.getElementById("btn-cancel-game-deploy");

  if (badge) badge.textContent = (dep.eggName || "SERVEUR").toUpperCase();
  if (title) title.textContent = `Déploiement : ${dep.name}`;
  if (termTitle) termTitle.innerHTML = `<span>⚡</span> container@noos-nas:~/games/${escapeHtml(serverId)}`;
  if (heroIcon) {
    if (dep.iconUrl) {
      heroIcon.innerHTML = `<img src="${dep.iconUrl}" style="width:48px;height:48px;object-fit:contain;filter:drop-shadow(0 4px 10px rgba(0,0,0,0.6));">`;
    } else {
      heroIcon.textContent = dep.icon || "🎮";
    }
  }
  if (heroName) heroName.textContent = dep.name;
  if (heroStatus) {
    heroStatus.textContent = dep.statusMessage;
    heroStatus.style.color = dep.isError ? "var(--red)" : (dep.isComplete ? "var(--green)" : "var(--mauve)");
  }
  if (heroPercent) heroPercent.textContent = `${dep.progressPercent}%`;
  if (percentLabel) percentLabel.textContent = `${dep.progressPercent}%`;
  if (barFill) {
    barFill.style.width = `${dep.progressPercent}%`;
    barFill.style.background = dep.isError ? "var(--red)" : (dep.isComplete ? "linear-gradient(90deg, var(--green), #a6e3a1)" : "linear-gradient(90deg, var(--mauve), var(--blue))");
  }
  if (detailSubtext) detailSubtext.textContent = dep.detail || dep.statusMessage;
  if (logsBox) {
    if (dep.logs && dep.logs.length > 0) {
      let html = "";
      let lineIdx = 1;
      for (const rawLine of dep.logs) {
        const parsed = formatDeployLogLine(rawLine);
        if (parsed) {
          html += `<div class="deploy-log-line">` +
            `<span class="deploy-log-num">${lineIdx}</span>` +
            `<span class="deploy-log-text ${parsed.cls}">${escapeHtml(parsed.text)}</span>` +
          `</div>`;
          lineIdx++;
        }
      }
      html += `<div class="deploy-log-line"><span class="deploy-log-num"></span><span class="deploy-log-text"><span class="terminal-cursor"></span></span></div>`;
      logsBox.innerHTML = html;
      if (deployAutoScrollEnabled) logsBox.scrollTop = logsBox.scrollHeight;
    } else {
      logsBox.innerHTML = `<div class="deploy-log-empty">⚡ Connexion aux logs de déploiement en direct...</div>`;
    }
  }

  if (finishBtn) finishBtn.innerHTML = dep.isComplete ? `<span>🎉 Serveur Prêt !</span>` : `<span>Fermer</span>`;
  if (toConsoleBtn) toConsoleBtn.style.display = (dep.isComplete && !dep.isError) ? "inline-flex" : "none";
  if (cancelBtn) {
    cancelBtn.style.display = dep.isComplete && !dep.isError ? "none" : "inline-flex";
    cancelBtn.disabled = false;
    cancelBtn.innerHTML = dep.isError ? `<span>🗑️</span> Supprimer le serveur en erreur` : `<span>🗑️</span> Annuler et supprimer`;
  }

  updateGameDeployStepUI(dep.stepIndex, dep.isComplete && !dep.isError, dep.isError);

  if (modal) modal.style.display = "flex";

  renderAllGameDeployToasts();
  ensureGameDeployTicker();
  pollAllActiveGameDeployments();
}

// Mise à jour de l'UI des 4 étapes du déploiement
function updateGameDeployStepUI(activeIndex, isComplete = false, isError = false) {
  for (let i = 1; i <= 4; i++) {
    const el = document.getElementById(`game-deploy-step-${i}`);
    if (!el) continue;
    el.className = "game-deploy-step-card";
    if (isError && i === activeIndex) {
      el.classList.add("step-error");
    } else if (isComplete || i < activeIndex) {
      el.classList.add("step-done");
    } else if (i === activeIndex) {
      el.classList.add("step-active");
    }
  }
}

// Polling centralisé de TOUS les déploiements actifs en parallèle
async function pollAllActiveGameDeployments() {
  const serverIds = Object.keys(activeGameDeployments);
  if (serverIds.length === 0) return;

  for (const sId of serverIds) {
    const dep = activeGameDeployments[sId];
    if (!dep) continue;
    if (dep.isComplete || dep.isError) continue; // Déjà achevé

    try {
      const res = await fetch(`/api/games/${encodeURIComponent(sId)}/deploy-status`);
      const json = await res.json();
      if (!json.success || !json.data) continue;

      const d = json.data;
      dep.progressPercent = d.progress_percent || 15;
      dep.stepIndex = d.step_index || 2;
      dep.statusMessage = d.status_message || "En cours...";
      dep.detail = d.detail || d.status_message || "";
      if (Array.isArray(d.logs)) dep.logs = d.logs;
      dep.isComplete = Boolean(d.is_complete);
      dep.isError = Boolean(d.is_error);
      if (d.server_name) dep.name = d.server_name;
      if (d.icon) dep.icon = d.icon;

      // Si ce serveur est actuellement ouvert dans la grande modale, on met à jour son UI
      if (activeGameDeployModalServerId === sId) {
        const heroStatus = document.getElementById("game-deploy-hero-status");
        const heroPercent = document.getElementById("game-deploy-hero-percent-badge");
        const barFill = document.getElementById("game-deploy-bar-fill");
        const percentLabel = document.getElementById("game-deploy-percent-label");
        const detailSubtext = document.getElementById("game-deploy-detail-subtext");
        const logsBox = document.getElementById("game-deploy-logs-box");
        const finishBtn = document.getElementById("btn-game-deploy-finish");
        const toConsoleBtn = document.getElementById("btn-game-deploy-to-console");
        const cancelBtn = document.getElementById("btn-cancel-game-deploy");

        if (heroPercent) heroPercent.textContent = `${dep.progressPercent}%`;
        if (percentLabel) percentLabel.textContent = `${dep.progressPercent}%`;
        if (barFill) {
          barFill.style.width = `${dep.progressPercent}%`;
          if (dep.isError) barFill.style.background = "var(--red)";
          else if (dep.isComplete) barFill.style.background = "linear-gradient(90deg, var(--green), #a6e3a1)";
        }
        if (heroStatus) {
          const cleanSt = formatDeployLogLine(dep.statusMessage);
          heroStatus.textContent = dep.isError ? `❌ ${dep.statusMessage}` : (dep.isComplete ? `🎉 ${dep.statusMessage}` : (cleanSt ? cleanSt.text : dep.statusMessage));
          heroStatus.style.color = dep.isError ? "var(--red)" : (dep.isComplete ? "var(--green)" : "var(--mauve)");
        }
        if (detailSubtext) {
          const cleanDet = formatDeployLogLine(dep.detail);
          detailSubtext.textContent = cleanDet ? cleanDet.text : dep.detail;
        }

        if (logsBox && dep.logs && dep.logs.length > 0) {
          let html = "";
          let lineIdx = 1;
          for (const rawLine of dep.logs) {
            const parsed = formatDeployLogLine(rawLine);
            if (parsed) {
              html += `<div class="deploy-log-line">` +
                `<span class="deploy-log-num">${lineIdx}</span>` +
                `<span class="deploy-log-text ${parsed.cls}">${escapeHtml(parsed.text)}</span>` +
              `</div>`;
              lineIdx++;
            }
          }
          html += `<div class="deploy-log-line"><span class="deploy-log-num"></span><span class="deploy-log-text"><span class="terminal-cursor"></span></span></div>`;
          logsBox.innerHTML = html;
          if (deployAutoScrollEnabled) logsBox.scrollTop = logsBox.scrollHeight;
        }

        updateGameDeployStepUI(dep.stepIndex, dep.isComplete && !dep.isError, dep.isError);

        if (dep.isComplete || dep.isError) {
          if (dep.isError) {
            if (cancelBtn) {
              cancelBtn.style.display = "inline-flex";
              cancelBtn.innerHTML = `<span>🗑️</span> Supprimer le serveur en erreur`;
            }
          } else {
            if (finishBtn) finishBtn.innerHTML = `<span>🎉 Serveur Prêt !</span>`;
            if (toConsoleBtn) toConsoleBtn.style.display = "inline-flex";
            if (cancelBtn) cancelBtn.style.display = "none";
          }
          loadGameServers();
        }
      }

      if (dep.isComplete || dep.isError) {
        loadGameServers();
      }
    } catch (e) {
      console.warn(`Échec polling déploiement ${sId} :`, e);
    }
  }

  // Mettre à jour toutes les cartes popups dans le dock
  renderAllGameDeployToasts();
}

// Rendu et synchronisation de TOUTES les popups flottantes dans le dock
function renderAllGameDeployToasts() {
  const stack = document.getElementById("floating-dock-stack");
  if (!stack) return;

  const serverIds = Object.keys(activeGameDeployments);

  // Supprimer les cartes orphelines qui ne sont plus dans le registre
  const existingCards = Array.from(stack.querySelectorAll(".game-deploy-floating-toast"));
  for (const card of existingCards) {
    const sId = card.getAttribute("data-server-id");
    if (!sId || !activeGameDeployments[sId]) {
      card.remove();
    }
  }

  // Mettre à jour ou injecter la popup de chaque déploiement
  for (const sId of serverIds) {
    const dep = activeGameDeployments[sId];
    if (!dep) continue;

    // Si la grande modale plein écran est actuellement ouverte pour ce serveur précis,
    // on masque sa carte miniature pour éviter le doublon visuel
    const isModalOpenForThisServer = (activeGameDeployModalServerId === sId);

    let card = document.getElementById(`game-deploy-floating-toast-${sId}`);
    if (!card) {
      card = document.createElement("div");
      card.className = "floating-task-card game-deploy-floating-toast";
      card.id = `game-deploy-floating-toast-${sId}`;
      card.setAttribute("data-server-id", sId);

      // Insérer avant le premier toast docker si présent, sinon ajouter à la pile
      const firstDockerToast = stack.querySelector(".docker-deploy-floating-toast");
      if (firstDockerToast && firstDockerToast.parentNode === stack) {
        stack.insertBefore(card, firstDockerToast);
      } else {
        stack.appendChild(card);
      }
    }

    if (isModalOpenForThisServer) {
      card.style.display = "none";
      continue;
    } else {
      card.style.display = "block";
    }

    const iconHtml = dep.iconUrl
      ? `<img src="${dep.iconUrl}" style="width:28px;height:28px;object-fit:contain;filter:drop-shadow(0 2px 5px rgba(0,0,0,0.5));">`
      : (dep.icon || "🎮");

    const cleanSt = formatDeployLogLine(dep.statusMessage);
    const statusText = dep.isError
      ? `❌ Erreur : ${dep.statusMessage}`
      : (dep.isComplete ? `🎉 100% - Prêt et opérationnel !` : `${dep.progressPercent}% - ${cleanSt ? cleanSt.text : dep.statusMessage}`);

    const barBg = dep.isError
      ? "var(--red)"
      : (dep.isComplete ? "linear-gradient(90deg, var(--green), #a6e3a1)" : "var(--mauve)");

    let actionBtnsHtml = "";
    if (dep.isComplete && !dep.isError) {
      actionBtnsHtml = `<button type="button" class="btn btn-success btn-xs" onclick="event.stopPropagation(); openServerConsoleView('${sId}')" title="Accéder à la console">💻</button>`;
    } else if (dep.isError) {
      actionBtnsHtml = `<button type="button" class="btn btn-danger btn-xs" onclick="event.stopPropagation(); cancelAndRemoveGameDeployment('${sId}')" title="Supprimer ce serveur en erreur">🗑️</button>`;
    } else {
      actionBtnsHtml = `<button type="button" class="btn btn-danger btn-xs" onclick="event.stopPropagation(); cancelAndRemoveGameDeployment('${sId}')" title="Annuler le déploiement et supprimer ce serveur" style="padding:3px 7px; background:rgba(243,139,168,0.2); border:1px solid rgba(243,139,168,0.35); color:var(--red); border-radius:6px; cursor:pointer;">🗑️</button>`;
    }

    const closeBtnHtml = (dep.isComplete || dep.isError)
      ? `<button type="button" class="btn-toast-close" onclick="event.stopPropagation(); dismissGameDeployToast('${sId}')" title="Masquer la notification">&times;</button>`
      : "";

    card.innerHTML = `
      <div class="game-deploy-toast-card" onclick="openGameDeployProgressModal('${sId}')" title="Cliquer pour afficher le terminal et les détails en plein écran">
        <div style="display:flex; align-items:center; gap:10px; flex:1; min-width:0;">
          <span style="font-size:1.35rem; display:flex; align-items:center; justify-content:center; flex-shrink:0;">${iconHtml}</span>
          <div style="flex:1; min-width:0;">
            <div class="game-deploy-toast-title" style="font-weight:700; font-size:0.85rem; color:var(--text); overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">${escapeHtml(dep.name)}</div>
            <div class="game-deploy-toast-desc" style="font-size:0.75rem; color:${dep.isError ? "var(--red)" : (dep.isComplete ? "var(--green)" : "var(--subtext0)")}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">${escapeHtml(statusText)}</div>
            <div class="game-deploy-bar-wrap" style="height:4px; border-radius:999px; background:rgba(255,255,255,0.08); margin-top:5px; overflow:hidden;">
              <div style="width:${dep.progressPercent}%; height:100%; background:${barBg}; transition:width 0.4s ease;"></div>
            </div>
          </div>
        </div>
        <div style="display:flex; align-items:center; gap:6px; margin-left:10px;" onclick="event.stopPropagation()">
          <button type="button" class="floating-card-action-btn" onclick="toggleFloatingCardMinimize('game-deploy-floating-toast-${sId}')" title="Réduire / Dérouler">_</button>
          ${actionBtnsHtml}
          <button type="button" class="btn btn-secondary btn-xs" onclick="openGameDeployProgressModal('${sId}')" title="Agrandir la fenêtre de déploiement en plein écran">⛶</button>
          ${closeBtnHtml}
        </div>
      </div>
    `;
  }

  updateFloatingDockLayout();
}

// Réduit la modale plein écran en popup flottante dans le dock
function minimizeGameDeployModal() {
  const modal = document.getElementById("modal-game-deploy-progress");
  if (modal) modal.style.display = "none";
  activeGameDeployModalServerId = null;
  renderAllGameDeployToasts();
  updateFloatingDockLayout();
}

// Ferme la modale (réduit également en popup si toujours en cours)
function closeGameDeployProgressModal() {
  const modal = document.getElementById("modal-game-deploy-progress");
  if (modal) modal.style.display = "none";
  activeGameDeployModalServerId = null;
  renderAllGameDeployToasts();
  updateFloatingDockLayout();
  loadGameServers();
}

// Masque définitivement une popup de déploiement achevé
function dismissGameDeployToast(serverId) {
  if (serverId && activeGameDeployments[serverId]) {
    delete activeGameDeployments[serverId];
  }
  const card = document.getElementById(`game-deploy-floating-toast-${serverId}`);
  if (card) card.remove();
  updateFloatingDockLayout();
  checkStopGameDeployTicker();
}

// Annule un déploiement et supprime le conteneur et les fichiers
async function cancelAndRemoveGameDeployment(targetServerId) {
  const serverId = targetServerId || activeGameDeployModalServerId;
  if (!serverId) return;

  const dep = activeGameDeployments[serverId];
  const sName = dep ? dep.name : serverId;

  const ok = confirm(`Voulez-vous vraiment annuler le déploiement et supprimer le serveur "${sName}" ainsi que toutes ses données ?`);
  if (!ok) return;

  const cancelBtn = document.getElementById("btn-cancel-game-deploy");
  if (cancelBtn && activeGameDeployModalServerId === serverId) {
    cancelBtn.disabled = true;
    cancelBtn.innerHTML = `<span>⏳</span> Annulation en cours...`;
  }

  try {
    const res = await fetch(`/api/games/${encodeURIComponent(serverId)}/delete?delete_data=true`, {
      method: "POST"
    });
    const json = await res.json();
    if (json.success) {
      showToast("Déploiement annulé et serveur supprimé avec succès.", "success");
    } else {
      showToast("Erreur lors de la suppression : " + (json.message || "inconnue"), "error");
    }
  } catch (err) {
    console.error("Erreur annulation déploiement :", err);
    showToast("Erreur lors de l'annulation du déploiement.", "error");
  } finally {
    if (activeGameDeployModalServerId === serverId) {
      closeGameDeployProgressModal();
    }
    delete activeGameDeployments[serverId];
    const card = document.getElementById(`game-deploy-floating-toast-${serverId}`);
    if (card) card.remove();
    updateFloatingDockLayout();
    checkStopGameDeployTicker();
    loadGameServers();
  }
}

// Bascule directement sur l'onglet console pour le serveur actuellement sélectionné
function jumpToGameConsole() {
  const serverId = activeGameDeployModalServerId;
  closeGameDeployProgressModal();
  if (serverId) {
    switchConsoleActiveServer(serverId);
    switchGamesSubtab("console");
  }
}

// --------------------------------------------------------------------------
// GESTION ET NETTOYAGE DES IMAGES DOCKER (DISK PRUNING)
// --------------------------------------------------------------------------
let dockerImagesData = null;
let currentDockerImageFilter = "all";

async function loadDockerImages(forceToast = false) {
  try {
    const res = await fetch("/api/docker/images");
    const json = await res.json();
    if (json.success && json.data) {
      dockerImagesData = json.data;
      renderDockerImagesOverview();
      renderDockerImagesTable();
      if (forceToast) showToast("Images Docker actualisées", "info");
    } else {
      showToast(`Erreur chargement images : ${json.message || "inconnue"}`, "error");
    }
  } catch (e) {
    console.error("Échec chargement images Docker :", e);
    showToast(`Erreur réseau : ${e.message}`, "error");
  }
}

function renderDockerImagesOverview() {
  if (!dockerImagesData) return;
  const totalEl = document.getElementById("docker-metric-total-images");
  const sizeEl = document.getElementById("docker-metric-total-size");
  const unusedEl = document.getElementById("docker-metric-unused-images");
  const recEl = document.getElementById("docker-metric-reclaimable");
  const countBadge = document.getElementById("docker-images-count");

  if (totalEl) totalEl.textContent = dockerImagesData.total_images;
  if (sizeEl) sizeEl.textContent = dockerImagesData.total_size;
  if (unusedEl) unusedEl.textContent = dockerImagesData.unused_images;
  if (recEl) recEl.textContent = dockerImagesData.reclaimable_size;
  if (countBadge) countBadge.textContent = dockerImagesData.total_images;

  const pillAll = document.getElementById("count-pill-all");
  const pillUnused = document.getElementById("count-pill-unused");
  const pillUsed = document.getElementById("count-pill-used");

  const usedCount = dockerImagesData.total_images - dockerImagesData.unused_images;
  if (pillAll) pillAll.textContent = dockerImagesData.total_images;
  if (pillUnused) pillUnused.textContent = dockerImagesData.unused_images;
  if (pillUsed) pillUsed.textContent = usedCount;
}

function setImagesFilter(filter) {
  currentDockerImageFilter = filter;
  ["all", "unused", "used"].forEach(f => {
    const el = document.getElementById(`pill-images-filter-${f}`) || document.getElementById(`pill-filter-${f}`);
    if (el) el.classList.toggle("active", f === filter);
  });
  renderDockerImagesTable();
}

function filterDockerImagesList() {
  renderDockerImagesTable();
}

function renderDockerImagesTable() {
  const tbody = document.getElementById("docker-images-table-body");
  if (!tbody || !dockerImagesData) return;

  const search = (document.getElementById("docker-images-search")?.value || "").toLowerCase().trim();

  let list = dockerImagesData.images || [];

  if (currentDockerImageFilter === "unused") {
    list = list.filter(i => !i.is_used);
  } else if (currentDockerImageFilter === "used") {
    list = list.filter(i => i.is_used);
  }

  if (search) {
    list = list.filter(i => 
      i.repository.toLowerCase().includes(search) || 
      i.tag.toLowerCase().includes(search) || 
      i.id.toLowerCase().includes(search)
    );
  }

  if (list.length === 0) {
    tbody.innerHTML = `
      <tr>
        <td colspan="5" style="text-align:center; padding:50px 20px; color:var(--subtext0);">
          <span style="font-size:2rem; display:block; margin-bottom:10px;">✨</span>
          Aucune image Docker trouvée selon les critères sélectionnés.
        </td>
      </tr>
    `;
    return;
  }

  tbody.innerHTML = list.map(img => {
    const isUnused = !img.is_used;
    const repoTag = `${img.repository}:${img.tag}`;
    const statusHtml = isUnused 
      ? `<span class="badge" style="background:rgba(249,226,175,0.15); color:var(--yellow); border:1px solid rgba(249,226,175,0.3); font-weight:700;">🟡 Inutilisée (Orpheline)</span>`
      : `<span class="badge badge-success" title="Utilisée par : ${img.used_by.join(", ")}">🟢 Active (${img.used_by.join(", ")})</span>`;

    return `
      <tr class="docker-image-row ${isUnused ? "image-unused" : "image-used"}">
        <td>
          <div style="display:flex; align-items:center; gap:10px;">
            <span style="font-size:1.3rem;">🐳</span>
            <div style="min-width:0;">
              <div style="font-weight:700; color:var(--text); word-break:break-all;" title="${escapeHtml(repoTag)}">${escapeHtml(img.repository)}<span style="color:var(--mauve); font-weight:800;">:${escapeHtml(img.tag)}</span></div>
              <div style="font-size:0.72rem; color:var(--subtext0);">${escapeHtml(img.created_at)}</div>
            </div>
          </div>
        </td>
        <td>
          <span class="font-mono text-subtext" style="font-size:0.8rem; background:rgba(255,255,255,0.04); padding:2px 6px; border-radius:4px;">${img.id}</span>
        </td>
        <td>
          <span class="font-mono" style="font-weight:700; color:var(--text);">${escapeHtml(img.size)}</span>
        </td>
        <td>
          ${statusHtml}
        </td>
        <td style="text-align:right;">
          ${isUnused ? `
            <button type="button" class="btn btn-danger btn-xs" onclick="confirmDeleteDockerImage('${img.id}', '${escapeHtml(repoTag)}')" title="Supprimer cette image pour libérer de l'espace">
              <span>🗑️</span> Supprimer
            </button>
          ` : `
            <button type="button" class="btn btn-secondary btn-xs" disabled title="Cette image est utilisée par un conteneur et ne peut pas être supprimée">
              <span>🔒</span> En usage
            </button>
          `}
        </td>
      </tr>
    `;
  }).join("");
}

async function confirmDeleteDockerImage(id, repoTag) {
  if (!confirm(`Voulez-vous vraiment supprimer l'image Docker suivante ?\n\n${repoTag} (${id})\n\nCette action libérera l'espace disque immédiatement.`)) {
    return;
  }

  try {
    const res = await fetch(`/api/docker/images/${encodeURIComponent(id)}`, {
      method: "DELETE"
    });
    const json = await res.json();
    if (json.success) {
      showToast(json.data || "Image supprimée avec succès", "success");
      loadDockerImages();
    } else {
      showToast(`Erreur : ${json.message || "inconnue"}`, "error");
    }
  } catch (e) {
    showToast(`Erreur réseau : ${e.message}`, "error");
  }
}

async function confirmPruneDockerImages() {
  const unusedCount = dockerImagesData ? dockerImagesData.unused_images : 0;
  const reclaimable = dockerImagesData ? dockerImagesData.reclaimable_size : "inconnu";
  
  if (!confirm(`🧹 NETTOYAGE COMPLET DES IMAGES DOCKER\n\nÊtes-vous sûr de vouloir supprimer TOUTES les images Docker inutilisées ?\n\n• Images ciblées : ${unusedCount} orpheline(s)\n• Espace estimé récupérable : ${reclaimable}\n\nAucun conteneur en cours d'exécution ou configuré ne sera affecté.`)) {
    return;
  }

  showToast("Purge des images Docker en cours...", "info");
  const btn = document.getElementById("btn-prune-images");
  if (btn) btn.disabled = true;

  try {
    const res = await fetch("/api/docker/images/prune", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ all: true })
    });
    const json = await res.json();
    if (json.success) {
      showToast(`Nettoyage réussi : ${json.data || "espace libéré"}`, "success");
      loadDockerImages();
    } else {
      showToast(`Erreur de nettoyage : ${json.message || "inconnue"}`, "error");
    }
  } catch (e) {
    showToast(`Erreur réseau : ${e.message}`, "error");
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ==========================================================================
// 👥 GESTION DES UTILISATEURS, GROUPES ET SÉCURITÉ (STEvE_OS NAS)
// ==========================================================================

let usersData = [];
let groupsData = [];
let activeUsersSubtab = 'accounts';
let userRoleFilter = 'all';
let userViewMode = (typeof localStorage !== 'undefined' && localStorage.getItem('noos_users_view_mode')) || 'grid';
let createUserStep = 1;
let currentLoggedInUser = '';

function switchUsersSubtab(subtabId, updateHash = true) {
  activeUsersSubtab = subtabId;
  try {
    localStorage.setItem("noos_subtab_users", subtabId);
  } catch (e) {}
  if (updateHash && activeTab === "tab-users") {
    updateUrlHash("tab-users", subtabId);
  }
  document.querySelectorAll('.users-subtab-btn').forEach(btn => {
    btn.classList.toggle('active', btn.id === `btn-users-subtab-${subtabId}`);
  });
  document.querySelectorAll('.users-subview').forEach(view => {
    view.classList.toggle('active', view.id === `users-subview-${subtabId}`);
    view.style.display = view.id === `users-subview-${subtabId}` ? 'block' : 'none';
  });

  if (subtabId === 'audit') loadSecurityAudit();
  if (subtabId === 'sessions') loadActiveSessions();
}

async function loadUsersAndGroups(showNotice = false) {
  try {
    const [usersRes, groupsRes] = await Promise.all([
      fetch('/api/users').then(async r => {
        const data = await r.json().catch(() => ({}));
        if (!r.ok) {
          throw new Error(data.error || data.message || `Erreur HTTP ${r.status}`);
        }
        return data;
      }),
      fetch('/api/groups').then(async r => {
        const data = await r.json().catch(() => ({}));
        if (!r.ok) {
          throw new Error(data.error || data.message || `Erreur HTTP ${r.status}`);
        }
        return data;
      })
    ]);

    if (usersRes && usersRes.success) {
      usersData = usersRes.users || [];
      if (usersRes.current_user) {
        currentLoggedInUser = usersRes.current_user;
      }
      if (usersRes.main_admin_user) {
        mainAdminUser = usersRes.main_admin_user;
      }
      updateUsersHeroStats();
      renderUsersList();
    } else {
      showToast(usersRes.error || "Impossible de charger les utilisateurs", "error");
    }

    if (groupsRes && groupsRes.success) {
      groupsData = groupsRes.groups || [];
      renderGroupsList();
    }

    if (showNotice) {
      showToast("Données utilisateurs actualisées avec succès", "success");
    }
  } catch (err) {
    console.error("Erreur lors du chargement des utilisateurs/groupes :", err);
    showToast(err.message || "Erreur lors de l'actualisation des utilisateurs", "error");
  }
}

async function loadUsersBadge() {
  try {
    const res = await fetch('/api/users').then(r => r.json());
    if (res && res.success && res.users) {
      const badge = document.getElementById("users-count-badge");
      if (badge) {
        badge.textContent = res.users.length;
        badge.style.display = res.users.length > 0 ? "inline-block" : "none";
      }
    }
  } catch (e) {
    // Silencieux
  }
}

function updateUsersHeroStats() {
  const managedUsers = usersData.filter(u => u.username !== 'root' && !u.is_system);
  const total = managedUsers.length;
  const admins = managedUsers.filter(u => u.is_admin).length;
  const samba = managedUsers.filter(u => u.samba_enabled).length;
  const activeSessions = usersData.reduce((acc, u) => acc + (u.active_sessions_count || 0), 0);

  const elTotal = document.getElementById('users-stat-total');
  const elAdmins = document.getElementById('users-stat-admins');
  const elSamba = document.getElementById('users-stat-samba');
  const elSessions = document.getElementById('users-stat-sessions');

  if (elTotal) elTotal.textContent = total;
  if (elAdmins) elAdmins.textContent = admins;
  if (elSamba) elSamba.textContent = samba;
  if (elSessions) elSessions.textContent = activeSessions;

  const badge = document.getElementById("users-count-badge");
  if (badge) {
    badge.textContent = total;
    badge.style.display = total > 0 ? "inline-block" : "none";
  }
}

function setUserRoleFilter(filter) {
  userRoleFilter = filter;
  document.querySelectorAll('.user-filter-chip').forEach(chip => {
    chip.classList.toggle('active', chip.getAttribute('data-filter') === filter);
  });
  renderUsersList();
}

function setUserViewMode(mode) {
  userViewMode = mode;
  try {
    localStorage.setItem('noos_users_view_mode', mode);
  } catch(e) {}

  const btnGrid = document.getElementById('btn-users-view-grid');
  const btnTable = document.getElementById('btn-users-view-table');
  const gridContainer = document.getElementById('users-cards-grid');
  const tableContainer = document.getElementById('users-table-container');

  if (btnGrid) {
    btnGrid.style.background = mode === 'grid' ? 'var(--mauve)' : 'transparent';
    btnGrid.style.color = mode === 'grid' ? '#11111b' : 'var(--subtext1)';
    btnGrid.style.fontWeight = mode === 'grid' ? '600' : 'normal';
  }
  if (btnTable) {
    btnTable.style.background = mode === 'table' ? 'var(--mauve)' : 'transparent';
    btnTable.style.color = mode === 'table' ? '#11111b' : 'var(--subtext1)';
    btnTable.style.fontWeight = mode === 'table' ? '600' : 'normal';
  }

  if (gridContainer) gridContainer.style.display = mode === 'grid' ? 'grid' : 'none';
  if (tableContainer) tableContainer.style.display = mode === 'table' ? 'block' : 'none';

  renderUsersList();
}

function filterUsersList() {
  renderUsersList();
}

function renderUsersList() {
  try {
    const search = (document.getElementById('users-search-input')?.value || '').toLowerCase().trim();
  const gridContainer = document.getElementById('users-cards-grid');
  const tableBody = document.getElementById('users-table-body');

  let filtered = usersData.filter(u => {
    const isRootUser = u.username === 'root';
    // Masquer root par défaut sauf si le filtre spécifique 'system' est sélectionné
    if (isRootUser && userRoleFilter !== 'system') {
      return false;
    }

    // Filtre rôle
    if (userRoleFilter === 'admin' && !u.is_admin) return false;
    if (userRoleFilter === 'storage' && !u.groups.includes('storage')) return false;
    if (userRoleFilter === 'docker' && !u.groups.includes('docker')) return false;
    if (userRoleFilter === 'locked' && !u.locked) return false;
    if (userRoleFilter === 'system' && !isRootUser && !u.is_system) return false;

    // Filtre texte
    if (search) {
      const matchUsername = u.username.toLowerCase().includes(search);
      const matchFullName = (u.full_name || '').toLowerCase().includes(search);
      const matchEmail = (u.email || '').toLowerCase().includes(search);
      const matchGroup = u.groups.some(g => g.toLowerCase().includes(search));
      if (!matchUsername && !matchFullName && !matchEmail && !matchGroup) return false;
    }
    return true;
  });

  if (filtered.length === 0) {
    const emptyMsg = `<div style="grid-column: 1 / -1; text-align: center; padding: 48px 20px; background: var(--base); border-radius: var(--radius-lg); border: 1px dashed var(--surface0); color: var(--subtext0);">
      <div style="font-size: 2.2rem; margin-bottom: 12px;">🔍</div>
      <h3 style="margin: 0; color: var(--text);">Aucun utilisateur trouvé</h3>
      <p style="margin: 8px 0 0 0; font-size: 0.9rem;">Aucun compte ne correspond aux critères de recherche actuels.</p>
    </div>`;
    if (gridContainer) gridContainer.innerHTML = emptyMsg;
    if (tableBody) tableBody.innerHTML = `<tr><td colspan="6" style="text-align:center; padding: 36px;">Aucun utilisateur trouvé.</td></tr>`;
    return;
  }

  // Rendu Grille
  if (gridContainer && userViewMode === 'grid') {
    gridContainer.innerHTML = filtered.map(u => {
      const initial = (u.full_name || u.username).charAt(0).toUpperCase();
      const colorClass = `user-avatar-${u.avatar_color || 'sapphire'}`;
      const isSelf = u.username === currentLoggedInUser;
      const isRoot = u.username === 'root';

      let badgesHtml = '';
      if (u.locked) {
        badgesHtml += `<span class="user-badge user-badge-locked">🔒 Verrouillé</span>`;
      } else {
        badgesHtml += `<span class="user-badge user-badge-active">🟢 Actif</span>`;
      }
      if (u.is_admin) {
        badgesHtml += `<span class="user-badge user-badge-admin">👑 Admin</span>`;
      }
      if (u.samba_enabled) {
        badgesHtml += `<span class="user-badge user-badge-storage">📁 Samba</span>`;
      }
      if (u.groups.includes('docker')) {
        badgesHtml += `<span class="user-badge user-badge-docker">🐳 Docker</span>`;
      }
      if (u.active_sessions_count > 0) {
        badgesHtml += `<span class="user-badge" style="background: rgba(166, 227, 161, 0.15); color: #a6e3a1; border: 1px solid rgba(166, 227, 161, 0.3);">⚡ ${u.active_sessions_count} session(s)</span>`;
      }

      const groupsChips = u.groups.slice(0, 4).map(g => 
        `<span style="padding: 2px 7px; border-radius: 4px; background: var(--surface0); font-size: 0.72rem; color: var(--subtext1);">${escapeHtml(g)}</span>`
      ).join(' ') + (u.groups.length > 4 ? ` <span style="font-size: 0.72rem; color: var(--subtext0);">+${u.groups.length - 4}</span>` : '');

      const isShellNoLogin = u.shell.endsWith('nologin') || u.shell.endsWith('false');
      const shellBadge = isShellNoLogin 
        ? `<span style="color: var(--subtext0); font-size: 0.78rem;">🔒 Aucun shell (Partage seul)</span>`
        : `<span style="color: var(--peach); font-size: 0.78rem; font-family: monospace;">🐚 ${escapeHtml(u.shell.split('/').pop())}</span>`;

      return `
        <div class="user-card ${u.locked ? 'is-locked' : ''}">
          <!-- EN-TÊTE CARTE -->
          <div style="display: flex; gap: 14px; align-items: flex-start;">
            <div class="user-avatar-circle ${colorClass}">${initial}</div>
            <div style="flex: 1; min-width: 0;">
              <div style="display: flex; align-items: center; gap: 8px; flex-wrap: wrap;">
                <h4 style="margin: 0; font-size: 1.05rem; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                  ${escapeHtml(u.username)}
                </h4>
                ${isSelf ? '<span class="badge" style="background: rgba(203, 166, 247, 0.2); color: var(--mauve); font-size: 0.7rem; padding: 2px 6px; border-radius: 4px;">Vous</span>' : ''}
              </div>
              <div style="font-size: 0.85rem; color: var(--subtext0); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                ${escapeHtml(u.full_name || u.username)}
              </div>
              ${u.email ? `<div style="font-size: 0.78rem; color: var(--subtext0); margin-top: 1px;">${escapeHtml(u.email)}</div>` : ''}
            </div>
          </div>

          <!-- BADGES DE RÔLES -->
          <div style="display: flex; flex-wrap: wrap; gap: 6px;">
            ${badgesHtml}
          </div>

          <!-- DÉTAILS SYSTÈME -->
          <div style="background: var(--surface0); border-radius: var(--radius-md); padding: 12px; font-size: 0.82rem; display: flex; flex-direction: column; gap: 6px;">
            <div style="display: flex; justify-content: space-between;">
              <span style="color: var(--subtext0);">Dossier personnel :</span>
              <span style="color: var(--text); font-family: monospace;">${escapeHtml(u.home_dir)} (${formatBytes(u.disk_usage_bytes || 0)})</span>
            </div>
            <div style="display: flex; justify-content: space-between; align-items: center;">
              <span style="color: var(--subtext0);">Accès terminal :</span>
              ${shellBadge}
            </div>
            <div style="display: flex; justify-content: space-between; align-items: center;">
              <span style="color: var(--subtext0);">Partage Samba :</span>
              <span style="color: ${u.samba_enabled ? 'var(--teal)' : 'var(--subtext0)'}; font-weight: 600;">
                ${u.samba_enabled ? '✓ Connecté (SMB)' : '✗ Désactivé'}
              </span>
            </div>
            <div style="margin-top: 2px; display: flex; gap: 4px; align-items: center; flex-wrap: wrap;">
              <span style="color: var(--subtext0); font-size: 0.75rem; margin-right: 4px;">Groupes :</span>
              ${groupsChips}
            </div>
          </div>

          <!-- ACTIONS CARD -->
          ${isRoot ? `
            <div style="display: flex; gap: 8px; justify-content: space-between; align-items: center; border-top: 1px solid rgba(255,255,255,0.06); padding-top: 14px; font-size: 0.8rem; color: var(--subtext0);">
              <span>🛡️ Superviseur système (root)</span>
              <span style="padding: 2px 8px; border-radius: 4px; background: rgba(243, 139, 168, 0.15); color: var(--red); font-size: 0.72rem; font-weight: 600;">🔒 Immuable</span>
            </div>
          ` : (typeof mainAdminUser !== 'undefined' && u.username === mainAdminUser) ? `
            <div style="display: flex; gap: 6px; justify-content: space-between; align-items: center; border-top: 1px solid rgba(255,255,255,0.06); padding-top: 14px; flex-wrap: wrap;">
              <span style="font-size: 0.75rem; color: var(--mauve); font-weight: 600;">👑 Admin NixOS</span>
              <div style="display: flex; gap: 6px;">
                <button type="button" class="btn btn-secondary btn-xs" onclick="openSambaPasswordModal('${escapeHtml(u.username)}')" title="Définir ou synchroniser le mot de passe réseau Samba (SMB)">
                  <span>📁</span> ${u.samba_enabled ? 'Mdp Samba' : 'Activer Samba'}
                </button>
                <button type="button" class="btn btn-secondary btn-xs" onclick="openChangePasswordModal('${escapeHtml(u.username)}')" title="Changer le mot de passe">
                  <span>🔑</span> Mdp
                </button>
                <button type="button" class="btn btn-secondary btn-xs" onclick="openEditUserModal('${escapeHtml(u.username)}')" title="Modifier les informations (profil, avatar)">
                  <span>✏️</span> Profil
                </button>
              </div>
            </div>
          ` : `
            <div style="display: flex; gap: 6px; justify-content: flex-end; align-items: center; border-top: 1px solid rgba(255,255,255,0.06); padding-top: 14px;">
              <button type="button" class="btn btn-secondary btn-xs" onclick="openSambaPasswordModal('${escapeHtml(u.username)}')" title="Configurer l'accès Samba (SMB)">
                <span>📁</span> ${u.samba_enabled ? 'Samba' : '+ SMB'}
              </button>
              <button type="button" class="btn btn-secondary btn-xs" onclick="openEditUserModal('${escapeHtml(u.username)}')" title="Modifier les informations et groupes">
                <span>✏️</span> Modifier
              </button>
              <button type="button" class="btn btn-secondary btn-xs" onclick="openChangePasswordModal('${escapeHtml(u.username)}')" title="Changer le mot de passe">
                <span>🔑</span> Mdp
              </button>
              ${!isSelf ? `
                <button type="button" class="btn btn-secondary btn-xs" onclick="toggleUserLock('${escapeHtml(u.username)}')" title="${u.locked ? 'Déverrouiller le compte' : 'Verrouiller le compte'}">
                  <span>${u.locked ? '🔓' : '🔒'}</span>
                </button>
                <button type="button" class="btn btn-secondary btn-xs" style="color: var(--red);" onclick="openDeleteUserModal('${escapeHtml(u.username)}')" title="Supprimer l'utilisateur">
                  <span>🗑️</span>
                </button>
              ` : ''}
            </div>
          `}
        </div>
      `;
    }).join('');
  }

  // Rendu Tableau
  if (tableBody && userViewMode === 'table') {
    tableBody.innerHTML = filtered.map(u => {
      const isSelf = u.username === currentLoggedInUser;
      const isRoot = u.username === 'root';
      const colorClass = `user-avatar-${u.avatar_color || 'sapphire'}`;
      const initial = (u.full_name || u.username).charAt(0).toUpperCase();

      return `
        <tr style="border-bottom: 1px solid var(--surface0); font-size: 0.9rem;">
          <td style="padding: 12px 16px;">
            <div style="display: flex; align-items: center; gap: 10px;">
              <div class="user-avatar-circle ${colorClass}" style="width: 34px; height: 34px; font-size: 0.9rem;">${initial}</div>
              <div>
                <strong style="color: var(--text);">${escapeHtml(u.username)}</strong>
                ${isSelf ? '<span class="badge" style="background: rgba(203, 166, 247, 0.2); color: var(--mauve); font-size: 0.65rem; padding: 1px 4px; margin-left: 4px;">Vous</span>' : ''}
                <div style="font-size: 0.78rem; color: var(--subtext0);">${escapeHtml(u.full_name || '')}</div>
              </div>
            </div>
          </td>
          <td style="padding: 12px 16px;">
            ${u.locked ? '<span class="user-badge user-badge-locked">🔒 Verrouillé</span>' : '<span class="user-badge user-badge-active">🟢 Actif</span>'}
            ${u.is_admin ? '<span class="user-badge user-badge-admin" style="margin-left: 4px;">👑 Admin</span>' : ''}
          </td>
          <td style="padding: 12px 16px;">
            <div style="display: flex; gap: 4px; flex-wrap: wrap; max-width: 220px;">
              ${u.groups.slice(0, 3).map(g => `<span style="padding: 2px 6px; border-radius: 4px; background: var(--surface0); font-size: 0.72rem; color: var(--subtext1);">${escapeHtml(g)}</span>`).join('')}
              ${u.groups.length > 3 ? `<span style="font-size: 0.72rem; color: var(--subtext0);">+${u.groups.length - 3}</span>` : ''}
            </div>
          </td>
          <td style="padding: 12px 16px; font-family: monospace; font-size: 0.82rem;">
            ${formatBytes(u.disk_usage_bytes || 0)}
          </td>
          <td style="padding: 12px 16px; font-size: 0.82rem;">
            <div>${u.samba_enabled ? '<span style="color: var(--teal);">✓ Samba</span>' : '<span style="color: var(--subtext0);">✗ Samba</span>'}</div>
            <div style="color: var(--subtext0); font-size: 0.75rem;">${u.shell.endsWith('nologin') ? 'Pas de shell' : escapeHtml(u.shell.split('/').pop())}</div>
          </td>
          <td style="padding: 12px 16px; text-align: right;">
            ${isRoot ? `
              <span style="padding: 3px 8px; border-radius: 4px; background: rgba(243, 139, 168, 0.15); color: var(--red); font-size: 0.72rem; font-weight: 600;">🔒 Immuable</span>
            ` : (typeof mainAdminUser !== 'undefined' && u.username === mainAdminUser) ? `
              <div style="display: flex; gap: 6px; justify-content: flex-end;">
                <button type="button" class="btn btn-secondary btn-xs" onclick="openSambaPasswordModal('${escapeHtml(u.username)}')" title="Mot de passe Samba">📁 SMB</button>
                <button type="button" class="btn btn-secondary btn-xs" onclick="openChangePasswordModal('${escapeHtml(u.username)}')" title="Mot de passe">🔑 Mdp</button>
                <button type="button" class="btn btn-secondary btn-xs" onclick="openEditUserModal('${escapeHtml(u.username)}')" title="Profil">✏️</button>
              </div>
            ` : `
              <div style="display: flex; gap: 6px; justify-content: flex-end;">
                <button type="button" class="btn btn-secondary btn-xs" onclick="openSambaPasswordModal('${escapeHtml(u.username)}')" title="Accès Samba">📁</button>
                <button type="button" class="btn btn-secondary btn-xs" onclick="openEditUserModal('${escapeHtml(u.username)}')" title="Modifier">✏️</button>
                <button type="button" class="btn btn-secondary btn-xs" onclick="openChangePasswordModal('${escapeHtml(u.username)}')" title="Mot de passe">🔑</button>
                ${!isSelf ? `
                  <button type="button" class="btn btn-secondary btn-xs" onclick="toggleUserLock('${escapeHtml(u.username)}')" title="${u.locked ? 'Déverrouiller' : 'Verrouiller'}">${u.locked ? '🔓' : '🔒'}</button>
                  <button type="button" class="btn btn-secondary btn-xs" style="color: var(--red);" onclick="openDeleteUserModal('${escapeHtml(u.username)}')" title="Supprimer">🗑️</button>
                ` : ''}
              </div>
            `}
          </td>
        </tr>
      `;
    }).join('');
  }
  } catch (err) {
    console.error("Erreur de rendu dans renderUsersList :", err);
  }
}

function renderGroupsList() {
  const container = document.getElementById('groups-list-container');
  if (!container) return;

  if (groupsData.length === 0) {
    container.innerHTML = `<div style="grid-column: 1 / -1; text-align: center; padding: 36px; color: var(--subtext0);">Aucun groupe trouvé.</div>`;
    return;
  }

  container.innerHTML = groupsData.map(g => {
    const isSensitive = ['wheel', 'docker', 'disk', 'storage'].includes(g.name);
    const memberChips = g.members.map(m => 
      `<span style="padding: 3px 8px; border-radius: 999px; background: var(--surface1); font-size: 0.75rem; color: var(--text);">👤 ${escapeHtml(m)}</span>`
    ).join(' ') || '<span style="color: var(--subtext0); font-size: 0.8rem; font-style: italic;">Aucun membre assigné</span>';

    return `
      <div class="card" style="background: var(--base); border: 1px solid var(--surface0); border-radius: var(--radius-lg); padding: 20px; display: flex; flex-direction: column; justify-content: space-between; gap: 14px;">
        <div>
          <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
            <div style="display: flex; align-items: center; gap: 8px;">
              <span style="font-size: 1.2rem;">${isSensitive ? '🛡️' : '👥'}</span>
              <strong style="font-size: 1.05rem; color: var(--text);">${escapeHtml(g.name)}</strong>
            </div>
            <div>
              ${g.is_system ? '<span class="badge" style="background: rgba(137, 180, 250, 0.15); color: var(--blue); font-size: 0.7rem; padding: 2px 7px; border-radius: 4px;">Système</span>' : '<span class="badge" style="background: rgba(166, 227, 161, 0.15); color: var(--green); font-size: 0.7rem; padding: 2px 7px; border-radius: 4px;">Personnalisé</span>'}
            </div>
          </div>
          <p style="margin: 0 0 12px 0; font-size: 0.85rem; color: var(--subtext0); min-height: 38px;">
            ${escapeHtml(g.description)}
          </p>
          <div style="background: var(--surface0); border-radius: var(--radius-md); padding: 10px; margin-bottom: 8px;">
            <div style="font-size: 0.75rem; color: var(--subtext0); margin-bottom: 6px; font-weight: 600;">Membres (${g.members.length}) :</div>
            <div style="display: flex; flex-wrap: wrap; gap: 6px;">
              ${memberChips}
            </div>
          </div>
        </div>
        <div style="display: flex; justify-content: flex-end; gap: 8px; border-top: 1px solid rgba(255,255,255,0.06); padding-top: 12px;">
          <button type="button" class="btn btn-secondary btn-xs" onclick="openManageGroupMembersModal('${escapeHtml(g.name)}')">
            <span>👥</span> Gérer les membres
          </button>
          ${!g.is_system ? `
            <button type="button" class="btn btn-secondary btn-xs" style="color: var(--red);" onclick="deleteGroup('${escapeHtml(g.name)}')">
              <span>🗑️</span> Supprimer
            </button>
          ` : ''}
        </div>
      </div>
    `;
  }).join('');
}

async function loadSecurityAudit() {
  try {
    const res = await fetch('/api/users/audit').then(r => r.json());
    if (!res || !res.success || !res.report) return;

    const report = res.report;
    const warningsBox = document.getElementById('security-warnings-box');
    const matrixContainer = document.getElementById('security-matrix-container');

    if (warningsBox) {
      if (report.warnings && report.warnings.length > 0) {
        warningsBox.style.display = 'block';
        warningsBox.innerHTML = `
          <div style="background: rgba(250, 179, 135, 0.1); border: 1px solid rgba(250, 179, 135, 0.3); border-radius: var(--radius-md); padding: 16px;">
            <h4 style="margin: 0 0 8px 0; color: var(--peach); display: flex; align-items: center; gap: 8px;">
              <span>⚠️</span> Alertes de Sécurité & Bonnes Pratiques
            </h4>
            <ul style="margin: 0; padding-left: 20px; font-size: 0.88rem; color: var(--text);">
              ${report.warnings.map(w => `<li style="margin-bottom: 4px;">${escapeHtml(w)}</li>`).join('')}
            </ul>
          </div>
        `;
      } else {
        warningsBox.style.display = 'none';
      }
    }

    if (matrixContainer) {
      matrixContainer.innerHTML = `
        <table class="security-matrix-table">
          <thead>
            <tr style="color: var(--subtext0); font-size: 0.82rem; border-bottom: 1px solid var(--surface0);">
              <th>Utilisateur</th>
              <th>Dashboard Admin</th>
              <th>Samba (SMB)</th>
              <th>SFTP / SSH</th>
              <th>Conteneurs Docker</th>
              <th>Virtualisation KVM</th>
              <th>GPU Transcodage</th>
            </tr>
          </thead>
          <tbody>
            ${report.users.map(u => `
              <tr>
                <td>
                  <strong>${escapeHtml(u.username)}</strong>
                  <div style="font-size: 0.75rem; color: var(--subtext0);">${escapeHtml(u.full_name || '')}</div>
                </td>
                <td>${u.is_admin ? '<span style="color: var(--mauve); font-weight: bold;">✓ OUI (root)</span>' : '<span style="color: var(--subtext0);">✗</span>'}</td>
                <td>${u.samba_enabled ? '<span style="color: var(--teal); font-weight: bold;">✓ OUI</span>' : '<span style="color: var(--subtext0);">✗</span>'}</td>
                <td>${!u.shell.endsWith('nologin') ? '<span style="color: var(--peach); font-weight: bold;">✓ OUI</span>' : '<span style="color: var(--subtext0);">✗ Non</span>'}</td>
                <td>${u.groups.includes('docker') ? '<span style="color: var(--blue); font-weight: bold;">✓ OUI</span>' : '<span style="color: var(--subtext0);">✗</span>'}</td>
                <td>${(u.groups.includes('kvm') || u.groups.includes('libvirtd')) ? '<span style="color: var(--green); font-weight: bold;">✓ OUI</span>' : '<span style="color: var(--subtext0);">✗</span>'}</td>
                <td>${(u.groups.includes('video') || u.groups.includes('render')) ? '<span style="color: var(--text); font-weight: bold;">✓ OUI</span>' : '<span style="color: var(--subtext0);">✗</span>'}</td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      `;
    }
  } catch (err) {
    console.error("Erreur chargement audit de sécurité :", err);
  }
}

async function loadActiveSessions() {
  const container = document.getElementById('sessions-list-container');
  if (!container) return;

  try {
    const res = await fetch('/api/users/sessions').then(r => r.json());
    if (res && res.success && res.sessions) {
      if (res.sessions.length === 0) {
        container.innerHTML = `<p style="color: var(--subtext0); font-style: italic;">Aucune session active enregistrée.</p>`;
        return;
      }

      container.innerHTML = `
        <table class="table" style="width: 100%; border-collapse: collapse; font-size: 0.88rem;">
          <thead>
            <tr style="text-align: left; color: var(--subtext0); border-bottom: 1px solid var(--surface0);">
              <th style="padding: 10px 12px;">Utilisateur</th>
              <th style="padding: 10px 12px;">Rôle</th>
              <th style="padding: 10px 12px;">Connexion</th>
              <th style="padding: 10px 12px;">Expiration</th>
              <th style="padding: 10px 12px; text-align: right;">Action</th>
            </tr>
          </thead>
          <tbody>
            ${res.sessions.map(s => `
              <tr style="border-bottom: 1px solid var(--surface0);">
                <td style="padding: 10px 12px;"><strong>${escapeHtml(s.username)}</strong></td>
                <td style="padding: 10px 12px;">${s.is_admin ? '<span class="user-badge user-badge-admin">👑 Admin</span>' : '<span class="user-badge user-badge-storage">Standard</span>'}</td>
                <td style="padding: 10px 12px; color: var(--subtext0);">${new Date(s.created_at * 1000).toLocaleString('fr-FR')}</td>
                <td style="padding: 10px 12px; color: var(--subtext0);">${new Date(s.expires_at * 1000).toLocaleString('fr-FR')}</td>
                <td style="padding: 10px 12px; text-align: right;">
                  <button type="button" class="btn btn-secondary btn-xs" style="color: var(--red);" onclick="revokeSession('${s.token}')">
                    <span>🚫</span> Révoquer
                  </button>
                </td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      `;
    }
  } catch (err) {
    console.error("Erreur sessions :", err);
  }
}

async function revokeSession(token) {
  if (!confirm("Voulez-vous vraiment déconnecter cette session active ?")) return;
  try {
    const res = await fetch(`/api/users/sessions/${token}/revoke`, { method: 'POST' }).then(r => r.json());
    if (res && res.success) {
      showToast("Session révoquée avec succès", "success");
      loadActiveSessions();
      loadUsersAndGroups();
    } else {
      showToast(res.error || "Erreur de révocation", "error");
    }
  } catch (err) {
    showToast("Erreur de communication", "error");
  }
}

// --------------------------------------------------------------------------
// WIZARD CRÉATION UTILISATEUR
// --------------------------------------------------------------------------
function openCreateUserModal() {
  createUserStep = 1;
  setCreateUserStep(1);

  document.getElementById('cu-username').value = '';
  document.getElementById('cu-fullname').value = '';
  document.getElementById('cu-email').value = '';
  document.getElementById('cu-password').value = '';
  document.getElementById('cu-password-confirm').value = '';
  document.getElementById('cu-sshkey').value = '';
  document.getElementById('cu-role-admin').checked = false;
  document.getElementById('cu-role-storage').checked = true;
  document.getElementById('cu-role-docker').checked = false;
  document.getElementById('cu-role-shell').checked = false;
  document.getElementById('cu-shell-select-box').style.display = 'none';
  document.getElementById('cu-samba-access').checked = true;
  document.getElementById('cu-create-share').checked = true;
  document.getElementById('cu-username-error').style.display = 'none';

  const modal = document.getElementById('modal-create-user');
  if (modal) modal.style.display = 'flex';
}

function closeCreateUserModal() {
  const modal = document.getElementById('modal-create-user');
  if (modal) modal.style.display = 'none';
}

function setCreateUserStep(step) {
  createUserStep = step;
  for (let i = 1; i <= 4; i++) {
    const node = document.getElementById(`wstep-node-${i}`);
    const pane = document.getElementById(`wstep-pane-${i}`);
    if (node) node.classList.toggle('active', i <= step);
    if (pane) pane.style.display = i === step ? 'block' : 'none';
  }

  const btnPrev = document.getElementById('btn-cu-prev');
  const btnNext = document.getElementById('btn-cu-next');
  const btnSubmit = document.getElementById('btn-cu-submit');

  if (btnPrev) btnPrev.style.display = step > 1 ? 'inline-block' : 'none';
  if (btnNext) btnNext.style.display = step < 4 ? 'inline-block' : 'none';
  if (btnSubmit) btnSubmit.style.display = step === 4 ? 'inline-block' : 'none';
}

function nextCreateUserStep() {
  if (createUserStep === 1) {
    const username = document.getElementById('cu-username')?.value.trim().toLowerCase();
    if (!username || !/^[a-z_][a-z0-9_-]{0,31}$/.test(username)) {
      showToast("Veuillez renseigner un identifiant valide (lettres minuscules, chiffres, tirets).", "warning");
      document.getElementById('cu-username')?.focus();
      return;
    }
  }
  if (createUserStep === 2) {
    const pwd = document.getElementById('cu-password')?.value;
    const pwdConfirm = document.getElementById('cu-password-confirm')?.value;
    if (!pwd || pwd.length < 6) {
      showToast("Le mot de passe doit comporter au moins 6 caractères.", "warning");
      return;
    }
    if (pwd !== pwdConfirm) {
      showToast("Les deux mots de passe ne correspondent pas.", "error");
      return;
    }
  }

  if (createUserStep < 4) {
    setCreateUserStep(createUserStep + 1);
  }
}

function prevCreateUserStep() {
  if (createUserStep > 1) {
    setCreateUserStep(createUserStep - 1);
  }
}

function validateNewUsername(input) {
  const val = input.value.trim().toLowerCase();
  input.value = val;
  const errBox = document.getElementById('cu-username-error');
  if (!errBox) return;

  if (!val) {
    errBox.style.display = 'none';
    return;
  }

  if (!/^[a-z_]/.test(val)) {
    errBox.textContent = "L'identifiant doit commencer par une lettre minuscule ou un underscore.";
    errBox.style.display = 'block';
    return;
  }

  if (!/^[a-z_][a-z0-9_-]*$/.test(val)) {
    errBox.textContent = "Caractères autorisés : minuscules (a-z), chiffres (0-9), tirets (-) et underscores (_). Pas d'espaces ni d'accents.";
    errBox.style.display = 'block';
    return;
  }

  errBox.style.display = 'none';
}

function checkPasswordStrength(pwd, textId, barId) {
  const textEl = document.getElementById(textId);
  const barEl = document.getElementById(barId);
  if (!textEl || !barEl) return;

  if (!pwd) {
    barEl.style.width = '0%';
    textEl.textContent = 'Minimum 6 caractères recommandé';
    return;
  }

  let score = 0;
  if (pwd.length >= 6) score += 25;
  if (pwd.length >= 10) score += 25;
  if (/[0-9]/.test(pwd)) score += 25;
  if (/[^A-Za-z0-9]/.test(pwd)) score += 25;

  barEl.style.width = `${score}%`;
  if (score <= 25) {
    barEl.style.background = 'var(--red)';
    textEl.textContent = 'Faible (ajoutez des chiffres et des caractères spéciaux)';
    textEl.style.color = 'var(--red)';
  } else if (score <= 75) {
    barEl.style.background = 'var(--peach)';
    textEl.textContent = 'Moyen (satisfaisant)';
    textEl.style.color = 'var(--peach)';
  } else {
    barEl.style.background = 'var(--green)';
    textEl.textContent = 'Excellent (robuste)';
    textEl.style.color = 'var(--green)';
  }
}

function toggleAdminRole(checkbox) {
  if (checkbox.checked) {
    document.getElementById('cu-role-storage').checked = true;
    document.getElementById('cu-role-docker').checked = true;
  }
}

function toggleShellOption(checkbox) {
  const box = document.getElementById('cu-shell-select-box');
  if (box) box.style.display = checkbox.checked ? 'block' : 'none';
}

async function submitCreateUser() {
  const username = document.getElementById('cu-username')?.value.trim().toLowerCase();
  const fullName = document.getElementById('cu-fullname')?.value.trim();
  const email = document.getElementById('cu-email')?.value.trim();
  const password = document.getElementById('cu-password')?.value;
  const avatarColor = document.querySelector('input[name="cu-color"]:checked')?.value || 'sapphire';

  const isAdmin = document.getElementById('cu-role-admin')?.checked || false;
  const isStorage = document.getElementById('cu-role-storage')?.checked || false;
  const isDocker = document.getElementById('cu-role-docker')?.checked || false;
  const allowShell = document.getElementById('cu-role-shell')?.checked || false;
  const shellChoice = document.getElementById('cu-shell-choice')?.value;
  const sambaAccess = document.getElementById('cu-samba-access')?.checked || false;
  const createShare = document.getElementById('cu-create-share')?.checked || false;

  const rawSsh = document.getElementById('cu-sshkey')?.value.trim();
  const sshKeys = rawSsh ? [rawSsh] : [];

  const groups = [];
  if (isAdmin) groups.push('wheel');
  if (isStorage) groups.push('storage');
  if (isDocker) groups.push('docker');

  const payload = {
    username,
    full_name: fullName,
    email,
    avatar_color: avatarColor,
    password,
    role: isAdmin ? 'admin' : (allowShell ? 'standard' : 'share_only'),
    groups,
    allow_shell: allowShell,
    shell: allowShell ? shellChoice : undefined,
    samba_access: sambaAccess,
    create_dedicated_share: createShare,
    ssh_keys: sshKeys
  };

  const btnSubmit = document.getElementById('btn-cu-submit');
  if (btnSubmit) {
    btnSubmit.disabled = true;
    btnSubmit.innerHTML = `<span>⏳</span> Création en cours...`;
  }

  try {
    const res = await fetch('/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload)
    }).then(r => r.json());

    if (res && res.success) {
      showToast(`Utilisateur '${username}' créé avec succès !`, "success");
      closeCreateUserModal();
      loadUsersAndGroups();
    } else {
      showToast(res.error || "Échec de la création du compte", "error");
    }
  } catch (err) {
    showToast("Erreur de connexion avec le serveur", "error");
  } finally {
    if (btnSubmit) {
      btnSubmit.disabled = false;
      btnSubmit.innerHTML = `<span>✨</span> Créer l'Utilisateur`;
    }
  }
}

// --------------------------------------------------------------------------
// MODAL ÉDITION UTILISATEUR
// --------------------------------------------------------------------------
function openEditUserModal(username) {
  if (username === 'root') {
    showToast("Le compte 'root' est le superviseur du système et ne peut pas être modifié depuis le tableau de bord.", "error");
    return;
  }
  const user = usersData.find(u => u.username === username);
  if (!user) return;

  document.getElementById('eu-username').value = user.username;
  document.getElementById('eu-modal-title').textContent = `Modifier '${user.username}'`;
  document.getElementById('eu-fullname').value = user.full_name || '';
  document.getElementById('eu-email').value = user.email || '';

  const colorRadio = document.querySelector(`input[name="eu-color"][value="${user.avatar_color || 'sapphire'}"]`);
  if (colorRadio) colorRadio.checked = true;

  document.getElementById('eu-samba-access').checked = user.samba_enabled;

  const isShellNoLogin = user.shell.endsWith('nologin') || user.shell.endsWith('false');
  const checkShell = document.getElementById('eu-shell-access');
  const boxShell = document.getElementById('eu-shell-select-box');
  const selShell = document.getElementById('eu-shell-choice');

  if (checkShell) checkShell.checked = !isShellNoLogin;
  if (boxShell) boxShell.style.display = !isShellNoLogin ? 'block' : 'none';
  if (selShell && !isShellNoLogin) selShell.value = user.shell;

  // Chips des groupes
  const chipsContainer = document.getElementById('eu-groups-chips-container');
  if (chipsContainer) {
    const allGroupNames = groupsData.map(g => g.name);
    user.groups.forEach(g => {
      if (!allGroupNames.includes(g)) allGroupNames.push(g);
    });

    chipsContainer.innerHTML = allGroupNames.map(g => {
      const isSelected = user.groups.includes(g);
      const isWheelAndSelf = (g === 'wheel' && user.username === currentLoggedInUser);
      return `
        <label class="group-user-chip ${isSelected ? 'selected' : ''}" style="${isWheelAndSelf ? 'opacity: 0.6; cursor: not-allowed;' : ''}">
          <input type="checkbox" name="eu-groups" value="${escapeHtml(g)}" ${isSelected ? 'checked' : ''} ${isWheelAndSelf ? 'disabled' : ''} onchange="this.parentElement.classList.toggle('selected', this.checked)">
          <span>${escapeHtml(g)}</span>
        </label>
      `;
    }).join('');
  }

  const modal = document.getElementById('modal-edit-user');
  if (modal) modal.style.display = 'flex';
}

function closeEditUserModal() {
  const modal = document.getElementById('modal-edit-user');
  if (modal) modal.style.display = 'none';
}

function toggleEditShellSelect(checkbox) {
  const box = document.getElementById('eu-shell-select-box');
  if (box) box.style.display = checkbox.checked ? 'block' : 'none';
}

async function submitEditUser() {
  const username = document.getElementById('eu-username')?.value;
  if (!username) return;

  const fullName = document.getElementById('eu-fullname')?.value.trim();
  const email = document.getElementById('eu-email')?.value.trim();
  const avatarColor = document.querySelector('input[name="eu-color"]:checked')?.value || 'sapphire';
  const sambaAccess = document.getElementById('eu-samba-access')?.checked || false;
  const allowShell = document.getElementById('eu-shell-access')?.checked || false;
  const shellChoice = document.getElementById('eu-shell-choice')?.value;

  const selectedGroups = [];
  document.querySelectorAll('input[name="eu-groups"]:checked').forEach(cb => {
    selectedGroups.push(cb.value);
  });

  const payload = {
    full_name: fullName,
    email,
    avatar_color: avatarColor,
    groups: selectedGroups,
    allow_shell: allowShell,
    shell: allowShell ? shellChoice : undefined,
    samba_access: sambaAccess
  };

  try {
    const res = await fetch(`/api/users/${username}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload)
    }).then(r => r.json());

    if (res && res.success) {
      showToast(`Utilisateur '${username}' mis à jour avec succès`, "success");
      closeEditUserModal();
      loadUsersAndGroups();
    } else {
      showToast(res.error || "Erreur de mise à jour", "error");
    }
  } catch (err) {
    showToast("Erreur réseau", "error");
  }
}

// --------------------------------------------------------------------------
// MODAL MOT DE PASSE
// --------------------------------------------------------------------------
function openChangePasswordModal(username) {
  if (username === 'root') {
    showToast("Le mot de passe de 'root' ne peut pas être modifié depuis l'interface web pour des raisons de sécurité.", "error");
    return;
  }
  document.getElementById('cp-username').value = username;
  document.getElementById('cp-modal-title').textContent = `Modifier le Mot de Passe de '${username}'`;
  document.getElementById('cp-password').value = '';
  document.getElementById('cp-password-confirm').value = '';
  document.getElementById('cp-update-samba').checked = true;
  document.getElementById('cp-revoke-sessions').checked = true;

  checkPasswordStrength('', 'cp-pwd-strength', 'cp-pwd-bar');

  const modal = document.getElementById('modal-user-password');
  if (modal) modal.style.display = 'flex';
}

function closeChangePasswordModal() {
  const modal = document.getElementById('modal-user-password');
  if (modal) modal.style.display = 'none';
}

async function submitChangePassword() {
  const username = document.getElementById('cp-username')?.value;
  const pwd = document.getElementById('cp-password')?.value;
  const pwdConfirm = document.getElementById('cp-password-confirm')?.value;
  const updateSamba = document.getElementById('cp-update-samba')?.checked || false;
  const revokeSessions = document.getElementById('cp-revoke-sessions')?.checked || false;

  if (!pwd || pwd.length < 6) {
    showToast("Le mot de passe doit comporter au moins 6 caractères.", "warning");
    return;
  }
  if (pwd !== pwdConfirm) {
    showToast("Les deux mots de passe ne correspondent pas.", "error");
    return;
  }

  try {
    const res = await fetch(`/api/users/${username}/password`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ password: pwd, update_samba: updateSamba, revoke_sessions: revokeSessions })
    }).then(r => r.json());

    if (res && res.success) {
      showToast(`Mot de passe modifié pour '${username}'.`, "success");
      closeChangePasswordModal();
      loadUsersAndGroups();
      if (typeof loadSambaOverview === 'function') loadSambaOverview();
    } else {
      showToast(res.error || "Erreur de modification du mot de passe", "error");
    }
  } catch (err) {
    showToast("Erreur de connexion", "error");
  }
}

// --------------------------------------------------------------------------
// MODAL MOT DE PASSE SAMBA (SMB)
// --------------------------------------------------------------------------
function openSambaPasswordModal(username) {
  const user = (usersData || []).find(u => u.username === username);
  const uName = user ? user.username : username;
  document.getElementById('sp-username').value = uName;
  const userDisplay = document.getElementById('sp-user-display');
  if (userDisplay) userDisplay.textContent = `'${uName}'`;
  const title = document.getElementById('sp-modal-title');
  if (title) title.textContent = `Accès Samba (SMB) — ${uName}`;
  document.getElementById('sp-password').value = '';
  document.getElementById('sp-password-confirm').value = '';
  const modal = document.getElementById('modal-samba-password');
  if (modal) modal.style.display = 'flex';
}

function closeSambaPasswordModal() {
  const modal = document.getElementById('modal-samba-password');
  if (modal) modal.style.display = 'none';
}

async function submitSambaPassword() {
  const username = document.getElementById('sp-username')?.value;
  const pwd = document.getElementById('sp-password')?.value;
  const pwdConfirm = document.getElementById('sp-password-confirm')?.value;
  const btn = document.getElementById('btn-sp-submit');

  if (!username) return;
  if (!pwd || pwd.length < 1) {
    showToast("Veuillez saisir un mot de passe pour Samba.", "warning");
    return;
  }
  if (pwd !== pwdConfirm) {
    showToast("Les deux mots de passe ne correspondent pas.", "error");
    return;
  }

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span>⏳</span> Configuration...`;
  }

  try {
    const res = await fetch(`/api/users/${username}/samba-password`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ password: pwd })
    }).then(r => r.json());

    if (res && res.success) {
      showToast(`Accès Samba configuré et activé pour '${username}' !`, "success");
      closeSambaPasswordModal();
      loadUsersAndGroups();
      if (typeof loadSambaOverview === 'function') loadSambaOverview();
    } else {
      showToast(res.error || "Erreur lors de la configuration Samba", "error");
    }
  } catch (err) {
    showToast("Erreur de communication avec le serveur", "error");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span>💾</span> Enregistrer l'Accès Samba`;
    }
  }
}

// --------------------------------------------------------------------------
// VERROUILLAGE / DÉVERROUILLAGE COMPTE
// --------------------------------------------------------------------------
async function toggleUserLock(username) {
  const user = usersData.find(u => u.username === username);
  const actionText = user && user.locked ? "déverrouiller" : "verrouiller";

  if (!confirm(`Confirmez-vous vouloir ${actionText} l'accès pour '${username}' ?`)) return;

  try {
    const res = await fetch(`/api/users/${username}/toggle-lock`, { method: 'POST' }).then(r => r.json());
    if (res && res.success) {
      showToast(res.message, "success");
      loadUsersAndGroups();
    } else {
      showToast(res.error || "Erreur de modification du verrouillage", "error");
    }
  } catch (err) {
    showToast("Erreur réseau", "error");
  }
}

// --------------------------------------------------------------------------
// MODAL SUPPRESSION UTILISATEUR
// --------------------------------------------------------------------------
let mainAdminUser = getCurrentDashboardUsername();

function openDeleteUserModal(username) {
  const inputUser = document.getElementById('du-username');
  const labelUser = document.getElementById('du-username-label');
  const checkHome = document.getElementById('du-delete-home');
  const checkShare = document.getElementById('du-delete-share');

  if (inputUser) inputUser.value = username;
  if (labelUser) labelUser.textContent = username;
  if (checkHome) checkHome.checked = true;
  if (checkShare) checkShare.checked = false;

  const confirmBtn = document.getElementById('btn-confirm-delete-user');
  if (confirmBtn) {
    confirmBtn.disabled = false;
    confirmBtn.innerHTML = '<span>🗑️</span> Confirmer la Suppression';
  }

  const modal = document.getElementById('modal-delete-user');
  if (modal) modal.style.display = 'flex';
}

function closeDeleteUserModal() {
  const modal = document.getElementById('modal-delete-user');
  if (modal) modal.style.display = 'none';
}

async function confirmDeleteUser() {
  const username = document.getElementById('du-username')?.value;
  if (!username) {
    showToast("Nom d'utilisateur introuvable", "error");
    return;
  }

  const deleteHome = document.getElementById('du-delete-home')?.checked || false;
  const deleteShare = document.getElementById('du-delete-share')?.checked || false;

  const confirmBtn = document.getElementById('btn-confirm-delete-user');
  const originalHtml = confirmBtn ? confirmBtn.innerHTML : '<span>🗑️</span> Confirmer la Suppression';

  if (confirmBtn) {
    confirmBtn.disabled = true;
    confirmBtn.innerHTML = '<span class="spinner-inline">⏳</span> Suppression en cours...';
  }

  try {
    const res = await fetch(`/api/users/${encodeURIComponent(username)}?delete_home=${deleteHome}&delete_share=${deleteShare}`, {
      method: 'DELETE'
    }).then(async r => {
      const data = await r.json().catch(() => ({}));
      if (!r.ok && !data.error) {
        data.error = `Erreur HTTP ${r.status}`;
      }
      return data;
    });

    if (res && res.success) {
      showToast(res.message || `Utilisateur '${username}' supprimé avec succès.`, "success");
      closeDeleteUserModal();
      await loadUsersAndGroups();
    } else {
      showToast(res.error || "Erreur lors de la suppression", "error");
    }
  } catch (err) {
    console.error("Erreur confirmDeleteUser :", err);
    showToast("Erreur de communication avec le serveur", "error");
  } finally {
    if (confirmBtn) {
      confirmBtn.disabled = false;
      confirmBtn.innerHTML = originalHtml;
    }
  }
}

// --------------------------------------------------------------------------
// GESTION DES GROUPES
// --------------------------------------------------------------------------
function openCreateGroupModal() {
  document.getElementById('cg-name').value = '';
  document.getElementById('cg-description').value = '';

  const chipsContainer = document.getElementById('cg-members-chips');
  if (chipsContainer) {
    chipsContainer.innerHTML = usersData.map(u => `
      <label class="group-user-chip">
        <input type="checkbox" name="cg-members" value="${escapeHtml(u.username)}" onchange="this.parentElement.classList.toggle('selected', this.checked)">
        <span>${escapeHtml(u.username)}</span>
      </label>
    `).join('');
  }

  const modal = document.getElementById('modal-create-group');
  if (modal) modal.style.display = 'flex';
}

function closeCreateGroupModal() {
  const modal = document.getElementById('modal-create-group');
  if (modal) modal.style.display = 'none';
}

async function submitCreateGroup() {
  const name = document.getElementById('cg-name')?.value.trim().toLowerCase();
  const description = document.getElementById('cg-description')?.value.trim();

  if (!name || !/^[a-z_][a-z0-9_-]{0,31}$/.test(name)) {
    showToast("Nom de groupe invalide. Utilisez des minuscules et chiffres.", "warning");
    return;
  }

  const members = [];
  document.querySelectorAll('input[name="cg-members"]:checked').forEach(cb => {
    members.push(cb.value);
  });

  try {
    const res = await fetch('/api/groups', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, description, members })
    }).then(r => r.json());

    if (res && res.success) {
      showToast(`Groupe '${name}' créé avec succès !`, "success");
      closeCreateGroupModal();
      loadUsersAndGroups();
    } else {
      showToast(res.error || "Erreur de création de groupe", "error");
    }
  } catch (err) {
    showToast("Erreur réseau", "error");
  }
}

function openManageGroupMembersModal(groupName) {
  const group = groupsData.find(g => g.name === groupName);
  if (!group) return;

  document.getElementById('mgm-group-name').value = groupName;
  document.getElementById('mgm-modal-title').textContent = `Membres du groupe '${groupName}'`;
  document.getElementById('mgm-group-desc').textContent = group.description;

  const usersListEl = document.getElementById('mgm-users-list');
  if (usersListEl) {
    usersListEl.innerHTML = usersData.map(u => {
      const isMember = group.members.includes(u.username);
      const isWheelAndSelf = (groupName === 'wheel' && u.username === currentLoggedInUser);

      return `
        <label style="display: flex; align-items: center; justify-content: space-between; padding: 10px; background: var(--base); border-radius: var(--radius-sm); cursor: pointer; ${isWheelAndSelf ? 'opacity: 0.7;' : ''}">
          <div style="display: flex; align-items: center; gap: 10px;">
            <div class="user-avatar-circle user-avatar-${u.avatar_color || 'sapphire'}" style="width: 28px; height: 28px; font-size: 0.8rem;">
              ${(u.full_name || u.username).charAt(0).toUpperCase()}
            </div>
            <div>
              <strong style="color: var(--text);">${escapeHtml(u.username)}</strong>
              <span style="color: var(--subtext0); font-size: 0.78rem; margin-left: 6px;">(${escapeHtml(u.full_name || '')})</span>
            </div>
          </div>
          <input type="checkbox" name="mgm-user-check" value="${escapeHtml(u.username)}" ${isMember ? 'checked' : ''} ${isWheelAndSelf ? 'disabled' : ''}>
        </label>
      `;
    }).join('');
  }

  const modal = document.getElementById('modal-manage-group-members');
  if (modal) modal.style.display = 'flex';
}

function closeManageGroupMembersModal() {
  const modal = document.getElementById('modal-manage-group-members');
  if (modal) modal.style.display = 'none';
}

async function submitManageGroupMembers() {
  const groupName = document.getElementById('mgm-group-name')?.value;
  if (!groupName) return;

  const members = [];
  document.querySelectorAll('input[name="mgm-user-check"]:checked').forEach(cb => {
    members.push(cb.value);
  });

  try {
    const res = await fetch(`/api/groups/${groupName}/members`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ members })
    }).then(r => r.json());

    if (res && res.success) {
      showToast(`Membres du groupe '${groupName}' mis à jour`, "success");
      closeManageGroupMembersModal();
      loadUsersAndGroups();
    } else {
      showToast(res.error || "Erreur de mise à jour des membres", "error");
    }
  } catch (err) {
    showToast("Erreur de connexion", "error");
  }
}

async function deleteGroup(groupName) {
  if (!confirm(`Supprimer définitivement le groupe '${groupName}' ?`)) return;

  try {
    const res = await fetch(`/api/groups/${groupName}`, { method: 'DELETE' }).then(r => r.json());
    if (res && res.success) {
      showToast(`Groupe '${groupName}' supprimé avec succès.`, "success");
      loadUsersAndGroups();
    } else {
      showToast(res.error || "Erreur de suppression", "error");
    }
  } catch (err) {
    showToast("Erreur réseau", "error");
  }
}




// =========================================================================
// 👵 EASTER EGG : MAMIE PIXEL EN SPRINT AU-DESSUS DES MENUS (5 CLICS LOGO)
// =========================================================================

const RUNNER_GRANNY_SVG_TEMPLATE = `<div id="granny-runner" class="granny-runner" data-frame="0">
  <div class="granny-speech-bubble" id="granny-speech-bubble">
    <span id="granny-speech-text">Vite, ma tarte aux pommes ! 🥧</span>
  </div>
  <svg class="granny-sprite granny-f0" viewBox="0 0 28 28" width="56" height="56" shape-rendering="crispEdges"><g><rect x="7" y="1" width="1" height="1" fill="#F38BA8"/><rect x="6" y="2" width="3" height="1" fill="#BAC2DE"/><rect x="5" y="3" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="3" width="2" height="1" fill="#CDD6F4"/><rect x="8" y="3" width="2" height="1" fill="#BAC2DE"/><rect x="5" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="4" width="1" height="1" fill="#CDD6F4"/><rect x="7" y="4" width="2" height="1" fill="#9399B2"/><rect x="9" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="15" y="4" width="3" height="1" fill="#F9E2AF"/><rect x="6" y="5" width="3" height="1" fill="#9399B2"/><rect x="14" y="5" width="1" height="1" fill="#F9E2AF"/><rect x="18" y="5" width="1" height="1" fill="#F9E2AF"/><rect x="5" y="6" width="6" height="1" fill="#BAC2DE"/><rect x="15" y="6" width="1" height="1" fill="#F9E2AF"/><rect x="4" y="7" width="1" height="1" fill="#BAC2DE"/><rect x="5" y="7" width="3" height="1" fill="#CDD6F4"/><rect x="8" y="7" width="3" height="1" fill="#BAC2DE"/><rect x="13" y="7" width="1" height="1" fill="#F5C2E7"/><rect x="16" y="7" width="1" height="1" fill="#F9E2AF"/><rect x="4" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="6" y="8" width="3" height="1" fill="#9399B2"/><rect x="9" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="12" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="13" y="8" width="1" height="1" fill="#89B4FA"/><rect x="14" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="6" y="9" width="4" height="1" fill="#F5C2E7"/><rect x="10" y="9" width="1" height="1" fill="#FAB387"/><rect x="12" y="9" width="1" height="1" fill="#89B4FA"/><rect x="13" y="9" width="1" height="1" fill="#FFFFFF"/><rect x="14" y="9" width="1" height="1" fill="#89B4FA"/><rect x="6" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="7" y="10" width="1" height="1" fill="#FAB387"/><rect x="8" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="9" y="10" width="1" height="1" fill="#FAB387"/><rect x="10" y="10" width="1" height="1" fill="#11111B"/><rect x="12" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="13" y="10" width="1" height="1" fill="#FAB387"/><rect x="14" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="7" y="11" width="2" height="1" fill="#F5C2E7"/><rect x="9" y="11" width="1" height="1" fill="#FAB387"/><rect x="12" y="11" width="1" height="1" fill="#F38BA8"/><rect x="7" y="12" width="1" height="1" fill="#F5C2E7"/><rect x="6" y="13" width="5" height="1" fill="#CBA6F7"/><rect x="12" y="13" width="1" height="1" fill="#FAB387"/><rect x="3" y="14" width="1" height="1" fill="#FAB387"/><rect x="6" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="8" y="14" width="1" height="1" fill="#B4BEFE"/><rect x="9" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="12" y="14" width="1" height="1" fill="#FAB387"/><rect x="2" y="15" width="3" height="1" fill="#FAB387"/><rect x="6" y="15" width="2" height="1" fill="#CBA6F7"/><rect x="8" y="15" width="1" height="1" fill="#B4BEFE"/><rect x="9" y="15" width="4" height="1" fill="#CBA6F7"/><rect x="2" y="16" width="3" height="1" fill="#FAB387"/><rect x="6" y="16" width="7" height="1" fill="#CBA6F7"/><rect x="3" y="17" width="1" height="1" fill="#FAB387"/><rect x="6" y="17" width="7" height="1" fill="#CBA6F7"/><rect x="6" y="18" width="7" height="1" fill="#F5E0DC"/><rect x="6" y="19" width="7" height="1" fill="#E78284"/><rect x="6" y="20" width="7" height="1" fill="#E78284"/><rect x="7" y="21" width="5" height="1" fill="#E78284"/><rect x="6" y="22" width="1" height="1" fill="#F5C2E7"/><rect x="11" y="22" width="1" height="1" fill="#F5C2E7"/><rect x="5" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="12" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="4" y="24" width="2" height="1" fill="#313244"/><rect x="13" y="24" width="2" height="1" fill="#F5C2E7"/><rect x="4" y="25" width="2" height="1" fill="#313244"/><rect x="14" y="25" width="2" height="1" fill="#313244"/><rect x="15" y="26" width="3" height="1" fill="#313244"/></g></svg>
  <svg class="granny-sprite granny-f1" viewBox="0 0 28 28" width="56" height="56" shape-rendering="crispEdges"><g><rect x="8" y="1" width="1" height="1" fill="#F38BA8"/><rect x="7" y="2" width="3" height="1" fill="#BAC2DE"/><rect x="6" y="3" width="1" height="1" fill="#BAC2DE"/><rect x="7" y="3" width="2" height="1" fill="#CDD6F4"/><rect x="9" y="3" width="2" height="1" fill="#BAC2DE"/><rect x="6" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="7" y="4" width="1" height="1" fill="#CDD6F4"/><rect x="8" y="4" width="2" height="1" fill="#9399B2"/><rect x="10" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="15" y="4" width="3" height="1" fill="#F9E2AF"/><rect x="7" y="5" width="3" height="1" fill="#9399B2"/><rect x="14" y="5" width="1" height="1" fill="#F9E2AF"/><rect x="18" y="5" width="1" height="1" fill="#F9E2AF"/><rect x="6" y="6" width="6" height="1" fill="#BAC2DE"/><rect x="15" y="6" width="1" height="1" fill="#F9E2AF"/><rect x="5" y="7" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="7" width="3" height="1" fill="#CDD6F4"/><rect x="9" y="7" width="3" height="1" fill="#BAC2DE"/><rect x="13" y="7" width="1" height="1" fill="#F5C2E7"/><rect x="16" y="7" width="1" height="1" fill="#F9E2AF"/><rect x="5" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="7" y="8" width="3" height="1" fill="#9399B2"/><rect x="10" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="13" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="14" y="8" width="1" height="1" fill="#89B4FA"/><rect x="15" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="7" y="9" width="4" height="1" fill="#F5C2E7"/><rect x="11" y="9" width="1" height="1" fill="#FAB387"/><rect x="13" y="9" width="1" height="1" fill="#89B4FA"/><rect x="14" y="9" width="1" height="1" fill="#FFFFFF"/><rect x="15" y="9" width="1" height="1" fill="#89B4FA"/><rect x="7" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="8" y="10" width="1" height="1" fill="#FAB387"/><rect x="9" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="10" y="10" width="1" height="1" fill="#FAB387"/><rect x="11" y="10" width="1" height="1" fill="#11111B"/><rect x="13" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="14" y="10" width="1" height="1" fill="#FAB387"/><rect x="15" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="8" y="11" width="2" height="1" fill="#F5C2E7"/><rect x="10" y="11" width="1" height="1" fill="#FAB387"/><rect x="13" y="11" width="1" height="1" fill="#F38BA8"/><rect x="8" y="12" width="1" height="1" fill="#F5C2E7"/><rect x="7" y="13" width="5" height="1" fill="#CBA6F7"/><rect x="13" y="13" width="1" height="1" fill="#FAB387"/><rect x="4" y="14" width="1" height="1" fill="#FAB387"/><rect x="7" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="9" y="14" width="1" height="1" fill="#B4BEFE"/><rect x="10" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="13" y="14" width="1" height="1" fill="#FAB387"/><rect x="3" y="15" width="3" height="1" fill="#FAB387"/><rect x="7" y="15" width="2" height="1" fill="#CBA6F7"/><rect x="9" y="15" width="1" height="1" fill="#B4BEFE"/><rect x="10" y="15" width="4" height="1" fill="#CBA6F7"/><rect x="3" y="16" width="3" height="1" fill="#FAB387"/><rect x="7" y="16" width="7" height="1" fill="#CBA6F7"/><rect x="4" y="17" width="1" height="1" fill="#FAB387"/><rect x="7" y="17" width="7" height="1" fill="#CBA6F7"/><rect x="7" y="18" width="7" height="1" fill="#F5E0DC"/><rect x="7" y="19" width="7" height="1" fill="#E78284"/><rect x="7" y="20" width="7" height="1" fill="#E78284"/><rect x="8" y="21" width="5" height="1" fill="#E78284"/><rect x="8" y="22" width="1" height="1" fill="#F5C2E7"/><rect x="12" y="22" width="1" height="1" fill="#F5C2E7"/><rect x="7" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="12" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="6" y="24" width="2" height="1" fill="#313244"/><rect x="13" y="24" width="2" height="1" fill="#313244"/><rect x="6" y="25" width="3" height="1" fill="#313244"/><rect x="12" y="25" width="3" height="1" fill="#313244"/></g></svg>
  <svg class="granny-sprite granny-f2" viewBox="0 0 28 28" width="56" height="56" shape-rendering="crispEdges"><g><rect x="7" y="1" width="1" height="1" fill="#F38BA8"/><rect x="6" y="2" width="3" height="1" fill="#BAC2DE"/><rect x="5" y="3" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="3" width="2" height="1" fill="#CDD6F4"/><rect x="8" y="3" width="2" height="1" fill="#BAC2DE"/><rect x="5" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="4" width="1" height="1" fill="#CDD6F4"/><rect x="7" y="4" width="2" height="1" fill="#9399B2"/><rect x="9" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="5" width="3" height="1" fill="#9399B2"/><rect x="15" y="5" width="3" height="1" fill="#F9E2AF"/><rect x="5" y="6" width="6" height="1" fill="#BAC2DE"/><rect x="14" y="6" width="1" height="1" fill="#F9E2AF"/><rect x="18" y="6" width="1" height="1" fill="#F9E2AF"/><rect x="4" y="7" width="1" height="1" fill="#BAC2DE"/><rect x="5" y="7" width="3" height="1" fill="#CDD6F4"/><rect x="8" y="7" width="3" height="1" fill="#BAC2DE"/><rect x="15" y="7" width="1" height="1" fill="#F9E2AF"/><rect x="4" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="6" y="8" width="3" height="1" fill="#9399B2"/><rect x="9" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="12" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="13" y="8" width="1" height="1" fill="#89B4FA"/><rect x="14" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="16" y="8" width="1" height="1" fill="#F9E2AF"/><rect x="6" y="9" width="4" height="1" fill="#F5C2E7"/><rect x="10" y="9" width="1" height="1" fill="#FAB387"/><rect x="12" y="9" width="1" height="1" fill="#89B4FA"/><rect x="13" y="9" width="1" height="1" fill="#FFFFFF"/><rect x="14" y="9" width="1" height="1" fill="#89B4FA"/><rect x="17" y="9" width="1" height="1" fill="#F9E2AF"/><rect x="6" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="7" y="10" width="1" height="1" fill="#FAB387"/><rect x="8" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="9" y="10" width="1" height="1" fill="#FAB387"/><rect x="10" y="10" width="1" height="1" fill="#11111B"/><rect x="12" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="13" y="10" width="1" height="1" fill="#FAB387"/><rect x="14" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="18" y="10" width="1" height="1" fill="#F9E2AF"/><rect x="7" y="11" width="2" height="1" fill="#F5C2E7"/><rect x="9" y="11" width="1" height="1" fill="#FAB387"/><rect x="12" y="11" width="1" height="1" fill="#F38BA8"/><rect x="19" y="11" width="1" height="1" fill="#F9E2AF"/><rect x="7" y="12" width="1" height="1" fill="#F5C2E7"/><rect x="6" y="13" width="5" height="1" fill="#CBA6F7"/><rect x="12" y="13" width="1" height="1" fill="#FAB387"/><rect x="6" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="8" y="14" width="1" height="1" fill="#B4BEFE"/><rect x="9" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="12" y="14" width="1" height="1" fill="#FAB387"/><rect x="6" y="15" width="2" height="1" fill="#CBA6F7"/><rect x="8" y="15" width="1" height="1" fill="#B4BEFE"/><rect x="9" y="15" width="4" height="1" fill="#CBA6F7"/><rect x="16" y="15" width="1" height="1" fill="#FAB387"/><rect x="6" y="16" width="7" height="1" fill="#CBA6F7"/><rect x="15" y="16" width="3" height="1" fill="#FAB387"/><rect x="6" y="17" width="7" height="1" fill="#CBA6F7"/><rect x="15" y="17" width="3" height="1" fill="#FAB387"/><rect x="6" y="18" width="7" height="1" fill="#F5E0DC"/><rect x="16" y="18" width="1" height="1" fill="#FAB387"/><rect x="6" y="19" width="7" height="1" fill="#E78284"/><rect x="6" y="20" width="7" height="1" fill="#E78284"/><rect x="7" y="21" width="5" height="1" fill="#E78284"/><rect x="6" y="22" width="1" height="1" fill="#F5C2E7"/><rect x="11" y="22" width="1" height="1" fill="#F5C2E7"/><rect x="5" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="12" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="4" y="24" width="2" height="1" fill="#F5C2E7"/><rect x="13" y="24" width="2" height="1" fill="#313244"/><rect x="3" y="25" width="2" height="1" fill="#313244"/><rect x="13" y="25" width="2" height="1" fill="#313244"/><rect x="3" y="26" width="3" height="1" fill="#313244"/></g></svg>
  <svg class="granny-sprite granny-f3" viewBox="0 0 28 28" width="56" height="56" shape-rendering="crispEdges"><g><rect x="7" y="1" width="1" height="1" fill="#F38BA8"/><rect x="6" y="2" width="3" height="1" fill="#BAC2DE"/><rect x="5" y="3" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="3" width="2" height="1" fill="#CDD6F4"/><rect x="8" y="3" width="2" height="1" fill="#BAC2DE"/><rect x="5" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="6" y="4" width="1" height="1" fill="#CDD6F4"/><rect x="7" y="4" width="2" height="1" fill="#9399B2"/><rect x="9" y="4" width="1" height="1" fill="#BAC2DE"/><rect x="14" y="4" width="3" height="1" fill="#F9E2AF"/><rect x="6" y="5" width="3" height="1" fill="#9399B2"/><rect x="13" y="5" width="1" height="1" fill="#F9E2AF"/><rect x="17" y="5" width="1" height="1" fill="#F9E2AF"/><rect x="5" y="6" width="6" height="1" fill="#BAC2DE"/><rect x="14" y="6" width="1" height="1" fill="#F9E2AF"/><rect x="4" y="7" width="1" height="1" fill="#BAC2DE"/><rect x="5" y="7" width="3" height="1" fill="#CDD6F4"/><rect x="8" y="7" width="3" height="1" fill="#BAC2DE"/><rect x="15" y="7" width="1" height="1" fill="#F9E2AF"/><rect x="4" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="6" y="8" width="3" height="1" fill="#9399B2"/><rect x="9" y="8" width="2" height="1" fill="#BAC2DE"/><rect x="12" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="13" y="8" width="1" height="1" fill="#89B4FA"/><rect x="14" y="8" width="1" height="1" fill="#F5C2E7"/><rect x="16" y="8" width="1" height="1" fill="#F9E2AF"/><rect x="6" y="9" width="4" height="1" fill="#F5C2E7"/><rect x="10" y="9" width="1" height="1" fill="#FAB387"/><rect x="12" y="9" width="1" height="1" fill="#89B4FA"/><rect x="13" y="9" width="1" height="1" fill="#FFFFFF"/><rect x="14" y="9" width="1" height="1" fill="#89B4FA"/><rect x="17" y="9" width="1" height="1" fill="#F9E2AF"/><rect x="6" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="7" y="10" width="1" height="1" fill="#FAB387"/><rect x="8" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="9" y="10" width="1" height="1" fill="#FAB387"/><rect x="10" y="10" width="1" height="1" fill="#11111B"/><rect x="12" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="13" y="10" width="1" height="1" fill="#FAB387"/><rect x="14" y="10" width="1" height="1" fill="#F5C2E7"/><rect x="18" y="10" width="1" height="1" fill="#F9E2AF"/><rect x="7" y="11" width="2" height="1" fill="#F5C2E7"/><rect x="9" y="11" width="1" height="1" fill="#FAB387"/><rect x="12" y="11" width="1" height="1" fill="#F38BA8"/><rect x="19" y="11" width="1" height="1" fill="#F9E2AF"/><rect x="7" y="12" width="1" height="1" fill="#F5C2E7"/><rect x="6" y="13" width="5" height="1" fill="#CBA6F7"/><rect x="12" y="13" width="1" height="1" fill="#FAB387"/><rect x="6" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="8" y="14" width="1" height="1" fill="#B4BEFE"/><rect x="9" y="14" width="2" height="1" fill="#CBA6F7"/><rect x="12" y="14" width="1" height="1" fill="#FAB387"/><rect x="2" y="15" width="1" height="1" fill="#FAB387"/><rect x="6" y="15" width="2" height="1" fill="#CBA6F7"/><rect x="8" y="15" width="1" height="1" fill="#B4BEFE"/><rect x="9" y="15" width="4" height="1" fill="#CBA6F7"/><rect x="1" y="16" width="3" height="1" fill="#FAB387"/><rect x="6" y="16" width="7" height="1" fill="#CBA6F7"/><rect x="1" y="17" width="3" height="1" fill="#FAB387"/><rect x="6" y="17" width="7" height="1" fill="#CBA6F7"/><rect x="2" y="18" width="1" height="1" fill="#FAB387"/><rect x="6" y="18" width="7" height="1" fill="#F5E0DC"/><rect x="6" y="19" width="7" height="1" fill="#E78284"/><rect x="6" y="20" width="7" height="1" fill="#E78284"/><rect x="7" y="21" width="5" height="1" fill="#E78284"/><rect x="5" y="22" width="2" height="1" fill="#F5C2E7"/><rect x="11" y="22" width="2" height="1" fill="#F5C2E7"/><rect x="4" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="12" y="23" width="2" height="1" fill="#F5C2E7"/><rect x="2" y="24" width="2" height="1" fill="#313244"/><rect x="14" y="24" width="2" height="1" fill="#313244"/><rect x="2" y="25" width="3" height="1" fill="#313244"/><rect x="15" y="25" width="3" height="1" fill="#313244"/><rect x="1" y="26" width="2" height="1" fill="#A6ADC8"/><rect x="0" y="27" width="3" height="1" fill="#A6ADC8"/></g></svg>
  <span class="granny-dust-puff">💨</span>
</div>`;

let grannyLogoClickCount = 0;
let grannyLogoClickTimer = null;
let isGrannyRunning = false;
let grannyAudioCtx = null;

function getGrannyAudioContext() {
  if (!grannyAudioCtx) {
    const AudioCtx = window.AudioContext || window.webkitAudioContext;
    if (AudioCtx) {
      grannyAudioCtx = new AudioCtx();
    }
  }
  if (grannyAudioCtx && grannyAudioCtx.state === 'suspended') {
    grannyAudioCtx.resume();
  }
  return grannyAudioCtx;
}

function playGrannyFootstep(stepIndex) {
  try {
    const ctx = getGrannyAudioContext();
    if (!ctx) return;
    const now = ctx.currentTime;

    // 1. Tonalité de pas bois / semelle (alternance pied gauche / pied droit)
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    const isLeft = (stepIndex % 2 === 0);
    const baseFreq = isLeft ? 245 : 290;

    osc.type = 'triangle';
    osc.frequency.setValueAtTime(baseFreq, now);
    osc.frequency.exponentialRampToValueAtTime(75, now + 0.045);

    gain.gain.setValueAtTime(0.14, now);
    gain.gain.exponentialRampToValueAtTime(0.001, now + 0.045);

    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.05);

    // 2. Micro-claquement pantoufle (bruit blanc filtré passe-bande)
    const bufLen = Math.floor(ctx.sampleRate * 0.02);
    const buffer = ctx.createBuffer(1, bufLen, ctx.sampleRate);
    const channelData = buffer.getChannelData(0);
    for (let i = 0; i < bufLen; i++) {
      channelData[i] = (Math.random() * 2 - 1) * Math.exp(-i / (bufLen * 0.3));
    }
    const noise = ctx.createBufferSource();
    noise.buffer = buffer;

    const filter = ctx.createBiquadFilter();
    filter.type = 'bandpass';
    filter.frequency.setValueAtTime(isLeft ? 1150 : 1500, now);
    filter.Q.setValueAtTime(2.2, now);

    const noiseGain = ctx.createGain();
    noiseGain.gain.setValueAtTime(0.07, now);
    noiseGain.gain.exponentialRampToValueAtTime(0.001, now + 0.02);

    noise.connect(filter);
    filter.connect(noiseGain);
    noiseGain.connect(ctx.destination);
    noise.start(now);
  } catch (e) {
    // Silencieux si l'audio context est restreint par le navigateur
  }
}

function playGrannyVictoryDing() {
  try {
    const ctx = getGrannyAudioContext();
    if (!ctx) return;
    const now = ctx.currentTime;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();

    osc.type = 'sine';
    osc.frequency.setValueAtTime(1046.5, now);       // C6
    osc.frequency.setValueAtTime(1318.5, now + 0.08); // E6

    gain.gain.setValueAtTime(0.12, now);
    gain.gain.exponentialRampToValueAtTime(0.001, now + 0.4);

    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.45);
  } catch (e) {}
}

const GRANNY_QUOTES = [
  "C'est parti mon zizi ! 🚀",
  "Je vais remettre une petite vieille au milieu du confessionnal ! ⛪",
  "J'adore les bûcherons ! 🪓",
  "Ouh là là, j'ai laissé la tarte aux pommes au four ! 🥧",
  "Poussez-vous les jeunes, mamie est pressée ! 👵💨",
  "Vite, les soldes sur les disques durs SSD ! 💾",
  "Noos ! N'oublie pas ton pull en laine ! 🧶",
  "Hop hop hop, un serveur NAS ça n'attend pas ! ⚡",
  "Mon tricot va refroidir ! 🧣"
];

function handleLogoEasterEggClick() {
  grannyLogoClickCount++;

  const logoWrap = document.getElementById('header-logo-wrap') || document.querySelector('.logo-wrap');
  if (logoWrap) {
    logoWrap.classList.remove('logo-click-bounce');
    void logoWrap.offsetWidth;
    logoWrap.classList.add('logo-click-bounce');
  }

  clearTimeout(grannyLogoClickTimer);
  grannyLogoClickTimer = setTimeout(() => {
    grannyLogoClickCount = 0;
  }, 2500);

  if (grannyLogoClickCount >= 5) {
    grannyLogoClickCount = 0;
    triggerGrannyEasterEgg();
  }
}

function triggerGrannyEasterEgg() {
  if (isGrannyRunning) return;
  isGrannyRunning = true;

  const track = document.getElementById('granny-easter-egg-track');
  if (!track) return;
  track.innerHTML = RUNNER_GRANNY_SVG_TEMPLATE;

  const runner = document.getElementById('granny-runner');
  const bubble = document.getElementById('granny-speech-bubble');
  const bubbleText = document.getElementById('granny-speech-text');

  if (bubbleText) {
    bubbleText.textContent = GRANNY_QUOTES[Math.floor(Math.random() * GRANNY_QUOTES.length)];
  }

  // Déverrouiller le contexte audio lors de ce clic utilisateur
  getGrannyAudioContext();

  let currentFrame = 0;
  const stepInterval = setInterval(() => {
    currentFrame = (currentFrame + 1) % 4;
    if (runner) runner.setAttribute('data-frame', currentFrame);
    playGrannyFootstep(currentFrame);
  }, 140);

  // Apparition de la bulle après 1.2s de course
  setTimeout(() => {
    if (bubble) bubble.classList.add('visible');
  }, 1200);

  // Disparition de la bulle avant la sortie d'écran
  setTimeout(() => {
    if (bubble) bubble.classList.remove('visible');
  }, 4200);

  // Fin du sprint : arrêt de la boucle sonore, tintement doux et nettoyage
  setTimeout(() => {
    clearInterval(stepInterval);
    playGrannyVictoryDing();
    if (track) track.innerHTML = '';
    isGrannyRunning = false;
  }, 5800);
}


// ==========================================================================
// GESTION DU PARTITIONNEMENT & PÉRIPHÉRIQUES AMOVIBLES
// ==========================================================================
let currentPartFreeBytes = 0;

function openCreatePartitionModal(diskPath, diskModel, freeBytes) {
  const modal = document.getElementById("modal-create-partition");
  if (!modal) return;

  currentPartFreeBytes = freeBytes || 0;
  const diskPathInput = document.getElementById("part-create-disk-path");
  const diskLabel = document.getElementById("part-create-disk-label");
  const freeLabel = document.getElementById("part-create-free-label");

  if (diskPathInput) diskPathInput.value = diskPath || "";
  if (diskLabel) diskLabel.textContent = `${diskPath} (${diskModel || "Disque"})`;
  if (freeLabel) freeLabel.textContent = formatFileSize(currentPartFreeBytes);

  setPartSizePercent(100);

  const cleanDisk = (diskPath || "data").replace("/dev/", "").toLowerCase();
  const mountInput = document.getElementById("part-create-mountpoint");
  if (mountInput) mountInput.value = `/mnt/${cleanDisk}`;

  modal.style.display = "flex";
}

function closeCreatePartitionModal() {
  const modal = document.getElementById("modal-create-partition");
  if (modal) modal.style.display = "none";
}

function setPartSizePercent(pct) {
  if (!currentPartFreeBytes) return;
  const targetBytes = (currentPartFreeBytes * pct) / 100;
  const unitSel = document.getElementById("part-create-unit");
  const sizeInput = document.getElementById("part-create-size");
  if (!unitSel || !sizeInput) return;

  const freeGb = targetBytes / (1024 * 1024 * 1024);
  if (freeGb >= 1) {
    unitSel.value = "gb";
    sizeInput.value = Math.max(1, Math.floor(freeGb));
  } else {
    unitSel.value = "mb";
    sizeInput.value = Math.max(50, Math.floor(targetBytes / (1024 * 1024)));
  }
}

async function submitCreatePartition() {
  const disk = document.getElementById("part-create-disk-path")?.value;
  const sizeVal = parseFloat(document.getElementById("part-create-size")?.value || "0");
  const unit = document.getElementById("part-create-unit")?.value || "gb";
  const fsType = document.getElementById("part-create-fstype")?.value || "btrfs";
  const label = document.getElementById("part-create-label")?.value.trim() || "DATA";
  const mnt = document.getElementById("part-create-mountpoint")?.value.trim() || null;
  const btn = document.getElementById("btn-submit-create-part");

  if (!disk) return;

  let sizeMb = 0;
  if (sizeVal > 0) {
    sizeMb = unit === "gb" ? Math.floor(sizeVal * 1024) : Math.floor(sizeVal);
  }

  if (btn) {
    btn.disabled = true;
    btn.textContent = "Création et formatage en cours...";
  }

  try {
    const res = await fetch("/api/storage/partition/create", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        disk_path: disk,
        size_mb: sizeMb > 0 ? sizeMb : null,
        fs_type: fsType,
        label: label,
        mountpoint: mnt,
      }),
    });

    const data = await res.json();
    if (data.success) {
      showToast(data.data || "Partition créée avec succès !", "success");
      closeCreatePartitionModal();
      loadStorage();
    } else {
      showSystemError("Échec du partitionnement", `Impossible de créer la partition sur ${disk}`, data.message || "Erreur inconnue", 12000);
    }
  } catch (err) {
    showToast(`Erreur réseau : ${err.message}`, "error", 10000);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "➕ Créer & Formater la Partition";
    }
  }
}

function openDeletePartitionModal(partPath, partSize) {
  const modal = document.getElementById("modal-delete-partition");
  if (!modal) return;

  const pathInput = document.getElementById("part-delete-path");
  const targetLabel = document.getElementById("part-delete-target-label");
  const sizeLabel = document.getElementById("part-delete-size-label");

  if (pathInput) pathInput.value = partPath || "";
  if (targetLabel) targetLabel.textContent = partPath || "";
  if (sizeLabel) sizeLabel.textContent = partSize || "";

  const cb = document.getElementById("part-delete-confirm-check");
  if (cb) cb.checked = false;
  toggleDeletePartBtn();

  modal.style.display = "flex";
}

function closeDeletePartitionModal() {
  const modal = document.getElementById("modal-delete-partition");
  if (modal) modal.style.display = "none";
}

function toggleDeletePartBtn() {
  const cb = document.getElementById("part-delete-confirm-check");
  const btn = document.getElementById("btn-submit-delete-part");
  if (!btn) return;
  if (cb && cb.checked) {
    btn.disabled = false;
    btn.classList.remove("disabled");
  } else {
    btn.disabled = true;
    btn.classList.add("disabled");
  }
}

async function submitDeletePartition() {
  const part = document.getElementById("part-delete-path")?.value;
  const btn = document.getElementById("btn-submit-delete-part");
  if (!part) return;

  if (btn) {
    btn.disabled = true;
    btn.textContent = "Suppression en cours...";
  }

  try {
    const res = await fetch("/api/storage/partition/delete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ partition_path: part }),
    });

    const data = await res.json();
    if (data.success) {
      showToast(data.data || "Partition supprimée avec succès !", "success");
      closeDeletePartitionModal();
      loadStorage();
    } else {
      showSystemError("Échec de la suppression", `Impossible de supprimer la partition ${part}`, data.message || "Erreur inconnue", 12000);
    }
  } catch (err) {
    showToast(`Erreur réseau : ${err.message}`, "error", 10000);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "🗑️ Supprimer Définitivement";
    }
  }
}

let safeRemovalTimer = null;

function showSafeRemovalToast(msg) {
  const toast = document.getElementById("safe-removal-toast");
  const msgEl = document.getElementById("safe-removal-msg");
  const progressEl = document.getElementById("safe-removal-progress");
  if (!toast) return;

  if (msgEl) msgEl.textContent = msg || "Votre périphérique peut être déconnecté en toute sécurité.";

  toast.style.display = "flex";

  if (progressEl) {
    progressEl.style.animation = "none";
    progressEl.offsetHeight; // trigger reflow
    progressEl.style.animation = "toastShrink 10s linear forwards";
  }

  if (safeRemovalTimer) clearTimeout(safeRemovalTimer);
  safeRemovalTimer = setTimeout(() => {
    hideSafeRemovalToast();
  }, 10000);
}

function hideSafeRemovalToast() {
  const toast = document.getElementById("safe-removal-toast");
  if (toast) toast.style.display = "none";
  if (safeRemovalTimer) {
    clearTimeout(safeRemovalTimer);
    safeRemovalTimer = null;
  }
}

async function ejectRemovableDevice(devPath, devName) {
  try {
    showToast(`Éjection de ${devName || devPath}...`, "info", 3000);
    const res = await fetch("/api/storage/removable/eject", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        device_path: devPath,
        name: devName,
      }),
    });

    const data = await res.json();
    if (data.success) {
      showSafeRemovalToast(data.data || `Votre périphérique '${devName || devPath}' peut être déconnecté en toute sécurité !`);
      loadStorage();
    } else {
      showSystemError("Échec de l'éjection", `Impossible d'éjecter le périphérique ${devName || devPath}`, data.message || "Erreur inconnue", 12000);
    }
  } catch (err) {
    showToast(`Erreur réseau lors de l'éjection : ${err.message}`, "error", 10000);
  }
}


/* ==========================================================================
   GESTION DU MONTAGE DES PÉRIPHÉRIQUES AMOVIBLES (USB, OPTIQUE)
   ========================================================================== */

let suggestedRemovableMountPath = "/media/usb";

function openMountRemovableModal(devicePath, deviceName, deviceModel, fstype, label, sizeHuman) {
  const modal = document.getElementById("modal-mount-removable");
  if (!modal) return;

  const pathHidden = document.getElementById("removable-mount-device-path");
  const nameHidden = document.getElementById("removable-mount-device-name");
  const fsHidden = document.getElementById("removable-mount-detected-fs");
  const modelEl = document.getElementById("removable-mount-model");
  const sizeEl = document.getElementById("removable-mount-size");
  const fsBadge = document.getElementById("removable-mount-fs-badge");
  const devLabel = document.getElementById("removable-mount-dev-label");
  const labelWrap = document.getElementById("removable-mount-label-wrap");
  const labelEl = document.getElementById("removable-mount-label");
  const mountInput = document.getElementById("removable-mount-path");
  const iconEl = document.getElementById("removable-mount-icon");
  const titleEl = document.getElementById("removable-modal-title");
  const badgeEl = document.getElementById("removable-modal-badge");

  const isOptical = (devicePath || "").includes("sr") || (deviceName || "").startsWith("sr");
  if (iconEl) iconEl.textContent = isOptical ? "💿" : "🔌";
  if (badgeEl) badgeEl.textContent = isOptical ? "DISQUE OPTIQUE" : "MÉDIA AMOVIBLE";
  if (titleEl) titleEl.textContent = isOptical ? "Monter le Disque Optique" : `Monter : ${deviceModel || deviceName}`;

  if (pathHidden) pathHidden.value = devicePath || "";
  if (nameHidden) nameHidden.value = deviceName || "";
  if (fsHidden) fsHidden.value = fstype || "";

  if (modelEl) modelEl.textContent = deviceModel || deviceName || "Périphérique Amovible";
  if (sizeEl) sizeEl.textContent = sizeHuman || "--";
  if (fsBadge) {
    fsBadge.textContent = (fstype || "AUTO").toUpperCase();
    fsBadge.className = fstype ? "badge badge-accent" : "badge badge-secondary";
  }
  if (devLabel) devLabel.textContent = devicePath || "";

  if (labelWrap && labelEl) {
    if (label && label.trim()) {
      labelWrap.style.display = "inline";
      labelEl.textContent = label.trim();
    } else {
      labelWrap.style.display = "none";
    }
  }

  // Calcul du point de montage suggéré propre
  let cleanName = (label && label.trim()) ? label.trim() : (deviceName || "usb");
  cleanName = cleanName.toLowerCase().replace(/[^a-z0-9_-]/g, "_").replace(/^_+|_+$/g, "");
  if (!cleanName) cleanName = "usb";

  suggestedRemovableMountPath = `/media/${cleanName}`;
  if (mountInput) {
    mountInput.value = suggestedRemovableMountPath;
  }

  const rwCheckbox = document.getElementById("removable-mount-opt-rw");
  if (rwCheckbox) rwCheckbox.checked = true;

  const persistCheckbox = document.getElementById("removable-mount-opt-persist");
  if (persistCheckbox) persistCheckbox.checked = false;

  const btn = document.getElementById("btn-submit-mount-removable");
  if (btn) {
    btn.disabled = false;
    btn.innerHTML = `<span>⚡</span> Monter le Périphérique`;
  }

  modal.style.display = "flex";
}

function closeMountRemovableModal() {
  const modal = document.getElementById("modal-mount-removable");
  if (modal) modal.style.display = "none";
}

function resetRemovableMountPath() {
  const mountInput = document.getElementById("removable-mount-path");
  if (mountInput) mountInput.value = suggestedRemovableMountPath;
}

function setRemovableMountPrefix(prefix) {
  const mountInput = document.getElementById("removable-mount-path");
  if (!mountInput) return;
  const current = mountInput.value.trim();
  const baseName = current.split("/").filter(Boolean).pop() || "usb";
  mountInput.value = `${prefix.replace(/\/+$/, "")}/${baseName}`;
}

async function submitMountRemovable() {
  const devicePath = document.getElementById("removable-mount-device-path")?.value;
  const deviceName = document.getElementById("removable-mount-device-name")?.value;
  const detectedFs = document.getElementById("removable-mount-detected-fs")?.value;
  const mountInput = document.getElementById("removable-mount-path");
  const rwOpt = document.getElementById("removable-mount-opt-rw")?.checked ?? true;
  const persistOpt = document.getElementById("removable-mount-opt-persist")?.checked ?? false;
  const btn = document.getElementById("btn-submit-mount-removable");

  const mountpoint = (mountInput?.value || "").trim();
  if (!devicePath || !mountpoint) {
    showToast("Veuillez spécifier un point de montage valide.", "warning");
    return;
  }

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span class="spinner-border spinner-border-sm" role="status" style="display:inline-block; width:14px; height:14px; border:2px solid currentColor; border-right-color:transparent; border-radius:50%; animation:spin 0.75s linear infinite; margin-right:6px;"></span> Montage en cours...`;
  }

  const options = ["defaults", "noatime"];
  if (rwOpt) {
    options.push("rw");
  }

  try {
    const res = await fetch("/api/storage/mount", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: deviceName || "removable",
        device: devicePath,
        mountpoint: mountpoint,
        fs_type: detectedFs || "auto",
        options: options,
        persist: persistOpt,
        force_format: false
      })
    });

    const json = await res.json();
    if (json.success) {
      closeMountRemovableModal();
      showToast(`✅ Périphérique monté avec succès sur ${mountpoint} !`, "success", 6000);
      loadStorage();
    } else {
      showSystemError("Échec du montage amovible", `Impossible de monter ${devicePath} sur ${mountpoint}`, json.message || "Erreur inconnue", 12000);
    }
  } catch (err) {
    showToast(`Erreur réseau lors du montage : ${err.message}`, "error", 10000);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span>⚡</span> Monter le Périphérique`;
    }
  }
}

function openFilesAtPath(path) {
  if (!path) return;
  try {
    localStorage.setItem("noos_files_path", path);
  } catch (e) {}
  switchTab("tab-files");
  if (typeof navigateToPath === "function") {
    navigateToPath(path);
  }
}


/* ==========================================================================
   GESTION DNS & RÉSOLVEURS RÉSEAU (Catppuccin Mocha)
   ========================================================================== */

const DEFAULT_DNS_PROVIDERS = [
  {
    id: "cloudflare",
    name: "Cloudflare (1.1.1.1)",
    icon: "⚡",
    description: "Résolveur mondial le plus rapide, politique stricte de confidentialité (zéro vente de données, purge des logs en 24h).",
    ipv4: ["1.1.1.1", "1.0.0.1"],
    ipv6: ["2606:4700:4700::1111", "2606:4700:4700::1001"],
    tags: ["Ultra-Rapide", "Confidentialité", "Anycast"],
    ping_ms: 12,
    is_active: true
  },
  {
    id: "quad9",
    name: "Quad9 (Protection Malwares)",
    icon: "🛡️",
    description: "Fondation suisse sans but lucratif bloquant automatiquement les domaines malveillants, botnets et phishing en temps réel.",
    ipv4: ["9.9.9.9", "149.112.112.112"],
    ipv6: ["2620:fe::fe", "2620:fe::9"],
    tags: ["Anti-Malware", "Suisse / RGPD", "Zero-Log"],
    ping_ms: 28,
    is_active: false
  },
  {
    id: "adguard",
    name: "AdGuard DNS (Anti-Pub & Traqueurs)",
    icon: "🚫",
    description: "Bloque les bannières publicitaires, compteurs analytiques et traqueurs au niveau DNS pour tous les appareils du réseau.",
    ipv4: ["94.140.14.14", "94.140.15.15"],
    ipv6: ["2a10:50c0::ad1:ff", "2a10:50c0::ad2:ff"],
    tags: ["Anti-Pub", "Anti-Traqueur", "Protection Web"],
    ping_ms: 18,
    is_active: false
  },
  {
    id: "google",
    name: "Google Public DNS",
    icon: "🌐",
    description: "Infrastructure robuste haute disponibilité à couverture planétaire, accélération de la résolution et résilience mondiale.",
    ipv4: ["8.8.8.8", "8.8.4.4"],
    ipv6: ["2001:4860:4860::8888", "2001:4860:4860::8844"],
    tags: ["Haute Disponibilité", "Anycast Mondial", "Standard"],
    ping_ms: 14,
    is_active: false
  },
  {
    id: "mullvad",
    name: "Mullvad DNS (Non censuré & Chiffré)",
    icon: "🔒",
    description: "Orienté vie privée maximale basé en Suède, zéro journalisation, conforme aux normes strictes de non-surveillance.",
    ipv4: ["194.242.2.2"],
    ipv6: ["2a07:e340::2"],
    tags: ["Confidentialité Maximale", "Suède", "Anti-Censure"],
    ping_ms: 16,
    is_active: false
  }
];

function renderDnsProvidersList(providers, container, activeMode = "cloudflare") {
  if (!container) return;
  container.innerHTML = providers.map(p => {
    const isActive = p.is_active || (activeMode && p.id === activeMode);
    let pingHtml = "";
    if (p.ping_ms != null) {
      const pingClass = p.ping_ms < 20 ? "ping-fast" : p.ping_ms < 50 ? "ping-medium" : "ping-slow";
      pingHtml = `<span class="dns-ping-tag ${pingClass}">⚡ ${p.ping_ms} ms</span>`;
    } else {
      pingHtml = `<span class="dns-ping-tag ping-fast">⚡ En attente</span>`;
    }

    const ipTags = (p.ipv4 || []).map(ip => `
      <span class="dns-ip-tag" onclick="copyDnsIp('${escapeHtml(ip)}')" title="Cliquer pour copier l'IP">
        <code>${escapeHtml(ip)}</code> 📋
      </span>
    `).join(" ");

    const featureTags = (p.tags || []).map(t => `
      <span class="badge badge-secondary" style="font-size:0.72rem;">${escapeHtml(t)}</span>
    `).join(" ");

    return `
      <div class="dns-provider-row ${isActive ? 'is-active' : ''}">
        <div class="dns-provider-left">
          <div class="dns-provider-icon">${p.icon || '🌐'}</div>
          <div class="dns-provider-info">
            <div class="dns-provider-header">
              <span class="dns-provider-name">${escapeHtml(p.name)}</span>
              ${pingHtml}
              ${isActive ? '<span class="badge badge-success" style="font-size:0.75rem; font-weight:800;">✓ ACTIF</span>' : ''}
              ${featureTags}
            </div>
            <div class="dns-provider-desc">${escapeHtml(p.description)}</div>
            <div class="dns-provider-ips">
              <span style="font-size:0.75rem; color:var(--subtext0); margin-right:4px;">Adresses IPv4 :</span>
              ${ipTags}
            </div>
          </div>
        </div>
        <div class="dns-provider-actions">
          ${isActive 
            ? `<button type="button" class="btn btn-secondary btn-sm" disabled style="opacity:0.8; cursor:default;">
                 ✓ En Cours
               </button>`
            : `<button type="button" class="btn btn-glow-mount btn-sm" onclick="applyPopularDns('${escapeHtml(p.id)}', '${escapeHtml(p.name)}')">
                 <span>⚡</span> Activer à chaud
               </button>`}
        </div>
      </div>
    `;
  }).join("");
}

let cachedDnsData = null;

async function loadDnsSettings(showToastFeedback = false) {
  const container = document.getElementById("dns-providers-container");
  if (!container) return;

  // Hydratation SWR immédiate (0ms) : rendu instantané du catalogue
  if (!cachedDnsData || container.children.length <= 1) {
    renderDnsProvidersList(DEFAULT_DNS_PROVIDERS, container, (cachedDnsData && cachedDnsData.mode) || "cloudflare");
  }

  if (showToastFeedback) {
    showToast("Mesure du ping et vérification de la résolution DNS...", "info", 2500);
  }

  try {
    const res = await fetch("/api/network/dns");
    if (!res.ok) {
      console.warn("API /api/network/dns a retourné HTTP", res.status);
      return;
    }
    const json = await res.json();
    if (!json || !json.success || !json.data) return;

    const dns = json.data;
    cachedDnsData = dns;

    // 1. Mise à jour de la carte Héro
    const heroStatus = document.getElementById("dns-hero-status-badge");
    const heroActive = document.getElementById("dns-hero-active-badge");
    const port53Status = document.getElementById("dns-port-53-status-text");
    const activePing = document.getElementById("dns-active-ping-text");
    const fallbackList = document.getElementById("dns-fallback-list-text");
    const navBadge = document.getElementById("badge-subtab-dns");
    const togglePort53 = document.getElementById("dns-toggle-free-port");

    if (heroStatus) {
      if (dns.resolution_status) {
        heroStatus.textContent = "🟢 Résolution Opérationnelle";
        heroStatus.className = "badge badge-success";
      } else {
        heroStatus.textContent = "🔴 Résolution Instable";
        heroStatus.className = "badge badge-danger";
      }
    }

    if (heroActive) {
      const pingText = dns.active_ping_ms != null ? ` • ${dns.active_ping_ms} ms` : "";
      heroActive.textContent = `${dns.mode.toUpperCase()}${pingText}`;
    }

    if (navBadge) {
      navBadge.textContent = dns.mode === "custom" ? "Personnalisé" : (dns.active_servers[0] || dns.mode);
    }

    if (port53Status) {
      if (dns.port_53_available) {
        port53Status.innerHTML = `<span style="color:var(--green); font-weight:700;">🛡️ Libre &amp; Disponible</span>`;
      } else {
        port53Status.innerHTML = `<span style="color:var(--yellow); font-weight:700;">⚠️ Occupé (systemd-resolved)</span>`;
      }
    }

    if (activePing) {
      if (dns.active_ping_ms != null) {
        const pingClass = dns.active_ping_ms < 20 ? "color:var(--green)" : dns.active_ping_ms < 50 ? "color:var(--yellow)" : "color:var(--red)";
        activePing.innerHTML = `<span style="${pingClass}; font-weight:700;">${dns.active_ping_ms} ms</span>`;
      } else {
        activePing.innerHTML = `<span style="color:var(--subtext0); font-weight:700;">Non mesuré</span>`;
      }
    }

    if (fallbackList) {
      fallbackList.textContent = (dns.fallback_servers || []).join(", ") || "1.1.1.1, 9.9.9.9";
    }

    if (togglePort53) {
      togglePort53.checked = dns.free_port_53;
    }

    // 2. Rendu de la liste des fournisseurs populaires
    const providers = (dns.providers && dns.providers.length > 0) ? dns.providers : DEFAULT_DNS_PROVIDERS;
    renderDnsProvidersList(providers, container, dns.mode);

    // 3. Pré-remplissage du formulaire personnalisé
    const customInput = document.getElementById("dns-custom-ip");
    if (customInput && dns.mode === "custom" && (dns.custom_servers || []).length > 0) {
      customInput.value = dns.custom_servers.join(", ");
    }

    if (showToastFeedback) {
      showToast("Télémétrie DNS actualisée avec succès !", "success", 2000);
    }
  } catch (err) {
    console.warn("Erreur chargement DNS:", err);
  }
}

function copyDnsIp(ip) {
  if (!ip) return;
  navigator.clipboard.writeText(ip).then(() => {
    showToast(`Adresse IP ${ip} copiée dans le presse-papier !`, "info", 2000);
  }).catch(() => {});
}

async function applyPopularDns(providerId, providerName) {
  const freePort = document.getElementById("dns-toggle-free-port")?.checked ?? true;
  showToast(`Application de ${providerName} à chaud...`, "info", 3000);

  try {
    const res = await fetch("/api/network/dns", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        mode: providerId,
        free_port_53: freePort,
        fallback_servers: ["1.1.1.1", "9.9.9.9"]
      })
    });

    const json = await res.json();
    if (json.success) {
      showToast(json.data || `Fournisseur DNS ${providerName} appliqué avec succès !`, "success", 4000);
      loadDnsSettings();
    } else {
      showSystemError("Échec du changement DNS", "La configuration DNS n'a pas pu être appliquée.", json.message || "Erreur inconnue", 12000);
    }
  } catch (err) {
    showToast(`Erreur réseau lors du changement DNS : ${err.message}`, "error", 10000);
  }
}

async function applyCustomDns() {
  const input = document.getElementById("dns-custom-ip");
  const fallbackSel = document.getElementById("dns-fallback-select");
  const freePortToggle = document.getElementById("dns-toggle-free-port");
  const btn = document.getElementById("btn-apply-custom-dns");

  const rawIp = (input?.value || "").trim();
  if (!rawIp) {
    showToast("Veuillez saisir l'adresse IP de votre serveur DNS local (ex: 127.0.0.1 ou 192.168.1.50).", "warning", 5000);
    input?.focus();
    return;
  }

  const customServers = rawIp.split(/[,;\s]+/).filter(Boolean);
  let fallbacks = ["1.1.1.1", "9.9.9.9"];
  const selVal = fallbackSel?.value || "cloudflare_quad9";
  if (selVal === "cloudflare") fallbacks = ["1.1.1.1", "1.0.0.1"];
  if (selVal === "quad9") fallbacks = ["9.9.9.9", "149.112.112.112"];
  if (selVal === "google") fallbacks = ["8.8.8.8", "8.8.4.4"];

  const freePort = freePortToggle?.checked ?? true;

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `<span class="spinner-border spinner-border-sm" role="status" style="display:inline-block; width:14px; height:14px; border:2px solid currentColor; border-right-color:transparent; border-radius:50%; animation:spin 0.75s linear infinite; margin-right:6px;"></span> Application à chaud...`;
  }

  try {
    const res = await fetch("/api/network/dns", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        mode: "custom",
        custom_servers: customServers,
        fallback_servers: fallbacks,
        free_port_53: freePort
      })
    });

    const json = await res.json();
    if (json.success) {
      showToast(`DNS local appliqué avec succès ! Résolution vérifiée.`, "success", 5000);
      loadDnsSettings();
    } else {
      showSystemError("Échec du DNS personnalisé", "Le serveur DNS spécifié ne répond pas correctement.", json.message || "Erreur de configuration", 12000);
    }
  } catch (err) {
    showToast(`Erreur réseau lors de l'application DNS : ${err.message}`, "error", 10000);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span>🚀</span> Appliquer le DNS Personnalisé à chaud`;
    }
  }
}


// ==========================================================================
// MOTEUR DE TÂCHES ASYNCHRONES DE STOCKAGE (DISSOLUTION & ASSEMBLAGE RAID)
// ==========================================================================

let storageJobPollingTimer = null;
let storageToastDismissTimer = null;
let activeStorageJob = null;

function startPollingStorageJob() {
  if (storageJobPollingTimer) clearInterval(storageJobPollingTimer);
  pollStorageJob();
  storageJobPollingTimer = setInterval(pollStorageJob, 1200);
}

async function pollStorageJob() {
  try {
    const res = await fetch("/api/storage/jobs/active");
    if (!res.ok) return;
    const json = await res.json();
    if (json.success && json.data) {
      activeStorageJob = json.data;
      updateStorageJobView(json.data);
      if (json.data.status === "completed" || json.data.status === "failed") {
        if (storageJobPollingTimer) {
          clearInterval(storageJobPollingTimer);
          storageJobPollingTimer = null;
        }
        loadStorage();
        scheduleStorageToastDismiss(10000);
      }
    } else {
      activeStorageJob = null;
      hideStorageJobView();
      if (storageJobPollingTimer) {
        clearInterval(storageJobPollingTimer);
        storageJobPollingTimer = null;
      }
    }
  } catch (err) {
    console.warn("[STORAGE] Erreur polling tâche stockage:", err);
  }
}

function updateStorageJobView(job) {
  const toast = document.getElementById("storage-floating-toast");
  const toastTitle = document.getElementById("storage-toast-title");
  const toastPercent = document.getElementById("storage-toast-percent");
  const toastDetail = document.getElementById("storage-toast-detail");
  const toastBar = document.getElementById("storage-toast-progress-bar");
  const toastIcon = document.getElementById("storage-toast-icon");
  const toastClose = document.getElementById("btn-close-storage-toast");

  if (toast) {
    toast.style.display = "block";
    toast.classList.remove("theme-destroy", "theme-create", "toast-fading-out");
    if (job.job_type === "destroy_raid") {
      toast.classList.add("theme-destroy");
    } else {
      toast.classList.add("theme-create");
    }

    if (job.status === "completed") {
      if (toastIcon) toastIcon.textContent = "✨";
      if (toastTitle) toastTitle.textContent = "Opération terminée avec succès !";
      if (toastPercent) toastPercent.textContent = "100%";
      if (toastDetail) toastDetail.textContent = job.result_message || job.step_detail;
      if (toastBar) toastBar.style.width = "100%";
      if (toastClose) toastClose.style.display = "block";
    } else if (job.status === "failed") {
      if (toastIcon) toastIcon.textContent = "🛑";
      if (toastTitle) toastTitle.textContent = "Échec de l'opération de stockage";
      if (toastPercent) toastPercent.textContent = `${job.progress_percent}%`;
      if (toastDetail) toastDetail.textContent = job.error || job.step_detail;
      if (toastBar) toastBar.style.width = `${job.progress_percent}%`;
      if (toastClose) toastClose.style.display = "block";
    } else {
      if (toastIcon) toastIcon.textContent = job.job_type === "destroy_raid" ? "🧨" : "🛠️";
      const isResumed = job.status === "resumed";
      const prefix = isResumed ? "⚡ [Reprise reboot] " : "";
      if (toastTitle) {
        toastTitle.textContent = `${prefix}${job.job_type === "destroy_raid" ? "Dissolution" : "Assemblage"} : ${job.target_name}`;
      }
      if (toastPercent) toastPercent.textContent = `${job.progress_percent}%`;
      if (toastDetail) toastDetail.textContent = `Étape ${job.current_step}/${job.total_steps} : ${job.step_detail}`;
      if (toastBar) toastBar.style.width = `${job.progress_percent}%`;
      if (toastClose) toastClose.style.display = "none";
    }
  }

  // Mettre à jour le panneau intégré dans Stockage & Disque
  updateStorageRaidJobPanel(job);
}

function updateStorageRaidJobPanel(job) {
  const panel = document.getElementById("storage-raid-job-panel");
  if (!panel) return;

  if (!job || (job.status !== "running" && job.status !== "resumed" && job.status !== "completed" && job.status !== "failed")) {
    panel.style.display = "none";
    return;
  }

  panel.style.display = "block";
  panel.className = `storage-raid-job-panel ${job.job_type === "destroy_raid" ? "panel-destroy" : "panel-create"}`;

  const isDestroy = job.job_type === "destroy_raid";
  const icon = isDestroy ? "🧨" : "🛠️";
  const badgeClass = isDestroy ? "badge-destroy" : "badge-create";
  const isResumed = job.status === "resumed";
  const statusBadge = isResumed
    ? `<span class="storage-raid-job-badge badge-destroy">⚡ Reprise suite au redémarrage</span>`
    : `<span class="storage-raid-job-badge ${badgeClass}">⚙️ Tâche d'arrière-plan sécurisée</span>`;

  const total = job.total_steps || (isDestroy ? 6 : 5);
  let stepsHtml = "";
  for (let s = 1; s <= total; s++) {
    let cls = "stepper-pill";
    let iconStep = `○`;
    if (s < job.current_step || job.status === "completed") {
      cls += " step-done";
      iconStep = "✔";
    } else if (s === job.current_step && job.status !== "completed" && job.status !== "failed") {
      cls += " step-active";
      iconStep = "⏳";
    }
    stepsHtml += `<div class="${cls}"><span>${iconStep}</span> Étape ${s}</div>`;
  }

  panel.innerHTML = `
    <div class="storage-raid-job-header">
      <div class="storage-raid-job-title-wrap">
        <span style="font-size:1.3rem;">${icon}</span>
        <div>
          <div class="storage-raid-job-title">${isDestroy ? "Dissolution et Destruction de Grappe RAID" : "Création et Assemblage de Grappe RAID"} : <span style="color:var(--mauve); font-weight:800;">${job.target_name}</span></div>
          <div style="font-size:0.75rem; color:var(--subtext0); margin-top:2px;">Disques cibles : ${job.member_devices && job.member_devices.length ? job.member_devices.join(", ") : job.target_device}</div>
        </div>
        ${statusBadge}
      </div>
      <div class="storage-raid-job-percent">${job.progress_percent}%</div>
    </div>
    <div class="storage-raid-job-stepper">
      ${stepsHtml}
    </div>
    <div class="storage-raid-job-progress-wrap">
      <div class="storage-raid-job-progress-bar" style="width: ${job.progress_percent}%;"></div>
    </div>
    <div class="storage-raid-job-footer">
      <div><strong>Phase active :</strong> ${job.step_name} — <em>${job.step_detail}</em></div>
      <div>ID: <code>${job.id}</code></div>
    </div>
  `;
}

function hideStorageJobView() {
  const toast = document.getElementById("storage-floating-toast");
  if (toast) toast.style.display = "none";
  const panel = document.getElementById("storage-raid-job-panel");
  if (panel) panel.style.display = "none";
}

function scheduleStorageToastDismiss(delayMs) {
  if (storageToastDismissTimer) clearTimeout(storageToastDismissTimer);
  storageToastDismissTimer = setTimeout(() => {
    dismissStorageJobToast();
  }, delayMs);
}

async function dismissStorageJobToast() {
  if (storageToastDismissTimer) {
    clearTimeout(storageToastDismissTimer);
    storageToastDismissTimer = null;
  }
  const toast = document.getElementById("storage-floating-toast");
  if (toast) {
    toast.classList.add("toast-fading-out");
    setTimeout(() => {
      toast.style.display = "none";
      toast.classList.remove("toast-fading-out");
    }, 450);
  }
  try {
    await fetch("/api/storage/jobs/dismiss", { method: "POST" });
  } catch (e) {}
}

async function checkActiveStorageJobOnLoad() {
  try {
    const res = await fetch("/api/storage/jobs/active");
    if (!res.ok) return;
    const json = await res.json();
    if (json.success && json.data) {
      const job = json.data;
      if (job.status === "running" || job.status === "resumed") {
        activeStorageJob = job;
        updateStorageJobView(job);
        startPollingStorageJob();
      } else if (job.status === "completed") {
        let elapsedSec = 0;
        if (job.completed_at) {
          const nowSec = Math.floor(Date.now() / 1000);
          elapsedSec = Math.max(0, nowSec - job.completed_at);
        }
        if (elapsedSec < 15) {
          updateStorageJobView(job);
          scheduleStorageToastDismiss(Math.max(2000, (15 - elapsedSec) * 1000));
        } else {
          dismissStorageJobToast();
        }
      }
    }
  } catch (e) {}
}


// =========================================================================
// 🗄️ GESTIONNAIRE COMPLET SAMBA (SMB / CIFS) — STEvE_OS CATPPUCCIN MOCHA
// =========================================================================

let currentSambaData = null;

async function loadSambaData(showFeedback = false) {
  try {
    const res = await fetch("/api/samba");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const json = await res.json();
    if (!json.success || !json.data) throw new Error(json.error || "Données indisponibles");

    const data = json.data;
    currentSambaData = data;

    // 1. En-tête et Badges d'état
    const statusBadge = document.getElementById("samba-status-badge");
    const versionBadge = document.getElementById("samba-version-badge");
    const pulseDot = document.getElementById("samba-hero-pulse-dot");

    if (statusBadge) {
      statusBadge.textContent = data.is_active ? "● En Ligne" : "● Inactif";
      statusBadge.className = `samba-status-tag ${data.is_active ? "active" : "inactive"}`;
    }
    if (versionBadge && data.version) {
      versionBadge.textContent = `Samba v${data.version}`;
    }
    if (pulseDot) {
      pulseDot.style.display = data.is_active ? "block" : "none";
    }

    // Badge sous-onglet dans la navigation
    const navBadge = document.getElementById("badge-subtab-samba");
    if (navBadge) {
      navBadge.textContent = data.is_active ? "Actif" : "Inactif";
      navBadge.className = `subtab-pill-badge ${data.is_active ? "badge-success" : "badge-secondary"}`;
    }

    // 2. Adresses de connexion rapide
    const uriWin = document.getElementById("samba-uri-val-win");
    const uriMac = document.getElementById("samba-uri-val-mac");
    const uriLnx = document.getElementById("samba-uri-val-lnx");

    const ip = data.primary_lan_ip || data.primary_ip || window.location.hostname;
    const host = data.hostname || "noos-nas";

    if (uriWin) uriWin.textContent = `\\${ip}`;
    if (uriMac) uriMac.textContent = `smb://${ip}`;
    if (uriLnx) uriLnx.textContent = `smb://${ip}`;

    // 3. Mise à jour des KPIs
    const kpiSharesCount = document.getElementById("samba-kpi-shares-count");
    const kpiSharesSub = document.getElementById("samba-kpi-shares-sub");
    const kpiSessionsCount = document.getElementById("samba-kpi-sessions-count");
    const kpiSessionsSub = document.getElementById("samba-kpi-sessions-sub");
    const kpiLocksCount = document.getElementById("samba-kpi-locks-count");
    const kpiLocksSub = document.getElementById("samba-kpi-locks-sub");
    const kpiFeaturesCount = document.getElementById("samba-kpi-features-count");
    const kpiFeaturesSub = document.getElementById("samba-kpi-features-sub");

    const shares = data.shares || [];
    const sessions = data.active_sessions || [];
    const locks = data.locked_files || [];

    const rwCount = shares.filter(s => !s.read_only).length;
    const tmCount = shares.filter(s => s.time_machine).length;
    const recycleCount = shares.filter(s => s.recycle_bin).length;

    if (kpiSharesCount) kpiSharesCount.textContent = shares.length;
    if (kpiSharesSub) kpiSharesSub.textContent = `${rwCount} en lecture/écriture, ${shares.length - rwCount} lecture seule`;

    if (kpiSessionsCount) kpiSessionsCount.textContent = sessions.length;
    if (kpiSessionsSub) kpiSessionsSub.textContent = sessions.length === 1 ? "1 client connecté" : `${sessions.length} clients connectés`;

    if (kpiLocksCount) kpiLocksCount.textContent = locks.length;
    if (kpiLocksSub) kpiLocksSub.textContent = locks.length === 1 ? "1 fichier ouvert" : `${locks.length} fichiers ouverts`;

    if (kpiFeaturesCount) kpiFeaturesCount.textContent = `${tmCount} TM / ${recycleCount} ♻️`;
    if (kpiFeaturesSub) kpiFeaturesSub.textContent = `${tmCount} Time Machine, ${recycleCount} corbeilles`;

    // 4. Rendu de la liste des partages
    renderSambaSharesList(shares, data.available_users || []);

    // 5. Rendu des sessions actives
    renderSambaSessionsTable(sessions);

    // 6. Rendu des verrous
    renderSambaLocksTable(locks);

    if (showFeedback) {
      showToast("Données Samba actualisées avec succès !", "success");
    }
  } catch (err) {
    console.error("Erreur chargement Samba:", err);
    if (showFeedback) {
      showToast("Erreur lors du chargement de Samba : " + err.message, "error");
    }
  }
}

function renderSambaSharesList(shares, availableUsers) {
  const container = document.getElementById("samba-shares-list");
  if (!container) return;

  if (!shares || shares.length === 0) {
    container.innerHTML = `
      <div style="padding:40px; text-align:center; background:var(--mantle); border-radius:var(--radius-md); border:1px dashed rgba(255,255,255,0.1);">
        <div style="font-size:2.5rem; margin-bottom:12px;">📂</div>
        <div style="font-size:1.1rem; font-weight:700; color:var(--text); margin-bottom:6px;">Aucun partage Samba configuré</div>
        <div style="font-size:0.85rem; color:var(--subtext0); max-width:440px; margin:0 auto 16px auto;">
          Créez votre premier partage réseau pour accéder à vos fichiers depuis Windows, Mac, Linux ou vos appareils mobiles.
        </div>
        <button type="button" class="btn btn-primary" onclick="openCreateSambaShareModal()">
          <span>➕</span> Créer un Partage Réseau
        </button>
      </div>
    `;
    return;
  }

  const primaryIp = currentSambaData?.primary_ip || window.location.hostname;

  container.innerHTML = shares.map(share => {
    // Choix de l'icône contextuelle
    let icon = "📂";
    let cardClass = "";
    if (share.time_machine) {
      icon = "🍏";
      cardClass = "timemachine";
    } else if (share.read_only) {
      icon = "🔒";
      cardClass = "readonly";
    } else if (share.name.toLowerCase().includes("media") || share.name.toLowerCase().includes("video") || share.name.toLowerCase().includes("film")) {
      icon = "🎬";
    } else if (share.name.toLowerCase().includes("backup") || share.name.toLowerCase().includes("sauvegarde")) {
      icon = "📦";
    }

    // Badges
    const badges = [];
    if (share.read_only) {
      badges.push(`<span class="badge badge-warning" title="Lecture seule forcée">🔒 Lecture Seule</span>`);
    } else {
      badges.push(`<span class="badge badge-success" title="Accès en écriture autorisé">✏️ Lecture / Écriture</span>`);
    }

    if (share.guest_ok) {
      badges.push(`<span class="badge badge-info" title="Connexion sans mot de passe">👤 Invités Autorisés</span>`);
    } else {
      badges.push(`<span class="badge badge-primary" title="Authentification requise">🔑 Authentifié</span>`);
    }

    if (share.recycle_bin) {
      badges.push(`<span class="badge" style="background:rgba(166,227,161,0.15); color:var(--green); border:1px solid rgba(166,227,161,0.3);" title="Corbeille réseau active (.recycle)">♻️ Corbeille</span>`);
    }

    if (share.time_machine) {
      badges.push(`<span class="badge" style="background:rgba(148,226,213,0.15); color:var(--teal); border:1px solid rgba(148,226,213,0.3);" title="Cible de sauvegarde macOS Time Machine">🍏 Time Machine</span>`);
    }

    if (share.shadow_copy) {
      badges.push(`<span class="badge" style="background:rgba(203,166,247,0.15); color:var(--mauve); border:1px solid rgba(203,166,247,0.3);" title="Clichés instantanés Windows VSS">📸 Versions Précédentes</span>`);
    }

    if (share.smb_encrypt) {
      badges.push(`<span class="badge" style="background:rgba(243,139,168,0.15); color:var(--red); border:1px solid rgba(243,139,168,0.3);" title="Chiffrement matériel AES SMB3 obligatoire">🔐 Chiffré</span>`);
    }

    if (!share.browseable) {
      badges.push(`<span class="badge badge-secondary" title="Partage masqué sur le réseau">👁️‍🗨️ Masqué</span>`);
    }

    // Formater utilisateurs autorisés
    let usersDisplay = `<span style="color:var(--subtext0); font-style:italic;">Tous les utilisateurs authentifiés</span>`;
    if (share.write_list && share.write_list.length > 0) {
      usersDisplay = share.write_list.map(u => `<span class="badge badge-primary" style="font-size:0.75rem;">👤 ${escapeHtml(u)}</span>`).join(" ");
    } else if (share.valid_users && share.valid_users.length > 0) {
      usersDisplay = share.valid_users.map(u => `<span class="badge badge-secondary" style="font-size:0.75rem;">👤 ${escapeHtml(u)}</span>`).join(" ");
    }

    const uncPath = `\\${primaryIp}\${share.name}`;

    return `
      <div class="samba-share-card ${cardClass}" id="samba-card-${escapeHtml(share.id)}">
        <div class="samba-share-header">
          <div class="samba-share-title-group">
            <div class="samba-share-badge-icon">${icon}</div>
            <div>
              <div class="samba-share-name">${escapeHtml(share.name)}</div>
              <div class="samba-share-comment">${escapeHtml(share.comment || "Aucune description")}</div>
            </div>
          </div>

          <div class="samba-share-badges">
            ${badges.join(" ")}
          </div>

          <div class="samba-share-actions">
            <button type="button" class="btn btn-secondary btn-xs" onclick="copyTextToClipboard('${uncPath}', 'Chemin UNC copié !')" title="Copier le chemin réseau UNC : ${uncPath}">
              📋 Copier UNC
            </button>
            <button type="button" class="btn btn-secondary btn-xs" onclick="openEditSambaShareModal('${escapeHtml(share.id)}')" title="Modifier la configuration du partage">
              ✏️ Modifier
            </button>
            <button type="button" class="btn btn-danger btn-xs" onclick="deleteSambaShareConfirm('${escapeHtml(share.id)}', '${escapeHtml(share.name)}')" title="Supprimer ce partage">
              🗑️
            </button>
          </div>
        </div>

        <div class="samba-share-meta-row">
          <div>
            <div class="samba-share-field-label">Chemin Unix Hôte :</div>
            <div class="samba-share-field-val">
              <code class="samba-share-path-code">${escapeHtml(share.path)}</code>
              <button type="button" class="btn-copy-uri" onclick="copyTextToClipboard('${escapeHtml(share.path)}', 'Chemin hôte copié !')" title="Copier le chemin Unix">📋</button>
            </div>
          </div>

          <div>
            <div class="samba-share-field-label">Accès Utilisateurs :</div>
            <div class="samba-share-field-val">
              ${usersDisplay}
            </div>
          </div>

          <div>
            <div class="samba-share-field-label">Permissions Création :</div>
            <div class="samba-share-field-val">
              <span style="font-family:var(--font-mono); font-size:0.75rem; color:var(--subtext1);">Fichiers: ${escapeHtml(share.create_mask || "0664")} | Dossiers: ${escapeHtml(share.directory_mask || "0775")}</span>
            </div>
          </div>
        </div>
      </div>
    `;
  }).join("");
}

function renderSambaSessionsTable(sessions) {
  const tbody = document.getElementById("samba-sessions-tbody");
  if (!tbody) return;

  if (!sessions || sessions.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6" style="text-align:center; color:var(--subtext0); padding:20px;">Aucune session cliente active détectée.</td></tr>`;
    return;
  }

  tbody.innerHTML = sessions.map(s => `
    <tr>
      <td><strong>👤 ${escapeHtml(s.user)}</strong></td>
      <td>
        <code style="color:var(--sapphire); font-weight:600;">${escapeHtml(s.client_ip)}</code>
        ${s.machine ? `<span style="font-size:0.75rem; color:var(--subtext0); display:block;">(${escapeHtml(s.machine)})</span>` : ""}
      </td>
      <td>
        <span class="badge badge-info">${escapeHtml(s.protocol)}</span>
        ${s.encryption ? `<span class="badge" style="background:rgba(243,139,168,0.15); color:var(--red); font-size:0.7rem; margin-left:4px;">${escapeHtml(s.encryption)}</span>` : ""}
      </td>
      <td>
        <span class="badge badge-secondary">📁 ${escapeHtml(s.share || "-")}</span>
      </td>
      <td>
        <span style="font-size:0.8rem; color:var(--subtext1);">${escapeHtml(s.login_time || "-")}</span>
      </td>
      <td style="text-align:right;">
        ${s.pid ? `
          <button type="button" class="btn btn-danger btn-xs" onclick="disconnectSambaSession(${s.pid})" title="Déconnecter ce client (kill PID ${s.pid})">
            🔌 Déconnecter
          </button>
        ` : "-"}
      </td>
    </tr>
  `).join("");
}

function renderSambaLocksTable(locks) {
  const tbody = document.getElementById("samba-locks-tbody");
  if (!tbody) return;

  if (!locks || locks.length === 0) {
    tbody.innerHTML = `<tr><td colspan="5" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun fichier verrouillé actuellement.</td></tr>`;
    return;
  }

  tbody.innerHTML = locks.map(l => `
    <tr>
      <td>
        <div style="font-family:var(--font-mono); font-size:0.8rem; color:var(--text); max-width:280px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;" title="${escapeHtml(l.path)}">
          📄 ${escapeHtml(l.path)}
        </div>
      </td>
      <td>
        <span class="badge badge-secondary">📁 ${escapeHtml(l.share)}</span>
      </td>
      <td>
        <span class="badge badge-warning" style="font-size:0.72rem;">${escapeHtml(l.lock_type)}</span>
      </td>
      <td>
        <code style="color:var(--mauve); font-size:0.8rem;">${l.pid}</code>
      </td>
      <td>
        <span style="font-size:0.78rem; color:var(--subtext0);">${escapeHtml(l.time || "-")}</span>
      </td>
    </tr>
  `).join("");
}

// =========================================================================
// MODALE PARTAGE SAMBA (CRÉATION / MODIFICATION)
// =========================================================================

function openCreateSambaShareModal() {
  document.getElementById("modal-samba-share-title").textContent = "Nouveau Partage Réseau Samba";
  document.getElementById("samba-share-id").value = "";
  document.getElementById("samba-share-name").value = "";
  document.getElementById("samba-share-name").readOnly = false;
  document.getElementById("samba-share-comment").value = "";
  document.getElementById("samba-share-path").value = "/mnt/raid/shares/";

  document.getElementById("samba-opt-browseable").checked = true;
  document.getElementById("samba-opt-readonly").checked = false;
  document.getElementById("samba-opt-guest").checked = false;
  document.getElementById("samba-opt-recycle").checked = true;
  document.getElementById("samba-opt-timemachine").checked = false;
  document.getElementById("samba-opt-shadowcopy").checked = true;
  document.getElementById("samba-opt-encrypt").checked = false;

  renderSambaUsersCheckboxes([]);

  document.getElementById("modal-samba-share").style.display = "flex";
}

function openEditSambaShareModal(shareId) {
  if (!currentSambaData || !currentSambaData.shares) return;
  const share = currentSambaData.shares.find(s => s.id === shareId);
  if (!share) {
    showToast("Partage introuvable", "error");
    return;
  }

  document.getElementById("modal-samba-share-title").textContent = `Modifier le Partage : ${share.name}`;
  document.getElementById("samba-share-id").value = share.id;
  document.getElementById("samba-share-name").value = share.name;
  document.getElementById("samba-share-name").readOnly = true;
  document.getElementById("samba-share-comment").value = share.comment || "";
  document.getElementById("samba-share-path").value = share.path || "";

  document.getElementById("samba-opt-browseable").checked = share.browseable !== false;
  document.getElementById("samba-opt-readonly").checked = !!share.read_only;
  document.getElementById("samba-opt-guest").checked = !!share.guest_ok;
  document.getElementById("samba-opt-recycle").checked = !!share.recycle_bin;
  document.getElementById("samba-opt-timemachine").checked = !!share.time_machine;
  document.getElementById("samba-opt-shadowcopy").checked = !!share.shadow_copy;
  document.getElementById("samba-opt-encrypt").checked = !!share.smb_encrypt;

  const selectedUsers = share.write_list && share.write_list.length > 0 ? share.write_list : (share.valid_users || []);
  renderSambaUsersCheckboxes(selectedUsers);

  document.getElementById("modal-samba-share").style.display = "flex";
}

function closeSambaShareModal() {
  document.getElementById("modal-samba-share").style.display = "none";
}

function setSambaSharePathPreset(presetPath) {
  const input = document.getElementById("samba-share-path");
  const nameInput = document.getElementById("samba-share-name");
  const name = nameInput.value.trim();
  if (name) {
    input.value = `${presetPath}${name}`;
  } else {
    input.value = presetPath;
  }
}

function onSambaTimeMachineToggle() {
  const isTm = document.getElementById("samba-opt-timemachine").checked;
  if (isTm) {
    const commentInput = document.getElementById("samba-share-comment");
    if (commentInput && !commentInput.value) {
      commentInput.value = "Sauvegardes Apple Time Machine macOS";
    }
  }
}

function renderSambaUsersCheckboxes(selectedUsers = []) {
  const container = document.getElementById("samba-share-users-checkboxes");
  if (!container) return;

  const currentU = getCurrentDashboardUsername();
  const users = currentSambaData?.available_users || [currentU];
  const smbUsers = currentSambaData?.samba_users || [];
  if (users.length === 0) {
    container.innerHTML = `<div style="color:var(--subtext0); font-size:0.8rem;">Aucun utilisateur spécifique détecté sur le système.</div>`;
    return;
  }

  container.innerHTML = users.map(user => {
    const isChecked = selectedUsers.includes(user) || (selectedUsers.length === 0 && user === currentU);
    const hasSamba = smbUsers.includes(user);
    const statusBadge = hasSamba
      ? `<span style="font-size: 0.7rem; padding: 2px 6px; border-radius: 4px; background: rgba(166, 227, 161, 0.15); color: #a6e3a1; border: 1px solid rgba(166, 227, 161, 0.3); margin-left: 6px;">Samba OK</span>`
      : `<span style="font-size: 0.7rem; padding: 2px 6px; border-radius: 4px; background: rgba(250, 179, 135, 0.15); color: #fab387; border: 1px solid rgba(250, 179, 135, 0.3); margin-left: 6px;" title="Cet utilisateur n'a pas encore configuré de mot de passe Samba réseau">⚠️ Mot de passe SMB requis</span>`;
    return `
      <label class="samba-user-checkbox-item">
        <input type="checkbox" name="samba-user-perm" value="${escapeHtml(user)}" ${isChecked ? "checked" : ""}>
        <div style="display:inline-flex; align-items:center; gap:4px;">
          <span><strong>${escapeHtml(user)}</strong></span>
          ${statusBadge}
        </div>
      </label>
    `;
  }).join("");
}

async function submitSambaShareForm() {
  const id = document.getElementById("samba-share-id").value.trim();
  const name = document.getElementById("samba-share-name").value.trim();
  const path = document.getElementById("samba-share-path").value.trim();
  const comment = document.getElementById("samba-share-comment").value.trim();

  if (!name) {
    showToast("Veuillez saisir un nom de partage valide.", "warning");
    return;
  }
  if (!path || !path.startsWith("/")) {
    showToast("Veuillez saisir un chemin absolu valide (ex: /mnt/raid/shares/mon_dossier).", "warning");
    return;
  }

  // Utilisateurs sélectionnés
  const checkedUsers = [];
  document.querySelectorAll('input[name="samba-user-perm"]:checked').forEach(cb => {
    checkedUsers.push(cb.value);
  });

  const payload = {
    id: id || name,
    name: name,
    path: path,
    comment: comment,
    read_only: document.getElementById("samba-opt-readonly").checked,
    browseable: document.getElementById("samba-opt-browseable").checked,
    guest_ok: document.getElementById("samba-opt-guest").checked,
    recycle_bin: document.getElementById("samba-opt-recycle").checked,
    time_machine: document.getElementById("samba-opt-timemachine").checked,
    shadow_copy: document.getElementById("samba-opt-shadowcopy").checked,
    smb_encrypt: document.getElementById("samba-opt-encrypt").checked,
    valid_users: checkedUsers,
    write_list: document.getElementById("samba-opt-readonly").checked ? [] : checkedUsers,
    read_list: document.getElementById("samba-opt-readonly").checked ? checkedUsers : [],
    create_mask: "0664",
    directory_mask: "0775"
  };

  const btn = document.getElementById("btn-submit-samba-share");
  const oldText = btn.innerHTML;
  btn.disabled = true;
  btn.innerHTML = `<span>⏳</span> Application...`;

  try {
    const res = await fetch("/api/samba/shares", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de l'enregistrement");

    showToast(id ? "Partage Samba mis à jour avec succès !" : "Nouveau partage Samba créé !", "success");
    closeSambaShareModal();
    await loadSambaData();
  } catch (err) {
    console.error("Erreur enregistrement partage Samba:", err);
    showToast("Erreur : " + err.message, "error");
  } finally {
    btn.disabled = false;
    btn.innerHTML = oldText;
  }
}

async function deleteSambaShareConfirm(shareId, shareName) {
  if (!confirm(`Êtes-vous sûr de vouloir supprimer le partage « ${shareName} » ?

🛡️ NOTE IMPORTANTE : Vos fichiers et données sur le disque NE seront PAS supprimés. Seule la règle de partage réseau sera retirée.`)) {
    return;
  }

  try {
    const res = await fetch(`/api/samba/shares/${encodeURIComponent(shareId)}`, {
      method: "DELETE"
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de suppression");

    showToast(`Partage « ${shareName} » supprimé avec succès`, "success");
    await loadSambaData();
  } catch (err) {
    console.error("Erreur suppression partage Samba:", err);
    showToast("Erreur lors de la suppression : " + err.message, "error");
  }
}

// =========================================================================
// MODALE CONFIGURATION GLOBALE SAMBA
// =========================================================================

function openSambaGlobalModal() {
  if (currentSambaData && currentSambaData.global_config) {
    const cfg = currentSambaData.global_config;
    document.getElementById("samba-global-server-string").value = cfg.server_string || "Noos NAS";
    document.getElementById("samba-global-workgroup").value = cfg.workgroup || "WORKGROUP";
    document.getElementById("samba-global-min-protocol").value = cfg.min_protocol || "SMB2_10";
    document.getElementById("samba-global-encrypt").value = cfg.smb_encrypt || "auto";
    document.getElementById("samba-global-wsdd").checked = cfg.wsdd_enabled !== false;
    document.getElementById("samba-global-multichannel").checked = cfg.multi_channel !== false;
    document.getElementById("samba-global-apple").checked = cfg.apple_extensions !== false;
  }
  document.getElementById("modal-samba-global").style.display = "flex";
}

function closeSambaGlobalModal() {
  document.getElementById("modal-samba-global").style.display = "none";
}

async function submitSambaGlobalForm() {
  const payload = {
    server_string: document.getElementById("samba-global-server-string").value.trim() || "Noos NAS",
    workgroup: document.getElementById("samba-global-workgroup").value.trim().toUpperCase() || "WORKGROUP",
    min_protocol: document.getElementById("samba-global-min-protocol").value,
    smb_encrypt: document.getElementById("samba-global-encrypt").value,
    wsdd_enabled: document.getElementById("samba-global-wsdd").checked,
    multi_channel: document.getElementById("samba-global-multichannel").checked,
    apple_extensions: document.getElementById("samba-global-apple").checked
  };

  try {
    const res = await fetch("/api/samba/global", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de l'enregistrement");

    showToast("Paramètres globaux Samba mis à jour et appliqués !", "success");
    closeSambaGlobalModal();
    await loadSambaData();
  } catch (err) {
    console.error("Erreur enregistrement global Samba:", err);
    showToast("Erreur : " + err.message, "error");
  }
}

// =========================================================================
// MODALE DIAGNOSTIC TESTPARM
// =========================================================================

function openSambaDiagModal() {
  document.getElementById("modal-samba-diag").style.display = "flex";
  runSambaDiag();
}

function closeSambaDiagModal() {
  document.getElementById("modal-samba-diag").style.display = "none";
}

async function runSambaDiag() {
  const pre = document.getElementById("samba-diag-output");
  const banner = document.getElementById("samba-diag-status-banner");
  if (pre) pre.textContent = "Exécution de testparm -s en cours...";
  if (banner) banner.innerHTML = "";

  try {
    const res = await fetch("/api/samba/diagnostics");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec du diagnostic");

    const diag = json.data;
    if (pre) {
      pre.textContent = diag.output || "Aucune sortie.";
    }

    if (banner) {
      if (diag.valid) {
        banner.innerHTML = `
          <div style="background:rgba(166,227,161,0.12); border:1px solid rgba(166,227,161,0.3); border-radius:var(--radius-sm); padding:10px 14px; display:flex; align-items:center; gap:10px; color:var(--green); font-size:0.86rem; font-weight:600;">
            <span>✅</span> Syntaxe et structure de la configuration Samba parfaitement valides.
          </div>
        `;
      } else {
        banner.innerHTML = `
          <div style="background:rgba(243,139,168,0.12); border:1px solid rgba(243,139,168,0.3); border-radius:var(--radius-sm); padding:10px 14px; display:flex; align-items:center; gap:10px; color:var(--red); font-size:0.86rem; font-weight:600;">
            <span>⚠️</span> Des avertissements ou erreurs ont été détectés lors de la vérification syntaxique.
          </div>
        `;
      }
    }
  } catch (err) {
    if (pre) pre.textContent = "Erreur : " + err.message;
    if (banner) {
      banner.innerHTML = `
        <div style="background:rgba(243,139,168,0.12); border:1px solid rgba(243,139,168,0.3); border-radius:var(--radius-sm); padding:10px 14px; color:var(--red); font-size:0.86rem;">
          ❌ Impossible d'exécuter testparm : ${escapeHtml(err.message)}
        </div>
      `;
    }
  }
}

// =========================================================================
// MODALE GUIDE DE CONNEXION INTERACTIF
// =========================================================================

function openSambaGuideModal() {
  const ip = currentSambaData?.primary_lan_ip || currentSambaData?.primary_ip || window.location.hostname;
  const host = currentSambaData?.hostname || "noos-nas";

  const winCode = document.getElementById("guide-code-win");
  const macCode = document.getElementById("guide-code-mac");
  const lnxGuiCode = document.getElementById("guide-code-lnx-gui");
  const lnxFstabCode = document.getElementById("guide-code-lnx-fstab");

  if (winCode) winCode.textContent = `\\${ip}`;
  if (macCode) macCode.textContent = `smb://${ip}`;
  if (lnxGuiCode) lnxGuiCode.textContent = `smb://${ip}/`;
  if (lnxFstabCode) lnxFstabCode.textContent = `//${ip}/partage /mnt/nas_partage cifs username=VOTRE_USER,password=VOTRE_MDP,uid=1000,gid=100,iocharset=utf8 0 0`;

  switchSambaGuideTab("win");
  document.getElementById("modal-samba-guide").style.display = "flex";
}

function closeSambaGuideModal() {
  document.getElementById("modal-samba-guide").style.display = "none";
}

function switchSambaGuideTab(os) {
  document.querySelectorAll(".samba-guide-tab").forEach(tab => {
    tab.classList.toggle("active", tab.getAttribute("data-guide-os") === os);
  });
  document.querySelectorAll(".samba-guide-pane").forEach(pane => {
    pane.classList.toggle("active", pane.id === `samba-guide-pane-${os}`);
  });
}

// =========================================================================
// ACTIONS RAPIDES SAMBA (RELOAD, DISCONNECT)
// =========================================================================

async function reloadSambaAction() {
  const btn = document.getElementById("btn-reload-samba");
  if (btn) btn.disabled = true;

  try {
    const res = await fetch("/api/samba/reload", { method: "POST" });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec du rechargement");

    showToast("Samba rechargé avec succès sans déconnecter les clients !", "success");
    await loadSambaData();
  } catch (err) {
    console.error("Erreur rechargement Samba:", err);
    showToast("Erreur : " + err.message, "error");
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function disconnectSambaSession(pid) {
  if (!confirm(`Voulez-vous vraiment déconnecter la session SMB associée au processus PID ${pid} ?`)) {
    return;
  }

  try {
    const res = await fetch("/api/samba/sessions/disconnect", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ pid: pid })
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de déconnexion");

    showToast(`Session PID ${pid} déconnectée.`, "success");
    await loadSambaData();
  } catch (err) {
    console.error("Erreur déconnexion session:", err);
    showToast("Erreur : " + err.message, "error");
  }
}


// =========================================================================
// 🚀 GESTIONNAIRE COMPLET sFTP (SSH / CHROOT) — STEvE_OS CATPPUCCIN MOCHA
// =========================================================================

let currentSftpData = null;

async function loadSftpData(showFeedback = false) {
  try {
    const res = await fetch("/api/sftp");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const json = await res.json();
    if (!json.success || !json.data) throw new Error(json.error || "Données indisponibles");

    const data = json.data;
    currentSftpData = data;

    // 1. En-tête et Badges d'état
    const statusBadge = document.getElementById("sftp-status-badge");
    const portBadge = document.getElementById("sftp-port-badge");
    const fail2banBadge = document.getElementById("sftp-fail2ban-badge");
    const pulseDot = document.getElementById("sftp-hero-pulse-dot");

    if (statusBadge) {
      statusBadge.textContent = data.is_active ? "● OpenSSH En Ligne" : "● Inactif";
      statusBadge.className = `sftp-status-tag ${data.is_active ? "active" : "inactive"}`;
    }
    if (portBadge) {
      portBadge.textContent = `Port ${data.port} TCP`;
    }
    if (fail2banBadge) {
      if (data.fail2ban_active) {
        fail2banBadge.textContent = data.fail2ban_banned_count > 0 
          ? `🛡️ Fail2ban (${data.fail2ban_banned_count} IP bannie${data.fail2ban_banned_count > 1 ? "s" : ""})`
          : "🛡️ Fail2ban Actif";
        fail2banBadge.style.color = "var(--green)";
      } else {
        fail2banBadge.textContent = "🛡️ Fail2ban Inactif";
        fail2banBadge.style.color = "var(--subtext0)";
      }
    }
    if (pulseDot) {
      pulseDot.style.display = data.is_active ? "block" : "none";
    }

    // Badge sous-onglet dans la navigation
    const navBadge = document.getElementById("badge-subtab-sftp");
    if (navBadge) {
      navBadge.textContent = data.is_active ? `Port ${data.port}` : "Inactif";
      navBadge.className = `subtab-pill-badge ${data.is_active ? "badge-success" : "badge-secondary"}`;
    }

    // 2. Adresses de connexion rapide
    const uriClient = document.getElementById("sftp-uri-val-client");
    const uriCli = document.getElementById("sftp-uri-val-cli");
    const uriSshfs = document.getElementById("sftp-uri-val-sshfs");

    const ip = data.primary_lan_ip || window.location.hostname;
    // Remplacer systématiquement root par l'utilisateur actuellement connecté au dashboard
    const connectedUser = getCurrentDashboardUsername();
    const primaryUser = (connectedUser && connectedUser !== "root")
      ? connectedUser
      : ((data.allowed_users && data.allowed_users.find(u => u.username !== "root")?.username) || "noos");

    if (uriClient) uriClient.textContent = `sftp://${primaryUser}@${ip}:${data.port}`;
    if (uriCli) uriCli.textContent = `sftp -P ${data.port} ${primaryUser}@${ip}`;
    if (uriSshfs) uriSshfs.textContent = `sshfs -p ${data.port} ${primaryUser}@${ip}:/ /mnt/nas`;

    // 3. Mise à jour des KPIs
    const kpiPortVal = document.getElementById("sftp-kpi-port-val");
    const kpiPortSub = document.getElementById("sftp-kpi-port-sub");
    const kpiSharesCount = document.getElementById("sftp-kpi-shares-count");
    const kpiSharesSub = document.getElementById("sftp-kpi-shares-sub");
    const kpiUsersCount = document.getElementById("sftp-kpi-users-count");
    const kpiUsersSub = document.getElementById("sftp-kpi-users-sub");
    const kpiSecVal = document.getElementById("sftp-kpi-security-val");
    const kpiSecSub = document.getElementById("sftp-kpi-security-sub");

    const shares = data.shares || [];
    const users = data.allowed_users || [];
    const sessions = data.active_sessions || [];

    const chrootCount = shares.filter(s => s.chroot_enforced).length;
    const rwCount = shares.filter(s => !s.read_only).length;
    const keyUsersCount = users.filter(u => u.has_ssh_keys).length;

    if (kpiPortVal) kpiPortVal.textContent = `Port ${data.port}`;
    if (kpiPortSub) kpiPortSub.textContent = data.is_active ? "OpenSSH 10.5 Chiffré" : "Serveur arrêté";

    if (kpiSharesCount) kpiSharesCount.textContent = shares.length;
    if (kpiSharesSub) kpiSharesSub.textContent = `${rwCount} en écriture, ${shares.length - rwCount} lecture seule`;

    if (kpiUsersCount) kpiUsersCount.textContent = users.length;
    if (kpiUsersSub) kpiUsersSub.textContent = `${keyUsersCount} avec clés SSH, ${users.length - keyUsersCount} mdp`;

    if (kpiSecVal) kpiSecVal.textContent = chrootCount > 0 ? "Prison Chroot" : "Accès Standard";
    if (kpiSecSub) kpiSecSub.textContent = data.fail2ban_active 
      ? `Fail2ban Actif (${data.fail2ban_banned_count} IP bannie${data.fail2ban_banned_count > 1 ? "s" : ""})`
      : "Fail2ban Inactif";

    // 4. Rendu de la liste des partages sFTP
    renderSftpSharesList(shares);

    // 5. Rendu du tableau des utilisateurs autorisés SSH
    renderSftpUsersTable(users);

    // 6. Rendu des sessions actives
    renderSftpSessionsTable(sessions);

    // 7. Rendu des logs de sécurité
    renderSftpLogsTable(data.recent_logs || []);

    if (showFeedback) {
      showToast("Données sFTP actualisées avec succès !", "success");
    }
  } catch (err) {
    console.error("Erreur chargement sFTP:", err);
    if (showFeedback) {
      showToast("Erreur lors du chargement de sFTP : " + err.message, "error");
    }
  }
}

function renderSftpSharesList(shares) {
  const container = document.getElementById("sftp-shares-list");
  if (!container) return;

  if (!shares || shares.length === 0) {
    container.innerHTML = `
      <div style="padding:40px; text-align:center; background:var(--mantle); border-radius:var(--radius-md); border:1px dashed rgba(255,255,255,0.1);">
        <div style="font-size:2.5rem; margin-bottom:12px;">📁</div>
        <div style="font-size:1.1rem; font-weight:700; color:var(--text); margin-bottom:6px;">Aucun point d'accès sFTP configuré</div>
        <div style="font-size:0.85rem; color:var(--subtext0); max-width:440px; margin:0 auto 16px auto;">
          Créez un partage sFTP pour permettre à vos utilisateurs SSH de transférer des fichiers en toute sécurité avec isolation Chroot.
        </div>
        <button type="button" class="btn btn-primary" onclick="openCreateSftpShareModal()">
          <span>➕</span> Créer un Partage sFTP
        </button>
      </div>
    `;
    return;
  }

  const primaryIp = currentSftpData?.primary_lan_ip || window.location.hostname;
  const port = currentSftpData?.port || 22;

  container.innerHTML = shares.map(share => {
    let cardClass = "";
    if (share.chroot_enforced) {
      cardClass = "chroot";
    } else if (share.read_only) {
      cardClass = "readonly";
    }

    const badges = [];
    if (share.chroot_enforced) {
      badges.push(`<span class="badge" style="background:rgba(148,226,213,0.15); color:var(--teal); border:1px solid rgba(148,226,213,0.3);" title="Confinement strict : l'utilisateur ne peut pas remonter au-dessus de ce dossier">🏰 Prison Chroot</span>`);
    } else {
      badges.push(`<span class="badge badge-secondary" title="Accès système standard">🌐 Accès Système</span>`);
    }

    if (share.read_only) {
      badges.push(`<span class="badge badge-warning" title="Upload interdit">🔒 Lecture Seule (-R)</span>`);
    } else {
      badges.push(`<span class="badge badge-success" title="Envoi et modification de fichiers autorisés">✏️ Lecture / Écriture</span>`);
    }

    let usersDisplay = `<span style="color:var(--subtext0); font-style:italic;">Aucun utilisateur restreint</span>`;
    if (share.allowed_users && share.allowed_users.length > 0) {
      usersDisplay = share.allowed_users.map(u => {
        const canWrite = !share.read_only && share.write_users && share.write_users.includes(u);
        return `<span class="badge ${canWrite ? "badge-primary" : "badge-secondary"}" style="font-size:0.75rem;">👤 ${escapeHtml(u)} ${canWrite ? "(RW)" : "(RO)"}</span>`;
      }).join(" ");
    }

    const connectedUser = getCurrentDashboardUsername();
    const shareUser = (share.allowed_users && share.allowed_users.includes(connectedUser))
      ? connectedUser
      : ((share.allowed_users && share.allowed_users.find(u => u !== "root")) || share.allowed_users?.[0] || connectedUser);
    const sftpUri = `sftp://${shareUser}@${primaryIp}:${port}${share.path}`;

    return `
      <div class="sftp-share-card ${cardClass}" id="sftp-card-${escapeHtml(share.id)}">
        <div class="sftp-share-header">
          <div class="sftp-share-title-group">
            <div class="sftp-share-badge-icon">📁</div>
            <div>
              <div class="sftp-share-name">${escapeHtml(share.name)}</div>
              <div class="sftp-share-comment">${escapeHtml(share.comment || "Point d'accès sFTP sécurisé")}</div>
            </div>
          </div>

          <div class="sftp-share-badges">
            ${badges.join(" ")}
          </div>

          <div class="sftp-share-actions">
            <button type="button" class="btn btn-secondary btn-xs" onclick="copyTextToClipboard('${sftpUri}', 'URI sFTP copiée !')" title="Copier l'URI complète : ${sftpUri}">
              📋 Copier URI
            </button>
            <button type="button" class="btn btn-secondary btn-xs" onclick="openEditSftpShareModal('${escapeHtml(share.id)}')" title="Modifier la configuration du partage">
              ✏️ Modifier
            </button>
            <button type="button" class="btn btn-danger btn-xs" onclick="deleteSftpShareConfirm('${escapeHtml(share.id)}', '${escapeHtml(share.name)}')" title="Supprimer ce partage">
              🗑️
            </button>
          </div>
        </div>

        <div class="sftp-share-meta-row">
          <div>
            <div class="sftp-share-field-label">Chemin Unix Réel :</div>
            <div class="sftp-share-field-val">
              <code class="sftp-share-path-code">${escapeHtml(share.path)}</code>
              <button type="button" class="btn-copy-uri" onclick="copyTextToClipboard('${escapeHtml(share.path)}', 'Chemin copié !')" title="Copier le chemin">📋</button>
            </div>
          </div>

          <div>
            <div class="sftp-share-field-label">Utilisateurs SSH Autorisés :</div>
            <div class="sftp-share-field-val">
              ${usersDisplay}
            </div>
          </div>

          <div>
            <div class="sftp-share-field-label">Sous-système :</div>
            <div class="sftp-share-field-val">
              <span style="font-family:var(--font-mono); font-size:0.75rem; color:var(--subtext1);">internal-sftp (OpenSSH 10.5)</span>
            </div>
          </div>
        </div>
      </div>
    `;
  }).join("");
}

function renderSftpUsersTable(users) {
  const tbody = document.getElementById("sftp-users-tbody");
  if (!tbody) return;

  if (!users || users.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun utilisateur avec accès SSH détecté.</td></tr>`;
    return;
  }

  tbody.innerHTML = users.map(u => {
    const keyBadge = u.has_ssh_keys 
      ? `<span class="badge badge-success">🔑 ${u.ssh_keys_count} clé${u.ssh_keys_count > 1 ? "s" : ""} SSH</span>`
      : `<span class="badge badge-warning">⚠️ Mot de passe seul</span>`;

    return `
      <tr>
        <td>
          <div style="display:flex; align-items:center; gap:8px;">
            <div style="width:28px; height:28px; border-radius:50%; background:rgba(203,166,247,0.2); color:var(--mauve); display:flex; align-items:center; justify-content:center; font-weight:700; font-size:0.75rem;">
              ${escapeHtml(u.username.substring(0, 2).toUpperCase())}
            </div>
            <strong>${escapeHtml(u.username)}</strong>
          </div>
        </td>
        <td>
          <div>${escapeHtml(u.full_name)}</div>
          <div style="font-size:0.75rem; color:var(--subtext0);">${escapeHtml(u.role)}</div>
        </td>
        <td>
          <code style="color:var(--teal); font-size:0.8rem;">${escapeHtml(u.chroot_dir || u.home_dir)}</code>
        </td>
        <td>
          ${keyBadge}
        </td>
        <td>
          <code style="color:var(--subtext1); font-size:0.75rem;">${escapeHtml(u.shell)}</code>
        </td>
        <td style="text-align:right;">
          <span class="badge badge-success">● Actif</span>
        </td>
      </tr>
    `;
  }).join("");
}

function renderSftpSessionsTable(sessions) {
  const tbody = document.getElementById("sftp-sessions-tbody");
  if (!tbody) return;

  if (!sessions || sessions.length === 0) {
    tbody.innerHTML = `<tr><td colspan="5" style="text-align:center; color:var(--subtext0); padding:20px;">Aucune session sFTP ou SSH active en direct.</td></tr>`;
    return;
  }

  tbody.innerHTML = sessions.map(s => `
    <tr>
      <td><strong>👤 ${escapeHtml(s.user)}</strong></td>
      <td>
        <code style="color:var(--sapphire); font-weight:600;">${escapeHtml(s.client_ip)}:${s.client_port}</code>
      </td>
      <td>
        <span class="badge ${s.session_type.includes("sFTP") ? "badge-primary" : "badge-info"}">${escapeHtml(s.session_type)}</span>
      </td>
      <td>
        <span class="badge badge-success">● Connecté</span>
      </td>
      <td style="text-align:right;">
        ${s.pid ? `
          <button type="button" class="btn btn-danger btn-xs" onclick="disconnectSftpSession(${s.pid})" title="Déconnecter cette session (kill PID ${s.pid})">
            🔌 Déconnecter
          </button>
        ` : "-"}
      </td>
    </tr>
  `).join("");
}

function renderSftpLogsTable(logs) {
  const tbody = document.getElementById("sftp-logs-tbody");
  if (!tbody) return;

  if (!logs || logs.length === 0) {
    tbody.innerHTML = `<tr><td colspan="4" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun événement d'audit récent.</td></tr>`;
    return;
  }

  tbody.innerHTML = logs.map(l => {
    let badgeClass = "badge-secondary";
    let eventLabel = l.event_type;

    if (l.event_type === "accepted") {
      badgeClass = "badge-success";
      eventLabel = "✅ Connexion Réussie";
    } else if (l.event_type === "failed") {
      badgeClass = "badge-danger";
      eventLabel = "❌ Échec Auth";
    } else if (l.event_type === "closed") {
      badgeClass = "badge-info";
      eventLabel = "🔌 Déconnexion";
    } else if (l.event_type === "banned") {
      badgeClass = "badge-purple";
      eventLabel = "🛡️ IP Bannie (Fail2ban)";
    }

    return `
      <tr>
        <td><span style="font-size:0.78rem; color:var(--subtext1); font-family:var(--font-mono);">${escapeHtml(l.timestamp)}</span></td>
        <td><span class="badge ${badgeClass}" style="font-size:0.75rem;">${eventLabel}</span></td>
        <td><strong>${escapeHtml(l.user)}</strong></td>
        <td><code>${escapeHtml(l.ip)}</code></td>
      </tr>
    `;
  }).join("");
}

// =========================================================================
// MODALE PARTAGE sFTP (CRÉATION / MODIFICATION)
// =========================================================================

function openCreateSftpShareModal() {
  document.getElementById("modal-sftp-share-title").textContent = "Nouveau Partage Réseau sFTP";
  document.getElementById("sftp-share-id").value = "";
  document.getElementById("sftp-share-name").value = "";
  document.getElementById("sftp-share-name").readOnly = false;
  document.getElementById("sftp-share-comment").value = "";
  document.getElementById("sftp-share-path").value = "/storage/data";
  document.getElementById("sftp-opt-chroot").checked = true;
  document.getElementById("sftp-opt-readonly").checked = false;

  renderSftpUsersCheckboxes([]);
  document.getElementById("modal-sftp-share").style.display = "flex";
}

function openEditSftpShareModal(shareId) {
  if (!currentSftpData || !currentSftpData.shares) return;
  const share = currentSftpData.shares.find(s => s.id === shareId);
  if (!share) {
    showToast("Partage introuvable", "error");
    return;
  }

  document.getElementById("modal-sftp-share-title").textContent = `Modifier le Partage sFTP : ${share.name}`;
  document.getElementById("sftp-share-id").value = share.id;
  document.getElementById("sftp-share-name").value = share.name;
  document.getElementById("sftp-share-name").readOnly = true;
  document.getElementById("sftp-share-comment").value = share.comment || "";
  document.getElementById("sftp-share-path").value = share.path || "";
  document.getElementById("sftp-opt-chroot").checked = share.chroot_enforced !== false;
  document.getElementById("sftp-opt-readonly").checked = !!share.read_only;

  renderSftpUsersCheckboxes(share.allowed_users || []);
  document.getElementById("modal-sftp-share").style.display = "flex";
}

function closeSftpShareModal() {
  document.getElementById("modal-sftp-share").style.display = "none";
}

function setSftpSharePathPreset(presetPath) {
  document.getElementById("sftp-share-path").value = presetPath;
}

function renderSftpUsersCheckboxes(selectedUsers = []) {
  const container = document.getElementById("sftp-share-users-checkboxes");
  if (!container) return;

  const users = currentSftpData?.allowed_users || [];
  if (users.length === 0) {
    container.innerHTML = `<div style="color:var(--subtext0); font-size:0.8rem;">Aucun utilisateur SSH détecté.</div>`;
    return;
  }

  container.innerHTML = users.map(u => {
    const isChecked = selectedUsers.includes(u.username) || (selectedUsers.length === 0 && u.is_admin);
    return `
      <label class="sftp-user-checkbox-item">
        <input type="checkbox" name="sftp-user-perm" value="${escapeHtml(u.username)}" ${isChecked ? "checked" : ""}>
        <span><strong>${escapeHtml(u.username)}</strong> <small style="color:var(--subtext0);">(${escapeHtml(u.role)})</small></span>
      </label>
    `;
  }).join("");
}

async function submitSftpShareForm() {
  const id = document.getElementById("sftp-share-id").value.trim();
  const name = document.getElementById("sftp-share-name").value.trim();
  const path = document.getElementById("sftp-share-path").value.trim();
  const comment = document.getElementById("sftp-share-comment").value.trim();

  if (!name) {
    showToast("Veuillez saisir un nom de partage valide.", "warning");
    return;
  }
  if (!path || !path.startsWith("/")) {
    showToast("Veuillez saisir un chemin absolu valide (ex: /storage/data).", "warning");
    return;
  }

  const checkedUsers = [];
  document.querySelectorAll('input[name="sftp-user-perm"]:checked').forEach(cb => {
    checkedUsers.push(cb.value);
  });

  const payload = {
    id: id || name.toLowerCase().replace(/\s+/g, "-"),
    name: name,
    path: path,
    comment: comment,
    read_only: document.getElementById("sftp-opt-readonly").checked,
    chroot_enforced: document.getElementById("sftp-opt-chroot").checked,
    allowed_users: checkedUsers,
    write_users: document.getElementById("sftp-opt-readonly").checked ? [] : checkedUsers,
    quota_gb: null,
  };

  const btn = document.getElementById("btn-submit-sftp-share");
  const oldText = btn.innerHTML;
  btn.disabled = true;
  btn.innerHTML = `<span>⏳</span> Application...`;

  try {
    const url = id ? `/api/sftp/shares/${encodeURIComponent(id)}` : "/api/sftp/shares";
    const method = id ? "PUT" : "POST";

    const res = await fetch(url, {
      method: method,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de l'enregistrement");

    showToast(id ? "Partage sFTP mis à jour avec succès !" : "Nouveau partage sFTP créé !", "success");
    closeSftpShareModal();
    await loadSftpData();
  } catch (err) {
    console.error("Erreur enregistrement sFTP:", err);
    showToast("Erreur : " + err.message, "error");
  } finally {
    btn.disabled = false;
    btn.innerHTML = oldText;
  }
}

async function deleteSftpShareConfirm(shareId, shareName) {
  if (!confirm(`Êtes-vous sûr de vouloir supprimer le partage sFTP « ${shareName} » ?\n\n🛡️ NOTE IMPORTANTE : Vos fichiers physiques sur le disque NE seront PAS supprimés. Seule la règle d'accès réseau sFTP est retirée.`)) {
    return;
  }

  try {
    const res = await fetch(`/api/sftp/shares/${encodeURIComponent(shareId)}`, {
      method: "DELETE"
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de suppression");

    showToast(`Partage sFTP « ${shareName} » supprimé avec succès.`, "success");
    await loadSftpData();
  } catch (err) {
    console.error("Erreur suppression sFTP:", err);
    showToast("Erreur lors de la suppression : " + err.message, "error");
  }
}

// =========================================================================
// MODALE CONFIGURATION GLOBALE sFTP
// =========================================================================

function openSftpGlobalModal() {
  if (currentSftpData && currentSftpData.global_config) {
    const cfg = currentSftpData.global_config;
    document.getElementById("sftp-global-port").value = cfg.port || 22;
    document.getElementById("sftp-global-root").value = cfg.permit_root_login || "no";
    document.getElementById("sftp-global-chroot").value = cfg.default_chroot_dir || "/mnt/storage/sftp";
    document.getElementById("sftp-global-max-tries").value = cfg.max_auth_tries || 5;
    document.getElementById("sftp-global-password-auth").checked = cfg.password_authentication !== false;
  }
  document.getElementById("modal-sftp-global").style.display = "flex";
}

function closeSftpGlobalModal() {
  document.getElementById("modal-sftp-global").style.display = "none";
}

async function submitSftpGlobalForm() {
  const port = parseInt(document.getElementById("sftp-global-port").value, 10) || 22;
  const payload = {
    port: port,
    permit_root_login: document.getElementById("sftp-global-root").value,
    password_authentication: document.getElementById("sftp-global-password-auth").checked,
    pubkey_authentication: true,
    default_chroot_dir: document.getElementById("sftp-global-chroot").value.trim() || "/mnt/storage/sftp",
    max_auth_tries: parseInt(document.getElementById("sftp-global-max-tries").value, 10) || 5,
    idle_timeout_min: 15,
  };

  try {
    const res = await fetch("/api/sftp/global", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de l'enregistrement");

    showToast("Paramètres globaux sFTP enregistrés et appliqués !", "success");
    closeSftpGlobalModal();
    await loadSftpData();
  } catch (err) {
    console.error("Erreur enregistrement global sFTP:", err);
    showToast("Erreur : " + err.message, "error");
  }
}

// =========================================================================
// MODALE GUIDE DE CONNEXION INTERACTIF sFTP
// =========================================================================

function openSftpGuideModal() {
  const ip = currentSftpData?.primary_lan_ip || window.location.hostname;
  const port = currentSftpData?.port || 22;
  const connectedUser = getCurrentDashboardUsername();
  const user = (connectedUser && connectedUser !== "root")
    ? connectedUser
    : ((currentSftpData?.allowed_users && currentSftpData.allowed_users.find(u => u.username !== "root")?.username) || "noos");

  const hostEl = document.getElementById("sftp-guide-val-host");
  const portEl = document.getElementById("sftp-guide-val-port");
  const cliEl = document.getElementById("sftp-guide-cmd-cli");
  const sshfsEl = document.getElementById("sftp-guide-cmd-sshfs");
  const rsyncEl = document.getElementById("sftp-guide-cmd-rsync");

  if (hostEl) hostEl.textContent = ip;
  if (portEl) portEl.textContent = port;
  if (cliEl) cliEl.textContent = `sftp -P ${port} ${user}@${ip}`;
  if (sshfsEl) sshfsEl.textContent = `sshfs -p ${port} ${user}@${ip}:/ /mnt/nas_sftp`;
  if (rsyncEl) rsyncEl.textContent = `rsync -avz --progress -e "ssh -p ${port}" /mon/dossier/local/ ${user}@${ip}:/storage/data/`;

  switchSftpGuideTab("filezilla");
  document.getElementById("modal-sftp-guide").style.display = "flex";
}

function closeSftpGuideModal() {
  document.getElementById("modal-sftp-guide").style.display = "none";
}

function switchSftpGuideTab(tabKey) {
  document.querySelectorAll(".sftp-guide-tab").forEach(tab => {
    tab.classList.toggle("active", tab.getAttribute("data-sftp-guide") === tabKey);
  });
  document.querySelectorAll(".sftp-guide-pane").forEach(pane => {
    pane.classList.toggle("active", pane.id === `sftp-guide-pane-${tabKey}`);
  });
}

// =========================================================================
// ACTIONS RAPIDES sFTP (RELOAD, DISCONNECT)
// =========================================================================

async function reloadSftpAction() {
  const btn = document.getElementById("btn-reload-sftp");
  if (btn) btn.disabled = true;

  try {
    const res = await fetch("/api/sftp/reload", { method: "POST" });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec du rechargement");

    showToast("Service SSH / sFTP rechargé avec succès sans déconnecter les sessions !", "success");
    await loadSftpData();
  } catch (err) {
    console.error("Erreur rechargement sFTP:", err);
    showToast("Erreur : " + err.message, "error");
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function disconnectSftpSession(pid) {
  if (!confirm(`Voulez-vous vraiment déconnecter la session SSH/sFTP associée au processus PID ${pid} ?`)) {
    return;
  }

  try {
    const res = await fetch("/api/sftp/sessions/disconnect", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ pid: pid })
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || "Échec de déconnexion");

    showToast(`Session PID ${pid} déconnectée.`, "success");
    await loadSftpData();
  } catch (err) {
    console.error("Erreur déconnexion session sFTP:", err);
    showToast("Erreur : " + err.message, "error");
  }
}


// =========================================================================
// GESTION DES POINTS DE MONTAGE & DISQUES ÉPINGLÉS (PINNED MOUNTS)
// =========================================================================

let draggedMountIndex = null;

async function loadPinnedMounts() {
  try {
    const res = await fetch("/api/files/pinned-mounts");
    const json = await res.json();
    if (json.success && json.data) {
      let serverPins = json.data;

      // Appliquer l'ordre sauvegardé en localStorage s'il existe pour une fluidité instantanée
      try {
        const savedOrder = JSON.parse(localStorage.getItem("noos_pinned_mounts_order") || "[]");
        if (Array.isArray(savedOrder) && savedOrder.length > 0) {
          const pinMap = new Map(serverPins.map(p => [p.path, p]));
          const ordered = [];
          for (const p of savedOrder) {
            if (pinMap.has(p)) {
              ordered.push(pinMap.get(p));
              pinMap.delete(p);
            }
          }
          // Ajouter les nouveaux disques montés qui ne figuraient pas encore dans l'ordre
          for (const p of pinMap.values()) {
            ordered.push(p);
          }
          serverPins = ordered;
        }
      } catch (e) {}

      pinnedMountsList = serverPins;
      renderPinnedMounts(pinnedMountsList);
    }
  } catch (err) {
    console.error("Erreur chargement montages épinglés:", err);
  }
}

function renderPinnedMounts(pins) {
  const container = document.getElementById("files-pinned-mounts-container");
  if (!container) return;

  if (!pins || pins.length === 0) {
    container.innerHTML = `
      <div style="padding:8px 10px; font-size:0.75rem; color:var(--subtext0); font-style:italic;">
        Aucun disque ou volume détecté.
      </div>
    `;
    return;
  }

  container.innerHTML = pins.map((p, index) => {
    const cleanPath = p.path.replace(/\/+$/, "") || "/";
    const isActive = currentFolderPath === p.path || currentFolderPath === cleanPath;
    const isRoot = p.path === "/";

    return `
      <div class="files-mount-item ${isActive ? "active" : ""}"
           draggable="true"
           data-mount-path="${escapeHtml(p.path)}"
           data-index="${index}"
           onclick="navigateToPath('${escapeHtml(p.path)}')"
           title="Glisser pour réorganiser • Accéder à : ${escapeHtml(p.path)}">
        <div class="files-mount-left">
          <span class="files-mount-drag-handle" title="Glisser pour réorganiser l'ordre">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2">
              <circle cx="9" cy="6" r="1.5"/><circle cx="15" cy="6" r="1.5"/>
              <circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/>
              <circle cx="9" cy="18" r="1.5"/><circle cx="15" cy="18" r="1.5"/>
            </svg>
          </span>
          <span class="files-mount-icon">${escapeHtml(p.icon || "💾")}</span>
          <div class="files-mount-info">
            <span class="files-mount-label">${escapeHtml(p.label)}</span>
            <span class="files-mount-meta">${escapeHtml(p.path)}</span>
          </div>
        </div>
        ${!isRoot ? `
        <button type="button" class="files-mount-unpin-btn" onclick="unpinMountAction('${escapeHtml(p.path)}', event)" title="Désépingler ce point de montage">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
        ` : ''}
      </div>
    `;
  }).join("");

  attachPinnedMountsDragAndDrop(container);
}

function attachPinnedMountsDragAndDrop(container) {
  if (!container) return;
  const items = container.querySelectorAll(".files-mount-item");

  items.forEach(item => {
    item.addEventListener("dragstart", (e) => {
      draggedMountIndex = parseInt(item.getAttribute("data-index"), 10);
      item.classList.add("is-dragging");
      if (e.dataTransfer) {
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", item.getAttribute("data-mount-path") || "");
      }
    });

    item.addEventListener("dragover", (e) => {
      e.preventDefault();
      if (draggedMountIndex === null) return;
      if (e.dataTransfer) e.dataTransfer.dropEffect = "move";

      const targetIndex = parseInt(item.getAttribute("data-index"), 10);
      if (targetIndex === draggedMountIndex) return;

      const rect = item.getBoundingClientRect();
      const relY = e.clientY - rect.top;
      if (relY < rect.height / 2) {
        item.classList.add("drag-over-top");
        item.classList.remove("drag-over-bottom");
      } else {
        item.classList.add("drag-over-bottom");
        item.classList.remove("drag-over-top");
      }
    });

    item.addEventListener("dragleave", () => {
      item.classList.remove("drag-over-top");
      item.classList.remove("drag-over-bottom");
    });

    item.addEventListener("drop", async (e) => {
      e.preventDefault();
      item.classList.remove("drag-over-top");
      item.classList.remove("drag-over-bottom");

      if (draggedMountIndex === null) return;
      const targetIndex = parseInt(item.getAttribute("data-index"), 10);
      if (targetIndex === draggedMountIndex || isNaN(targetIndex) || isNaN(draggedMountIndex)) return;

      const rect = item.getBoundingClientRect();
      const relY = e.clientY - rect.top;
      const insertAfter = relY >= rect.height / 2;

      // Déplacer l'élément dans la liste locale
      const movedItem = pinnedMountsList.splice(draggedMountIndex, 1)[0];
      if (!movedItem) return;

      let newIndex = targetIndex;
      if (draggedMountIndex < targetIndex) {
        newIndex = insertAfter ? targetIndex : targetIndex - 1;
      } else {
        newIndex = insertAfter ? targetIndex + 1 : targetIndex;
      }
      newIndex = Math.max(0, Math.min(newIndex, pinnedMountsList.length));

      pinnedMountsList.splice(newIndex, 0, movedItem);

      // Re-render immédiat (0ms flicker)
      renderPinnedMounts(pinnedMountsList);

      // Persistance locale et distante
      const orderedPaths = pinnedMountsList.map(p => p.path);
      try {
        localStorage.setItem("noos_pinned_mounts_order", JSON.stringify(orderedPaths));
      } catch (e) {}

      try {
        await fetch("/api/files/pinned-mounts/reorder", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ paths: orderedPaths })
        });
      } catch (err) {
        console.warn("Erreur sauvegarde ordre distant des épingles:", err);
      }
    });

    item.addEventListener("dragend", () => {
      draggedMountIndex = null;
      items.forEach(el => {
        el.classList.remove("is-dragging", "drag-over-top", "drag-over-bottom");
      });
    });
  });
}

async function pinCurrentFolder() {
  if (!currentFolderPath) return;
  const cleanPath = currentFolderPath.replace(/\/+$/, "") || "/";
  const defaultLabel = cleanPath === "/" ? "Système Root" : cleanPath.split("/").pop();

  try {
    const res = await fetch("/api/files/pinned-mounts", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        path: currentFolderPath,
        label: defaultLabel,
        icon: currentFolderPath.includes("raid") ? "💽" : (currentFolderPath.includes("remote") ? "🌐" : "💾")
      })
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.message || "Échec de l épinglage");

    showToast(`Dossier « ${defaultLabel} » épinglé au volet latéral !`, "success");
    await loadPinnedMounts();
  } catch (err) {
    showToast("Erreur : " + err.message, "error");
  }
}

async function unpinMountAction(path, event) {
  if (event) event.stopPropagation();

  try {
    const res = await fetch("/api/files/pinned-mounts", {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path: path })
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.message || "Échec");

    try {
      let savedOrder = JSON.parse(localStorage.getItem("noos_pinned_mounts_order") || "[]");
      savedOrder = savedOrder.filter(p => p !== path);
      localStorage.setItem("noos_pinned_mounts_order", JSON.stringify(savedOrder));
    } catch (e) {}

    showToast("Point de montage retiré des favoris.", "info");
    await loadPinnedMounts();
  } catch (err) {
    showToast("Erreur : " + err.message, "error");
  }
}

// Modale de gestion des points de montage système
async function openStorageMountsModal() {
  const container = document.getElementById("storage-mounts-modal-list");
  if (container) {
    container.innerHTML = `<div style="text-align:center; padding:30px; color:var(--subtext0);">Chargement des points de montage...</div>`;
  }
  document.getElementById("modal-storage-mounts").style.display = "flex";

  try {
    const res = await fetch("/api/files/storage-mounts");
    const json = await res.json();
    if (json.success && json.data) {
      storageMountsList = json.data;
      renderStorageMountsModalList(storageMountsList);
    }
  } catch (err) {
    if (container) container.innerHTML = `<div style="color:var(--red); padding:20px;">Erreur : ${escapeHtml(err.message)}</div>`;
  }
}

function closeStorageMountsModal() {
  document.getElementById("modal-storage-mounts").style.display = "none";
}

function renderStorageMountsModalList(mounts) {
  const container = document.getElementById("storage-mounts-modal-list");
  if (!container) return;

  if (!mounts || mounts.length === 0) {
    container.innerHTML = `<div style="text-align:center; color:var(--subtext0); padding:20px;">Aucun point de montage détecté.</div>`;
    return;
  }

  container.innerHTML = mounts.map(m => {
    let barColorClass = "";
    if (m.usage_percent > 90) barColorClass = "crit";
    else if (m.usage_percent > 75) barColorClass = "warn";

    return `
      <div class="storage-mount-card">
        <div style="display:flex; align-items:center; gap:12px; min-width:0; flex:1;">
          <div style="font-size:1.6rem; width:40px; height:40px; display:flex; align-items:center; justify-content:center; background:rgba(255,255,255,0.05); border-radius:8px;">
            ${escapeHtml(m.icon)}
          </div>
          <div style="min-width:0; flex:1;">
            <div style="font-weight:700; color:var(--text); font-size:0.92rem; display:flex; align-items:center; gap:8px;">
              <span>${escapeHtml(m.label)}</span>
              <span class="badge badge-secondary" style="font-size:0.7rem;">${escapeHtml(m.filesystem)}</span>
            </div>
            <div style="font-size:0.78rem; color:var(--subtext0); font-family:var(--font-mono); margin-top:2px;">
              ${escapeHtml(m.mount_point)} &bull; ${escapeHtml(m.used_human)} / ${escapeHtml(m.total_human)} (${m.usage_percent}%)
            </div>
            <div class="files-mount-bar-wrap" style="height:5px; margin-top:6px; max-width:260px;">
              <div class="files-mount-bar-fill ${barColorClass}" style="width:${Math.min(100, m.usage_percent)}%;"></div>
            </div>
          </div>
        </div>

        <div>
          <button type="button" class="btn ${m.is_pinned ? "btn-secondary" : "btn-primary"} btn-xs" onclick="togglePinStorageMount('${escapeHtml(m.mount_point)}', '${escapeHtml(m.label)}', '${escapeHtml(m.icon)}', ${m.is_pinned})">
            ${m.is_pinned ? "<span>📌</span> Épinglé" : "<span>➕</span> Épingler"}
          </button>
        </div>
      </div>
    `;
  }).join("");
}

async function togglePinStorageMount(path, label, icon, isPinned) {
  try {
    if (isPinned) {
      await fetch("/api/files/pinned-mounts", {
        method: "DELETE",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: path })
      });
      showToast(`Point de montage retiré des favoris.`, "info");
    } else {
      await fetch("/api/files/pinned-mounts", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: path, label: label, icon: icon })
      });
      showToast(`Point de montage « ${label} » épinglé !`, "success");
    }

    await loadPinnedMounts();
    const res = await fetch("/api/files/storage-mounts");
    const json = await res.json();
    if (json.success) renderStorageMountsModalList(json.data);
  } catch (err) {
    showToast("Erreur : " + err.message, "error");
  }
}

// =========================================================================
// MONTAGES DISTANTS sFTP & SMB
// =========================================================================

async function loadRemoteMounts() {
  try {
    const res = await fetch("/api/files/remote-mounts");
    const json = await res.json();
    if (json.success && json.data) {
      remoteMountsList = json.data;
      renderRemoteMounts(remoteMountsList);
    }
  } catch (err) {
    console.error("Erreur chargement partages distants:", err);
  }
}

function renderRemoteMounts(mounts) {
  const container = document.getElementById("files-remote-mounts-container");
  if (!container) return;

  if (!mounts || mounts.length === 0) {
    container.innerHTML = `
      <div style="padding:6px 10px; font-size:0.75rem; color:var(--subtext0); font-style:italic;">
        Aucun partage distant monté.
      </div>
    `;
    return;
  }

  container.innerHTML = mounts.map(m => {
    const isActive = currentFolderPath.startsWith(m.mount_point);
    const dotClass = m.is_mounted ? "online" : "offline";

    return `
      <div class="files-remote-item ${isActive ? "active" : ""}" data-mount-path="${escapeHtml(m.mount_point)}" onclick="navigateToPath('${escapeHtml(m.mount_point)}')" title="${escapeHtml(m.name)} (${escapeHtml(m.host)}) - ${escapeHtml(m.status_text)}">
        <div style="display:flex; align-items:center; gap:8px; min-width:0; flex:1;">
          <span class="remote-status-dot ${dotClass}" title="${m.is_mounted ? "Connecté & Monté" : "Déconnecté"}"></span>
          <span style="font-size:0.95rem;">${m.protocol === "sftp" ? "⚡" : "🪟"}</span>
          <div style="min-width:0; flex:1;">
            <div style="font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:0.8rem;">
              ${escapeHtml(m.name)}
            </div>
            <div style="font-size:0.68rem; color:var(--subtext0); font-family:var(--font-mono);">
              ${escapeHtml(m.host)} &bull; ${escapeHtml(m.protocol.toUpperCase())}
            </div>
          </div>
        </div>
        <div style="display:flex; gap:2px;">
          ${m.is_mounted ? `
            <button type="button" class="files-mount-unpin-btn" onclick="unmountRemoteAction('${escapeHtml(m.id)}', event)" title="Démonter ce partage">
              ⏏️
            </button>
          ` : `
            <button type="button" class="files-mount-unpin-btn" onclick="remountRemoteAction('${escapeHtml(m.id)}', event)" title="Reconnecter ce partage">
              🔄
            </button>
          `}
          <button type="button" class="files-mount-unpin-btn" onclick="deleteRemoteMountAction('${escapeHtml(m.id)}', '${escapeHtml(m.name)}', event)" title="Supprimer ce partage">
            🗑️
          </button>
        </div>
      </div>
    `;
  }).join("");
}

function openCreateRemoteMountModal(prefill = {}) {
  const form = document.getElementById("form-remote-mount");
  if (form) form.reset();

  const proto = prefill.protocol || "sftp";
  switchRemoteProto(proto);

  if (prefill.host) document.getElementById("remote-mount-host").value = prefill.host;
  if (prefill.port) document.getElementById("remote-mount-port").value = prefill.port;
  if (prefill.name) document.getElementById("remote-mount-name").value = prefill.name;
  if (prefill.remote_path) document.getElementById("remote-mount-path").value = prefill.remote_path;
  if (prefill.username) document.getElementById("remote-mount-user").value = prefill.username;

  updateRemoteMountPreview();
  document.getElementById("modal-remote-mount").style.display = "flex";
}

function closeRemoteMountModal() {
  document.getElementById("modal-remote-mount").style.display = "none";
}

function switchRemoteProto(proto) {
  document.getElementById("remote-mount-proto").value = proto;
  const isSftp = proto === "sftp";

  const btnSftp = document.getElementById("btn-proto-sftp");
  const btnSmb = document.getElementById("btn-proto-smb");
  if (btnSftp) btnSftp.classList.toggle("active", isSftp);
  if (btnSmb) btnSmb.classList.toggle("active", !isSftp);

  const badge = document.getElementById("remote-mount-proto-badge");
  if (badge) {
    badge.textContent = isSftp ? "sFTP / SSH" : "SMB / Windows";
    badge.className = `badge ${isSftp ? "badge-primary" : "badge-success"}`;
  }

  const portInput = document.getElementById("remote-mount-port");
  if (portInput) {
    if (isSftp && portInput.value === "445") portInput.value = "22";
    if (!isSftp && portInput.value === "22") portInput.value = "445";
  }

  const pathLabel = document.getElementById("remote-mount-path-label");
  if (pathLabel) {
    pathLabel.textContent = isSftp ? "Dossier distant sur le serveur sFTP :" : "Nom du partage SMB (ex: public, data) :";
  }

  const pathInput = document.getElementById("remote-mount-path");
  if (pathInput) {
    pathInput.placeholder = isSftp ? "ex: / ou /home/user" : "ex: public ou medias";
  }

  updateRemoteMountPreview();
}

function updateRemoteMountPreview() {
  const name = document.getElementById("remote-mount-name")?.value.trim() || "partage";
  const proto = document.getElementById("remote-mount-proto")?.value || "sftp";
  const slug = name.toLowerCase().replace(/[^a-z0-9-]+/g, "-");
  const preview = document.getElementById("remote-mount-preview-path");
  if (preview) {
    preview.textContent = `/mnt/remote/${proto}/${slug}`;
  }
}

document.addEventListener("DOMContentLoaded", () => {
  const nameInput = document.getElementById("remote-mount-name");
  if (nameInput) {
    nameInput.addEventListener("input", updateRemoteMountPreview);
  }
});

async function handleRemoteMountSubmit(event) {
  if (event) event.preventDefault();

  const proto = document.getElementById("remote-mount-proto").value;
  const name = document.getElementById("remote-mount-name").value.trim();
  const host = document.getElementById("remote-mount-host").value.trim();
  const port = parseInt(document.getElementById("remote-mount-port").value, 10) || (proto === "sftp" ? 22 : 445);
  const remotePath = document.getElementById("remote-mount-path").value.trim();
  const user = document.getElementById("remote-mount-user").value.trim();
  const pass = document.getElementById("remote-mount-pass").value;
  const key = document.getElementById("remote-mount-key").value.trim();
  const autoMount = document.getElementById("remote-mount-auto").checked;

  const btn = document.getElementById("btn-submit-remote-mount");
  const oldText = btn.innerHTML;
  btn.disabled = true;
  btn.innerHTML = `<span>⏳</span> Connexion et montage...`;

  try {
    const res = await fetch("/api/files/remote-mounts", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: name,
        protocol: proto,
        host: host,
        port: port,
        remote_path: remotePath,
        username: user,
        password: pass || null,
        ssh_key_path: key || null,
        auto_mount: autoMount
      })
    });

    const json = await res.json();
    if (!json.success || !json.data) throw new Error(json.message || "Échec de connexion au partage distant");

    showToast(`Partage « ${name} » monté avec succès !`, "success");
    closeRemoteMountModal();
    await loadRemoteMounts();
    await loadPinnedMounts();

    // Naviguer directement dans le dossier monté
    navigateToPath(json.data.mount_point);
  } catch (err) {
    console.error("Erreur montage distant:", err);
    showToast("Erreur : " + err.message, "error");
  } finally {
    btn.disabled = false;
    btn.innerHTML = oldText;
  }
}

async function unmountRemoteAction(id, event) {
  if (event) event.stopPropagation();

  try {
    const res = await fetch(`/api/files/remote-mounts/${encodeURIComponent(id)}/unmount`, { method: "POST" });
    const json = await res.json();
    if (!json.success) throw new Error(json.message || "Échec");

    showToast("Partage démonté.", "info");
    await loadRemoteMounts();
  } catch (err) {
    showToast("Erreur : " + err.message, "error");
  }
}

async function remountRemoteAction(id, event) {
  if (event) event.stopPropagation();

  try {
    const res = await fetch(`/api/files/remote-mounts/${encodeURIComponent(id)}/mount`, { method: "POST" });
    const json = await res.json();
    if (!json.success) throw new Error(json.message || "Échec");

    showToast("Partage reconnecté et monté.", "success");
    await loadRemoteMounts();
  } catch (err) {
    showToast("Erreur : " + err.message, "error");
  }
}

async function deleteRemoteMountAction(id, name, event) {
  if (event) event.stopPropagation();
  if (!confirm(`Supprimer le point d accès distant « ${name} » ?

🛡️ Les données sur le serveur distant ne seront pas altérées, seul le point de montage local est retiré.`)) {
    return;
  }

  try {
    const res = await fetch(`/api/files/remote-mounts/${encodeURIComponent(id)}`, { method: "DELETE" });
    const json = await res.json();
    if (!json.success) throw new Error(json.message || "Échec");

    showToast(`Partage distant « ${name} » supprimé.`, "info");
    await loadRemoteMounts();
    await loadPinnedMounts();
  } catch (err) {
    showToast("Erreur : " + err.message, "error");
  }
}

// =========================================================================
// RADAR & DÉCOUVERTE RÉSEAU LOCAL (LAN)
// =========================================================================

function openNetworkDiscoveryModal() {
  document.getElementById("modal-network-discovery").style.display = "flex";
  if (discoveredDevicesList.length === 0) {
    startNetworkDiscoveryScan();
  } else {
    renderDiscoveredDevices(discoveredDevicesList);
  }
}

function closeNetworkDiscoveryModal() {
  document.getElementById("modal-network-discovery").style.display = "none";
}

async function startNetworkDiscoveryScan() {
  const radarBox = document.getElementById("radar-animation-box");
  const statusText = document.getElementById("net-discovery-status-text");
  const spinner = document.getElementById("btn-net-scan-spinner");
  const scanBtn = document.getElementById("btn-start-net-scan");

  if (radarBox) radarBox.style.display = "flex";
  if (statusText) statusText.textContent = "Sondage des partages réseau LAN en cours...";
  if (spinner) spinner.textContent = "⏳";
  if (scanBtn) scanBtn.disabled = true;

  try {
    const res = await fetch("/api/files/network/discover");
    const json = await res.json();

    if (json.success && json.data) {
      discoveredDevicesList = json.data;
      renderDiscoveredDevices(discoveredDevicesList);
      if (statusText) statusText.textContent = `Scan terminé : ${discoveredDevicesList.length} périphérique${discoveredDevicesList.length > 1 ? "s" : ""} identifié${discoveredDevicesList.length > 1 ? "s" : ""}`;
    } else {
      throw new Error(json.message || "Erreur de scan");
    }
  } catch (err) {
    console.error("Erreur découverte réseau:", err);
    showToast("Erreur lors de la découverte réseau : " + err.message, "error");
    if (statusText) statusText.textContent = "Échec du scan réseau";
  } finally {
    if (radarBox) radarBox.style.display = "none";
    if (spinner) spinner.textContent = "🔍";
    if (scanBtn) scanBtn.disabled = false;
  }
}

function renderDiscoveredDevices(devices) {
  const grid = document.getElementById("net-discovered-grid");
  const countEl = document.getElementById("net-discovery-count-text");
  if (!grid) return;

  if (countEl) {
    countEl.textContent = `Périphériques détectés (${devices.length})`;
  }

  if (!devices || devices.length === 0) {
    grid.innerHTML = `
      <div style="grid-column:1/-1; padding:35px; text-align:center; color:var(--subtext0); background:var(--mantle); border-radius:var(--radius-md); border:1px dashed rgba(255,255,255,0.08);">
        <div style="font-size:2rem; margin-bottom:8px;">🔎</div>
        <div style="font-weight:600; color:var(--text); margin-bottom:4px;">Aucun partage détecté pour l instant</div>
        <div style="font-size:0.8rem;">Assurez-vous que vos autres PC, NAS ou serveurs sont sous tension et connectés au même réseau local.</div>
      </div>
    `;
    return;
  }

  grid.innerHTML = devices.map(dev => {
    let icon = "💻";
    if (dev.device_type.includes("NAS")) icon = "🗄️";
    else if (dev.device_type.includes("SSH")) icon = "🐧";
    else if (dev.device_type.includes("Routeur")) icon = "📡";

    const badges = [];
    if (dev.smb_available) {
      badges.push(`<span class="badge badge-success" style="font-size:0.7rem;">🪟 SMB (445)</span>`);
    }
    if (dev.sftp_available) {
      badges.push(`<span class="badge badge-primary" style="font-size:0.7rem;">⚡ sFTP (22)</span>`);
    }
    if (dev.nfs_available) {
      badges.push(`<span class="badge badge-purple" style="font-size:0.7rem;">📦 NFS (2049)</span>`);
    }

    let sharesHtml = "";
    if (dev.smb_shares && dev.smb_shares.length > 0) {
      sharesHtml = `
        <div class="dev-card-shares-list">
          <div style="font-weight:600; font-size:0.72rem; color:var(--subtext0); margin-bottom:4px;">Partages SMB publics :</div>
          <div style="display:flex; flex-wrap:wrap; gap:4px;">
            ${dev.smb_shares.map(s => `<span class="badge badge-secondary" style="font-size:0.68rem; cursor:pointer;" onclick="connectFromDiscovery('${escapeHtml(dev.ip)}', 'smb', '${escapeHtml(s)}')" title="Monter ce dossier">📁 ${escapeHtml(s)}</span>`).join(" ")}
          </div>
        </div>
      `;
    }

    return `
      <div class="discovered-device-card">
        <div class="dev-card-header">
          <div class="dev-card-icon">${icon}</div>
          <div style="min-width:0; flex:1;">
            <div class="dev-card-title" title="${escapeHtml(dev.hostname)}">${escapeHtml(dev.hostname)}</div>
            <div class="dev-card-ip">${escapeHtml(dev.ip)}</div>
          </div>
        </div>

        <div style="font-size:0.75rem; color:var(--subtext1);">
          ${escapeHtml(dev.device_type)}
        </div>

        <div class="dev-card-badges">
          ${badges.join(" ")}
        </div>

        ${sharesHtml}

        <div class="dev-card-actions">
          ${dev.sftp_available ? `
            <button type="button" class="btn btn-primary btn-xs" onclick="connectFromDiscovery('${escapeHtml(dev.ip)}', 'sftp')" title="Monter via sFTP (SSH)">
              <span>⚡</span> sFTP
            </button>
          ` : ""}
          ${dev.smb_available ? `
            <button type="button" class="btn btn-secondary btn-xs" onclick="connectFromDiscovery('${escapeHtml(dev.ip)}', 'smb')" title="Monter un partage SMB">
              <span>🪟</span> SMB
            </button>
          ` : ""}
          <button type="button" class="btn btn-secondary btn-xs" onclick="copyTextToClipboard('${escapeHtml(dev.ip)}', 'Adresse IP copiée !')" title="Copier l adresse IP">
            📋 IP
          </button>
        </div>
      </div>
    `;
  }).join("");
}

function filterDiscoveredDevices() {
  const query = (document.getElementById("net-discovery-filter")?.value || "").toLowerCase().trim();
  if (!query) {
    renderDiscoveredDevices(discoveredDevicesList);
    return;
  }

  const filtered = discoveredDevicesList.filter(d => 
    d.hostname.toLowerCase().includes(query) || d.ip.includes(query) || d.device_type.toLowerCase().includes(query)
  );
  renderDiscoveredDevices(filtered);
}

function connectFromDiscovery(ip, proto, shareName = "") {
  closeNetworkDiscoveryModal();
  openCreateRemoteMountModal({
    host: ip,
    protocol: proto,
    name: shareName ? `${shareName} sur ${ip}` : `Partage ${ip}`,
    remote_path: shareName ? `/${shareName}` : "/",
    port: proto === "sftp" ? 22 : 445
  });
}

// =========================================================================
// INTÉGRATION KDRIVE INFOMANIAK (STOCKAGE CLOUD API REST SÉCURISÉ)
// =========================================================================

let isKDriveView = false;
let currentKDriveAccountId = null;
let currentKDriveFolderId = 0;
let currentKDriveParentFolderId = null;
let currentKDriveAccountName = "";
let kdriveBreadcrumbsStack = [];
let kdriveAccountsList = [];
let currentKDriveActionFile = null;

async function loadKDriveAccounts() {
  try {
    const res = await fetch("/api/kdrive/accounts");
    const json = await res.json();
    if (json.success && json.data) {
      kdriveAccountsList = json.data;
      renderKDriveAccounts(kdriveAccountsList);
    }
  } catch (err) {
    console.error("Erreur chargement comptes kDrive:", err);
  }
}

function renderKDriveAccounts(accounts) {
  const container = document.getElementById("files-kdrive-accounts-container");
  if (!container) return;

  if (!accounts || accounts.length === 0) {
    container.innerHTML = `
      <div style="padding:6px 10px; font-size:0.75rem; color:var(--subtext0); font-style:italic;">
        Aucun compte kDrive lié.
      </div>
    `;
    return;
  }

  container.innerHTML = accounts.map(acc => {
    const isActive = isKDriveView && currentKDriveAccountId === acc.id;
    return `
      <div class="files-kdrive-item ${isActive ? "active" : ""}" 
           data-account-id="${escapeHtml(acc.id)}" 
           onclick="navigateToKDrive('${escapeHtml(acc.id)}', 0, '${escapeHtml(acc.name)}')" 
           title="kDrive : ${escapeHtml(acc.name)} (${escapeHtml(acc.used_size_human)} / ${escapeHtml(acc.total_size_human)})">
        <div style="display:flex; align-items:center; gap:8px; min-width:0; flex:1;">
          <span style="font-size:1.1rem; line-height:1;">☁️</span>
          <div style="min-width:0; flex:1;">
            <div style="font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:0.8rem; color:var(--text);">
              ${escapeHtml(acc.name)}
            </div>
            <div style="font-size:0.68rem; color:var(--subtext0); font-family:var(--font-mono);">
              ${escapeHtml(acc.used_size_human)} / ${escapeHtml(acc.total_size_human)}
            </div>
          </div>
        </div>
        <button type="button" class="files-mount-unpin-btn" onclick="disconnectKDriveAccount('${escapeHtml(acc.id)}', '${escapeHtml(acc.name)}', event)" title="Déconnecter ce kDrive du NAS">
          🗑️
        </button>
      </div>
    `;
  }).join("");
}

function openConnectKDriveModal() {
  const form = document.getElementById("form-connect-kdrive");
  if (form) form.reset();
  const detectedSec = document.getElementById("kdrive-detected-section");
  if (detectedSec) detectedSec.style.display = "none";
  const btnSubmit = document.getElementById("btn-submit-kdrive");
  if (btnSubmit) btnSubmit.disabled = true;
  const modal = document.getElementById("modal-connect-kdrive");
  if (modal) modal.style.display = "flex";
}

function closeConnectKDriveModal() {
  const modal = document.getElementById("modal-connect-kdrive");
  if (modal) modal.style.display = "none";
}

function toggleKDriveTokenVisibility() {
  const inp = document.getElementById("kdrive-input-token");
  const icon = document.getElementById("kdrive-token-eye-icon");
  if (!inp) return;
  if (inp.type === "password") {
    inp.type = "text";
    if (icon) icon.textContent = "🙈";
  } else {
    inp.type = "password";
    if (icon) icon.textContent = "👁️";
  }
}

async function testAndDetectKDrives() {
  const tokenInp = document.getElementById("kdrive-input-token");
  const token = tokenInp ? tokenInp.value.trim() : "";
  if (!token) {
    showToast("Veuillez saisir votre jeton d'accès API Infomaniak.", "warning");
    return;
  }

  const btn = document.getElementById("btn-detect-kdrives");
  const icon = document.getElementById("btn-detect-kdrives-icon");
  if (btn) btn.disabled = true;
  if (icon) icon.textContent = "⏳";

  try {
    const res = await fetch("/api/kdrive/detect", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ token })
    });
    const json = await res.json();

    if (json.success && json.data && json.data.length > 0) {
      const select = document.getElementById("kdrive-select-drive");
      if (select) {
        select.innerHTML = json.data.map(d => `
          <option value="${d.id}" data-name="${escapeHtml(d.name)}">
            ${escapeHtml(d.name)} (${escapeHtml(d.used_size_human)} / ${escapeHtml(d.size_human)})
          </option>
        `).join("");
      }
      const nameInp = document.getElementById("kdrive-input-name");
      if (nameInp) {
        nameInp.value = json.data[0].name;
      }
      const detectedSec = document.getElementById("kdrive-detected-section");
      if (detectedSec) detectedSec.style.display = "block";
      const btnSubmit = document.getElementById("btn-submit-kdrive");
      if (btnSubmit) btnSubmit.disabled = false;

      showToast(`✓ ${json.data.length} kDrive détecté(s) sur votre compte !`, "success");
    } else {
      showToast(json.message || "Aucun kDrive trouvé avec ce jeton. Vérifiez les permissions du jeton.", "error");
    }
  } catch (err) {
    showToast("Erreur lors de la détection : " + err, "error");
  } finally {
    if (btn) btn.disabled = false;
    if (icon) icon.textContent = "🔍";
  }
}

function onKDriveSelectChange() {
  const select = document.getElementById("kdrive-select-drive");
  const nameInp = document.getElementById("kdrive-input-name");
  if (select && nameInp && select.selectedOptions[0]) {
    nameInp.value = select.selectedOptions[0].getAttribute("data-name") || "Mon kDrive";
  }
}

async function submitConnectKDrive() {
  const tokenInp = document.getElementById("kdrive-input-token");
  const nameInp = document.getElementById("kdrive-input-name");
  const select = document.getElementById("kdrive-select-drive");
  const token = tokenInp ? tokenInp.value.trim() : "";
  const name = nameInp ? nameInp.value.trim() : "";
  const drive_id = select ? parseInt(select.value, 10) : 0;

  if (!token || !drive_id) {
    showToast("Veuillez d'abord détecter et sélectionner un kDrive.", "warning");
    return;
  }

  const btn = document.getElementById("btn-submit-kdrive");
  if (btn) btn.disabled = true;

  try {
    const res = await fetch("/api/kdrive/accounts", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, token, drive_id })
    });
    const json = await res.json();
    if (json.success && json.data) {
      showToast(`☁️ kDrive '${json.data.name}' connecté avec succès !`, "success");
      closeConnectKDriveModal();
      await loadKDriveAccounts();
      navigateToKDrive(json.data.id, 0, json.data.name);
    } else {
      showToast(json.message || "Échec de l'enregistrement du kDrive", "error");
    }
  } catch (err) {
    showToast("Erreur lors de la connexion du kDrive : " + err, "error");
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function disconnectKDriveAccount(accountId, accountName, event) {
  if (event) event.stopPropagation();
  if (!confirm(`Voulez-vous vraiment déconnecter le compte kDrive "${accountName}" du NAS ?\n\nVos fichiers distants sur Infomaniak restent totalement intacts.`)) {
    return;
  }

  try {
    const res = await fetch(`/api/kdrive/accounts/${encodeURIComponent(accountId)}`, {
      method: "DELETE"
    });
    const json = await res.json();
    if (json.success) {
      showToast(`Compte kDrive "${accountName}" déconnecté.`, "info");
      await loadKDriveAccounts();
      if (isKDriveView && currentKDriveAccountId === accountId) {
        navigateToPath(getUserHome());
      }
    } else {
      showToast(json.message || "Erreur lors de la déconnexion", "error");
    }
  } catch (err) {
    showToast("Erreur réseau : " + err, "error");
  }
}

async function navigateToKDrive(accountId, folderId = 0, folderName = null) {
  isTrashView = false;
  isKDriveView = true;
  selectedTrashItem = null;
  clearFileSelection();

  const normalTb = document.getElementById("files-toolbar-normal");
  const trashTb = document.getElementById("files-toolbar-trash");
  if (normalTb) normalTb.style.display = "flex";
  if (trashTb) trashTb.style.display = "none";

  currentKDriveAccountId = accountId;
  currentKDriveFolderId = folderId;
  currentFolderPath = `kdrive://${accountId}/${folderId}`;

  try {
    const res = await fetch(`/api/kdrive/accounts/${encodeURIComponent(accountId)}/files?folder_id=${folderId}`);
    const json = await res.json();

    if (!json.success || !json.data) {
      showToast(json.message || "Impossible de charger le dossier kDrive", "error");
      return;
    }

    const data = json.data;
    currentKDriveAccountName = data.account_name;
    currentKDriveParentFolderId = data.parent_folder_id;
    currentEntries = data.entries || [];

    // Gestion de la pile de miettes kDrive
    if (folderId === 0) {
      kdriveBreadcrumbsStack = [{ id: 0, name: data.account_name || folderName || "kDrive" }];
    } else {
      const existingIdx = kdriveBreadcrumbsStack.findIndex(c => c.id === folderId);
      if (existingIdx !== -1) {
        kdriveBreadcrumbsStack = kdriveBreadcrumbsStack.slice(0, existingIdx + 1);
      } else {
        kdriveBreadcrumbsStack.push({ id: folderId, name: data.current_folder_name || folderName || "Dossier" });
      }
    }

    updateFilesBreadcrumbs();
    renderFilesList(currentEntries);
    updateFilesStatusBar(data.total_items, data.total_size_bytes);

    // Mettre à jour l'élément actif dans la barre latérale
    document.querySelectorAll(".files-nav-item").forEach(item => item.classList.remove("active"));
    document.querySelectorAll(".files-mount-item").forEach(item => item.classList.remove("active"));
    document.querySelectorAll(".files-kdrive-item").forEach(item => {
      item.classList.toggle("active", item.getAttribute("data-account-id") === accountId);
    });

  } catch (err) {
    showToast("Erreur lors de la navigation kDrive : " + err, "error");
  }
}

function openKDriveFileActionModal(accountId, fileId, fileName) {
  currentKDriveActionFile = { accountId, fileId, fileName };
  const label = document.getElementById("kdrive-copy-file-name");
  if (label) label.textContent = fileName;
  const modal = document.getElementById("modal-kdrive-copy-nas");
  if (modal) modal.style.display = "flex";
}

function closeKDriveCopyModal() {
  const modal = document.getElementById("modal-kdrive-copy-nas");
  if (modal) modal.style.display = "none";
  currentKDriveActionFile = null;
}

function setKDriveCopyDest(path) {
  const inp = document.getElementById("kdrive-copy-dest-dir");
  if (inp) inp.value = path;
}

function downloadKDriveFileDirectly() {
  if (!currentKDriveActionFile) return;
  const { accountId, fileId, fileName } = currentKDriveActionFile;
  const dlUrl = `/api/kdrive/accounts/${encodeURIComponent(accountId)}/download/${fileId}`;
  window.open(dlUrl, "_blank");
  closeKDriveCopyModal();
  showToast(`Téléchargement lancé pour "${fileName}"`, "info");
}

async function submitKDriveCopyToNas() {
  if (!currentKDriveActionFile) return;
  const { accountId, fileId, fileName } = currentKDriveActionFile;
  const destDirInp = document.getElementById("kdrive-copy-dest-dir");
  const destDir = destDirInp ? destDirInp.value.trim() : "/storage/media";

  const btn = document.getElementById("btn-submit-kdrive-copy");
  if (btn) btn.disabled = true;

  showToast(`🚀 Copie en cours vers ${destDir} en tâche de fond...`, "info");
  closeKDriveCopyModal();

  try {
    const res = await fetch(`/api/kdrive/accounts/${encodeURIComponent(accountId)}/copy-to-nas`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        file_id: fileId,
        file_name: fileName,
        dest_dir: destDir
      })
    });
    const json = await res.json();
    if (json.success) {
      showToast(`✓ Fichier "${fileName}" copié avec succès dans ${destDir} !`, "success");
    } else {
      showToast(json.message || "Échec de la copie vers le NAS", "error");
    }
  } catch (err) {
    showToast("Erreur lors de la copie sur le NAS : " + err, "error");
  }
}

