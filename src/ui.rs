//! Centralised console / CI presentation layer.
//!
//! All human-facing build output goes through this module so that the output is
//! consistent: one event per line, aligned columns, status glyphs with an ASCII
//! fallback, collapsible phases on GitHub Actions, warnings de-duplicated and
//! reported once in the final summary, and raw tool output captured to
//! `build/logs/` instead of the console.
//!
//! Everything here writes to **stderr**; stdout is reserved for machine
//! readable output (`--json`, build matrices, ...).
//!
//! Environment:
//! * `VERBOSE=1` / `--verbose` / `RUST_LOG=debug` / `RUNNER_DEBUG=1` (Actions
//!   debug logging) – show detail on the console.
//! * `NO_COLOR` – no ANSI colour, ASCII status markers.
//! * `REVANCEX_LOG_DIR` – log directory (default `build/logs`).
//! * `GITHUB_ACTIONS` – emit `::group::` / `::warning::` / `::error::`.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Fail,
    Warn,
    Skip,
    Info,
}

impl Status {
    pub fn glyph(self, unicode: bool) -> &'static str {
        match (self, unicode) {
            (Status::Ok, true) => "✔",
            (Status::Ok, false) => "[OK]",
            (Status::Fail, true) => "✖",
            (Status::Fail, false) => "[FAIL]",
            (Status::Warn, true) => "⚠",
            (Status::Warn, false) => "[WARN]",
            (Status::Skip, true) => "↷",
            (Status::Skip, false) => "[SKIP]",
            (Status::Info, true) => "●",
            (Status::Info, false) => "[INFO]",
        }
    }

    fn ansi(self) -> &'static str {
        match self {
            Status::Ok => "32",
            Status::Fail => "31",
            Status::Warn => "33",
            Status::Skip => "90",
            Status::Info => "36",
        }
    }
}

pub struct Row {
    pub app: String,
    pub version: String,
    pub ok: usize,
    pub skip: usize,
    pub fail: Option<usize>,
    pub size: Option<u64>,
    pub dur: Option<Duration>,
    pub status: Status,
}

struct Pending {
    ts: String,
    status: Status,
    body: String,
    count: u32,
}

struct State {
    verbose: bool,
    console_level: Level,
    unicode: bool,
    color: bool,
    ci: bool,
    started: Instant,
    log_dir: PathBuf,
    pending: Option<Pending>,
    warnings: Vec<(String, u32)>,
    durations: HashMap<String, Duration>,
    items: HashSet<String>,
    secrets: Vec<String>,
    group_open: bool,
    banner_done: bool,
}

