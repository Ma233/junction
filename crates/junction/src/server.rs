use std::net::SocketAddr;
use std::sync::Arc;

use poem::get;
use poem::handler;
use poem::listener::TcpListener;
use poem::middleware::AddData;
use poem::middleware::Cors;
use poem::web::Data;
use poem::web::Json;
use poem::web::Path;
use poem::Endpoint;
use poem::EndpointExt;
use poem::Response;
use poem::Result;
use poem::Route;
use poem::Server;
use tokio::process::Command;

use crate::config::ResolvedConfig;

pub fn app(config: ResolvedConfig) -> impl Endpoint {
    Route::new()
        .at("/config", get(get_config))
        .at("/output/:slug", get(get_output))
        .with(Cors::new())
        .with(AddData::new(Arc::new(config)))
}

pub async fn serve(server_addr: SocketAddr, config: ResolvedConfig) -> Result<(), std::io::Error> {
    let app = app(config);

    tracing::info!("Starting server at {}", server_addr);
    Server::new(TcpListener::bind(server_addr)).run(app).await
}

#[handler]
async fn get_config(config: Data<&Arc<ResolvedConfig>>) -> Json<ResolvedConfig> {
    Json(config.as_ref().clone())
}

#[handler]
async fn get_output(
    config: Data<&Arc<ResolvedConfig>>,
    Path(slug): Path<String>,
) -> Result<Response> {
    let output_config = config
        .get_output_by_slug(&slug)
        .ok_or_else(|| poem::Error::from_status(poem::http::StatusCode::NOT_FOUND))?;

    let (cmd, args) = output_config.get_command_parts();
    let mut command = Command::new(cmd);
    command.args(args).current_dir(&config.data_dir);

    if let Some(modified_path) = get_modified_path(&config.data_dir) {
        command.env("PATH", modified_path);
    }

    let output = command.output().await.map_err(|e| {
        poem::Error::from_string(
            format!("Failed to execute command: {e}"),
            poem::http::StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;

    // Always log stderr to server logs
    if !output.stderr.is_empty() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            tracing::info!("Command stderr output:\n{}", stderr_str);
        } else {
            tracing::error!(
                "Command failed with status: {}. Stderr:\n{}",
                output.status,
                stderr_str
            );
        }
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(poem::Error::from_string(
            stderr.to_string(),
            poem::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
    }

    let content = String::from_utf8(output.stdout.clone())
        .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).to_string());

    Ok(Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(content))
}

fn get_modified_path(data_dir: &std::path::Path) -> Option<String> {
    let Ok(current_path) = std::env::var("PATH") else {
        tracing::warn!("Failed to read PATH environment variable");
        return None;
    };

    let mut path_parts = Vec::new();

    // Try to add current executable directory to PATH
    match std::env::current_exe() {
        Ok(current_exe) => {
            match current_exe.parent() {
                Some(exe_dir) => {
                    let exe_dir_str = exe_dir.to_string_lossy();
                    // Add exe_dir if not already in PATH
                    if !current_path.split(':').any(|p| p == exe_dir_str) {
                        path_parts.push(exe_dir_str.to_string());
                    }
                }
                None => {
                    tracing::warn!("Failed to get parent directory of executable");
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to get current executable path: {}", e);
        }
    }

    // Try to add data_dir to PATH
    let data_dir_str = data_dir.to_string_lossy();
    if !current_path.split(':').any(|p| p == data_dir_str) {
        path_parts.push(data_dir_str.to_string());
    }

    // In case data directory might be the same as current executable directory
    path_parts.dedup();

    Some(path_parts.join(":"))
}
