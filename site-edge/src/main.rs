//! The Phase 1 edge server: builds the business-site release, proves the digest
//! match, then serves human pages, agent twins and the public CLI on 127.0.0.1.

use agentrix_site_edge::{RateLimit, build_release, business_site, digest_match, router};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let release = build_release(business_site(), "business", vec!["fixture".into()]);
    if let Err(problems) = digest_match(&release) {
        for problem in &problems {
            tracing::error!(problem = %problem, "digest match failed");
        }
        eprintln!("DIGEST MATCH FAILED: {} problem(s)", problems.len());
        std::process::exit(1);
    }
    let app = router(release, RateLimit::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8471")
        .await
        .expect("bind 127.0.0.1:8471");
    tracing::info!(addr = %listener.local_addr().expect("local addr"), "serving the read-only dual site");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .expect("serve");
}
