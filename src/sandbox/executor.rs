use libc;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command as TokioCommand;

/// Permission modes for sandbox execution
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionMode {
    /// Read-only access to files, no network, no ability to modify system
    ReadOnly,
    /// Sandboxed execution requiring explicit approval for certain operations
    SandboxedWithApproval,
    /// Automatic repair allowed (limited system access)
    AutoRepair,
    /// Full access (dangerous, should be avoided)
    DangerFullAccess,
}

/// Network policy for execution
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkPolicy {
    Disabled,
    Enabled,
}

/// Resource limits for execution
#[derive(Clone, Debug)]
pub struct ResourceLimits {
    pub max_memory_bytes: Option<u64>,
    pub max_file_size_bytes: Option<u64>,
    pub max_processes: Option<u32>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: Some(512 * 1024 * 1024),   // 512MB
            max_file_size_bytes: Some(50 * 1024 * 1024), // 50MB
            max_processes: Some(32),
        }
    }
}

/// Structured request for safe, isolated execution
#[derive(Clone, Debug)]
pub struct ExecutionRequest {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub timeout: Duration,
    pub environment_allowlist: Vec<String>,
    pub network_policy: NetworkPolicy,
    pub resource_limits: ResourceLimits,
}

impl ExecutionRequest {
    pub fn new<P: Into<PathBuf>>(program: P, cwd: P) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            working_directory: cwd.into(),
            timeout: Duration::from_secs(5),
            environment_allowlist: vec![
                "PATH".to_string(),
                "RUST_LOG".to_string(),
                "RUST_BACKTRACE".to_string(),
                "TERM".to_string(),
            ],
            network_policy: NetworkPolicy::Disabled,
            resource_limits: ResourceLimits::default(),
        }
    }
}

/// Configuration for sandbox execution
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    pub permission_mode: PermissionMode,
    pub timeout_ms: u64, // Default 5000ms
    pub cwd: PathBuf,
    pub enable_network: bool,
    pub enable_user_namespace: bool,
    pub drop_capabilities: bool,
    pub danger_mode: bool,
}

/// Result of sandbox execution
#[derive(Clone, Debug)]
pub struct SandboxResult {
    pub success: bool,
    pub return_code: Option<i32>,
    pub interrupted: bool, // true if timeout occurred
    pub stdout: String,
    pub stderr: String,
}

/// Errors that can occur during sandbox execution
#[derive(Debug)]
pub enum SandboxError {
    SpawnFailed(io::Error),
    WaitFailed(io::Error),
    PermissionDenied(String),
    InvalidConfig(String),
}

impl From<io::Error> for SandboxError {
    fn from(e: io::Error) -> Self {
        SandboxError::WaitFailed(e)
    }
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::SpawnFailed(e) => write!(f, "Failed to spawn sandbox process: {}", e),
            SandboxError::WaitFailed(e) => write!(f, "Failed to wait on sandbox process: {}", e),
            SandboxError::PermissionDenied(s) => write!(f, "Permission denied: {}", s),
            SandboxError::InvalidConfig(s) => write!(f, "Invalid sandbox configuration: {}", s),
        }
    }
}

impl std::error::Error for SandboxError {}

