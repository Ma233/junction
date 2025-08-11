use std::io::Write;
use std::io::{self};
use std::path::Path;

use clap::Arg;
use clap::ArgAction;
use clap::Command;
use clap::ValueEnum;
use serde_json::Value;
use tracing_subscriber::prelude::*;

#[derive(Clone, Debug, ValueEnum)]
enum MergeType {
    Json,
    Plaintext,
    Ini,
}

fn parse_args() -> Command {
    Command::new("junction-merger")
        .about("Merge files from multiple Junction sources")
        .version("0.1.0")
        .arg(
            Arg::new("sources")
                .help("Source URLs or file paths to fetch and merge")
                .required(true)
                .num_args(1..)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("type")
                .short('t')
                .long("type")
                .help("Type of files to merge")
                .value_parser(clap::value_parser!(MergeType))
                .required(true)
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Output file (default: stdout)")
                .num_args(1)
                .action(ArgAction::Set),
        )
}

async fn fetch_content(
    client: &reqwest::Client,
    source: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    if source.starts_with("http://") || source.starts_with("https://") {
        tracing::info!("Fetching from URL: {}", source);
        let response = client.get(source).send().await?;
        if response.status().is_success() {
            Ok(response.text().await?)
        } else {
            Err(format!("HTTP error {} from {}", response.status(), source).into())
        }
    } else {
        tracing::info!("Reading from file: {}", source);
        let path = Path::new(source);
        if !path.exists() {
            return Err(format!("File does not exist: {source}").into());
        }
        Ok(std::fs::read_to_string(path)?)
    }
}

fn merge_json_contents(contents: Vec<String>) -> Result<String, Box<dyn std::error::Error>> {
    let mut merged_object = serde_json::Map::new();

    for content in contents {
        let value: Value = serde_json::from_str(&content)?;

        if let Value::Object(obj) = value {
            for (key, val) in obj {
                merged_object.insert(key, val);
            }
        } else {
            return Err("All JSON sources must be objects".into());
        }
    }

    Ok(serde_json::to_string_pretty(&Value::Object(merged_object))?)
}

fn merge_plaintext_contents(contents: Vec<String>) -> String {
    contents.join("\n")
}

fn merge_ini_contents(contents: Vec<String>) -> Result<String, Box<dyn std::error::Error>> {
    use indexmap::IndexMap;

    let mut merged_map: IndexMap<String, IndexMap<String, Option<String>>> = IndexMap::new();

    for content in contents {
        // Parse INI content manually to preserve case
        let mut current_section = String::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                // Section header
                current_section = line[1..line.len() - 1].to_string();
                merged_map.entry(current_section.clone()).or_default();
            } else if let Some(eq_pos) = line.find('=') {
                // Key-value pair
                let key = line[..eq_pos].trim().to_string();
                let value = line[eq_pos + 1..].trim().to_string();

                let section_map = merged_map.entry(current_section.clone()).or_default();
                section_map.insert(key, if value.is_empty() { None } else { Some(value) });
            } else {
                // Key without value
                let key = line.to_string();
                let section_map = merged_map.entry(current_section.clone()).or_default();
                section_map.insert(key, None);
            }
        }
    }

    // Convert back to INI format
    let mut output = String::new();

    for (section_name, section) in merged_map {
        if !section_name.is_empty() {
            output.push_str(&format!("[{section_name}]\n"));
        }

        for (key, value) in section {
            match value {
                Some(val) => output.push_str(&format!("{key}={val}\n")),
                None => output.push_str(&format!("{key}\n")),
            }
        }
        output.push('\n');
    }

    Ok(output)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let matches = parse_args().get_matches();
    let sources: Vec<&String> = matches
        .get_many::<String>("sources")
        .unwrap_or_default()
        .collect();

    let merge_type = matches.get_one::<MergeType>("type").unwrap();
    let output_file = matches.get_one::<String>("output");

    let client = reqwest::Client::new();
    let mut contents = Vec::new();

    for source in sources {
        match fetch_content(&client, source).await {
            Ok(content) => contents.push(content),
            Err(e) => {
                tracing::error!("Failed to fetch from {source}: {e}");
                eprintln!("Failed to fetch from {source}: {e}");
                std::process::exit(1);
            }
        }
    }

    let merged_content = match merge_type {
        MergeType::Json => merge_json_contents(contents)?,
        MergeType::Plaintext => merge_plaintext_contents(contents),
        MergeType::Ini => merge_ini_contents(contents)?,
    };

    if let Some(output_path) = output_file {
        std::fs::write(output_path, &merged_content)?;
        tracing::info!("Output written to: {}", output_path);
    } else {
        io::stdout().write_all(merged_content.as_bytes())?;
    }

    Ok(())
}
