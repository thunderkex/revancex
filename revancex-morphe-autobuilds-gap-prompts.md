# ReVanceX x Morphe-AutoBuilds: gap analysis and prompts

Compares thunderkex/revancex @ b879ebc (Rust) with RookieEnough/Morphe-AutoBuilds @ 3dcc9bb (Python). Both are GPL-3.0, so porting logic is license-compatible; keep a credit line in files derived from theirs.

## 1. What they have that we lack

I read every file in their `src/` and grepped our `src/`, `config/` and `.github/` for each feature.

| Feature | Their file | Ours |
| --- | --- | --- |
| FlareSolverr Cloudflare bypass | flaresolverr.py | none |
| Trawl browser-render service, sticky per-site mode | trawl.py, apkmirror._cf_get | none (only an uptodown breaker) |
| APKPure official app API (no Cloudflare) | apkpure.py | none; we scrape d.apkpure.com, which is the URL that returned your 403 |
| APKCombo downloader | apkcombo.py | none |
| APKFab downloader | apkfab.py | none |
| Aptoide website-first scrape, slug overrides | aptoide.py | partial, needs an audit |
| Codeberg and GitLab as APK and patch hosts | codeberg.py, utils.detect_release | none |
| Release lookup by latest / dev / prerelease / tag | utils.py | GitHub only |
| Fail-loud policies: unknown compat, 0 patches, version mismatch, fingerprint retry | downloader.py, __main__.py | none |
| Browser TLS impersonation | __init__.py (curl_cffi) | headers only (reqwest) |
| GitLab release mirror, R2 upload | gitlab_api.py, r2.py | none |
| FlareSolverr service container, startup jitter in CI | patch.yml | none |

We already have: GitHub-release `apk_url`, uptodown circuit breaker, apkeep, APKEditor merge, arch strip, an apkmirror scraper, BUILD_REPORT. Don't re-port those.

## 2. Five corrections before you port anything

1. **FlareSolverr returns HTML only. It cannot download an APK.** Their `solve()` returns cookies and a user agent, but `get_with_bypass` keeps only the cookies, and later downloads use a different UA. `cf_clearance` is bound to the UA and IP, so that is a latent bug. Our version must reuse the solved cookies and the exact UA for the download itself (Prompt A, item 3).
2. **Their challenge check is too broad.** It treats every 403 or 503 as a Cloudflare challenge. A CDN 403 is usually a referer or IP block, and a browser will not fix it; it just burns 60 seconds. Detect markers or the `cf-mitigated: challenge` header instead.
3. **The APKPure API trick is ToS-grey.** It sends headers that mimic the APKPure Android client, including a made-up device profile. It works today and may stop. Put the headers in config, add a kill switch, and don't hardcode a device fingerprint in Rust.
4. **Aptoide's website only gives a download URL for the latest version.** Older versions must be a miss, never a silent substitution of latest.
5. **No global mutable state.** Their `_use_trawl` and `_blocked_by_cloudflare` module globals become a per-run struct in Rust.

I have not verified that the APKPure API resolves Gboard specifically. It is the most likely fix for the 403, but run `sources probe` (Prompt G) before relying on it.

## 3. Prompts

Run Prompt 0 once, then B, A, C, E, D, F, G, H in that order. If you want the Gboard fix this week without the refactor, implement only C1 (apkpure_api) and C4 (aptoide website-first) inside the current `fetch_apk` match, then migrate them when B lands.

### Prompt 0: session context

