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
  healthPass: document.getElementById('health-pass'),
  healthSourcesCount: document.getElementById('health-sources-count'),
  healthSourcesList: document.getElementById('health-sources-list'),
  searchInput: document.getElementById('search-input'),
  sortSelect: document.getElementById('sort-select'),
  resultsCount: document.getElementById('results-count'),
  resetBtn: document.getElementById('reset-filters-btn'),
  grid: document.getElementById('catalog-grid'),
  statusBox: document.getElementById('catalog-status'),
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
  if (typeof bytes !== 'number' || !Number.isFinite(bytes) || bytes <= 0) return '—';
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
  if (!iso) return '—';
  const ts = Date.parse(iso);
  if (Number.isNaN(ts)) return '—';
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

function renderHealth(health) {
  if (!health) return;
  els.healthTag.textContent = health.release_tag || 'No auto release yet';
  els.healthCli.textContent = health.cli_version ? `v${health.cli_version}` : '—';

  if (health.built_at) {
    els.healthBuilt.innerHTML = `<time datetime="${escapeHtml(health.built_at)}" title="${escapeHtml(health.built_at)}">${escapeHtml(formatRelativeTime(health.built_at))}</time>`;
  } else {
    els.healthBuilt.textContent = '—';
  }

  if (typeof health.pass_rate === 'number') {
    els.healthPass.textContent = `${Math.round(health.pass_rate * 1000) / 10}%`;
  } else {
    els.healthPass.textContent = 'N/A (static registry)';
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
  els.sortSelect.value = state.sort;
  syncChipButtons();
}

function syncChipButtons() {
  document.querySelectorAll('.chip[data-filter]').forEach((btn) => {
    const kind = btn.dataset.filter;
    const val = btn.dataset.value;
    const active = state[kind] === val;
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
  const safe = ['passed', 'warning', 'failed', 'unknown'].includes(status) ? status : 'unknown';
  const icons = { passed: '✓', warning: '!', failed: '✕', unknown: '?' };
  return `<span class="badge badge-${safe}" title="Validation status: ${safe}">${icons[safe]} ${safe}</span>`;
}

function formatBadgeClass(fmt) {
  if (fmt === 'apk') return 'badge-format-apk';
  if (fmt === 'single_module') return 'badge-format-module';
  return 'badge-format-bundle';
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
      const srcBadge = item.sourceRepo
        ? `<span class="badge">${escapeHtml(item.sourceRepo)}</span>`
        : '';
      const archBadges = (item.supportedArchs || [])
        .map((a) => `<span class="badge mono">${escapeHtml(a)}</span>`)
        .join('');
      const relTime = item.updatedAt
        ? `<time datetime="${escapeHtml(item.updatedAt)}" title="${escapeHtml(item.updatedAt)}">${escapeHtml(formatRelativeTime(item.updatedAt))}</time>`
        : '—';

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
        ? `<button type="button" class="btn" data-action="qr" data-key="${escapeHtml(item.key)}">QR</button>`
        : '';

      const shaBtn = item.sha256
        ? `<button type="button" class="btn" data-action="sha" data-key="${escapeHtml(item.key)}">Copy SHA-256</button>`
        : `<button type="button" class="btn" disabled title="SHA-256 unavailable for legacy registry entry">SHA-256 —</button>`;

      const updateJsonBtn = (item.format === 'single_module' || item.format === 'bundle') && item.moduleId
        ? `<button type="button" class="btn btn-tonal" data-action="ota" data-key="${escapeHtml(item.key)}">Copy updateJson</button>`
        : '';

      return `
        <article class="card" data-key="${escapeHtml(item.key)}">
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
            ${srcBadge}
            ${archBadges}
            <span class="badge">${escapeHtml(item.optimizationMode)}</span>
          </div>

          <div class="meta-row">
            <span class="meta-item">Size: <strong>${escapeHtml(formatBytes(item.sizeBytes))}</strong></span>
            <span class="meta-item">Updated: <strong>${relTime}</strong></span>
          </div>

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
  els.sortSelect.value = 'updated';
  syncChipButtons();
  syncStateToHash();
  renderCatalog();
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

function bindModalEvents() {
  els.qrCloseBtn.addEventListener('click', closeQrModal);
  els.qrDoneBtn.addEventListener('click', closeQrModal);
  els.qrCopyBtn.addEventListener('click', () => copyText(state.currentQrUrl, 'Download URL'));
  els.qrModal.addEventListener('click', (e) => {
    if (e.target === els.qrModal) closeQrModal();
  });

  document.addEventListener('keydown', (e) => {
    if (els.qrModal.hidden) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      closeQrModal();
      return;
    }
    if (e.key === 'Tab') {
      const focusable = Array.from(
        els.qrModal.querySelectorAll('button, [href], input, select, [tabindex]:not([tabindex="-1"])')
      ).filter((el) => !el.disabled && !el.hidden);
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
  });
}

function bindEvents() {
  els.searchInput.addEventListener('input', (e) => {
    state.q = e.target.value.trim();
    syncStateToHash();
    renderCatalog();
  });

  els.sortSelect.addEventListener('change', (e) => {
    state.sort = e.target.value;
    syncStateToHash();
    renderCatalog();
  });

  document.querySelectorAll('.chip[data-filter]').forEach((btn) => {
    btn.addEventListener('click', () => {
      state[btn.dataset.filter] = btn.dataset.value;
      syncChipButtons();
      syncStateToHash();
      renderCatalog();
    });
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
      copyText(`${OTA_BASE}/${item.moduleId}.json`, 'updateJson URL');
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
    renderHealth(data.build_health);
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
