use crate::config::sources::CircuitBreakersConfig;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    TurnstileChallenge,
    IpReputationBlock,
    RateLimited,
    NotFound,
    Other,
}

impl std::fmt::Display for BlockKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockKind::TurnstileChallenge => write!(f, "turnstile_challenge"),
            BlockKind::IpReputationBlock => write!(f, "ip_reputation_block"),
            BlockKind::RateLimited => write!(f, "rate_limited"),
            BlockKind::NotFound => write!(f, "not_found"),
            BlockKind::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl std::fmt::Display for BreakerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BreakerState::Closed => write!(f, "closed"),
            BreakerState::Open => write!(f, "open"),
            BreakerState::HalfOpen => write!(f, "half-open"),
        }
    }
}

#[derive(Debug)]
struct BreakerInner {
    state: BreakerState,
    failed_apps: HashSet<String>,
    opened_at: Option<Instant>,
    apps_since_opened: usize,
    probe_in_flight: bool,
    probe_app: Option<String>,
    probe_started_at: Option<Instant>,
    ever_opened: bool,
}

#[derive(Debug)]
pub struct SourceCircuitBreaker {
    pub name: String,
    pub threshold: u32,
    pub cooldown: Duration,
    pub cooldown_apps: usize,
    inner: Mutex<BreakerInner>,
}

impl SourceCircuitBreaker {
    pub fn new(name: &str, threshold: u32, cooldown_secs: u64, cooldown_apps: usize) -> Self {
        Self {
            name: name.to_string(),
            threshold,
            cooldown: Duration::from_secs(cooldown_secs),
            cooldown_apps,
            inner: Mutex::new(BreakerInner {
                state: BreakerState::Closed,
                failed_apps: HashSet::new(),
                opened_at: None,
                apps_since_opened: 0,
                probe_in_flight: false,
                probe_app: None,
                probe_started_at: None,
                ever_opened: false,
            }),
        }
    }

    pub fn can_try(&self, app_id: &str) -> Result<BreakerState, (BreakerState, String)> {
        let mut inner = self.inner.lock().unwrap();
        match inner.state {
            BreakerState::Closed => Ok(BreakerState::Closed),
            BreakerState::Open => {
                let time_expired = inner
                    .opened_at
                    .is_some_and(|opened| opened.elapsed() >= self.cooldown);
                let apps_expired =
                    self.cooldown_apps > 0 && inner.apps_since_opened >= self.cooldown_apps;

                if time_expired || apps_expired {
                    inner.state = BreakerState::HalfOpen;
                    inner.probe_in_flight = true;
                    inner.probe_app = Some(app_id.to_string());
                    inner.probe_started_at = Some(Instant::now());
                    Ok(BreakerState::HalfOpen)
                } else {
                    let reason = format!(
                        "circuit-breaker open ({}/{} failed apps)",
                        inner.failed_apps.len(),
                        self.threshold
                    );
                    Err((BreakerState::Open, reason))
                }
            }
            BreakerState::HalfOpen => {
                // Auto-clear stale probe if in flight for more than 45 seconds
                if inner.probe_in_flight
                    && inner
                        .probe_started_at
                        .is_some_and(|t| t.elapsed() > Duration::from_secs(45))
                {
                    inner.probe_in_flight = false;
                    inner.probe_app = None;
                    inner.probe_started_at = None;
                }

                if !inner.probe_in_flight {
                    inner.probe_in_flight = true;
                    inner.probe_app = Some(app_id.to_string());
                    inner.probe_started_at = Some(Instant::now());
                    Ok(BreakerState::HalfOpen)
                } else if inner.probe_app.as_deref() == Some(app_id) {
                    // Re-entrant call from the app currently probing this source
                    Ok(BreakerState::HalfOpen)
                } else {
                    Err((
                        BreakerState::HalfOpen,
                        "circuit-breaker half-open (probe in progress)".to_string(),
                    ))
                }
            }
        }
    }

    pub fn record_success(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.state = BreakerState::Closed;
        inner.failed_apps.clear();
        inner.probe_in_flight = false;
        inner.probe_app = None;
        inner.probe_started_at = None;
        inner.opened_at = None;
        inner.apps_since_opened = 0;
    }

    pub fn record_failure(&self, app_id: &str, block_kind: &BlockKind) {
        let mut inner = self.inner.lock().unwrap();
        match block_kind {
            BlockKind::NotFound => {
                if inner.state == BreakerState::HalfOpen
                    && inner.probe_app.as_deref() == Some(app_id)
                {
                    inner.probe_in_flight = false;
                    inner.probe_app = None;
                    inner.probe_started_at = None;
                }
            }
            BlockKind::IpReputationBlock => {
                // Immediate open on confirmed IP reputation block
                inner.state = BreakerState::Open;
                inner.opened_at = Some(Instant::now());
                inner.ever_opened = true;
                inner.failed_apps.insert(app_id.to_string());
                inner.probe_in_flight = false;
                inner.probe_app = None;
                inner.probe_started_at = None;
                inner.apps_since_opened = 0;
            }
            _ => {
                inner.failed_apps.insert(app_id.to_string());
                if inner.state == BreakerState::HalfOpen {
                    // Half-open probe failed -> re-open
                    inner.state = BreakerState::Open;
                    inner.opened_at = Some(Instant::now());
                    inner.probe_in_flight = false;
                    inner.probe_app = None;
                    inner.probe_started_at = None;
                    inner.apps_since_opened = 0;
                } else if inner.failed_apps.len() >= self.threshold as usize {
                    inner.state = BreakerState::Open;
                    inner.opened_at = Some(Instant::now());
                    inner.ever_opened = true;
                    inner.apps_since_opened = 0;
                }
            }
        }
    }