```
You are working on thunderkex/revancex (Rust, branch extended). Goal: port the APK-acquisition and build-safety features of RookieEnough/Morphe-AutoBuilds (Python, GPL-3.0) into our Rust code as clean Rust, not a line-by-line translation.

Rules:
1. Every endpoint, header set, slug override, retry count and timeout goes in config/*.yaml with serde defaults. No new hardcoded app ids or per-app special cases in Rust.
2. Adding a source for an app is config-only.
3. No process-global mutable statics for runtime state. Use a struct shared via Arc, created once per run.
4. Failures stay visible: every source attempt, challenge, bypass and fallback is recorded in the app's BUILD_REPORT entry (source, outcome, reason), not only logged.
5. A source miss returns an error. A source never silently substitutes a different version than the one requested.
6. Network code is tested against saved HTML/JSON fixtures in tests/fixtures/sources/. No live-network tests in cargo test.
7. cargo fmt, cargo clippy --all-targets -- -D warnings and cargo test pass before each commit. One prompt = one commit series, conventional commits.
8. Put a header comment crediting RookieEnough/Morphe-AutoBuilds (GPL-3.0) on files whose logic derives from theirs.

First read: src/builder/mod.rs (fetch_apk and every fetch_* fn), src/utils/mod.rs (make_browser_client, download helpers), src/config/apps.rs, config/sources.yaml, src/orchestrator/scheduler.rs. Confirm you have read them, then wait.
```

### Prompt B: ApkSource trait and per-app `sources:` map (do first, behavior-preserving)

```
fetch_apk in src/builder/mod.rs is ~1,800 lines with one hardcoded match over source names and one *_url field per source on AppConfig (uptodown_url, apkmirror_url, archive_url, apkpure_url, aptoide_url). Adding five more sources that way is unmaintainable. Refactor first, with no behavior change.

1. Create src/sources/ with mod.rs (trait + registry) and one file per existing source: apkeep, apkmirror, uptodown, apkpure, aptoide, archive, direct. Move the code out of builder/mod.rs. builder keeps only orchestration: version resolution, cache check, post-download validation.

2. Trait (async-trait, object safe):
   name() -> &'static str
   latest_version(ctx, app) -> Result<Option<String>>
   resolve(ctx, app, version: Option<&str>, arch) -> Result<Resolved>   // finds the URL/asset WITHOUT downloading, so --dry-run and probe work
   fetch(ctx, resolved, dest) -> Result<PathBuf>
   Resolved carries: url, file_kind (apk|xapk|apkm|apks), version_found, headers and cookies to use for the download, and a trace Vec<String> of the steps taken.
   ctx is a SourceCtx holding the shared Fetcher (Prompt A), Config, GitHub token and per-run circuit breakers.

3. Per-app config: replace the flat *_url fields with a map
     sources:
       apkpure: { enabled: true }
       apkmirror: { url: https://www.apkmirror.com/apk/google-inc/gboard }
       github: { repo: owner/mirror, asset_pattern: gboard-*{arch}*.apk }
   Keep the legacy flat fields parsing: normalize them into the map at load time and have `revancex validate` print one deprecation warning.

4. The default order moves to config/sources.yaml as apk_source_order; per-app source_priority still overrides. Add strict_sources: true per app: when set, no source outside the app's own list is tried. This generalizes their hardcoded Facebook-only-Codeberg special case in __main__.py.

5. Generalize the uptodown breaker (AtomicU32 + REVANCEX_UTD_CB_THRESHOLD) into a per-source breaker in SourceCtx: N consecutive hard blocks open it for the rest of the run. Reset per run; thresholds come from config.

6. Every attempt appends {source, outcome: hit|miss|blocked|skipped-breaker|error, reason, trace} to the app's BUILD_REPORT entry. Add attempted_sources to AppBuildReport in scheduler.rs; the current 'BUILD_REPORT resolved_source=' log lines go nowhere.

Acceptance: `revancex build` output for existing apps is unchanged; builder/mod.rs drops below ~700 lines; adding a stub source takes one new file plus one registry line.
```

### Prompt A: Fetcher with a Cloudflare bypass chain

