mod params;
mod server;

use std::sync::Arc;

use bulletd_core::config::{Config, resolve_data_dir};
use bulletd_core::ops::Store;
use rmcp::ServiceExt;
use rmcp::transport::stdio;

use server::{BulletdMcpServer, McpState};

/// Run the MCP server on stdio. Blocks until stdin is closed.
pub async fn run_server(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = resolve_data_dir(&config.general.data_dir);
    let store = Store::new(data_dir);

    let state = Arc::new(McpState {
        store,
        config: config.clone(),
    });

    let server = BulletdMcpServer::new(state);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
