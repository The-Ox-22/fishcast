mod config;
mod server;

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
    let _config = Config::load()?;

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    server::run(port).await
}
