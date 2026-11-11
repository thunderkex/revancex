pub(crate) mod builder;
pub mod cli;
pub(crate) mod config;
pub(crate) mod module;
pub(crate) mod net;
pub(crate) mod orchestrator;
pub(crate) mod pages;
pub(crate) mod patcher;
pub(crate) mod report;
pub(crate) mod sources;
pub(crate) mod testing;
pub(crate) mod ui;
pub(crate) mod utils;

#[cfg(test)]
mod ci_tests;
