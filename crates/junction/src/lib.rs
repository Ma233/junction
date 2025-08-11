mod config;
mod server;

pub use config::Config;
pub use config::ResolvedConfig;
pub use server::serve;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_VERSION: &str = git_version::git_version!();
static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
pub fn version() -> &'static str {
    VERSION
        .get_or_init(|| format!("{PKG_VERSION}-{GIT_VERSION}"))
        .as_str()
}
