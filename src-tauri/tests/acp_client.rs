use digest_lib::{AcpClient, AgentProvider, DigestService, McpLaunchSpec};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn opencode_uses_its_native_acp_entrypoint_without_a_shell() {
    let launch = AgentProvider::OpenCode.launch_spec();

    assert_eq!(launch.command, PathBuf::from("opencode"));
    assert_eq!(launch.args, vec!["acp"]);
}

#[test]
fn agy_uses_the_configured_adapter_without_guessing_its_cli() {
    let launch = AgentProvider::Agy {
        adapter_command: PathBuf::from("/opt/digest/bin/agy-acp"),
        adapter_args: vec!["--profile".into(), "learning".into()],
    }
    .launch_spec();

    assert_eq!(launch.command, PathBuf::from("/opt/digest/bin/agy-acp"));
    assert_eq!(launch.args, vec!["--profile", "learning"]);
}

#[test]
fn digest_mcp_is_injected_as_an_absolute_stdio_server() {
    let launch = McpLaunchSpec::new(
        PathBuf::from("/opt/digest/bin/digest-mcp"),
        PathBuf::from("/var/lib/digest"),
    )
    .expect("valid launch spec");
    let server = launch.acp_server();
    let encoded = serde_json::to_value(server).expect("serialize ACP server");

    assert_eq!(encoded["name"], "digest");
    assert_eq!(encoded["command"], "/opt/digest/bin/digest-mcp");
    assert_eq!(
        encoded["args"],
        serde_json::json!(["--data-dir", "/var/lib/digest"])
    );
}

#[test]
fn digest_mcp_rejects_relative_executable_paths() {
    let error = McpLaunchSpec::new(
        PathBuf::from("target/debug/digest-mcp"),
        PathBuf::from("/var/lib/digest"),
    )
    .expect_err("relative executable must be rejected");

    assert!(error.to_string().contains("absolute"));
}

/// Live harness validation. Spawns the real `opencode acp` binary and reads the
/// models it advertises. Ignored by default; run it explicitly:
///
/// ```text
/// cargo test --test acp_client -- --ignored
/// ```
#[tokio::test]
#[ignore = "requires the opencode binary on PATH"]
async fn opencode_advertises_selectable_models() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let models = AcpClient::new(service)
        .list_agent_models(&AgentProvider::OpenCode, directory.path())
        .await
        .expect("list models from live opencode");

    assert!(!models.is_empty(), "opencode must advertise at least one model");
    assert!(
        models.iter().all(|model| model.id.contains('/')),
        "model ids carry their provider: {}",
        serde_json::to_string(&models).expect("serialize models"),
    );
    assert!(
        models.iter().any(|model| model.current),
        "one advertised model is the harness default",
    );
}
