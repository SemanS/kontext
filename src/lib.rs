//! Standalone knowledge engine with a versioned application boundary and an embeddable verifier.
mod adapters;
mod api;
mod api_read;
mod api_write;
mod app;
pub mod application;
mod cli;
mod config;
mod connect;
mod distill;
mod drift;
mod events;
mod evidence;
mod freshness;
mod glob;
mod hooks;
mod inbox;
mod index;
mod init;
mod jpath;
mod ledger;
mod mcp;
mod model;
mod ops;
mod presets;
mod related;
mod repo;
mod route;
mod secrets;
mod signals;
mod skills;
mod store;
mod template;
mod threads;
mod tools;
mod util;
mod verification;

/// Run the normal CLI using the same private engine as application clients.
pub fn run_cli() {
    cli::main();
}
