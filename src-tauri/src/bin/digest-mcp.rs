use digest_lib::{DigestMcpServer, DigestService, DigestTools};
use rmcp::{transport::stdio, ServiceExt};
use std::{env, path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = data_dir_from_args()?;
    let service = Arc::new(DigestService::open(data_dir)?);
    let server = DigestMcpServer::new(DigestTools::new(service));
    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}

fn data_dir_from_args() -> Result<PathBuf, String> {
    let mut arguments = env::args_os().skip(1);
    if let Some(argument) = arguments.next() {
        if argument != "--data-dir" {
            return Err(format!("unknown argument: {}", argument.to_string_lossy()));
        }
        return arguments
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "--data-dir requires a path".into());
    }

    env::var_os("DIGEST_DATA_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "provide --data-dir or DIGEST_DATA_DIR".into())
}
