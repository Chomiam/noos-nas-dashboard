// ==========================================================================
// STEvE_OS NAS Edition — Application Client (Vanilla JavaScript)
// ==========================================================================

let activeTab = "tab-overview";

document.addEventListener("DOMContentLoaded", () => {
  initApp();
  setupPolling();
});

function initApp() {
  refreshAll(false);
  updateSftpUri();
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
      loadFirewall()
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

    // Pools Table
    const poolsTbody = document.getElementById("pools-tbody");
    if (poolsTbody) {
      poolsTbody.innerHTML = data.pools.map(p => `
        <tr>
          <td><strong>📁 ${escapeHtml(p.mountpoint)}</strong></td>
          <td><code style="color:var(--subtext0);">${escapeHtml(p.filesystem)}</code></td>
          <td>${p.used_human} / ${p.total_human}</td>
          <td><span style="color:var(--green); font-weight:600;">${p.free_human}</span></td>
          <td style="min-width:180px;">
            <div style="display:flex; align-items:center; gap:10px;">
              <div class="metric-progress-wrap" style="flex:1;">
                <div class="metric-progress-bar progress-peach" style="width: ${Math.min(p.usage_percent, 100)}%;"></div>
              </div>
              <span style="font-size:0.8rem; font-family:var(--font-mono);">${p.usage_percent}%</span>
            </div>
          </td>
        </tr>
      `).join("");
    }

    // Physical Disks Table
    const disksTbody = document.getElementById("disks-tbody");
    if (disksTbody) {
      disksTbody.innerHTML = data.physical_disks.map(d => {
        const isStandby = d.power_state.toLowerCase().includes("veille") || d.power_state.toLowerCase().includes("standby");
        const stateBadge = isStandby 
          ? `<span class="badge badge-warning">🌙 ${escapeHtml(d.power_state)}</span>`
          : `<span class="badge badge-success">⚡ ${escapeHtml(d.power_state)}</span>`;

        const spindownBtn = d.is_rotational
          ? `<button type="button" class="btn btn-secondary btn-xs" onclick="triggerSpindown('${escapeHtml(d.name)}')">Mettre en veille</button>`
          : `<span class="badge badge-info">SSD (Sans moteur)</span>`;

        return `
          <tr>
            <td><strong>💽 ${escapeHtml(d.path)}</strong></td>
            <td>${escapeHtml(d.model)}</td>
            <td><span class="badge badge-mauve">${escapeHtml(d.disk_type)}</span></td>
            <td>${escapeHtml(d.size_human)}</td>
            <td>${stateBadge}</td>
            <td><span class="badge badge-success">${escapeHtml(d.smart_status)}</span></td>
            <td style="text-align:right;">${spindownBtn}</td>
          </tr>
        `;
      }).join("");
    }
  } catch (err) {
    console.warn("Erreur fetch /api/storage:", err);
  }
}

// --------------------------------------------------------------------------
// SERVICES & SESSIONS
// --------------------------------------------------------------------------
async function loadServices() {
  try {
    const res = await fetch("/api/services");
    const json = await res.json();
    if (!json.success || !json.data) return;

    const s = json.data;

    // Quick services bar in overview
    const quickWrap = document.getElementById("quick-services-wrap");
    if (quickWrap) {
      const items = [s.samba, s.sshd, s.jellyfin, s.docker, s.cockpit, s.fail2ban];
      quickWrap.innerHTML = items.map(item => `
        <div class="status-pill" style="padding:8px 14px;">
          <span class="pulse-dot" style="${item.is_active ? '' : 'background:var(--red); box-shadow:none;'}"></span>
          <span><strong>${escapeHtml(item.name)}</strong>: ${item.is_active ? 'Actif' : 'Arrêté'}</span>
        </div>
      `).join("");
    }

    // Sessions sFTP & Samba
    const sessionsTbody = document.getElementById("sessions-tbody");
    if (sessionsTbody) {
      const allSessions = [...(s.active_sftp_sessions || []), ...(s.active_samba_sessions || [])];
      if (allSessions.length === 0) {
        sessionsTbody.innerHTML = `<tr><td colspan="4" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun client actuellement connecté sur vos partages.</td></tr>`;
      } else {
        sessionsTbody.innerHTML = allSessions.map(sess => `
          <tr>
            <td><strong>👤 ${escapeHtml(sess.user)}</strong></td>
            <td><span class="badge badge-mauve">${escapeHtml(sess.protocol)}</span></td>
            <td><code style="color:var(--sky);">${escapeHtml(sess.client_ip)}</code></td>
            <td><span class="badge badge-success">Connecté</span></td>
          </tr>
        `).join("");
      }
    }

    // Conteneurs Docker
    const containersTbody = document.getElementById("containers-tbody");
    if (containersTbody) {
      if (!s.docker_containers || s.docker_containers.length === 0) {
        containersTbody.innerHTML = `<tr><td colspan="4" style="text-align:center; color:var(--subtext0); padding:20px;">Aucun conteneur Docker détecté sur le NAS.</td></tr>`;
      } else {
        containersTbody.innerHTML = s.docker_containers.map(c => `
          <tr>
            <td><strong>🐳 ${escapeHtml(c.name)}</strong></td>
            <td><code style="color:var(--subtext0); font-size:0.8rem;">${escapeHtml(c.image)}</code></td>
            <td><span class="badge ${c.is_running ? 'badge-success' : 'badge-warning'}">${escapeHtml(c.status)}</span></td>
            <td style="text-align:right;">
              <button type="button" class="btn btn-secondary btn-xs" onclick="restartContainer('${escapeHtml(c.id)}')">Redémarrer</button>
            </td>
          </tr>
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

    const vendorEl = document.getElementById("gpu-vendor");
    if (vendorEl) vendorEl.textContent = gpu.vendor;

    const modelEl = document.getElementById("gpu-model");
    if (modelEl) modelEl.textContent = gpu.model_name;

    const driStatus = document.getElementById("gpu-dri-status");
    if (driStatus) {
      driStatus.textContent = gpu.dri_available ? "Actif & Initialisé" : "Non disponible";
      driStatus.style.color = gpu.dri_available ? "var(--green)" : "var(--red)";
    }

    const renderNode = document.getElementById("gpu-render-node");
    if (renderNode) {
      renderNode.textContent = gpu.render_node || "Aucun render node (/dev/dri/renderD128)";
    }

    const jobsEl = document.getElementById("gpu-active-jobs");
    if (jobsEl) jobsEl.textContent = `${gpu.active_transcoding_jobs} flux actif(s)`;

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
