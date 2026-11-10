const SESSION_TOKEN_KEY = 'revancex_gh_pat';
const CATEGORY_ORDER = ['core', 'multimedia', 'social', 'productivity', 'tools', 'other'];
const CATEGORY_LABELS = {
  core: 'Core',
  multimedia: 'Multimedia',
  social: 'Social',
  productivity: 'Productivity',
  tools: 'Tools',
  other: 'Other Apps',
};

function detectRepoSlug() {
  const hostMatch = window.location.hostname.match(/^(.+)\.github\.io$/);
  const pathRepo = window.location.pathname.split('/').filter(Boolean)[0];
  if (hostMatch && pathRepo) {
    return `${hostMatch[1]}/${pathRepo}`;
  }
  return 'thunderkex/revancex';
}

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function groupAppsByCategory(apps) {
  const groups = new Map(CATEGORY_ORDER.map((c) => [c, []]));
  for (const app of apps) {
    if (!app.enabled) continue;
    const cats = (app.categories || []).filter((c) => c !== 'all');
    if (cats.length === 0) {
      groups.get('other').push(app);
    } else {
      for (const c of cats) {
        if (!groups.has(c)) groups.set(c, []);
        groups.get(c).push(app);
      }
    }
  }
  return groups;
}

export function initComposer({ getCatalog, copyText, showToast, openTracker }) {
  const openBtn = document.getElementById('open-composer-btn');
  const modal = document.getElementById('composer-modal');
  const closeBtn = document.getElementById('composer-close-btn');
  const groupsEl = document.getElementById('composer-app-groups');
  const selectAllBtn = document.getElementById('composer-select-all');
  const deselectAllBtn = document.getElementById('composer-deselect-all');
  const archSelect = document.getElementById('composer-arch');
  const modeSelect = document.getElementById('composer-mode');
  const packagingSelect = document.getElementById('composer-packaging');
  const webuiCheck = document.getElementById('composer-webui');
  const constraintNote = document.getElementById('composer-constraint-note');
  const selectedSummary = document.getElementById('composer-selected-summary');
  const localCmdEl = document.getElementById('composer-local-cmd');
  const copyCmdBtn = document.getElementById('composer-copy-cmd');
  const tokenInput = document.getElementById('composer-token');
  const clearTokenBtn = document.getElementById('composer-clear-token');
  const dispatchBtn = document.getElementById('composer-dispatch-btn');
  const cloudStatus = document.getElementById('composer-cloud-status');
  const fallbackLink = document.getElementById('composer-fallback-link');
  const tabBtns = modal.querySelectorAll('[data-composer-tab]');
  const localPanel = document.getElementById('composer-panel-local');
  const cloudPanel = document.getElementById('composer-panel-cloud');

  const repoSlug = detectRepoSlug();
  fallbackLink.href = `https://github.com/${repoSlug}/actions/workflows/custom-build.yml`;

  const selected = new Set(['youtube', 'youtube_music', 'microg']);
  let lastFocused = null;
  let rendered = false;

  try {
    const saved = window.sessionStorage.getItem(SESSION_TOKEN_KEY);
    if (saved) tokenInput.value = saved;
  } catch {
    // Ignore sessionStorage restrictions
  }

  function getAppsList() {
    return (getCatalog()?.apps || []).filter((a) => a.enabled);
  }

  function enforceConstraints() {
    const apps = [...selected];
    const onlyMicrog = apps.length > 0 && apps.every((id) => id === 'microg');
    const bundleOpt = packagingSelect.querySelector('option[value="bundle"]');
    const singleOpt = packagingSelect.querySelector('option[value="single_module"]');

    if (onlyMicrog) {
      bundleOpt.disabled = true;
      singleOpt.disabled = true;
      if (packagingSelect.value !== 'apk') {
        packagingSelect.value = 'apk';
      }
      constraintNote.textContent = 'MicroG is standalone APK only — module packaging disabled.';
      constraintNote.hidden = false;
    } else {
      bundleOpt.disabled = false;
      singleOpt.disabled = false;
      if (packagingSelect.value === 'apk') {
        constraintNote.textContent = 'Embedded WebUI applies only to Root Modules and Bundles.';
        constraintNote.hidden = false;
      } else {
        constraintNote.hidden = true;
      }
    }

    const isApkOnly = packagingSelect.value === 'apk';
    webuiCheck.disabled = isApkOnly;
    if (isApkOnly) {
      webuiCheck.checked = false;
    }
  }

  function buildLocalCommand() {
    enforceConstraints();
    const apps = [...selected].sort();
    const appsArg = apps.join(',');
    const arch = archSelect.value;
    const mode = modeSelect.value;
    const packaging = packagingSelect.value;
    const includeWebui = !webuiCheck.disabled && webuiCheck.checked;

    selectedSummary.textContent = `${apps.length} app${apps.length === 1 ? '' : 's'} selected`;
    dispatchBtn.disabled = apps.length === 0;

    if (apps.length === 0) {
      localCmdEl.textContent = '# Select at least one app to generate CLI commands.';
      return;
    }

    const buildLine = `./revancex build --apps "${appsArg}" --arch ${arch} --mode ${mode} --output ./output`;
    if (packaging === 'apk') {
      localCmdEl.textContent = buildLine;
      return;
    }

    const moduleApps = apps.filter((id) => id !== 'microg').join(',');
    const webuiFlag = includeWebui ? '--include-webui' : '--no-webui';
    const pkgFlag = packaging === 'single_module' ? '--single' : '--bundle';
    const modLine = `./revancex module ${pkgFlag} --apps "${moduleApps || appsArg}" ${webuiFlag} --output ./output`;
    localCmdEl.textContent = `${buildLine}\n${modLine}`;
  }

  function renderAppGroups() {
    const apps = getAppsList();
    if (apps.length === 0) {
      groupsEl.innerHTML = '<p class="hint-text">Catalogue apps not loaded yet.</p>';
      return;
    }

    const groups = groupAppsByCategory(apps);
    const seenInCategory = new Set();
    let html = '';

    for (const [cat, list] of groups.entries()) {
      if (list.length === 0) continue;
      html += `
        <fieldset class="composer-cat">
          <legend>${escapeHtml(CATEGORY_LABELS[cat] || cat)}</legend>
          <div class="composer-chip-grid">
            ${list
              .map((app) => {
                const isSelected = selected.has(app.id);
                const dupNote = seenInCategory.has(app.id);
                seenInCategory.add(app.id);
                const deps = (app.dependencies || []).length
                  ? ` (+${app.dependencies.join(', ')})`
                  : '';
                return `
                  <label class="app-pick ${isSelected ? 'is-selected' : ''}" data-app-pick="${escapeHtml(app.id)}">
                    <input type="checkbox" value="${escapeHtml(app.id)}" ${isSelected ? 'checked' : ''} ${dupNote ? 'data-dup="1"' : ''}>
                    <span>
                      <strong>${escapeHtml(app.name)}</strong>
                      <small class="mono">${escapeHtml(app.id)}${escapeHtml(deps)}</small>
                    </span>
                  </label>
                `;
              })
              .join('')}
          </div>
        </fieldset>
      `;
    }

    groupsEl.innerHTML = html;
    rendered = true;
    buildLocalCommand();
  }

  function syncCheckboxes() {
    groupsEl.querySelectorAll('label[data-app-pick]').forEach((lbl) => {
      const id = lbl.dataset.appPick;
      const checked = selected.has(id);
      lbl.classList.toggle('is-selected', checked);
      const cb = lbl.querySelector('input[type="checkbox"]');
      if (cb) cb.checked = checked;
    });
  }

  function openModal() {
    lastFocused = document.activeElement;
    if (!rendered) renderAppGroups();
    syncCheckboxes();
    buildLocalCommand();
    modal.hidden = false;
    closeBtn.focus();
  }

  function closeModal() {
    if (modal.hidden) return;
    modal.hidden = true;
    if (lastFocused && typeof lastFocused.focus === 'function') {
      lastFocused.focus();
    }
  }

  openBtn.addEventListener('click', openModal);
  closeBtn.addEventListener('click', closeModal);
  modal.addEventListener('click', (e) => {
    if (e.target === modal) closeModal();
  });

  document.addEventListener('keydown', (e) => {
    if (modal.hidden) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      closeModal();
      return;
    }
    if (e.key === 'Tab') {
      const focusable = Array.from(
        modal.querySelectorAll('button, [href], input, select, [tabindex]:not([tabindex="-1"])')
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
  });

  groupsEl.addEventListener('change', (e) => {
    const cb = e.target;
    if (cb.type !== 'checkbox') return;
    const id = cb.value;
    if (cb.checked) {
      selected.add(id);
      const appObj = getAppsList().find((a) => a.id === id);
      for (const dep of appObj?.dependencies || []) {
        selected.add(dep);
      }
    } else {
      selected.delete(id);
    }
    syncCheckboxes();
    buildLocalCommand();
  });

  selectAllBtn.addEventListener('click', () => {
    for (const app of getAppsList()) selected.add(app.id);
    syncCheckboxes();
    buildLocalCommand();
  });

  deselectAllBtn.addEventListener('click', () => {
    selected.clear();
    syncCheckboxes();
    buildLocalCommand();
  });

  [archSelect, modeSelect, packagingSelect, webuiCheck].forEach((el) => {
    el.addEventListener('change', buildLocalCommand);
  });

  copyCmdBtn.addEventListener('click', () => {
    copyText(localCmdEl.textContent, 'CLI command');
  });

  tabBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const target = btn.dataset.composerTab;
      tabBtns.forEach((b) => b.setAttribute('aria-pressed', b === btn ? 'true' : 'false'));
      localPanel.hidden = target !== 'local';
      cloudPanel.hidden = target !== 'cloud';
    });
  });

  tokenInput.addEventListener('input', () => {
    try {
      const val = tokenInput.value.trim();
      if (val) {
        window.sessionStorage.setItem(SESSION_TOKEN_KEY, val);
      } else {
        window.sessionStorage.removeItem(SESSION_TOKEN_KEY);
      }
    } catch {
      // Ignore
    }
  });

  clearTokenBtn.addEventListener('click', () => {
    tokenInput.value = '';
    try {
      window.sessionStorage.removeItem(SESSION_TOKEN_KEY);
    } catch {
      // Ignore
    }
    showToast('Token cleared from sessionStorage');
  });

  dispatchBtn.addEventListener('click', async () => {
    const token = tokenInput.value.trim();
    if (!token) {
      cloudStatus.hidden = false;
      cloudStatus.className = 'cloud-status is-error';
      cloudStatus.textContent = 'Enter a GitHub fine-grained PAT (Actions: write) or use the no-token Run workflow link below.';
      return;
    }

    const apps = [...selected].sort().join(',');
    const arch = archSelect.value;
    const mode = modeSelect.value;
    const packaging = packagingSelect.value;
    const includeWebui = !webuiCheck.disabled && webuiCheck.checked;

    dispatchBtn.disabled = true;
    dispatchBtn.textContent = 'Dispatching…';
    cloudStatus.hidden = true;

    try {
      const res = await fetch(
        `https://api.github.com/repos/${repoSlug}/actions/workflows/custom-build.yml/dispatches`,
        {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${token}`,
            Accept: 'application/vnd.github+json',
            'Content-Type': 'application/json',
            'X-GitHub-Api-Version': '2022-11-28',
          },
          body: JSON.stringify({
            ref: 'extended',
            inputs: {
              apps,
              arch,
              mode,
              packaging,
              include_webui: String(includeWebui),
            },
          }),
        }
      );

      if (res.status === 204) {
        cloudStatus.hidden = false;
        cloudStatus.className = 'cloud-status is-success';
        cloudStatus.innerHTML = `
          <span>Workflow dispatched for <strong>${escapeHtml(apps)}</strong> (${escapeHtml(arch)}, ${escapeHtml(mode)}, ${escapeHtml(packaging)}).</span>
          <button type="button" class="btn btn-tonal" id="composer-open-tracker-btn">Open Build Tracker</button>
        `;
        document.getElementById('composer-open-tracker-btn')?.addEventListener('click', () => {
          closeModal();
          openTracker();
        });
      } else {
        const body = await res.text();
        cloudStatus.hidden = false;
        cloudStatus.className = 'cloud-status is-error';
        cloudStatus.textContent = `GitHub API ${res.status}: ${body || res.statusText}`;
      }
    } catch (err) {
      cloudStatus.hidden = false;
      cloudStatus.className = 'cloud-status is-error';
      cloudStatus.textContent = `Network error: ${err.message}`;
    } finally {
      dispatchBtn.disabled = selected.size === 0;
      dispatchBtn.textContent = 'Dispatch Custom Build';
    }
  });
}