```
Create src/net/fetcher.rs, the single place every scraper uses to GET pages. Port the ideas of their flaresolverr.py, trawl.py and apkmirror._cf_get, applying the corrections in section 2 of this doc.

1. Chain per request, stopping at the first success:
   direct (shared client) -> TLS-impersonating client (if the feature is enabled) -> FlareSolverr -> Trawl.
   A stage runs only if configured and available. Availability is probed once per run (GET the service base URL, 3s timeout) and cached in the Fetcher.

2. is_challenge(status, headers, body): true when header cf-mitigated is challenge, or the first ~5 KB of the body contains a marker from config (defaults: 'Just a moment...', cf-chl-bypass, 'Checking your browser', cloudflare-static, 'Attention Required', cf-turnstile, 'verify you are human'). A bare 403/503 with no marker is NOT a challenge: return it as a normal error so the caller moves to the next source.

3. FlareSolverr client: POST {FLARESOLVERR_URL}/v1 with body {cmd: request.get, url, maxTimeout: <ms>}. Success when response.status == ok; read solution.response (HTML), solution.cookies[], solution.userAgent, solution.url. Default URL http://localhost:8191/v1, overridable by env FLARESOLVERR_URL or sources.yaml fetch.flaresolverr.url. Because it returns text only:
   a. store the returned cookies (notably cf_clearance) in a per-host cookie jar inside the Fetcher and store the exact userAgent per host;
   b. every later request AND the APK download itself to that host must use that jar and that exact User-Agent;
   c. Resolved (Prompt B) carries both so fetch() can reuse them.

4. Trawl client (optional, only when TRAWL_URL is set): POST {TRAWL_URL}/scrape with {url, maxTimeout: 60000, skipHttp: true, headers: {Referer}}. Accept only when statusCode is 200, html is non-empty and no blocked marker appears. Parse cookies and userAgent as in item 3.

5. Sticky mode per host (replaces their global _use_trawl): once a host needed a browser stage, try direct first (cookies may still be valid), then the same browser stage again, without abandoning the site. If every stage is challenged, mark the host blocked for the rest of the run and return a typed FetchError::Blocked{host}, which the source breaker (Prompt B item 5) counts.

6. TLS impersonation: our reqwest+rustls client only fakes headers, so the TLS fingerprint is still non-browser, which is what their curl_cffi impersonate=chrome avoids. Add an optional cargo feature `impersonate` using the wreq crate (formerly rquest). Verify the crate's current name, API and MSRV against rust-version 1.75 first; if it needs a newer toolchain keep it feature-gated and say so in the PR. The default build must not depend on it.

7. Config in config/sources.yaml:
     fetch:
       flaresolverr: { enabled: auto, url: http://localhost:8191/v1, timeout_secs: 60 }
       trawl: { enabled: auto, url: '' }
       challenge_markers: [...]
       per_host_delay_ms: { apkmirror.com: 1500 }
   enabled: auto means use the service only if the probe succeeds.

8. Replace the raw reqwest calls inside the existing apkmirror, uptodown, apkpure and aptoide scrapers with the Fetcher. Keep their existing error enums and map FetchError::Blocked into them.

Acceptance (mock HTTP server tests): (a) a clean 200 passes through; (b) a challenge page triggers the FlareSolverr stage; (c) a plain 403 does not; (d) after a solve, the next request and the download carry cf_clearance and the solved User-Agent; (e) a service that is down is skipped with no delay.
```

### Prompt C: new site downloaders

