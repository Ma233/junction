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
        tracing::debug!("Modify PATH environment variable to: {}", modified_path);
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

    // Add the original PATH at the end
    path_parts.push(current_path);

    Some(path_parts.join(":"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use poem::test::TestClient;
    use tempfile::TempDir;

    use super::*;

    fn create_test_config() -> ResolvedConfig {
        let outputs = HashMap::from([
            ("echo-hello".to_string(), crate::config::OutputConfig {
                slug: "echo-hello".to_string(),
                cmd: "/bin/echo".to_string(),
                args: vec!["hello".to_string(), "world".to_string()],
            }),
            ("pwd".to_string(), crate::config::OutputConfig {
                slug: "pwd".to_string(),
                cmd: "/bin/pwd".to_string(),
                args: vec![],
            }),
        ]);

        ResolvedConfig {
            outputs,
            data_dir: std::env::temp_dir(),
        }
    }

    #[tokio::test]
    async fn test_get_config_endpoint() {
        let config = create_test_config();
        let app = app(config.clone());
        let client = TestClient::new(app);

        let resp = client.get("/config").send().await;
        resp.assert_status_is_ok();
        resp.assert_content_type("application/json; charset=utf-8");

        let returned_config: ResolvedConfig = resp.json().await.value().deserialize();
        assert_eq!(returned_config.outputs.len(), config.outputs.len());
        assert!(returned_config.outputs.contains_key("echo-hello"));
        assert!(returned_config.outputs.contains_key("pwd"));
    }

    #[tokio::test]
    async fn test_get_output_existing_slug() {
        let config = create_test_config();
        let app = app(config);
        let client = TestClient::new(app);

        let resp = client.get("/output/echo-hello").send().await;

        let status = resp.0.status();
        let body = resp.0.into_body().into_string().await.unwrap();

        // Debug: Print status and body if not OK
        if status != poem::http::StatusCode::OK {
            panic!("Expected OK, got {status}: {body}");
        }

        assert_eq!(status, poem::http::StatusCode::OK);
        assert!(body.contains("hello world"));
    }

    #[tokio::test]
    async fn test_get_output_nonexistent_slug() {
        let config = create_test_config();
        let app = app(config);
        let client = TestClient::new(app);

        let resp = client.get("/output/nonexistent").send().await;
        resp.assert_status(poem::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_output_with_pwd_command() {
        let temp_dir = TempDir::new().unwrap();
        let outputs = HashMap::from([("pwd".to_string(), crate::config::OutputConfig {
            slug: "pwd".to_string(),
            cmd: "/bin/pwd".to_string(),
            args: vec![],
        })]);

        let config = ResolvedConfig {
            outputs,
            data_dir: temp_dir.path().to_path_buf(),
        };

        let app = app(config);
        let client = TestClient::new(app);

        let resp = client.get("/output/pwd").send().await;

        let status = resp.0.status();
        let body = resp.0.into_body().into_string().await.unwrap();

        if status != poem::http::StatusCode::OK {
            panic!("Expected OK, got {status}: {body}");
        }

        assert_eq!(status, poem::http::StatusCode::OK);
        assert!(body.contains(temp_dir.path().to_str().unwrap()));
    }

    #[tokio::test]
    async fn test_get_output_invalid_command() {
        let outputs = HashMap::from([("invalid".to_string(), crate::config::OutputConfig {
            slug: "invalid".to_string(),
            cmd: "this-command-does-not-exist-12345".to_string(),
            args: vec![],
        })]);

        let config = ResolvedConfig {
            outputs,
            data_dir: std::env::temp_dir(),
        };

        let app = app(config);
        let client = TestClient::new(app);

        let resp = client.get("/output/invalid").send().await;
        resp.assert_status(poem::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_get_modified_path_with_existing_path() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Set a mock PATH environment variable for testing
        let original_path = std::env::var("PATH").unwrap_or_default();
        let test_path = format!("/usr/bin:/bin:{original_path}");
        std::env::set_var("PATH", &test_path);

        let result = get_modified_path(data_dir);
        assert!(result.is_some());

        let modified_path = result.unwrap();
        assert!(modified_path.contains(data_dir.to_str().unwrap()));
        assert!(modified_path.contains(&test_path));

        // Restore original PATH
        std::env::set_var("PATH", original_path);
    }

    #[test]
    fn test_get_modified_path_already_in_path() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Set PATH to already include the data_dir
        let original_path = std::env::var("PATH").unwrap_or_default();
        let test_path = format!(
            "{}:/usr/bin:/bin:{}",
            data_dir.to_str().unwrap(),
            original_path
        );
        std::env::set_var("PATH", &test_path);

        let result = get_modified_path(data_dir);
        assert!(result.is_some());

        let modified_path = result.unwrap();
        // Should contain the original PATH which already includes data_dir
        let path_count = modified_path
            .split(':')
            .filter(|p| *p == data_dir.to_str().unwrap())
            .count();
        assert_eq!(path_count, 1); // Should be 1 from the original PATH

        // Restore original PATH
        std::env::set_var("PATH", original_path);
    }

    #[test]
    fn test_output_config_get_command_parts() {
        let output = crate::config::OutputConfig {
            slug: "test".to_string(),
            cmd: "ls".to_string(),
            args: vec!["-la".to_string(), "/tmp".to_string()],
        };

        let (cmd, args) = output.get_command_parts();
        assert_eq!(cmd, "ls");
        assert_eq!(args, vec!["-la", "/tmp"]);
    }
}
