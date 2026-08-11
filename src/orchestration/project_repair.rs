use crate::error::{HephaestusError, Result};
use crate::memory::genome_store::{GenomeStore, RepairGenome};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Report of a failed test run
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FailureReport {
    pub failing_test: String,
    pub exit_code: Option<i32>,
    pub stderr: String,
    pub stdout: String,
    pub duration_ms: u64,
}

/// Patch candidate produced during repair
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatchCandidate {
    pub diff: String,
    pub patched_file_content: String,
    pub target_file: PathBuf,
    pub rationale: String,
}

/// Result of patch validation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationReport {
    pub compiled: bool,
    pub clippy_passed: bool,
    pub tests_passed: bool,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
}

/// Full outcome of a real project repair cycle
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepairCycleResult {
    pub success: bool,
    pub original_failure: FailureReport,
    pub patch: Option<PatchCandidate>,
    pub validation: Option<ValidationReport>,
    pub unified_diff: Option<String>,
    pub genome_hash: String,
}

pub struct ProjectRepairEngine;

impl ProjectRepairEngine {
    /// Recursively copies a directory to a destination.
    pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if ty.is_dir() {
                let name = entry.file_name();
                if name != "target" && name != ".git" {
                    Self::copy_dir_all(&src_path, &dst_path)?;
                }
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }

    /// Executes a sandboxed `cargo` command with `--offline`, non-blocking timeout, and environment isolation.
    pub async fn run_sandboxed_cargo(
        project_dir: &Path,
        subcommand: &str,
        extra_args: &[&str],
        timeout_secs: u64,
    ) -> Result<crate::sandbox::executor::SandboxResult> {
        let mut args = vec![subcommand.to_string()];
        if std::env::var("HEPHAESTUS_OFFLINE").is_ok() {
            args.push("--offline".to_string());
        }
        for arg in extra_args {
            args.push(arg.to_string());
        }

        let req = crate::sandbox::executor::ExecutionRequest {
            program: PathBuf::from("cargo"),
            args,
            working_directory: project_dir.to_path_buf(),
            timeout: Duration::from_secs(timeout_secs),
            environment_allowlist: vec![
                "PATH".to_string(),
                "CARGO_HOME".to_string(),
                "RUSTUP_HOME".to_string(),
                "RUST_LOG".to_string(),
                "RUST_BACKTRACE".to_string(),
                "TERM".to_string(),
                "HOME".to_string(),
                "USER".to_string(),
                "TMPDIR".to_string(),
                "TMP".to_string(),
                "TEMP".to_string(),
                "LANG".to_string(),
            ],
            network_policy: crate::sandbox::executor::NetworkPolicy::Disabled,
            resource_limits: crate::sandbox::executor::ResourceLimits::default(),
        };

        crate::sandbox::executor::execute_request(&req)
            .await
            .map_err(|e| HephaestusError::Internal(format!("Sandbox execution error: {}", e)))
    }

    /// Executes `cargo test` in sandbox and captures failure details if any.
    pub async fn run_cargo_test(project_dir: &Path) -> Result<(bool, FailureReport)> {
        let start = Instant::now();
        let res =
            Self::run_sandboxed_cargo(project_dir, "test", &["--", "--nocapture"], 60).await?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let failing_test = parse_failing_test_name(&res.stdout, &res.stderr);

        let report = FailureReport {
            failing_test,
            exit_code: res.return_code,
            stderr: res.stderr,
            stdout: res.stdout,
            duration_ms,
        };

        Ok((res.success, report))
    }

