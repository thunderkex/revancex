pub mod fetcher;

use crate::config::Config;
use anyhow::Result;
pub use fetcher::{FetchError, Fetcher};
use reqwest::Client;
use std::sync::Arc;

/// Process-wide HTTP clients and bypass fetcher built once per run and shared via `Arc`.
#[derive(Clone)]
pub struct SharedNet {
    pub browser_client: Arc<Client>,
    pub no_redirect_client: Arc<Client>,
    pub github_client: Arc<Client>,
    pub fetcher: Arc<Fetcher>,
}

impl SharedNet {
    pub fn new(cfg: &Config) -> Result<Self> {
        let browser = Arc::new(crate::utils::make_browser_client(cfg)?);
        let no_redirect = Arc::new(crate::utils::make_no_redirect_client(15)?);
        let github = Arc::new(crate::utils::make_github_client(cfg)?);
        let fetcher = Arc::new(Fetcher::new((*browser).clone(), cfg.fetch.clone()));
        Ok(Self {
            browser_client: browser,
            no_redirect_client: no_redirect,
            github_client: github,
            fetcher,
        })
    }
}