impl State {
    fn detect() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let ci = std::env::var_os("GITHUB_ACTIONS").is_some();
        let tty = std::io::stderr().is_terminal();
        let ascii_forced = std::env::var_os("REVANCEX_ASCII").is_some();
        let rich = !no_color && (tty || ci);
        let env_verbose = ["VERBOSE", "RUNNER_DEBUG"]
            .iter()
            .any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0"));
        let rust_log = std::env::var("RUST_LOG").unwrap_or_default().to_lowercase();
        let console_level = if env_verbose || rust_log.contains("trace") {
            if rust_log.contains("trace") {
                Level::TRACE
            } else {
                Level::DEBUG
            }
        } else if rust_log.contains("debug") {
            Level::DEBUG
        } else if rust_log.contains("info") {
            Level::INFO
        } else {
            Level::WARN
        };
        let mut secrets = Vec::new();
        for key in [
            "GITHUB_TOKEN",
            "KEYSTORE_B64",
            "KEYSTORE_PASSWORD",
            "TG_TOKEN",
        ] {
            if let Ok(v) = std::env::var(key) {
                if v.len() >= 6 {
                    secrets.push(v);
                }
            }
        }
        State {
            verbose: console_level >= Level::DEBUG,
            console_level,
            unicode: rich && !ascii_forced,
            color: rich,
            ci,
            started: Instant::now(),
            log_dir: std::env::var_os("REVANCEX_LOG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("build/logs")),
            pending: None,
            warnings: Vec::new(),
            durations: HashMap::new(),
            items: HashSet::new(),
            secrets,
            group_open: false,
            banner_done: false,
        }
    }
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn st() -> MutexGuard<'static, State> {
    STATE
        .get_or_init(|| Mutex::new(State::detect()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// formatting helpers (pure, unit-tested)
// ---------------------------------------------------------------------------

pub fn fmt_dur(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let t = d.as_secs();
        if t >= 3600 {
            format!("{}h {:02}m {:02}s", t / 3600, (t % 3600) / 60, t % 60)
        } else {
            format!("{}m {:02}s", t / 60, t % 60)
        }
    }
}

pub fn fmt_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.1} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn redact_with(secrets: &[String], text: &str) -> String {
    let mut out = text.to_string();
    for s in secrets {
        if !s.is_empty() && out.contains(s.as_str()) {
            out = out.replace(s.as_str(), "***");
        }
    }
    out
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

pub fn render_table(rows: &[Row], unicode: bool) -> Vec<String> {
    let header = [
        "App",
        "Version",
        "Patches (ok/skip/fail)",
        "Size",
        "Duration",
        "Status",
    ];
    let cells: Vec<[String; 6]> = rows
        .iter()
        .map(|r| {
            [
                r.app.clone(),
                if r.version.is_empty() {
                    "-".into()
                } else {
                    r.version.clone()
                },
                format!(
                    "{} / {} / {}",
                    r.ok,
                    r.skip,
                    r.fail.map_or("-".to_string(), |f| f.to_string())
                ),
                r.size.map_or("-".into(), fmt_size),
                r.dur.map_or("-".into(), fmt_dur),
                r.status.glyph(unicode).to_string(),
            ]
        })
        .collect();
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in &cells {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let fmt_row = |cols: Vec<&str>| -> String {
        let mut line = String::from("  ");
        for (i, c) in cols.iter().enumerate() {
            let pad = widths[i].saturating_sub(c.chars().count());
            line.push_str(c);
            if i + 1 < cols.len() {
                line.push_str(&" ".repeat(pad + 2));
            }
        }
        line
    };
    let mut out = vec![fmt_row(header.to_vec())];
    for row in &cells {
        out.push(fmt_row(row.iter().map(|s| s.as_str()).collect()));
    }
    out
}

pub fn hint_for(err: &str) -> &'static str {
    let e = err.to_lowercase();
    if e.contains("0/") && e.contains("patches applied") {
        "likely: the APK version is not covered by the patches; pin a supported version or update the patch bundle"
    } else if e.contains("integrity") || e.contains("not a valid zip") || e.contains("captcha") {
        "likely: the download is corrupted or returned a CAPTCHA page; retry or use another source"
    } else if e.contains("rate limit") || e.contains("429") {
        "likely: rate limited; set GITHUB_TOKEN or try again later"
    } else if e.contains("patch bundle download failed") {
        "likely: failed to download the patch bundle; check your connection / GITHUB_TOKEN"
    } else if e.contains("version mismatch") {
        "likely: the downloaded APK does not match the target version; clear the cache and retry with --force"
    } else if e.contains("java") {
        "likely: JDK 21+ is not installed or is not in PATH"
    } else if e.contains("circuit") || e.contains("unavailable on any source") {
        "likely: all download sources failed or are blocked; try `revancex sources probe <app>`"
    } else {
        "see the log file for details"
    }
}

// ---------------------------------------------------------------------------
// low-level emit (dedup of identical consecutive lines)
// ---------------------------------------------------------------------------

fn now_hms() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn colorize(s: &State, status: Status, glyph: &str) -> String {
    if s.color {
        format!("\x1b[{}m{glyph}\x1b[0m", status.ansi())
    } else {
        glyph.to_string()
    }
}

fn write_err(text: &str) {
    let mut e = std::io::stderr().lock();
    let _ = writeln!(e, "{text}");
}

/// Write one plain human-facing line through the UI layer (redacted, flushed after pending).
pub fn line(text: &str) {
    let mut s = st();
    flush_pending(&mut s);
    let safe = redact_with(&s.secrets, text);
    write_err(&safe);
}

fn render_pending(s: &State, p: &Pending) -> String {
    let w: usize = if s.unicode { 1 } else { 6 };
    let glyph = p.status.glyph(s.unicode);
    let pad = " ".repeat(w.saturating_sub(glyph.chars().count()));
    let suffix = if p.count > 1 {
        format!("  (x{})", p.count)
    } else {
        String::new()
    };
    format!(
        "[{}] {}{} {}{}",
        p.ts,
        colorize(s, p.status, glyph),
        pad,
        p.body,
        suffix
    )
}

fn flush_pending(s: &mut State) {
    if let Some(p) = s.pending.take() {
        let line = render_pending(s, &p);
        write_err(&line);
    }
}

fn emit(s: &mut State, status: Status, body: String) {
    let body = redact_with(&s.secrets, &body);
    if let Some(p) = s.pending.as_mut() {
        if p.status == status && p.body == body {
            p.count += 1;
            return;
        }
    }
    flush_pending(s);
    s.pending = Some(Pending {
        ts: now_hms(),
        status,
        body,
        count: 1,
    });
}

fn raw(s: &mut State, text: &str) {
    flush_pending(s);
    let text = redact_with(&s.secrets, text);
    write_err(&text);
}

fn log_append(s: &State, file_stem: &str, text: &str) {
    if std::fs::create_dir_all(&s.log_dir).is_err() {
        return;
    }
    let safe: String = file_stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = s.log_dir.join(format!("{safe}.log"));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(redact_with(&s.secrets, text).as_bytes());
    }
}

pub fn init(verbose: bool) {
    let mut s = st();
    if verbose && s.console_level < Level::DEBUG {
        s.console_level = Level::DEBUG;
    }
    s.verbose = s.console_level >= Level::DEBUG;
    // combined log is append-only; mark where this run starts
    let header = format!(
        "===== revancex {} run {} =====\n",
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    log_append(&s, "revancex", &header);
}

pub fn is_verbose() -> bool {
    st().verbose
}

pub fn log_dir() -> PathBuf {
    st().log_dir.clone()
}

pub fn mask(secret: &str) {
    if secret.len() < 4 {
        return;
    }
    let mut s = st();
    if !s.secrets.iter().any(|x| x == secret) {
        s.secrets.push(secret.to_string());
        if s.ci {
            raw(&mut s, &format!("::add-mask::{secret}"));
        }
    }
}

pub fn banner(fields: &[(&str, String)]) {
    let mut s = st();
    if s.banner_done {
        return;
    }
    s.banner_done = true;
    let mut text = format!("revancex {}", env!("CARGO_PKG_VERSION"));
    for (k, v) in fields {
        let _ = write!(text, "\n  {k:<9} {v}");
    }
    raw(&mut s, &text);
}

pub fn phase(name: &str) {
    let mut s = st();
    close_group(&mut s);
    if s.ci {
        raw(&mut s, &format!("::group::{name}"));
        s.group_open = true;
    } else {
        raw(&mut s, &format!("── {name} ──"));
    }
    log_append(&s, "revancex", &format!("=== phase: {name} ===\n"));
}

fn close_group(s: &mut State) {
    if s.group_open {
        raw(s, "::endgroup::");
        s.group_open = false;
    }
}

pub fn end_phase() {
    let mut s = st();
    close_group(&mut s);
}

pub fn item(status: Status, name: &str, version: &str, detail: &str, dur: Option<Duration>) {
    let mut s = st();
    let mut body = format!("{name:<14} {version:<10} ");
    match dur {
        Some(d) => {
            let _ = write!(body, "{detail:<34} ({})", fmt_dur(d));
        }
        None => body.push_str(detail),
    }
    let body = body.trim_end().to_string();
    log_append(
        &s,
        "revancex",
        &format!("{} item {}\n", status.glyph(false), body),
    );
    emit(&mut s, status, body);
}

pub fn item_ok(name: &str, version: &str, detail: &str, dur: Option<Duration>) {
    item(Status::Ok, name, version, detail, dur);
}

pub fn item_fail(id: &str, name: &str, version: &str, err: &str, dur: Option<Duration>) {
    let mut s = st();
    let summary = first_line(err).to_string();
    let mut body = format!("{name:<14} {version:<10} ");
    match dur {
        Some(d) => {
            let _ = write!(body, "{summary} ({})", fmt_dur(d));
        }
        None => body.push_str(&summary),
    }
    log_append(&s, id, &format!("--- FAILED ---\n{err}\n"));
    log_append(&s, "revancex", &format!("[FAIL] item {id}: {err}\n"));
    emit(&mut s, Status::Fail, body.trim_end().to_string());
    flush_pending(&mut s);
    let log_path = s.log_dir.join(format!("{id}.log"));
    let tail = tail_of(&log_path, 25);
    let mut lines = vec![format!("    penyebab: {}", hint_for(err))];
    lines.push(format!("    log     : {}", log_path.display()));
    lines.push(format!(
        "    ulangi  : revancex build --apps {id} --verbose"
    ));
    if !tail.is_empty() {
        lines.push("    ── 25 baris terakhir ──".to_string());
        lines.extend(tail.into_iter().map(|l| format!("    │ {l}")));
    }
    let text = lines.join("\n");
    raw(&mut s, &text);
    if s.ci {
        let msg = redact_with(&s.secrets, &format!("{name}: {summary}")).replace('\n', " ");
        raw(&mut s, &format!("::error title=Build failed::{msg}"));
    }
}

pub fn warn_once(msg: &str) {
    let mut s = st();
    collect_warning(&mut s, msg);
}

fn collect_warning(s: &mut State, msg: &str) {
    let msg = redact_with(&s.secrets, first_line(msg));
    if msg.is_empty() {
        return;
    }
    if let Some(w) = s.warnings.iter_mut().find(|(m, _)| *m == msg) {
        w.1 += 1;
    } else {
        s.warnings.push((msg.clone(), 1));
        if s.ci {
            let one_line = msg.replace('\n', " ");
            raw(s, &format!("::warning::{one_line}"));
        }
    }
}

pub fn info(msg: &str) {
    let mut s = st();
    log_append(&s, "revancex", &format!("[INFO] {msg}\n"));
    if s.verbose {
        emit(&mut s, Status::Info, msg.to_string());
    }
}

pub fn register_item(id: &str) {
    st().items.insert(id.to_string());
}

pub fn record_duration(id: &str, d: Duration) {
    st().durations.insert(id.to_string(), d);
}

pub fn duration_of(id: &str) -> Option<Duration> {
    st().durations.get(id).copied()
}

pub fn tool_log(name: &str, title: &str, content: &str) {
    let s = st();
    let mut text = format!("--- {title} ---\n{content}");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    log_append(&s, name, &text);
}

fn tail_of(path: &std::path::Path, n: usize) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| l.to_string())
        .collect()
}

