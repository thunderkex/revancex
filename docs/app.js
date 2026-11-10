import qrcode from './vendor/qrcode-generator-1.4.4.mjs';
import { getThemePreference, setThemePreference } from './cache.js';
import { initComposer } from './composer.js';
import { initTracker } from './tracker.js';

const OTA_BASE = 'https://thunderkex.github.io/revancex/update';

const state = {
  items: [],
  catalog: null,
  q: '',
  format: 'all',
  category: 'all',
  arch: 'all_filter',
  sort: 'updated',
  lastFocused: null,
  currentQrUrl: '',
};

const els = {
  themeToggle: document.getElementById('theme-toggle'),
  themeIcon: document.getElementById('theme-icon'),
  themeLabel: document.getElementById('theme-label'),
  healthTag: document.getElementById('health-tag'),
  healthCli: document.getElementById('health-cli'),
  healthBuilt: document.getElementById('health-built'),
  healthPassLabel: document.getElementById('health-pass-label'),
  healthPass: document.getElementById('health-pass'),
  healthSourcesCount: document.getElementById('health-sources-count'),
  healthSourcesList: document.getElementById('health-sources-list'),
  searchInput: document.getElementById('search-input'),
  categorySelect: document.getElementById('category-select'),
  archSelect: document.getElementById('arch-select'),
  sortSelect: document.getElementById('sort-select'),
  resultsCount: document.getElementById('results-count'),
  resetBtn: document.getElementById('reset-filters-btn'),
  grid: document.getElementById('catalog-grid'),
  statusBox: document.getElementById('catalog-status'),
  guideModal: document.getElementById('guide-modal'),
  openGuideBtn: document.getElementById('open-guide-btn'),
  guideCloseBtn: document.getElementById('guide-close-btn'),
  guideDoneBtn: document.getElementById('guide-done-btn'),
  qrModal: document.getElementById('qr-modal'),
  qrSubtitle: document.getElementById('qr-modal-subtitle'),
  qrSvgWrap: document.getElementById('qr-svg-wrap'),
  qrUrlPreview: document.getElementById('qr-url-preview'),
  qrCloseBtn: document.getElementById('qr-close-btn'),
  qrDoneBtn: document.getElementById('qr-done-btn'),
  qrCopyBtn: document.getElementById('qr-copy-url-btn'),
  toast: document.getElementById('live-toast'),
};

let toastTimer = null;

export function showToast(msg) {
  els.toast.textContent = msg;
  els.toast.classList.add('show');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => els.toast.classList.remove('show'), 2600);
}

export async function copyText(text, label) {
  if (!text) return false;
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      showToast(`${label} copied`);
      return true;
    }
  } catch {
    // Fall through to execCommand fallback
  }
  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    ta.style.position = 'fixed';
    ta.style.left = '-9999px';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    if (ok) {
      showToast(`${label} copied`);
      return true;
    }
  } catch {
    // Ignore
  }
  showToast(`Could not copy ${label}`);
  return false;
}

function initTheme() {
  const apply = (pref) => {
    const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
    const effective = pref || (prefersDark ? 'dark' : 'light');
    if (pref) {
      document.documentElement.setAttribute('data-theme', pref);
    } else {
      document.documentElement.removeAttribute('data-theme');
    }
    els.themeToggle.setAttribute('aria-pressed', effective === 'dark' ? 'true' : 'false');
    els.themeIcon.textContent = effective === 'dark' ? '☀' : '☾';
    els.themeLabel.textContent = effective === 'dark' ? 'Light' : 'Dark';
  };

  apply(getThemePreference());

  els.themeToggle.addEventListener('click', () => {
    const currentAttr = document.documentElement.getAttribute('data-theme');
    const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
    const currentEffective = currentAttr || (prefersDark ? 'dark' : 'light');
    const next = currentEffective === 'dark' ? 'light' : 'dark';
    setThemePreference(next);
    apply(next);
  });
}

