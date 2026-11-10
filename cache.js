const DEFAULT_TTL_MS = 15 * 60 * 1000;

export function getCached(key, ttlMs = DEFAULT_TTL_MS) {
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed.ts !== 'number') return null;
    if (Date.now() - parsed.ts > ttlMs) return null;
    return parsed.data ?? null;
  } catch {
    return null;
  }
}

export function setCached(key, data) {
  try {
    window.localStorage.setItem(key, JSON.stringify({ ts: Date.now(), data }));
  } catch {
    // Ignore storage quota or private-mode errors
  }
}

export function getThemePreference() {
  try {
    const saved = window.localStorage.getItem('revancex_theme');
    return saved === 'light' || saved === 'dark' ? saved : null;
  } catch {
    return null;
  }
}

export function setThemePreference(theme) {
  try {
    if (theme === 'light' || theme === 'dark') {
      window.localStorage.setItem('revancex_theme', theme);
    } else {
      window.localStorage.removeItem('revancex_theme');
    }
  } catch {
    // Ignore storage errors
  }
}