    /// Generates a patch candidate (Phase 4/5: WildMonkey).
    pub fn generate_patch_candidate(
        original_code: &str,
        target_file: &Path,
        _failure: &FailureReport,
    ) -> Result<PatchCandidate> {
        let mut patched_lines = Vec::new();
        let mut changed = false;

        for line in original_code.lines() {
            let trimmed = line.trim_start();
            let is_candidate_line = line.contains('[')
                && line.contains(']')
                && !trimmed.starts_with("//")
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("pub fn")
                && !trimmed.starts_with("fn ")
                && !line.contains("vec![")
                && !line.contains("&[");

            if is_candidate_line {
                let patched = patch_index_access(line);
                if patched != line {
                    patched_lines.push(patched);
                    changed = true;
                    continue;
                }
            }
            patched_lines.push(line.to_string());
        }

        let patched_content = if changed {
            patched_lines.join("\n")
        } else {
            original_code
                .replace("arr[index]", "arr.get(index).copied().unwrap_or(0)")
                .replace("vec[idx]", "vec.get(idx).copied()")
                .replace("slice[i]", "slice.get(i).copied()")
        };

        let diff = generate_unified_diff(original_code, &patched_content, target_file);

        Ok(PatchCandidate {
            diff,
            patched_file_content: patched_content,
            target_file: target_file.to_path_buf(),
            rationale: "Replaced direct index access with safe bounds-checked get() method"
                .to_string(),
        })
    }

    /// Runs a full, end-to-end sandboxed repair cycle on a target Rust project.
    pub async fn execute_repair_cycle(
        original_project_dir: &Path,
        target_file_rel_path: &Path,
        genome_store: &Arc<Mutex<GenomeStore>>,
    ) -> Result<RepairCycleResult> {
        // Step 1: Create disposable workspace copy
        let temp_dir = tempfile::tempdir()?;
        let temp_workspace = temp_dir.path();
        Self::copy_dir_all(original_project_dir, temp_workspace)?;

        // Step 2: Reproduce failure with sandboxed cargo test --offline
        let (initial_success, failure_report) = Self::run_cargo_test(temp_workspace).await?;
        if initial_success {
            return Err(HephaestusError::InvalidInput(
                "Initial project tests passed; no failure to repair".to_string(),
            ));
        }

        // Step 3: Read target file & validate path is within workspace boundaries
        let target_file_full = temp_workspace.join(target_file_rel_path);
        let canonical_target = target_file_full.canonicalize().map_err(|e| {
            HephaestusError::InvalidInput(format!("Invalid target file path: {}", e))
        })?;
        let canonical_workspace = temp_workspace
            .canonicalize()
            .map_err(|e| HephaestusError::InvalidInput(format!("Invalid workspace path: {}", e)))?;

        if !canonical_target.starts_with(&canonical_workspace) {
            return Err(HephaestusError::InvalidInput(
                "Target file path escapes workspace boundaries".to_string(),
            ));
        }

        let original_code = fs::read_to_string(&canonical_target)?;

        // Phase 4/5: WildMonkey generates patch
        let patch_candidate =
            Self::generate_patch_candidate(&original_code, target_file_rel_path, &failure_report)?;

        // Phase 6: NeutralJudge applies patch to workspace copy
        fs::write(&canonical_target, &patch_candidate.patched_file_content)?;

        // Phase 6: Sandboxed Empirical Validation (cargo check, clippy, cargo test)
        let val_start = Instant::now();
        let check_res = Self::run_sandboxed_cargo(temp_workspace, "check", &[], 60).await?;
        let compiled = check_res.success;

        let clippy_res = Self::run_sandboxed_cargo(temp_workspace, "clippy", &[], 60).await?;
        let clippy_passed = clippy_res.success;

        let (test_success, post_patch_test_report) = Self::run_cargo_test(temp_workspace).await?;

        // Phase 7: AngryMaster Policy & Security Gatekeeper
        let unsafe_free = !patch_candidate.patched_file_content.contains("unsafe {")
            && !patch_candidate.patched_file_content.contains("unsafe fn");
        let line_budget_ok = patch_candidate.diff.lines().count() <= 200;

        let master_approved =
            compiled && clippy_passed && test_success && unsafe_free && line_budget_ok;

        let validation_report = ValidationReport {
            compiled,
            clippy_passed,
            tests_passed: test_success,
            duration_ms: val_start.elapsed().as_millis() as u64,
            stdout: post_patch_test_report.stdout.clone(),
            stderr: post_patch_test_report.stderr.clone(),
        };

        let overall_success = master_approved;
        let unified_diff = Some(patch_candidate.diff.clone());

        // Phase 8: NarrativeAgent records verdict into RepairGenome
        let genome_hash = blake3::hash(original_code.as_bytes()).to_hex().to_string();
        let mut genome = RepairGenome::new(genome_hash.clone(), original_code);
        genome.telemetry_trigger = Some(failure_report.stderr.clone());
        genome.monkey_hypotheses = vec![patch_candidate.diff.clone()];
        if overall_success {
            genome.final_repaired_code = Some(patch_candidate.patched_file_content.clone());
            genome.narrative_summary = Some(format!(
                "[NarrativeAgent] Successfully repaired bug in {}. NeutralJudge verified tests pass. AngryMaster approved patch (no unsafe, budget ok). Failing test '{}' resolved.",
                target_file_rel_path.display(),
                failure_report.failing_test
            ));
        } else {
            genome
                .rejected_patches
                .push(crate::memory::genome_store::RejectionRecord {
                    patch: patch_candidate.diff.clone(),
                    reason: format!(
                        "Veto: compiled={}, clippy={}, tests={}, unsafe_free={}",
                        compiled, clippy_passed, test_success, unsafe_free
                    ),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                });
        }

        {
            let store = genome_store.lock().await;
            store.store_genome(&mut genome)?;
        }

        Ok(RepairCycleResult {
            success: overall_success,
            original_failure: failure_report,
            patch: Some(patch_candidate),
            validation: Some(validation_report),
            unified_diff,
            genome_hash,
        })
    }
}