```
Implement each as an ApkSource (Prompt B), each with fixtures.

1. apkpure_api (highest priority; the likely fix for the Gboard 403). We scrape d.apkpure.com/b/XAPK/... and get 403. They use the app-client API instead:
   GET https://tapi.pureapk.com/v3/get_app_his_version?package_name=<pkg>&hl=en
   Response: version_list[], each with version_name and asset.urls[0] (a direct link). It lists only about the 20 newest versions, so older versions fall through to the existing page scrape (now via the Fetcher).
   Match: exact string, else equal after the shared normalize_version (Prompt E item 5). Prefer an entry that has a URL; an entry without a URL is a miss.
   The endpoint needs headers that mimic the APKPure Android client. Put the header set in config/sources.yaml under apk_sources.apkpure_api.headers, hardcode no device fingerprint in Rust, and add apk_sources.apkpure_api.enabled (default true) as a kill switch.
   Default position: ahead of the apkpure page scraper.

2. apkcombo (default position: last, as in their code).
   Versions: GET https://apkcombo.com/search/<pkg>/download; versions come from links shaped phone-<version>-(apk|xapk|apks).
   Link: for ext in [apk, xapk, apks], GET .../download/phone-<version>-<ext> and take the first a.variant[href]. If the href path is /r2, the real URL is its u= query parameter, URL-decoded.
   Dynamic fallback when no variant link exists: find xid in a script (xid = '...'), POST <app-path><xid>/dl with form fields package_name=<pkg> and version= (empty), headers Referer=<page url> and X-Requested-With=XMLHttpRequest, then parse the returned HTML fragment for a.variant[href].
   Add the scraper crate for CSS selectors (we have only regex today).

3. apkfab. Needs a slug per app: sources.apkfab.slug.
   App page https://apkfab.com/<slug>/<pkg>, versions page .../versions. Entries are under .version_history > div.list; the version text is in .package_info .title .version; the entry's a[href*='/download?sha1='] carries a 40-hex sha1.
   Then GET .../download?sha1=<sha1>; the final link is the anchor whose href contains d.apkfab.com/get-download. That URL is signed and expires: resolve it immediately before downloading, never cache it, and never trust the version text on that page for old versions.
   Latest version: the <title> contains 'APK x.y.z for Android'.

4. aptoide. First audit src/sources/aptoide.rs: does it scrape https://<slug>.en.aptoide.com/versions, or only call the ws75 API? The API backend is stale for many apps, which is the probable reason Aptoide returned no versions for Gboard. Required behavior: website first, API second.
   Website: find the script tag whose JSON starts with {'props' and read props.pageProps.versions[] (vername, vercode). Only the latest version carries a direct file.path; for any older version the website gives no URL, so that is a miss.
   API: listAppVersions?package_name=<pkg>&limit=100, then getAppMeta?package_name=<pkg>&vercode=<code>. The CPU filter is q=<base64 of 'myCPU=arm64-v8a,armeabi-v7a,armeabi&leanback=0'>.
   Slug: config sources.aptoide.slug (replaces their hardcoded _APTOIDE_SLUG_OVERRIDES dict).
   Strip a trailing variant such as -release-arm64-v8a before comparing versions (our failing Gboard request carried versionName=18.0.3.954559732-release-arm64-v8a).
   Never fall back to Aptoide's latest when a specific version was requested.

5. codeberg and gitlab as APK hosts (their Facebook case: stock APKs no mirror serves, hosted as release assets). They use the forge client from Prompt D. Config: sources.codeberg: { repo, tag, domain, keyword, version_regex }; sources.gitlab: { project, tag }. Pick the asset by keyword plus an arch score (+10 when the arch appears in the filename); accepted extensions .apk .apkm .xapk .apks. Take the version from version_regex when given; their default regexes on asset names are fragile, so make the regex explicit and use their pattern only as a fallback.

6. github: implement it here as one more ApkSource on the same forge client. This supersedes Part 2 of the previous prompt doc: the github_repo / github_asset_pattern fields become sources.github.*. Release selection: exact tag, then tag-prefix + version, then newest non-prerelease. The asset pattern supports {arch} substitution. Errors must say whether the release, the tag or the asset was missing.

Acceptance: `revancex fetch-apk gboard --dry-run` prints, per source, the resolved URL chain or the exact miss reason; fixture tests cover hit, miss and malformed page for every source.
```

### Prompt E: build-safety policies

