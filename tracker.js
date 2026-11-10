import { getCached, setCached } from './cache.js';

const MIN_POLL_INTERVAL_MS = 60 * 1000;
const RELEASES_CACHE_KEY = 'revancex_live_custom_releases_v1';
const RUNS_CACHE_KEY = 'revancex_live_dispatch_runs_v1';
const FIFTEEN_MIN_MS = 15 * 60 * 1000;

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

export function initTracker({ getCatalog, formatRelativeTime }) {
  const openBtn = document.getElementById('open-tracker-btn');
  const modal = document.getElementById('tracker-modal');
  const closeBtn = document.getElementById('tracker-close-btn');
  const refreshBtn = document.getElementById('tracker-refresh-btn');
  const metaEl = document.getElementById('tracker-poll-meta');
  const runsEl = document.getElementById('tracker-runs-list');
  const bundlesEl = document.getElementById('tracker-custom-bundles');

  const repoSlug = detectRepoSlug();
  let isOpen = false;
  let timerId = null;
  let lastPollAt = 0;
  let rateLimitedUntil = 0;
  let lastFocused = null;

  function renderCustomBundles(liveReleases) {
    const staticCustoms = getCatalog()?.custom_bundles || [];
    const items = [];
    const seenUrls = new Set();

    for (const rel of liveReleases || []) {
      if (!String(rel.tag_name || '').startsWith('custom-')) continue;
      for (const asset of rel.assets || []) {
        if (!asset.browser_download_url || seenUrls.has(asset.browser_download_url)) continue;
        seenUrls.add(asset.browser_download_url);
        items.push({
          name: asset.name,
          tag: rel.tag_name,
          url: asset.browser_download_url,
          sizeBytes: asset.size,
          updatedAt: rel.published_at || rel.created_at,
        });
      }
    }

    for (const cb of staticCustoms) {
      if (!cb.download_url || seenUrls.has(cb.download_url)) continue;
      seenUrls.add(cb.download_url);
      items.push({
        name: cb.filename || cb.name,
        tag: cb.release_tag || cb.tag || 'custom',
        url: cb.download_url,
        sizeBytes: cb.size_bytes,
        updatedAt: cb.updated_at,
      });
    }

    if (items.length === 0) {
      bundlesEl.innerHTML = '<p class="hint-text">No published custom bundles found yet.</p>';
      return;
    }

    bundlesEl.innerHTML = items
      .map(
        (item) => `
        <div class="tracker-item">
          <div class="tracker-item-main">
            <strong class="mono">${escapeHtml(item.name)}</strong>
            <span class="hint-text mono">${escapeHtml(item.tag)} · ${escapeHtml(formatBytes(item.sizeBytes))}${item.updatedAt ? ` · ${escapeHtml(formatRelativeTime(item.updatedAt))}` : ''}</span>
          </div>
          <a class="btn btn-primary" href="${escapeHtml(item.url)}" target="_blank" rel="noopener">Download</a>
        </div>
      `
      )
      .join('');
  }

  function renderRuns(runs) {
    if (!runs || runs.length === 0) {
      runsEl.innerHTML = '<p class="hint-text">No recent workflow_dispatch runs found.</p>';
      return;
    }

    runsEl.innerHTML = runs
      .map((run) => {
        const status = run.status || 'unknown';
        const conclusion = run.conclusion || '';
        const badgeClass =
          status !== 'completed'
            ? 'badge-warning'
            : conclusion === 'success'
              ? 'badge-passed'
              : 'badge-failed';
        const label = conclusion ? `${status} (${conclusion})` : status;
        return `
          <div class="tracker-item">
            <div class="tracker-item-main">
              <strong>#${escapeHtml(run.run_number || run.id)} — ${escapeHtml(run.name || 'Custom Bundle Build')}</strong>
              <span class="hint-text">${escapeHtml(formatRelativeTime(run.updated_at || run.created_at))}</span>
            </div>
            <div class="tracker-item-actions">
              <span class="badge ${badgeClass}">${escapeHtml(label)}</span>
              <a class="btn" href="${escapeHtml(run.html_url)}" target="_blank" rel="noopener">Logs</a>
            </div>
          </div>
        `;
      })
      .join('');
  }

  async function fetchLiveData(manual = false) {
    if (!isOpen) return;
    const now = Date.now();

    if (now < rateLimitedUntil) {
      const waitSec = Math.ceil((rateLimitedUntil - now) / 1000);
      metaEl.textContent = `GitHub API rate-limited; next retry in ${waitSec}s (showing cached data).`;
      renderRuns(getCached(RUNS_CACHE_KEY, FIFTEEN_MIN_MS));
      renderCustomBundles(getCached(RELEASES_CACHE_KEY, FIFTEEN_MIN_MS));
      return;
    }

    if (now - lastPollAt < MIN_POLL_INTERVAL_MS) {
      const waitSec = Math.ceil((MIN_POLL_INTERVAL_MS - (now - lastPollAt)) / 1000);
      if (manual) {
        metaEl.textContent = `Polling capped at once per 60s (available in ${waitSec}s).`;
      }
      renderRuns(getCached(RUNS_CACHE_KEY, FIFTEEN_MIN_MS));
      renderCustomBundles(getCached(RELEASES_CACHE_KEY, FIFTEEN_MIN_MS));
      return;
    }

    lastPollAt = now;
    metaEl.textContent = 'Polling GitHub Actions…';

    try {
      const runsRes = await fetch(
        `https://api.github.com/repos/${repoSlug}/actions/runs?event=workflow_dispatch&per_page=10`,
        { headers: { Accept: 'application/vnd.github+json' } }
      );

      if (runsRes.status === 403 || runsRes.status === 429) {
        const retryAfter = parseInt(runsRes.headers.get('Retry-After') || '120', 10);
        rateLimitedUntil = Date.now() + (Number.isFinite(retryAfter) ? retryAfter : 120) * 1000;
        metaEl.textContent = `Rate limited (${runsRes.status}); backing off for ${retryAfter}s.`;
        renderRuns(getCached(RUNS_CACHE_KEY, FIFTEEN_MIN_MS));
        renderCustomBundles(getCached(RELEASES_CACHE_KEY, FIFTEEN_MIN_MS));
        return;
      }

      if (!runsRes.ok) throw new Error(`HTTP ${runsRes.status}`);
      const runsJson = await runsRes.json();
      const customRuns = (runsJson.workflow_runs || []).filter(
        (r) =>
          String(r.path || '').includes('custom-build.yml') ||
          String(r.name || '').toLowerCase().includes('custom')
      );
      setCached(RUNS_CACHE_KEY, customRuns);
      renderRuns(customRuns);

      let releases = getCached(RELEASES_CACHE_KEY, FIFTEEN_MIN_MS);
      const hasCompletedSuccess = customRuns.some(
        (r) => r.status === 'completed' && r.conclusion === 'success'
      );
      if (!releases || hasCompletedSuccess) {
        const relRes = await fetch(
          `https://api.github.com/repos/${repoSlug}/releases?per_page=20`,
          { headers: { Accept: 'application/vnd.github+json' } }
        );
        if (relRes.status === 403 || relRes.status === 429) {
          const retryAfter = parseInt(relRes.headers.get('Retry-After') || '120', 10);
          rateLimitedUntil = Date.now() + (Number.isFinite(retryAfter) ? retryAfter : 120) * 1000;
        } else if (relRes.ok) {
          releases = await relRes.json();
          setCached(RELEASES_CACHE_KEY, releases);
        }
      }
      renderCustomBundles(releases);
      metaEl.textContent = `Last checked ${new Date().toLocaleTimeString()} (auto-refresh every 60s while open)`;
    } catch (err) {
      metaEl.textContent = `Live check paused (${err.message}); showing cached/static data.`;
      renderRuns(getCached(RUNS_CACHE_KEY, FIFTEEN_MIN_MS));
      renderCustomBundles(getCached(RELEASES_CACHE_KEY, FIFTEEN_MIN_MS));
    }
  }

  function openTracker() {
    lastFocused = document.activeElement;
    isOpen = true;
    modal.hidden = false;
    closeBtn.focus();
    fetchLiveData(false);
    if (timerId) clearInterval(timerId);
    timerId = setInterval(() => {
      if (isOpen) fetchLiveData(false);
    }, MIN_POLL_INTERVAL_MS);
  }

  function closeTracker() {
    if (!isOpen) return;
    isOpen = false;
    modal.hidden = true;
    if (timerId) {
      clearInterval(timerId);
      timerId = null;
    }
    if (lastFocused && typeof lastFocused.focus === 'function') {
      lastFocused.focus();
    }
  }

  openBtn.addEventListener('click', openTracker);
  closeBtn.addEventListener('click', closeTracker);
  refreshBtn.addEventListener('click', () => fetchLiveData(true));
  modal.addEventListener('click', (e) => {
    if (e.target === modal) closeTracker();
  });

  document.addEventListener('keydown', (e) => {
    if (!isOpen) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      closeTracker();
      return;
    }
    if (e.key === 'Tab') {
      const focusable = Array.from(
        modal.querySelectorAll('button, [href], input, select, [tabindex]:not([tabindex="-1"])')
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

  return { openTracker };
}