fn warnings_block(s: &mut State) {
    if s.warnings.is_empty() {
        return;
    }
    let warnings = std::mem::take(&mut s.warnings);
    let mut text = format!("Warning ({} items):", warnings.len());
    for (msg, n) in &warnings {
        let suffix = if *n > 1 {
            format!("  (x{n})")
        } else {
            String::new()
        };
        let _ = write!(text, "\n  {} {msg}{suffix}", Status::Warn.glyph(s.unicode));
    }
    raw(s, &text);
}

pub fn summary(rows: &[Row], artifacts: &[String]) {
    let mut s = st();
    close_group(&mut s);
    flush_pending(&mut s);
    let total = s.started.elapsed();
    let ok = rows.iter().filter(|r| r.status == Status::Ok).count();
    let skip = rows.iter().filter(|r| r.status == Status::Skip).count();
    let fail = rows.iter().filter(|r| r.status == Status::Fail).count();
    let unique_warnings = s.warnings.len();
    warnings_block(&mut s);

    let mut text = String::from("Ringkasan");
    for l in render_table(rows, s.unicode) {
        let _ = write!(text, "\n{l}");
    }
    let _ = write!(
        text,
        "\n  Total: {ok} success, {skip} skipped, {fail} failed — {}",
        fmt_dur(total)
    );
    let _ = write!(text, "\n  Warning: {unique_warnings}");
    if !artifacts.is_empty() {
        let _ = write!(text, "\n  Artifacts:");
        for a in artifacts {
            let _ = write!(text, "\n    {a}");
        }
    }
    let _ = write!(text, "\n  Log detail: {}", s.log_dir.display());
    raw(&mut s, &text);

    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let warn_cnt = rows.iter().filter(|r| r.status == Status::Warn).count();
        let mut md = String::from("### Build Results\n\n");
        md.push_str("| App | Version | Patches | Size | Duration | Status |\n");
        md.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for r in rows {
            let patches = if r.ok == 0 && r.skip == 0 && r.fail.is_none() {
                "-".to_string()
            } else {
                format!(
                    "{}/{}/{}",
                    r.ok,
                    r.skip,
                    r.fail.map_or("-".to_string(), |f| f.to_string())
                )
            };
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} |",
                r.app,
                if r.version.is_empty() {
                    "-"
                } else {
                    &r.version
                },
                patches,
                r.size.map_or("-".into(), fmt_size),
                r.dur.map_or("-".into(), fmt_dur),
                match r.status {
                    Status::Ok => "✅",
                    Status::Fail => "❌",
                    Status::Warn => "⚠️",
                    Status::Skip => "⏭️",
                    Status::Info => "ℹ️",
                }
            );
        }
        let mut parts = vec![format!("{ok} ok")];
        if warn_cnt > 0 {
            parts.push(format!("{warn_cnt} degraded"));
        }
        if skip > 0 {
            parts.push(format!("{skip} skipped"));
        }
        parts.push(format!("{fail} failed"));
        let _ = write!(md, "\n**Total:** {} ({})", parts.join(", "), fmt_dur(total));
        if unique_warnings > 0 {
            let _ = write!(md, " · **Warnings:** {unique_warnings}");
        }
        md.push('\n');
        let md = redact_with(&s.secrets, &md);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(md.as_bytes());
        }
    }
}

