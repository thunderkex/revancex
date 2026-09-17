function detectRepo() {
  const [, owner, repo] = window.location.hostname.match(/^(.+)\.github\.io$/) || [];
  const pathRepo = window.location.pathname.split('/').filter(Boolean)[0];
  return {
    owner: owner || 'your-username',
    repo: pathRepo || repo || 'revancex',
  };
}

const { owner, repo } = detectRepo();
const RAW_BASE = `https://raw.githubusercontent.com/${owner}/${repo}/extended`;
const API_BASE = `https://api.github.com/repos/${owner}/${repo}`;

let appsData = [];
let selected = new Set();

async function loadApps() {
  const grid = document.getElementById('app-grid');
  try {
    const res = await fetch(`${RAW_BASE}/config/apps.yaml`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const text = await res.text();
    appsData = parseAppsYaml(text);
    renderGrid();
  } catch (e) {
    grid.innerHTML = `<div class="loading" style="color:var(--error)">Failed to load apps.yaml: ${e.message}<br>Make sure the repo is public and GitHub Pages is enabled.</div>`;
  }
}

function parseAppsYaml(text) {
  const apps = [];
  const lines = text.split('\n');
  let currentApp = null;
  let inPatches = false;

  for (const raw of lines) {
    const line = raw.trimEnd();
    const indent = line.length - line.trimStart().length;

    if (indent === 2 && line.trim().endsWith(':') && !line.trim().startsWith('-')) {
      const id = line.trim().slice(0, -1);
      if (id !== 'apps') {
        currentApp = { id, patches: [], dependencies: [], architectures: [] };
        apps.push(currentApp);
        inPatches = false;
      }
    } else if (currentApp && indent === 4) {
      const trimmed = line.trim();
      if (trimmed.startsWith('enabled:')) currentApp.enabled = trimmed.includes('true');
      else if (trimmed.startsWith('package:')) currentApp.package = trimmed.split(':')[1].trim();
      else if (trimmed.startsWith('patch_source:')) currentApp.patch_source = trimmed.split(':')[1].trim();
      else if (trimmed.startsWith('mode:')) currentApp.mode = trimmed.split(':')[1].trim();
      else if (trimmed.startsWith('patches:')) inPatches = true;
      else if (trimmed.startsWith('dependencies:')) {
        inPatches = false;
        const inline = trimmed.replace('dependencies:', '').trim();
        if (inline.startsWith('[')) {
          currentApp.dependencies = inline.slice(1, -1).split(',').map(s => s.trim()).filter(Boolean);
        }
      } else if (trimmed.startsWith('architectures:')) {
        const inline = trimmed.replace('architectures:', '').trim();
        if (inline.startsWith('[')) {
          currentApp.architectures = inline.slice(1, -1).split(',').map(s => s.trim()).filter(Boolean);
        }
        inPatches = false;
      } else {
        inPatches = false;
      }
    } else if (currentApp && indent === 6 && inPatches && line.trim().startsWith('-')) {
      currentApp.patches.push(line.trim().slice(1).trim());
    }
  }
  return apps;
}

function renderGrid() {
  const grid = document.getElementById('app-grid');
  grid.innerHTML = '';
  for (const app of appsData) {
    const card = document.createElement('div');
    card.className = 'app-card' + (app.enabled ? '' : ' disabled-app');
    card.dataset.id = app.id;

    const deps = app.dependencies.length
      ? app.dependencies.map(d => `<span class="dep-badge">+${d}</span>`).join('')
      : '';

    card.innerHTML = `
      <h3>${formatName(app.id)}</h3>
      <div class="pkg">${app.package}</div>
      <span class="source-badge">${app.patch_source}</span>${deps}
    `;

    if (app.enabled) {
      card.addEventListener('click', () => toggleApp(app.id, card));
    }
    grid.appendChild(card);
  }
  updateCount();
}

function formatName(id) {
  return id.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
}

function toggleApp(id, card) {
  if (selected.has(id)) {
    selected.delete(id);
    card.classList.remove('selected');
  } else {
    selected.add(id);
    card.classList.add('selected');
    const app = appsData.find(a => a.id === id);
    for (const dep of (app?.dependencies || [])) {
      if (!selected.has(dep)) {
        selected.add(dep);
        document.querySelector(`.app-card[data-id="${dep}"]`)?.classList.add('selected');
      }
    }
  }
  updateCount();
}

function updateCount() {
  const n = selected.size;
  document.getElementById('selected-count').textContent = `${n} app${n !== 1 ? 's' : ''} selected`;
  document.getElementById('build-btn').disabled = n === 0;
}

document.getElementById('select-all').addEventListener('click', () => {
  for (const app of appsData) {
    if (app.enabled) {
      selected.add(app.id);
      document.querySelector(`.app-card[data-id="${app.id}"]`)?.classList.add('selected');
    }
  }
  updateCount();
});

document.getElementById('deselect-all').addEventListener('click', () => {
  selected.clear();
  document.querySelectorAll('.app-card.selected').forEach(c => c.classList.remove('selected'));
  updateCount();
});

document.getElementById('build-btn').addEventListener('click', async () => {
  const token = document.getElementById('gh-token').value.trim();
  if (!token) {
    showStatus('error', 'Paste your GitHub token (repo + workflow scopes) to trigger a build.');
    return;
  }

  const apps = [...selected].join(',');
  const arch = document.getElementById('arch-select').value;
  const btn = document.getElementById('build-btn');
  btn.disabled = true;
  btn.textContent = '⏳ Triggering…';

  try {
    const res = await fetch(`${API_BASE}/dispatches`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: 'application/vnd.github+json',
        'Content-Type': 'application/json',
        'X-GitHub-Api-Version': '2022-11-28',
      },
      body: JSON.stringify({
        event_type: 'custom-bundle-request',
        client_payload: { apps, arch },
      }),
    });

    if (res.status === 204) {
      showStatus('success', `✅ Build triggered! Apps: ${apps} | Arch: ${arch}\nCheck the Actions tab for progress. Release will be tagged custom-{run_id}.`);
    } else {
      const body = await res.text();
      showStatus('error', `GitHub API error ${res.status}: ${body}`);
    }
  } catch (e) {
    showStatus('error', `Request failed: ${e.message}`);
  } finally {
    btn.disabled = false;
    btn.textContent = '🚀 Build Custom Bundle';
  }
});

function showStatus(type, msg) {
  const el = document.getElementById('status');
  el.className = `status ${type}`;
  el.textContent = msg;
}

loadApps();
