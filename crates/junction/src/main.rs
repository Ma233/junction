use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;

use clap::value_parser;
use clap::Arg;
use clap::ArgAction;
use clap::Command;
use tracing_subscriber::prelude::*;

fn parse_args() -> Command {
    Command::new("junction")
        .about("A file generator")
        .version(junction::version())
        .arg(
            Arg::new("API_ADDR")
                .long("api-addr")
                .env("JUNCTION_API_ADDR")
                .num_args(1)
                .default_value("0.0.0.0:7749")
                .action(ArgAction::Set)
                .help("API listen address"),
        )
        .arg(
            Arg::new("DATA_DIR")
                .long("data-dir")
                .env("JUNCTION_DATA_DIR")
                .num_args(1)
                .default_value("./data")
                .action(ArgAction::Set)
                .help("Will set this path as the runtime directory for commands"),
        )
        .arg(
            Arg::new("CONFIG_FILE")
                .long("config")
                .env("JUNCTION_CONFIG")
                .num_args(1)
                .default_value("./data/config.yaml")
                .value_parser(value_parser!(PathBuf))
                .action(ArgAction::Set)
                .help("Path to config file (YAML format)"),
        )
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(
                    tracing_subscriber::EnvFilter::builder()
                        .with_default_directive(
                            tracing_subscriber::filter::LevelFilter::INFO.into(),
                        )
                        .from_env_lossy(),
                ),
        )
        .init();

    let args = parse_args().get_matches();

    let api_addr = args
        .get_one::<String>("API_ADDR")
        .unwrap()
        .parse::<SocketAddr>()
        .expect("Invalid API address");

    let data_dir = Path::new(args.get_one::<String>("DATA_DIR").unwrap());
    if !data_dir.exists() {
        fs::create_dir_all(data_dir).expect("Failed to create data directory");
        tracing::info!("Created data directory: {}", data_dir.display());
    }

    let config_file_path = args.get_one::<PathBuf>("CONFIG_FILE").unwrap();
    let config = junction::Config::from_yaml_file(config_file_path).expect("Failed to load config");
    let resolved_config = junction::ResolvedConfig::new(config, data_dir.to_path_buf())
        .expect("Failed to resolve config");

    junction::serve(api_addr, resolved_config)
        .await
        .expect("Failed to start the server");
}