```
Their downloader and __main__ encode hard-won policy about not shipping bad builds. We have none of it (grep for 'applying 0', 'fingerprint' and 'UnknownPatchCompatibility' finds nothing). Port the policy.

1. Unknown compatibility. Today, when list-patches fails or returns nothing, version resolution can fall back to latest. Make that explicit with a tri-state PatchCompat { Versions(Vec), Agnostic, Unknown }. Unknown + version: auto is a hard error for that app ('refusing to build latest, patch compatibility unknown'), configurable per app as on_unknown_compat: fail|latest (default fail). Agnostic means latest is fine. Never append the store's latest to the candidate list when the patches declare specific versions.

2. Zero-patch refusal. After the patcher exits 0, scan its output and fail the app (do not sign, do not package) when it reports applying 0 patches (case-insensitive, flexible whitespace).

3. Version-mismatch refusal. Parse the patcher's 'Filtering patches for <package> v<version>' lines. Strip a trailing ' build N' or '(N)' and a trailing '...' (the CLI truncates long versions), normalize, and fail when the result differs from the APK actually downloaded.

4. Retry with an older compatible version when the patcher fails with fingerprint-style errors: output containing 'failed to match the fingerprint', 'patch.patchexception', 'patching aborted', or both 'fingerprint' and 'failed'. Take the next candidate from the compatible list, re-download it through the source chain, up to build.version_fallback_attempts (already exists). Record every attempted version and the reason in BUILD_REPORT.

5. One shared utils::version module: normalize_version (numeric components; ignores build suffixes, -release-<arch> style suffixes and parenthesized build numbers) and versions_match(a, b). Replace the ad-hoc comparisons now spread across the apkpure, aptoide, apkmirror and uptodown code and the post-download check in fetch_apk. Unit-test with real strings: 18.0.3.954559732-release-arm64-v8a, 32.30.0(1575420), 6.6 build 002, 20.12.36.

6. Integrity gate before patching. After any download or bundle merge, require a readable zip (full CRC pass, not just the header), non-zero size, and for bundles at least one inner .apk. On failure delete the file and fall to the next source. Do not check Android signatures here: merged bundles are unsigned, and their code notes that signature pre-checks discarded good downloads.

Acceptance: unit tests for items 2, 3 and 5 using captured patcher output; a build that applies 0 patches exits non-zero for that app and is recorded as failed with the reason.
```

### Prompt D: forge-agnostic release lookup

```
Port their utils.detect_release, detect_github_release, detect_gitlab_release and detect_codeberg_release into src/utils/forge.rs.

1. enum Forge { Github, Gitlab, Codeberg{domain} } and one function release(forge, repo_or_project, selector) -> Release{tag, published_at, assets: Vec<Asset{name, url}>}. Normalize GitLab assets.links (direct_asset_url or url) and Gitea/Forgejo assets into the same Asset shape.

2. selector: latest | most-recent (max created_at, even if prerelease) | dev (newest tag containing 'dev') | prerelease (newest prerelease) | exact:<tag>.

3. Patch and CLI entries in config/sources.yaml get provider: github|gitlab|codeberg (default github), so patch bundles hosted off GitHub work; existing entries stay unchanged. The same module serves Prompt C items 5 and 6, the tool downloader and check-updates.

4. Robustness taken from theirs: up to 3 attempts with 3s and 6s backoff; authenticated requests when GITHUB_TOKEN or GITLAB_TOKEN is set; on a 401 for a repo outside the current repository, print the hint that the Actions GITHUB_TOKEN cannot read private external repos and a PAT secret is needed. Also distinguish 404 (no such release) from a 403 rate limit (tell the user to set a token) instead of one generic error; this closes the earlier audit finding that get_latest_release swallows errors.

5. Reuse one reqwest Client; never build a client per call.

Acceptance: fixture tests for each forge's JSON shape; check-updates works for a patch source hosted on Codeberg.
```

### Prompt F: CI wiring