pub fn fatal(err: &anyhow::Error) {
    let mut s = st();
    close_group(&mut s);
    flush_pending(&mut s);
    let mut text = format!(
        "{} {}",
        Status::Fail.glyph(s.unicode),
        first_line(&err.to_string())
    );
    for cause in err.chain().skip(1) {
        let _ = write!(text, "\n    cause: {}", first_line(&cause.to_string()));
    }
    let _ = write!(text, "\n    log detail: {}", s.log_dir.display());
    let _ = write!(text, "\n    add --verbose for detail");
    log_append(&s, "revancex", &format!("[FATAL] {err:?}\n"));
    let text = redact_with(&s.secrets, &text);
    if s.ci {
        raw(
            &mut s,
            &format!("::error::{}", first_line(&err.to_string())),
        );
    }
    raw(&mut s, &text);
}

pub fn flush() {
    let mut s = st();
    close_group(&mut s);
    flush_pending(&mut s);
    warnings_block(&mut s);
}

pub fn exit(code: i32) -> ! {
    flush();
    std::process::exit(code)
}

#[derive(Default)]
struct Visitor {
    msg: String,
    fields: String,
}

impl tracing::field::Visit for Visitor {
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            let _ = write!(self.msg, "{v:?}");
        } else {
            let _ = write!(self.fields, " {}={v:?}", f.name());
        }
    }
}