function formatBytes(bytes) {
  if (typeof bytes !== 'number' || !Number.isFinite(bytes) || bytes <= 0) return null;
  const units = ['B', 'KB', 'MB', 'GB'];
  let idx = 0;
  let val = bytes;
  while (val >= 1024 && idx < units.length - 1) {
    val /= 1024;
    idx += 1;
  }
  return `${val.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

export function formatRelativeTime(iso) {
  if (!iso) return 'Unknown';
  const ts = Date.parse(iso);
  if (Number.isNaN(ts)) return 'Unknown';
  const diffSec = Math.round((Date.now() - ts) / 1000);
  const abs = Math.abs(diffSec);
  if (abs < 60) return 'just now';
  if (abs < 3600) return `${Math.round(abs / 60)}m ago`;
  if (abs < 86400) return `${Math.round(abs / 3600)}h ago`;
  if (abs < 2592000) return `${Math.round(abs / 86400)}d ago`;
  return `${Math.round(abs / 2592000)}mo ago`;
}

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function isValidHttpUrl(url) {
  return typeof url === 'string' && (url.startsWith('https://') || url.startsWith('http://'));
}

function buildItemsFromCatalog(data) {
  const items = [];

  for (const app of data.apps || []) {
    const stock = app.releases?.stock || null;
    const hasStockUrl = stock ? isValidHttpUrl(stock.download_url) : isValidHttpUrl(app.download_url);
    if (stock || app.enabled) {
      const src = stock || app;
      items.push({
        key: `apk:${app.id}`,
        id: app.id,
        name: app.name,
        subtitle: app.package,
        format: 'apk',
        formatLabel: 'Standalone APK',
        categories: app.categories || [],
        sourceRepo: src.source_repo || app.source_repo || app.patch_source || null,
        patchSource: app.patch_source,
        version: src.version || app.version || null,
        releaseTag: src.release_tag || app.release_tag || app.tag || null,
        updatedAt: src.updated_at || app.updated_at || null,
        sizeBytes: src.size_bytes ?? app.size_bytes ?? null,
        sha256: src.sha256 || app.sha256 || null,
        supportedArchs: src.supported_archs || app.supported_archs || [],
        optimizationMode: src.optimization_mode || app.optimization_mode || 'full',
        validationStatus: src.validation_status || app.validation_status || 'unknown',
        downloadUrl: hasStockUrl ? (src.download_url || app.download_url) : null,
        moduleId: null,
        payload: null,
      });
    }

    const mod = app.releases?.module || null;
    if (mod) {
      const moduleId = mod.module_id || `revancex-${app.id}`;
      items.push({
        key: `single_module:${app.id}`,
        id: app.id,
        name: `${app.name} Module`,
        subtitle: `${moduleId} · ${app.package}`,
        format: 'single_module',
        formatLabel: 'Single Root Module',
        categories: app.categories || [],
        sourceRepo: mod.source_repo || app.source_repo || app.patch_source || null,
        patchSource: app.patch_source,
        version: mod.version || app.version || null,
        releaseTag: mod.release_tag || mod.tag || null,
        updatedAt: mod.updated_at || null,
        sizeBytes: mod.size_bytes ?? null,
        sha256: mod.sha256 || null,
        supportedArchs: mod.supported_archs || app.supported_archs || [],
        optimizationMode: mod.optimization_mode || 'full',
        validationStatus: mod.validation_status || 'unknown',
        downloadUrl: isValidHttpUrl(mod.download_url) ? mod.download_url : null,
        moduleId,
        payload: null,
      });
    }
  }

  for (const bundle of [...(data.bundles || []), ...(data.custom_bundles || [])]) {
    const cats = bundle.category === 'all'
      ? ['core', 'multimedia', 'social', 'productivity', 'tools']
      : [bundle.category];
    items.push({
      key: `bundle:${bundle.module_id}`,
      id: bundle.id,
      name: bundle.name,
      subtitle: `${bundle.module_id} · ${bundle.filename}`,
      format: 'bundle',
      formatLabel: 'Multi-App Bundle',
      categories: cats,
      sourceRepo: bundle.sources?.length ? `${bundle.sources.length} upstream sources` : null,
      sourcesList: bundle.sources || [],
      version: bundle.release_tag || bundle.tag || null,
      releaseTag: bundle.release_tag || bundle.tag || null,
      updatedAt: bundle.updated_at || null,
      sizeBytes: bundle.size_bytes ?? null,
      sha256: bundle.sha256 || null,
      supportedArchs: bundle.supported_archs || [],
      optimizationMode: bundle.optimization_mode || 'full',
      validationStatus: bundle.validation_status || 'unknown',
      downloadUrl: isValidHttpUrl(bundle.download_url) ? bundle.download_url : null,
      moduleId: bundle.module_id,
      payload: bundle.payload || [],
    });
  }

  return items;
}

function renderHealth(data) {
  const health = data?.build_health;
  if (!health) return;
  els.healthTag.textContent = health.release_tag || 'Pending';
  els.healthCli.textContent = health.cli_version ? `v${health.cli_version}` : 'Pending';

  if (health.built_at) {
    els.healthBuilt.innerHTML = `<time datetime="${escapeHtml(health.built_at)}" title="${escapeHtml(health.built_at)}">${escapeHtml(formatRelativeTime(health.built_at))}</time>`;
  } else {
    els.healthBuilt.textContent = 'Pending';
  }

  if (typeof health.pass_rate === 'number') {
    els.healthPassLabel.textContent = 'Pass Rate';
    els.healthPass.textContent = `${Math.round(health.pass_rate * 1000) / 10}%`;
  } else {
    const enabledCount = (data.apps || []).filter((a) => a.enabled).length;
    els.healthPassLabel.textContent = 'Configured';
    els.healthPass.textContent = `${enabledCount} apps`;
  }

  const sources = health.active_sources || [];
  els.healthSourcesCount.textContent = `${sources.length} repo${sources.length === 1 ? '' : 's'}`;
  els.healthSourcesList.innerHTML = sources
    .map((s) => `<li>${escapeHtml(s)}</li>`)
    .join('');
}

function syncStateToHash() {
  const params = new URLSearchParams();
  if (state.q) params.set('q', state.q);
  if (state.format !== 'all') params.set('fmt', state.format);
  if (state.category !== 'all') params.set('cat', state.category);
  if (state.arch !== 'all_filter') params.set('arch', state.arch);
  if (state.sort !== 'updated') params.set('sort', state.sort);
  const str = params.toString();
  const nextHash = str ? `#${str}` : '';
  if (window.location.hash !== nextHash) {
    history.replaceState(null, '', nextHash || window.location.pathname + window.location.search);
  }
}

