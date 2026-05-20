//! Minimal example: list all clusters and print basic info.
//!
//! Run with:
//!   VMS_ADDRESS=vms.example.com VMS_TOKEN=<token> cargo run --example list_clusters

use vast_rs::ClientConfig;

#[tokio::main]
async fn main() -> vast_rs::Result<()> {
    tracing_subscriber::fmt::init();

    let client = vast_rs::VastClient::new(ClientConfig::from_env()?)?;

    let clusters = client.clusters().list().await?;
    println!("Found {} cluster(s):", clusters.len());
    for c in &clusters {
        println!(
            "  [{id}] {name}  version={ver}  state={state}",
            id = c.id,
            name = c.name,
            ver = if c.sw_version.is_empty() { "unknown" } else { &c.sw_version },
            state = if c.state.is_empty() { "unknown" } else { &c.state },
        );
    }

    Ok(())
}
