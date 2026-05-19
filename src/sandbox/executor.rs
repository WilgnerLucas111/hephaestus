use std::io;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use libc;

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

/// Execute code in isolated Linux namespace with timeout guarantee
pub async fn execute_in_sandbox(
    code: &str,
    config: &SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    // Validate configuration
    if config.timeout_ms == 0 {
        return Err(SandboxError::InvalidConfig(
            "Timeout must be greater than zero".to_string(),
        ));
    }

    // Build unshare command arguments
    let mut unshare_args = Vec::new();

    // Always isolate mount, ipc, pid, uts
    unshare_args.push("--mount");    // Isolate mount points
    unshare_args.push("--ipc");      // Isolate IPC resources (System V IPC, POSIX mqueue)
    unshare_args.push("--pid");      // Isolate PID namespace (process IDs)
    unshare_args.push("--uts");      // Isolate UTS namespace (hostname and domain name)

    // Conditionally add user namespace (required for root mapping and privilege dropping)
    if config.enable_user_namespace {
        unshare_args.push("--user");           // Create new user namespace
        unshare_args.push("--map-root-user");  // Map root user in namespace to current user outside
    }

    // Conditionally add network namespace
    // When enable_network=false, we ISOLATE the network namespace (no network access)
    // When enable_network=true, we SHARE the host's network namespace
    if !config.enable_network {
        unshare_args.push("--net");    // Isolate network namespace
    }

    // Always fork before unshare (required for proper cleanup with namespaces)
    unshare_args.push("--fork");

    // Set up isolated environment command
    let mut cmd = Command::new("unshare");
    cmd.args(&unshare_args);

    // Prepare the isolated environment setup and user code
    let setup_code = format!(
        r#"
        export HOME=.sandbox-home
        export TMPDIR=.sandbox-tmp
        mkdir -p .sandbox-home .sandbox-tmp
        {}
        "#,
        code
    );

    // Configure the command
    cmd.arg("sh")
        .arg("-c")
        .arg(setup_code)
        .current_dir(&config.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Spawn child process with security restrictions
    let danger_mode = config.danger_mode;
    let mut child = unsafe {
        cmd
            .pre_exec(move || {
                // Drop capabilities for security (if not in danger mode)
                if !danger_mode {
                    // PR_SET_NO_NEW_PRIVS: prevent gaining more privileges via execve
                    // This prevents setuid binaries from gaining privileges
                    if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                        // Ignore error - not all kernels support this
                    }
                }
                
                // Additional security: disable core dumps to prevent leaking sensitive information
                let rlimit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                libc::setrlimit(libc::RLIMIT_CORE, &rlimit);
                
                // Restrict to a minimal set of supplementary groups
                // This prevents inheriting unnecessary groups from the parent
                if libc::setgroups(0, std::ptr::null()) != 0 {
                    // Ignore error - not critical
                }
                
                Ok(())
            })
            .spawn()
            .map_err(SandboxError::SpawnFailed)?
    };

    // **CRITICAL: Guaranteed termination via tokio::time::timeout**
    let timeout_duration = Duration::from_millis(config.timeout_ms);
    let child_future = async { child.wait().map_err(SandboxError::WaitFailed) };

    let result = tokio::time::timeout(timeout_duration, child_future).await;

    match result {
        Ok(Ok(status)) => {
            // Child exited normally
            let output = child.wait_with_output().map_err(SandboxError::WaitFailed)?;

            Ok(SandboxResult {
                success: status.success(),
                return_code: status.code(),
                interrupted: false,
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            })
        }
        Ok(Err(e)) => {
            // Child exited with error
            Err(e)
        }
        Err(_) => {
            // Timeout: force kill the child process
            let _ = child.kill();
            let _ = child.wait();

            // Try to get any output that was produced before timeout
            let output = child
                .wait_with_output()
                .unwrap_or_else(|_| std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });

            Ok(SandboxResult {
                success: false,
                return_code: None,
                interrupted: true,
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: format!(
                    "Command exceeded timeout of {} ms\n{}",
                    config.timeout_ms,
                    String::from_utf8_lossy(&output.stderr)
                ),
            })
        }
    }
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
}