    pub fn record_app_completed(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.state == BreakerState::Open {
            inner.apps_since_opened += 1;
        } else if inner.state == BreakerState::HalfOpen {
            inner.probe_in_flight = false;
            inner.probe_app = None;
            inner.probe_started_at = None;
        }
    }

    pub fn state(&self) -> BreakerState {
        self.inner.lock().unwrap().state
    }

    pub fn ever_opened(&self) -> bool {
        self.inner.lock().unwrap().ever_opened
    }
}

#[derive(Debug, Clone)]
pub struct BreakerRegistry {
    breakers: Arc<Mutex<HashMap<String, Arc<SourceCircuitBreaker>>>>,
    default_threshold: u32,
    default_cooldown_secs: u64,
    default_cooldown_apps: usize,
    source_configs: HashMap<String, (u32, u64, usize)>,
}

impl BreakerRegistry {
    pub fn new(config: &CircuitBreakersConfig) -> Self {
        let env_threshold = std::env::var("REVANCEX_CB_THRESHOLD")
            .or_else(|_| std::env::var("REVANCEX_UTD_CB_THRESHOLD"))
            .ok()
            .and_then(|v| v.parse::<u32>().ok());

        let env_cooldown = std::env::var("REVANCEX_CB_COOLDOWN_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok());

        let default_threshold = env_threshold.unwrap_or(config.default.threshold);
        let default_cooldown_secs = env_cooldown.unwrap_or(config.default.cooldown_secs);
        let default_cooldown_apps = config.default.cooldown_apps;

        let mut source_configs = HashMap::new();
        for (src, rule) in &config.sources {
            source_configs.insert(
                src.clone(),
                (rule.threshold, rule.cooldown_secs, rule.cooldown_apps),
            );
        }

        Self {
            breakers: Arc::new(Mutex::new(HashMap::new())),
            default_threshold,
            default_cooldown_secs,
            default_cooldown_apps,
            source_configs,
        }
    }

    pub fn get(&self, source_name: &str) -> Arc<SourceCircuitBreaker> {
        let mut lock = self.breakers.lock().unwrap();
        if let Some(cb) = lock.get(source_name) {
            cb.clone()
        } else {
            let (thresh, cd_secs, cd_apps) =
                self.source_configs.get(source_name).copied().unwrap_or((
                    self.default_threshold,
                    self.default_cooldown_secs,
                    self.default_cooldown_apps,
                ));
            let cb = Arc::new(SourceCircuitBreaker::new(
                source_name,
                thresh,
                cd_secs,
                cd_apps,
            ));
            lock.insert(source_name.to_string(), cb.clone());
            cb
        }
    }

    pub fn can_try(
        &self,
        source_name: &str,
        app_id: &str,
    ) -> Result<BreakerState, (BreakerState, String)> {
        self.get(source_name).can_try(app_id)
    }

    pub fn record_success(&self, source_name: &str) {
        self.get(source_name).record_success();
    }

    pub fn record_failure(&self, source_name: &str, app_id: &str, block_kind: &BlockKind) {
        self.get(source_name).record_failure(app_id, block_kind);
    }

    pub fn record_app_completed(&self) {
        let lock = self.breakers.lock().unwrap();
        for cb in lock.values() {
            cb.record_app_completed();
        }
    }

    pub fn open_breakers(&self) -> Vec<String> {
        let lock = self.breakers.lock().unwrap();
        let mut open = Vec::new();
        for (name, cb) in lock.iter() {
            if cb.ever_opened() || cb.state() == BreakerState::Open {
                open.push(name.clone());
            }
        }
        open.sort();
        open
    }
}

pub fn classify_error(err_str: &str) -> BlockKind {
    let lower = err_str.to_lowercase();
    if lower.contains("1020")
        || lower.contains("ipreputation")
        || lower.contains("ip_reputation")
        || lower.contains("access denied")
        || lower.contains("ip reputation")
    {
        BlockKind::IpReputationBlock
    } else if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("rate_limited")
    {
        BlockKind::RateLimited
    } else if lower.contains("404") || lower.contains("not found") || lower.contains("not_found") {
        BlockKind::NotFound
    } else if lower.contains("turnstile")
        || lower.contains("challenge")
        || lower.contains("just a moment")
        || lower.contains("cf-chl")
        || lower.contains("cloudflare")
    {
        BlockKind::TurnstileChallenge
    } else {
        BlockKind::Other
    }
}
