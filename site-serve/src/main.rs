//! The composed site server: `site-serve [--data DIR] [--addr ADDR] [--dry-run]`.

use std::path::PathBuf;

use agentrix_site_serve::{ServeConfig, serve};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let mut data: PathBuf = "data".into();
    let mut addr = "127.0.0.1:8472".to_string();
    let mut dry_run = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data" => {
                data = args
                    .next()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "data".into());
            }
            "--addr" => {
                addr = args.next().unwrap_or_else(|| "127.0.0.1:8472".into());
            }
            "--dry-run" => dry_run = true,
            other => {
                eprintln!("unknown argument {other}; use --data/--addr/--dry-run");
                std::process::exit(2);
            }
        }
    }
    let config = ServeConfig::from_env(data, addr, dry_run);
    if let Err(error) = serve(&config).await {
        eprintln!("SERVE FAILED: {error:#}");
        std::process::exit(1);
    }
}