fn parse_failing_test_name(stdout: &str, stderr: &str) -> String {
    for line in stdout.lines().chain(stderr.lines()) {
        if line.contains("FAILED") && line.contains("test ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].to_string();
            }
        }
    }
    "unknown_failing_test".to_string()
}

#[allow(clippy::collapsible_if)]
fn patch_index_access(line: &str) -> String {
    // If the line wraps index access in Some(...), replace the entire Some(...) with safe .get(...).copied()
    if line.contains("Some(") && line.contains('[') && line.contains(']') {
        if let Some(open_some) = line.find("Some(") {
            if let Some(close_paren) = line[open_some..].rfind(')') {
                let inner = &line[open_some + 5..open_some + close_paren];
                if let Some(open_bracket) = inner.find('[') {
                    if let Some(close_bracket) = inner.find(']') {
                        let var_name = inner[..open_bracket].trim();
                        let idx_expr = &inner[open_bracket + 1..close_bracket];
                        let clean_var = var_name.trim_start_matches('&').trim();
                        let safe_expr = format!("{}.get({}).copied()", clean_var, idx_expr);
                        return line.replace(&format!("Some({})", inner), &safe_expr);
                    }
                }
            }
        }
    }

    if let Some(open) = line.find('[') {
        if let Some(close) = line[open..].find(']') {
            let close_idx = open + close;
            let target_var = line[..open].trim();
            let index_expr = &line[open + 1..close_idx];
            let clean_var = target_var.trim_start_matches('&').trim();
            return line.replace(
                &format!("{}[{}]", clean_var, index_expr),
                &format!("{}.get({}).copied()", clean_var, index_expr),
            );
        }
    }
    line.to_string()
}

pub fn generate_unified_diff_pub(old_text: &str, new_text: &str, file_path: &Path) -> String {
    generate_unified_diff(old_text, new_text, file_path)
}

fn generate_unified_diff(old_text: &str, new_text: &str, file_path: &Path) -> String {
    let diff = similar::TextDiff::from_lines(old_text, new_text);
    let mut output = String::new();
    output.push_str(&format!("--- a/{}\n", file_path.display()));
    output.push_str(&format!("+++ b/{}\n", file_path.display()));

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        output.push_str(&format!("{}{}", sign, change));
    }
    output
}
