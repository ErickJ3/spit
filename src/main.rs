use clap::Parser;

use spit::{cli::{Cli, Commands}, load_config, start_server};

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Scan {
            url,
            port,
            host,
            delay,
            config: config_path,
        } => {
            let config = load_config(config_path)?;
            start_server(url, host, *port, *delay, config).await?;
        }
        Commands::File {
            path,
            port,
            host,
            delay,
            config: config_path,
        } => {
            let path = path.to_str().ok_or("Invalid path")?;
            let config = load_config(config_path)?;
            start_server(path, host, *port, *delay, config).await?;
        }
    }

    Ok(())
}
