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

function groupAppsByPrimaryCategory(apps) {
  const groups = new Map(CATEGORY_ORDER.map((c) => [c, []]));
  const seen = new Set();

  for (const app of apps) {
    if (!app.enabled || seen.has(app.id)) continue;
    seen.add(app.id);
    const cats = (app.categories || []).filter((c) => c !== 'all');
    const primary = cats[0] || 'other';
    if (!groups.has(primary)) groups.set(primary, []);
    groups.get(primary).push(app);
  }
  return groups;
}

function formatDispatchError(status, body, fallbackText) {
  if (status === 401) {
    return 'Token rejected (401 Unauthorized). Check that your PAT is valid and has not expired.';
  }
  if (status === 403) {
    return 'Permission denied or rate-limited (403). Ensure your Fine-Grained PAT grants "Actions: Read and write" on this repository.';
  }
  if (status === 404) {
    return 'Workflow not found (404). Verify that custom-build.yml exists on branch "extended" and your PAT can access this repo.';
  }
  if (status === 422) {
    return `GitHub rejected the workflow inputs (422). ${body || 'Check selected app IDs and options.'}`;
  }
  return `GitHub API error (${status}): ${body || fallbackText || 'Unknown error'}`;
}

export function initComposer({ getCatalog, copyText, showToast, openTracker }) {
  const openBtn = document.getElementById('open-composer-btn');
  const modal = document.getElementById('composer-modal');
  const closeBtn = document.getElementById('composer-close-btn');
  const doneBtn = document.getElementById('composer-done-btn');
  const groupsEl = document.getElementById('composer-app-groups');
  const appSearchInput = document.getElementById('composer-app-search');
  const selectedChipsEl = document.getElementById('composer-selected-chips');
  const presetYtBtn = document.getElementById('composer-preset-yt');
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
  const confirmBox = document.getElementById('composer-confirm-box');
  const confirmSummary = document.getElementById('composer-confirm-summary');
  const confirmCancelBtn = document.getElementById('composer-confirm-cancel');
  const confirmSubmitBtn = document.getElementById('composer-confirm-submit');
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

  function syncDispatchButton() {
    const hasToken = tokenInput.value.trim().length > 0;
    dispatchBtn.disabled = selected.size === 0 || !hasToken;
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
      constraintNote.textContent = 'MicroG is standalone APK only; module packaging is disabled.';
      constraintNote.hidden = false;
    } else {
      bundleOpt.disabled = false;
      singleOpt.disabled = false;
      if (packagingSelect.value === 'apk') {
        constraintNote.textContent = 'Embedded Root WebUI applies only to Root Modules and Bundles.';
        constraintNote.hidden = false;
      } else if (packagingSelect.value === 'single_module' && apps.filter((id) => id !== 'microg').length > 1) {
        constraintNote.textContent = 'Single Root Module builds one separate module zip for each selected eligible app.';
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

  function renderSelectedChips() {
    const appsById = new Map(getAppsList().map((a) => [a.id, a]));
    const ids = [...selected].sort();
    if (ids.length === 0) {
      selectedChipsEl.innerHTML = '<span class="hint-text">No apps selected yet.</span>';
      return;
    }

    selectedChipsEl.innerHTML = ids
      .map((id) => {
        const name = appsById.get(id)?.name || id;
        return `
          <button type="button" class="selected-pill" data-remove-app="${escapeHtml(id)}" aria-label="Remove ${escapeHtml(name)}">
            <span>${escapeHtml(name)}</span>
            <span aria-hidden="true">&times;</span>
          </button>
        `;
      })
      .join('');
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
    renderSelectedChips();
    syncDispatchButton();

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

  function filterAppGroups() {
    const q = (appSearchInput.value || '').trim().toLowerCase();
    groupsEl.querySelectorAll('.composer-cat').forEach((fieldset) => {
      let visibleCount = 0;
      fieldset.querySelectorAll('label[data-app-pick]').forEach((lbl) => {
        const hay = lbl.dataset.search || '';
        const match = !q || hay.includes(q);
        lbl.hidden = !match;
        if (match) visibleCount += 1;
      });
      fieldset.hidden = visibleCount === 0;
    });
  }

  function renderAppGroups() {
    const apps = getAppsList();
    if (apps.length === 0) {
      groupsEl.innerHTML = '<p class="hint-text">Catalogue apps are still loading. Close and retry once loaded.</p>';
      return;
    }

    const groups = groupAppsByPrimaryCategory(apps);
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
                const deps = (app.dependencies || []).length
                  ? ` (+${app.dependencies.join(', ')})`
                  : '';
                const searchStr = `${app.name} ${app.id} ${app.package || ''}`.toLowerCase();
                return `
                  <label class="app-pick ${isSelected ? 'is-selected' : ''}" data-app-pick="${escapeHtml(app.id)}" data-search="${escapeHtml(searchStr)}">
                    <input type="checkbox" value="${escapeHtml(app.id)}" ${isSelected ? 'checked' : ''}>
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
    filterAppGroups();
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

  function activateTab(tabBtn) {
    const target = tabBtn.dataset.composerTab;
    tabBtns.forEach((b) => {
      const active = b === tabBtn;
      b.setAttribute('aria-selected', active ? 'true' : 'false');
      b.tabIndex = active ? 0 : -1;
    });
    localPanel.hidden = target !== 'local';
    cloudPanel.hidden = target !== 'cloud';
    confirmBox.hidden = true;
  }

  function openModal() {
    lastFocused = document.activeElement;
    if (!rendered || getAppsList().length > 0) renderAppGroups();
    syncCheckboxes();
    buildLocalCommand();
    confirmBox.hidden = true;
    modal.hidden = false;
    closeBtn.focus();
  }

  function closeModal() {
    if (modal.hidden) return;
    modal.hidden = true;
    confirmBox.hidden = true;
    if (lastFocused && typeof lastFocused.focus === 'function') {
      lastFocused.focus();
    }
  }

  openBtn.addEventListener('click', openModal);
  closeBtn.addEventListener('click', closeModal);
  doneBtn.addEventListener('click', closeModal);
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
        modal.querySelectorAll('button, [href], input, select, summary, [tabindex]:not([tabindex="-1"])')
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

  appSearchInput.addEventListener('input', filterAppGroups);

  selectedChipsEl.addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-remove-app]');
    if (!btn) return;
    selected.delete(btn.dataset.removeApp);
    syncCheckboxes();
    buildLocalCommand();
  });

  groupsEl.addEventListener('change', (e) => {
    const cb = e.target;
    if (cb.type !== 'checkbox') return;
    const id = cb.value;
    if (cb.checked) {
      selected.add(id);
      const appObj = getAppsList().find((a) => a.id === id);
      const addedDeps = [];
      for (const dep of appObj?.dependencies || []) {
        if (!selected.has(dep)) addedDeps.push(dep);
        selected.add(dep);
      }
      if (addedDeps.length) {
        showToast(`Added required dependency: ${addedDeps.join(', ')}`);
      }
    } else {
      selected.delete(id);
      const dependents = getAppsList()
        .filter((a) => selected.has(a.id) && (a.dependencies || []).includes(id))
        .map((a) => a.name);
      if (dependents.length) {
        showToast(`Note: ${dependents.join(', ')} list ${id} as a dependency`);
      }
    }
    syncCheckboxes();
    buildLocalCommand();
  });

  presetYtBtn.addEventListener('click', () => {
    selected.clear();
    ['youtube', 'youtube_music', 'microg'].forEach((id) => selected.add(id));
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
    el.addEventListener('change', () => {
      confirmBox.hidden = true;
      buildLocalCommand();
    });
  });

  copyCmdBtn.addEventListener('click', () => {
    copyText(localCmdEl.textContent, 'CLI command');
  });

  tabBtns.forEach((btn, idx) => {
    btn.addEventListener('click', () => activateTab(btn));
    btn.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
      e.preventDefault();
      const next = e.key === 'ArrowRight'
        ? tabBtns[(idx + 1) % tabBtns.length]
        : tabBtns[(idx - 1 + tabBtns.length) % tabBtns.length];
      activateTab(next);
      next.focus();
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
    syncDispatchButton();
  });

  clearTokenBtn.addEventListener('click', () => {
    tokenInput.value = '';
    confirmBox.hidden = true;
    try {
      window.sessionStorage.removeItem(SESSION_TOKEN_KEY);
    } catch {
      // Ignore
    }
    syncDispatchButton();
    showToast('Token cleared from sessionStorage');
  });

  dispatchBtn.addEventListener('click', () => {
    const token = tokenInput.value.trim();
    if (!token) {
      cloudStatus.hidden = false;
      cloudStatus.className = 'cloud-status is-error';
      cloudStatus.textContent = 'Enter a GitHub Fine-Grained PAT (Actions: Read and write) or use "Run on GitHub (no token)".';
      return;
    }

    const apps = [...selected].sort().join(', ');
    const arch = archSelect.value;
    const mode = modeSelect.value;
    const packaging = packagingSelect.value;
    const includeWebui = !webuiCheck.disabled && webuiCheck.checked;
    confirmSummary.textContent = `Dispatch custom-build.yml on branch "extended" for ${selected.size} app(s) (${apps}) — ${arch}, ${mode}, ${packaging}${includeWebui ? ', with WebUI' : ''}?`;
    confirmBox.hidden = false;
    confirmSubmitBtn.focus();
  });

  confirmCancelBtn.addEventListener('click', () => {
    confirmBox.hidden = true;
    dispatchBtn.focus();
  });

  confirmSubmitBtn.addEventListener('click', async () => {
    const token = tokenInput.value.trim();
    if (!token || selected.size === 0) {
      confirmBox.hidden = true;
      syncDispatchButton();
      return;
    }

    const apps = [...selected].sort().join(',');
    const arch = archSelect.value;
    const mode = modeSelect.value;
    const packaging = packagingSelect.value;
    const includeWebui = !webuiCheck.disabled && webuiCheck.checked;

    confirmBox.hidden = true;
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
          openTracker({ force: true });
        });
      } else {
        const body = await res.text();
        cloudStatus.hidden = false;
        cloudStatus.className = 'cloud-status is-error';
        cloudStatus.textContent = formatDispatchError(res.status, body, res.statusText);
      }
    } catch (err) {
      cloudStatus.hidden = false;
      cloudStatus.className = 'cloud-status is-error';
      cloudStatus.textContent = `Network error reaching GitHub API: ${err.message}. Check your connection and try again.`;
    } finally {
      dispatchBtn.textContent = 'Dispatch Custom Build';
      syncDispatchButton();
    }
  });
}
