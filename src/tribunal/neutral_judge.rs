use crate::memory::genome_store::RepairGenome;
use crate::sandbox::executor::{execute_in_sandbox, SandboxConfig, PermissionMode};
// We don't need to import Result or HephaestusError if we use the fully qualified paths in the function signatures
// But we still need Result for the return type. Let's keep Result and remove the HephaestusError import.
use crate::error::Result;
use std::path::PathBuf;

/// The Neutral Judge actor evaluates patches and conducts trials.
pub struct NeutralJudge;

impl NeutralJudge {
    /// Execute trials on the given patches using the sandbox executor.
    ///
    /// # Arguments
    ///
    /// * `patches` - The patches to test
    ///
    /// # Returns
    ///
    /// * `Ok(vec![(patch, result), ...])` - Results for each patch
    /// * `Err(crate::error::HephaestusError)` if trial execution fails
    pub async fn execute_trials(
        &self,
        patches: Vec<String>,
        _genome: &RepairGenome,
    ) -> Result<Vec<(String, bool)>> {
        // Create a sandbox configuration for testing
        let config = SandboxConfig {
            permission_mode: PermissionMode::ReadOnly,
            timeout_ms: 2000, // 2 second timeout for trials
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp")),
            enable_network: false,
            enable_user_namespace: true,
            drop_capabilities: true,
            danger_mode: false,
        };

        let mut results = Vec::new();

        for patch in patches {
            // Execute the patch in sandbox to see if it compiles/runs
            match execute_in_sandbox(&patch, &config).await {
                Ok(result) => {
                    // Consider a patch successful if it executes without interruption
                    // In a real system, we'd check actual behavior vs expected
                    let success = !result.interrupted && result.success;
                    results.push((patch, success));
                }
                Err(_) => {
                    // If sandbox execution fails, consider the patch a failure
                    results.push((patch, false));
                }
            }
        }

        Ok(results)
    }

    /// Inspect safeguards to ensure the patches are safe.
    /// For now, we do a simple check for obviously dangerous patterns.
    ///
    /// # Arguments
    ///
    /// * `patches` - The patches to inspect
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if all patches pass safeguards
    /// * `Ok(false)` if any patch fails safeguards
    /// * `Err(crate::error::HephaestusError)` if inspection fails
    pub async fn inspect_safeguards(
        &self,
        patches: Vec<String>,
    ) -> Result<bool> {
        // Simple safeguard: reject patches containing obviously dangerous keywords
        let dangerous_keywords = [
            "unsafe", 
            "std::process::exit",
            "fs::remove_dir_all",
            "std::mem::forget",
        ];

        for patch in &patches {
            for keyword in &dangerous_keywords {
                if patch.contains(keyword) {
                    return Ok(false); // Failed safeguard check
                }
            }
        }

        Ok(true) // All patches passed safeguard check
    }
}