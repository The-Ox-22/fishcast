mod config;
mod conditions;
mod location;
mod report;
mod rules;
mod server;
mod solunar;
mod species;
mod tags;
mod water;
mod weather;

use config::Config;

#[tokio::main]
async fn main() {
    let result = run().await;

    if let Err(err) = result {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let config = Config::load()?;

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    server::run(port, config).await
}