/// Executes a structured `ExecutionRequest` asynchronously with non-blocking timeouts,
/// strict environment variable filtering, and process group isolation.
pub async fn execute_request(request: &ExecutionRequest) -> Result<SandboxResult, SandboxError> {
    if request.timeout.as_millis() == 0 {
        return Err(SandboxError::InvalidConfig(
            "Timeout must be greater than zero".to_string(),
        ));
    }

    let mut cmd = TokioCommand::new(&request.program);
    cmd.args(&request.args)
        .current_dir(&request.working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    cmd.env_clear();

    // Pass environment variables only if present in environment_allowlist and non-secret
    for var_name in &request.environment_allowlist {
        if let Ok(val) = std::env::var(var_name) {
            let key_upper = var_name.to_uppercase();
            let is_secret = key_upper.contains("_TOKEN")
                || key_upper.contains("_SECRET")
                || key_upper.contains("_PASSWORD")
                || key_upper.contains("_KEY")
                || key_upper.contains("DATABASE_URL");

            if !is_secret {
                cmd.env(var_name, val);
            }
        }
    }

    // Set process group in pre_exec for group termination on timeout
    unsafe {
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let child = cmd.spawn().map_err(SandboxError::SpawnFailed)?;
    let child_pid = child.id().map(|id| id as i32);

    match tokio::time::timeout(request.timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(SandboxResult {
            success: output.status.success(),
            return_code: output.status.code(),
            interrupted: false,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }),
        Ok(Err(e)) => Err(SandboxError::WaitFailed(e)),
        Err(_) => {
            // Kill entire process group with SIGKILL
            if let Some(pgid) = child_pid {
                unsafe {
                    libc::kill(-pgid, libc::SIGKILL);
                }
            }

            Ok(SandboxResult {
                success: false,
                return_code: None,
                interrupted: true,
                stdout: String::new(),
                stderr: format!(
                    "Execution timed out after {} seconds",
                    request.timeout.as_secs()
                ),
            })
        }
    }
}

/// Execute code in isolated Linux namespace with non-blocking Tokio timeout guarantee
pub async fn execute_in_sandbox(
    code: &str,
    config: &SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    if config.timeout_ms == 0 {
        return Err(SandboxError::InvalidConfig(
            "Timeout must be greater than zero".to_string(),
        ));
    }

    let mut unshare_args = vec![
        "--mount".to_string(),
        "--ipc".to_string(),
        "--pid".to_string(),
        "--uts".to_string(),
    ];

    if config.enable_user_namespace {
        unshare_args.push("--user".to_string());
        unshare_args.push("--map-root-user".to_string());
    }

    if !config.enable_network {
        unshare_args.push("--net".to_string());
    }

    unshare_args.push("--fork".to_string());

    let setup_code = format!(
        r#"
        export HOME=.sandbox-home
        export TMPDIR=.sandbox-tmp
        mkdir -p .sandbox-home .sandbox-tmp
        {}
        "#,
        code
    );

    unshare_args.push("sh".to_string());
    unshare_args.push("-c".to_string());
    unshare_args.push(setup_code);

    let req = ExecutionRequest {
        program: PathBuf::from("unshare"),
        args: unshare_args,
        working_directory: config.cwd.clone(),
        timeout: Duration::from_millis(config.timeout_ms),
        environment_allowlist: vec![
            "PATH".to_string(),
            "RUST_LOG".to_string(),
            "RUST_BACKTRACE".to_string(),
            "TERM".to_string(),
        ],
        network_policy: if config.enable_network {
            NetworkPolicy::Enabled
        } else {
            NetworkPolicy::Disabled
        },
        resource_limits: ResourceLimits::default(),
    };

    execute_request(&req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_defaults() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let config = SandboxConfig {
            permission_mode: PermissionMode::ReadOnly,
            timeout_ms: 5000,
            cwd: std::env::current_dir()?,
            enable_network: false,
            enable_user_namespace: true,
            drop_capabilities: true,
            danger_mode: false,
        };

        assert_eq!(config.permission_mode, PermissionMode::ReadOnly);
        assert_eq!(config.timeout_ms, 5000);
        assert!(!config.enable_network);
        assert!(config.enable_user_namespace);
        assert!(config.drop_capabilities);
        assert!(!config.danger_mode);
        Ok(())
    }

    #[test]
    fn test_sandbox_result_creation() {
        let result = SandboxResult {
            success: true,
            return_code: Some(0),
            interrupted: false,
            stdout: "test output".to_string(),
            stderr: "".to_string(),
        };

        assert!(result.success);
        assert_eq!(result.return_code, Some(0));
        assert!(!result.interrupted);
        assert_eq!(result.stdout, "test output");
        assert_eq!(result.stderr, "");
    }

    #[tokio::test]
    async fn test_async_execution_request_timeout() {
        let cwd = std::env::current_dir().unwrap();
        let mut req = ExecutionRequest::new("sleep", cwd.to_str().unwrap());
        req.args = vec!["10".to_string()];
        req.timeout = Duration::from_millis(100);

        let result = execute_request(&req).await.unwrap();
        assert!(result.interrupted, "Execution should have timed out");
        assert!(!result.success);
    }
}