function loadStateFromHash() {
  const raw = window.location.hash.replace(/^#/, '');
  const params = new URLSearchParams(raw);
  state.q = params.get('q') || '';
  state.format = params.get('fmt') || 'all';
  state.category = params.get('cat') || 'all';
  state.arch = params.get('arch') || 'all_filter';
  state.sort = params.get('sort') || 'updated';

  els.searchInput.value = state.q;
  els.categorySelect.value = state.category;
  els.archSelect.value = state.arch;
  els.sortSelect.value = state.sort;
  syncChipButtons();
}

function syncChipButtons() {
  document.querySelectorAll('.chip[data-filter="format"]').forEach((btn) => {
    const active = state.format === btn.dataset.value;
    btn.setAttribute('aria-pressed', active ? 'true' : 'false');
  });
}

function matchesFilters(item) {
  if (state.format !== 'all' && item.format !== state.format) return false;
  if (state.category !== 'all' && !(item.categories || []).includes(state.category)) return false;
  if (state.arch !== 'all_filter') {
    const archs = item.supportedArchs || [];
    if (!archs.includes(state.arch) && !(state.arch !== 'all' && archs.includes('all'))) {
      return false;
    }
  }
  if (state.q) {
    const q = state.q.toLowerCase();
    const hay = [
      item.name,
      item.id,
      item.subtitle,
      item.sourceRepo,
      item.patchSource,
      ...(item.sourcesList || []),
      ...(item.payload || []).flatMap((p) => [p.app, p.package]),
    ]
      .filter(Boolean)
      .join(' ')
      .toLowerCase();
    if (!hay.includes(q)) return false;
  }
  return true;
}

function sortItems(list) {
  return [...list].sort((a, b) => {
    if (state.sort === 'name') {
      return a.name.localeCompare(b.name);
    }
    if (state.sort === 'size') {
      const sa = typeof a.sizeBytes === 'number' ? a.sizeBytes : -1;
      const sb = typeof b.sizeBytes === 'number' ? b.sizeBytes : -1;
      return sb - sa || a.name.localeCompare(b.name);
    }
    const ta = a.updatedAt ? Date.parse(a.updatedAt) : 0;
    const tb = b.updatedAt ? Date.parse(b.updatedAt) : 0;
    return tb - ta || a.name.localeCompare(b.name);
  });
}

function validationBadgeHtml(status) {
  if (!['passed', 'warning', 'failed'].includes(status)) return '';
  const icons = { passed: '✓', warning: '!', failed: '✕' };
  const labels = { passed: 'Verified', warning: 'Warning', failed: 'Failed' };
  return `<span class="badge badge-${status}" title="Validation status: ${labels[status]}"><span aria-hidden="true">${icons[status]}</span> ${labels[status]}</span>`;
}

function formatBadgeClass(fmt) {
  if (fmt === 'apk') return 'badge-format-apk';
  if (fmt === 'single_module') return 'badge-format-module';
  return 'badge-format-bundle';
}

function summarizeArchs(archs) {
  if (!archs || archs.length === 0) return '';
  if (archs.includes('all')) return '<span class="badge mono">Universal</span>';
  if (archs.length === 1) return `<span class="badge mono">${escapeHtml(archs[0])}</span>`;
  return `<span class="badge mono" title="${escapeHtml(archs.join(', '))}">${archs.length} archs</span>`;
}

function renderCatalog() {
  const filtered = sortItems(state.items.filter(matchesFilters));
  const hasFilters = Boolean(
    state.q || state.format !== 'all' || state.category !== 'all' || state.arch !== 'all_filter'
  );
  els.resetBtn.hidden = !hasFilters;
  els.resultsCount.textContent = `Showing ${filtered.length} of ${state.items.length} artifacts`;

  if (filtered.length === 0) {
    els.grid.innerHTML = `
      <div class="state-card">
        <p><strong>No matching artifacts found.</strong></p>
        <p style="margin-top:6px;">Try clearing your search query or adjusting the format, category, or architecture filters.</p>
        <button type="button" class="btn btn-tonal" id="empty-reset-btn" style="margin-top:12px;">Reset all filters</button>
      </div>
    `;
    document.getElementById('empty-reset-btn')?.addEventListener('click', resetFilters);
    return;
  }

  els.grid.innerHTML = filtered
    .map((item) => {
      const verBadge = item.version
        ? `<span class="badge mono">${escapeHtml(item.version.startsWith('v') || item.version.startsWith('auto-') ? item.version : `v${item.version}`)}</span>`
        : '';
      const archBadge = summarizeArchs(item.supportedArchs);
      const sizeStr = formatBytes(item.sizeBytes);
      const relTime = item.updatedAt
        ? `<time datetime="${escapeHtml(item.updatedAt)}" title="${escapeHtml(item.updatedAt)}">${escapeHtml(formatRelativeTime(item.updatedAt))}</time>`
        : null;

      const metaParts = [];
      if (item.sourceRepo) {
        metaParts.push(`<span class="meta-item">Source: <strong class="mono">${escapeHtml(item.sourceRepo)}</strong></span>`);
      }
      if (sizeStr) {
        metaParts.push(`<span class="meta-item">Size: <strong>${escapeHtml(sizeStr)}</strong></span>`);
      }
      if (relTime) {
        metaParts.push(`<span class="meta-item">Updated: <strong>${relTime}</strong></span>`);
      }
      if (item.optimizationMode && item.optimizationMode !== 'full') {
        metaParts.push(`<span class="meta-item">Mode: <strong>${escapeHtml(item.optimizationMode)}</strong></span>`);
      }

      const payloadHtml = item.payload && item.payload.length
        ? `
          <details class="payload-drawer">
            <summary>Payload APKs (${item.payload.length})</summary>
            <ul class="payload-list">
              ${item.payload
                .map(
                  (p) => `
                  <li>
                    <span><strong>${escapeHtml(p.app)}</strong> ${p.version ? `v${escapeHtml(p.version)}` : ''}</span>
                    <span class="payload-path">${escapeHtml(p.path)}</span>
                  </li>
                `
                )
                .join('')}
            </ul>
          </details>
        `
        : '';

      const downloadBtn = item.downloadUrl
        ? `<a class="btn btn-primary" href="${escapeHtml(item.downloadUrl)}" target="_blank" rel="noopener">Download</a>`
        : `<button type="button" class="btn btn-primary" disabled title="No release asset published yet">Unavailable</button>`;

      const qrBtn = item.downloadUrl
        ? `<button type="button" class="btn" data-action="qr" data-key="${escapeHtml(item.key)}" title="Show QR code for mobile download">QR</button>`
        : '';

      const shaBtn = item.sha256
        ? `<button type="button" class="btn" data-action="sha" data-key="${escapeHtml(item.key)}">Copy SHA-256</button>`
        : '';

      const updateJsonBtn = (item.format === 'single_module' || item.format === 'bundle') && item.moduleId
        ? `<button type="button" class="btn btn-tonal" data-action="ota" data-key="${escapeHtml(item.key)}" title="Copy KernelSU / Magisk OTA update JSON URL">Copy OTA URL</button>`
        : '';

      const isRoot = item.format === 'single_module' || item.format === 'bundle';

      return `
        <article class="card ${isRoot ? 'card-root' : ''}" data-key="${escapeHtml(item.key)}">
          <div class="card-head">
            <div class="card-title-wrap">
              <h2 class="card-title">${escapeHtml(item.name)}</h2>
              <p class="card-pkg mono">${escapeHtml(item.subtitle)}</p>
            </div>
            ${validationBadgeHtml(item.validationStatus)}
          </div>

          <div class="badge-row">
            <span class="badge ${formatBadgeClass(item.format)}">${escapeHtml(item.formatLabel)}</span>
            ${verBadge}
            ${archBadge}
          </div>

          ${metaParts.length ? `<div class="meta-row">${metaParts.join('')}</div>` : ''}

          ${payloadHtml}

          <div class="card-actions">
            ${downloadBtn}
            ${qrBtn}
            ${shaBtn}
            ${updateJsonBtn}
          </div>
        </article>
      `;
    })
    .join('');
}

function resetFilters() {
  state.q = '';
  state.format = 'all';
  state.category = 'all';
  state.arch = 'all_filter';
  state.sort = 'updated';
  els.searchInput.value = '';
  els.categorySelect.value = 'all';
  els.archSelect.value = 'all_filter';
  els.sortSelect.value = 'updated';
  syncChipButtons();
  syncStateToHash();
  renderCatalog();
}

function trapFocus(modalEl, e) {
  if (e.key !== 'Tab') return;
  const focusable = Array.from(
    modalEl.querySelectorAll('button, [href], input, select, summary, [tabindex]:not([tabindex="-1"])')
  ).filter((el) => !el.disabled && !el.hidden && el.offsetParent !== null);
  if (focusable.length === 0) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}

function openQrModal(item) {
  if (!item?.downloadUrl) return;
  state.lastFocused = document.activeElement;
  state.currentQrUrl = item.downloadUrl;

  const qr = qrcode(0, 'M');
  qr.addData(item.downloadUrl);
  qr.make();
  els.qrSvgWrap.innerHTML = qr.createSvgTag({ cellSize: 4, margin: 2, scalable: true });
  els.qrSubtitle.textContent = item.name;
  els.qrUrlPreview.textContent = item.downloadUrl;
  els.qrModal.hidden = false;
  els.qrCloseBtn.focus();
}

function closeQrModal() {
  if (els.qrModal.hidden) return;
  els.qrModal.hidden = true;
  if (state.lastFocused && typeof state.lastFocused.focus === 'function') {
    state.lastFocused.focus();
  }
}

function openGuideModal() {
  state.lastFocused = document.activeElement;
  els.guideModal.hidden = false;
  els.guideCloseBtn.focus();
}

function closeGuideModal() {
  if (els.guideModal.hidden) return;
  els.guideModal.hidden = true;
  if (state.lastFocused && typeof state.lastFocused.focus === 'function') {
    state.lastFocused.focus();
  }
}

function bindModalEvents() {
  els.qrCloseBtn.addEventListener('click', closeQrModal);
  els.qrDoneBtn.addEventListener('click', closeQrModal);
  els.qrCopyBtn.addEventListener('click', () => copyText(state.currentQrUrl, 'Download URL'));
  els.qrModal.addEventListener('click', (e) => {
    if (e.target === els.qrModal) closeQrModal();
  });

  els.openGuideBtn.addEventListener('click', openGuideModal);
  els.guideCloseBtn.addEventListener('click', closeGuideModal);
  els.guideDoneBtn.addEventListener('click', closeGuideModal);
  els.guideModal.addEventListener('click', (e) => {
    if (e.target === els.guideModal) closeGuideModal();
  });

  document.addEventListener('keydown', (e) => {
    if (!els.qrModal.hidden) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeQrModal();
        return;
      }
      trapFocus(els.qrModal, e);
      return;
    }
    if (!els.guideModal.hidden) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeGuideModal();
        return;
      }
      trapFocus(els.guideModal, e);
    }
  });
}