pub struct UiLayer;

impl<S: Subscriber> Layer<S> for UiLayer {
    fn on_event(&self, ev: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = ev.metadata();
        let mut v = Visitor::default();
        ev.record(&mut v);
        let msg = format!("{}{}", v.msg, v.fields);
        handle_event(*meta.level(), meta.target(), &msg);
    }
}

fn handle_event(level: Level, target: &str, msg: &str) {
    let first_party = target.starts_with("revancex");
    let mut s = st();

    let file_ok = if first_party {
        level <= Level::DEBUG
    } else {
        level <= Level::WARN
    };
    if file_ok {
        let line = format!(
            "{} {:<5} {}\n",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            level.as_str(),
            msg
        );
        log_append(&s, "revancex", &line);
        if let Some((prefix, _)) = msg.split_once(": ") {
            if s.items.contains(prefix) {
                let prefix = prefix.to_string();
                log_append(&s, &prefix, &line);
            }
        }
    }

    if level == Level::ERROR {
        emit(&mut s, Status::Fail, first_line(msg).to_string());
        return;
    }
    if level == Level::WARN {
        if s.verbose {
            emit(&mut s, Status::Warn, first_line(msg).to_string());
        }
        collect_warning(&mut s, msg);
        return;
    }
    let console_ok = if first_party {
        level <= s.console_level
    } else {
        level <= s.console_level.min(Level::INFO)
    };
    if console_ok {
        emit(&mut s, Status::Info, first_line(msg).to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_format() {
        assert_eq!(fmt_dur(Duration::from_millis(12_400)), "12.4s");
        assert_eq!(fmt_dur(Duration::from_secs(72)), "1m 12s");
        assert_eq!(fmt_dur(Duration::from_secs(3_725)), "1h 02m 05s");
    }

    #[test]
    fn size_format() {
        assert_eq!(fmt_size(512), "512 B");
        assert_eq!(fmt_size(128 * 1024 * 1024), "128.0 MB");
    }

    #[test]
    fn glyph_fallback_is_ascii() {
        assert_eq!(Status::Ok.glyph(false), "[OK]");
        assert_eq!(Status::Fail.glyph(false), "[FAIL]");
        assert_eq!(Status::Warn.glyph(false), "[WARN]");
        assert_eq!(Status::Ok.glyph(true), "✔");
    }

    #[test]
    fn redacts_secrets() {
        let secrets = vec!["hunter2hunter2".to_string()];
        assert_eq!(
            redact_with(&secrets, "--keystore-password=hunter2hunter2"),
            "--keystore-password=***"
        );
    }

    #[test]
    fn identical_consecutive_lines_collapse() {
        let mut s = State::detect();
        s.color = false;
        emit(&mut s, Status::Warn, "same".into());
        emit(&mut s, Status::Warn, "same".into());
        emit(&mut s, Status::Warn, "same".into());
        let p = s.pending.as_ref().unwrap();
        assert_eq!(p.count, 3);
        assert!(render_pending(&s, p).ends_with("same  (x3)"));
    }

    #[test]
    fn warnings_dedupe_with_count() {
        let mut s = State::detect();
        s.ci = false;
        collect_warning(&mut s, "patch X incompatible");
        collect_warning(&mut s, "patch X incompatible");
        collect_warning(&mut s, "other");
        assert_eq!(s.warnings.len(), 2);
        assert_eq!(s.warnings[0].1, 2);
    }

    #[test]
    fn table_is_aligned() {
        let rows = vec![
            Row {
                app: "YouTube".into(),
                version: "19.16.39".into(),
                ok: 42,
                skip: 3,
                fail: Some(0),
                size: Some(128 * 1024 * 1024),
                dur: Some(Duration::from_secs(72)),
                status: Status::Ok,
            },
            Row {
                app: "X".into(),
                version: String::new(),
                ok: 0,
                skip: 0,
                fail: None,
                size: None,
                dur: None,
                status: Status::Fail,
            },
        ];
        let t = render_table(&rows, false);
        assert_eq!(t.len(), 3);
        let col = t[0].find("Versi").unwrap();
        assert_eq!(t[1].find("19.16.39").unwrap(), col);
        assert_eq!(t[2].find('-').unwrap(), col);
    }

    #[test]
    fn hints_are_actionable() {
        assert!(hint_for("a: 0/5 patches applied").contains("patch"));
        assert!(hint_for("zzz").contains("log"));
    }
}
