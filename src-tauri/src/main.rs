// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use digest_lib::{DigestMcpServer, DigestService, DigestTools};
use rmcp::{transport::stdio, ServiceExt};
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--digest-mcp") {
        let data_dir = parse_data_dir(args).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(2);
        });
        let runtime = tokio::runtime::Runtime::new().expect("create MCP runtime");
        runtime.block_on(async move {
            let service = Arc::new(DigestService::open(data_dir).expect("open Digest data"));
            let running = DigestMcpServer::new(DigestTools::new(service))
                .serve(stdio())
                .await
                .expect("start Digest MCP server");
            running.waiting().await.expect("run Digest MCP server");
        });
    } else {
        digest_lib::run();
    }
}

fn parse_data_dir(mut args: impl Iterator<Item = String>) -> Result<PathBuf, String> {
    match (args.next().as_deref(), args.next()) {
        (Some("--data-dir"), Some(path)) if args.next().is_none() => Ok(path.into()),
        _ => Err("usage: digest --digest-mcp --data-dir PATH".into()),
    }
}