function bindEvents() {
  els.searchInput.addEventListener('input', (e) => {
    state.q = e.target.value.trim();
    syncStateToHash();
    renderCatalog();
  });

  els.categorySelect.addEventListener('change', (e) => {
    state.category = e.target.value;
    syncStateToHash();
    renderCatalog();
  });

  els.archSelect.addEventListener('change', (e) => {
    state.arch = e.target.value;
    syncStateToHash();
    renderCatalog();
  });

  els.sortSelect.addEventListener('change', (e) => {
    state.sort = e.target.value;
    syncStateToHash();
    renderCatalog();
  });

  document.querySelectorAll('.chip[data-filter="format"]').forEach((btn) => {
    btn.addEventListener('click', () => {
      state.format = btn.dataset.value;
      syncChipButtons();
      syncStateToHash();
      renderCatalog();
    });
  });

  document.addEventListener('keydown', (e) => {
    if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey) return;
    const tag = document.activeElement?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
    if (!els.qrModal.hidden || !els.guideModal.hidden) return;
    if (!document.getElementById('composer-modal').hidden) return;
    if (!document.getElementById('tracker-modal').hidden) return;
    e.preventDefault();
    els.searchInput.focus();
  });

  els.resetBtn.addEventListener('click', resetFilters);
  window.addEventListener('hashchange', () => {
    loadStateFromHash();
    renderCatalog();
  });

  els.grid.addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-action]');
    if (!btn) return;
    const item = state.items.find((i) => i.key === btn.dataset.key);
    if (!item) return;

    if (btn.dataset.action === 'qr') {
      openQrModal(item);
    } else if (btn.dataset.action === 'sha') {
      copyText(item.sha256, 'SHA-256');
    } else if (btn.dataset.action === 'ota' && item.moduleId) {
      copyText(`${OTA_BASE}/${item.moduleId}.json`, 'OTA update URL');
    }
  });
}

export async function loadCatalog() {
  els.statusBox.hidden = true;
  try {
    const res = await fetch('apps.json', { cache: 'no-cache' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    state.catalog = data;
    state.items = buildItemsFromCatalog(data);
    renderHealth(data);
    renderCatalog();
  } catch (err) {
    els.resultsCount.textContent = 'Catalogue unavailable';
    els.grid.innerHTML = '';
    els.statusBox.hidden = false;
    els.statusBox.innerHTML = `
      <p><strong>Could not load static catalogue (apps.json).</strong></p>
      <p style="margin-top:6px;">${escapeHtml(err.message)}</p>
      <button type="button" class="btn btn-primary" id="retry-load-btn" style="margin-top:12px;">Retry</button>
    `;
    document.getElementById('retry-load-btn')?.addEventListener('click', loadCatalog);
  }
}

initTheme();
loadStateFromHash();
bindEvents();
bindModalEvents();
const tracker = initTracker({
  getCatalog: () => state.catalog,
  formatRelativeTime,
});
initComposer({
  getCatalog: () => state.catalog,
  copyText,
  showToast,
  openTracker: tracker.openTracker,
});
loadCatalog();