```
Wire the new network stack into .github/workflows/build-and-release.yml and custom-build.yml.

1. Add a FlareSolverr service container to the build job:
     services:
       flaresolverr:
         image: ghcr.io/flaresolverr/flaresolverr:<pinned version or digest>
         ports: ['8191:8191']
         options: --shm-size=1gb
   Pin the image: their workflows use :latest, and a solver update can break every build overnight. Set FLARESOLVERR_URL=http://localhost:8191/v1 on the build step.

2. Add a 'Wait for FlareSolverr' step: poll GET http://localhost:8191/ up to 30 times, 5s apart. On timeout, warn but do not fail, because the Fetcher treats an unreachable service as absent. Use continue-on-error on the step, not `|| true`.

3. Optional Trawl: pass secrets.TRAWL_URL through only if it is set.

4. Startup jitter. Their patch.yml sleeps up to 30s per matrix job so parallel jobs don't hit one site in the same second. Implement it in Rust as build.startup_jitter_secs, so it is configurable and also applies locally.

5. Key the tools/ cache on the resolved patch-bundle tags (tie in with the version_cache work), not only on hashFiles of config.

6. Write the per-app attempted_sources table to $GITHUB_STEP_SUMMARY so a source that keeps failing is visible without opening logs.

Acceptance: a run with FlareSolverr unreachable still completes; a run where one site is challenged shows the bypass in the summary.
```

### Prompt G: probe, doctor and fixtures

```
Make source breakage diagnosable in one command.

1. `revancex sources probe <app> [--arch] [--version]`: for every enabled source of the app, run resolve() only (no download) and print a table: source | outcome | version found | URL host | ms | reason. Exit non-zero when no source resolves.

2. Extend the existing `doctor` subcommand to report: java version, aria2 and apkeep availability, FlareSolverr and Trawl reachability (with the URL used), GITHUB_TOKEN presence and remaining rate limit, and whether the impersonate feature is compiled in.

3. Fixture library: tests/fixtures/sources/<source>/<case>.{html,json} for every source and case (hit, version-not-listed, challenge-page, malformed). Add scripts/refresh-fixtures.sh for maintainers to re-capture current pages; never run it in CI.

4. A weekly scheduled workflow that runs `sources probe` for a small canary set of apps (config key probe_canaries) and opens an issue when a source that worked last week now fails. Scrapers break silently; this is the early warning.

5. Document the sources: map, the fetch: block, the env vars and a troubleshooting table (symptom -> command to run) in CONFIG.md.
```

### Prompt H: stock-APK mirror and publish targets (optional, after A to G)

```
Turn flaky scraping into a one-time cost.

1. `revancex mirror --apps <ids> --to github|codeberg|gitlab`: resolve and download the stock APK through the normal source chain, validate it (Prompt E item 6), then upload it as a release asset on a mirror repo (config: mirror.repo, mirror.provider, tag convention <app>-<version>). The sources.github, sources.codeberg and sources.gitlab entries from Prompt C then read from that mirror, so an app that only one flaky site can serve is fetched once and served reliably after.

2. Retention: keep the newest N versions per app (config) and delete older assets, as their release.py prunes superseded releases. Compare dotted versions numerically (their tuple comparison maps non-numeric parts to 0), and never delete a release that a pinned app version references.

3. Optional publish targets for built artifacts, off by default and gated by config and secrets:
   - GitLab release plus generic package registry upload (their gitlab_api.py): sanitize filenames because GitLab rejects parentheses, retry on 429 and 5xx with 10s then 20s backoff, move the latest tag, then replace the asset links.
   - S3 or R2 upload with pruning of old objects (their r2.py), using the aws-sdk-s3 crate or a signed reqwest PUT.

4. Check redistribution terms before mirroring third-party APKs. Default mirror repos to private in the docs; private-repo asset URLs need a token, which the forge client already sends.
```

## 4. Order of work

1. **This week, Gboard only:** Prompt C items 1 and 4 inside the current `fetch_apk` match, plus Prompt E item 5 (the version normalizer, since your failing request carried a `-release-arm64-v8a` suffix).
2. **Then the foundation:** B, then A, then the rest of C.
3. **Then safety and reach:** E, D, F, G.
4. **Last:** H.

This pairs with the two earlier prompts: `allow_build_failure` (so one broken app can't fail the release) and the `report check` subcommand. Keep both; they cover the case where every source for an app is down.
