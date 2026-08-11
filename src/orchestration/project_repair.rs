use crate::error::{HephaestusError, Result};
use crate::memory::genome_store::{GenomeStore, RepairGenome};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
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

    /// Executes `cargo test` on a project path and captures failure details if any.
    pub async fn run_cargo_test(project_dir: &Path) -> Result<(bool, FailureReport)> {
        let start = Instant::now();
        let output = Command::new("cargo")
            .arg("test")
            .arg("--")
            .arg("--nocapture")
            .current_dir(project_dir)
            .output()
            .await
            .map_err(|e| {
                HephaestusError::Internal(format!("Failed to execute cargo test: {}", e))
            })?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let success = output.status.success();

        let failing_test = parse_failing_test_name(&stdout, &stderr);

        let report = FailureReport {
            failing_test,
            exit_code: output.status.code(),
            stderr,
            stdout,
            duration_ms,
        };

        Ok((success, report))
    }

    /// Generates a patch for common Rust errors like Index Out of Bounds.
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

    /// Runs a full, end-to-end repair cycle on a target Rust project.
    pub async fn execute_repair_cycle(
        original_project_dir: &Path,
        target_file_rel_path: &Path,
        genome_store: &Arc<Mutex<GenomeStore>>,
    ) -> Result<RepairCycleResult> {
        // Step 1: Create disposable workspace copy
        let temp_dir = tempfile::tempdir()?;
        let temp_workspace = temp_dir.path();
        Self::copy_dir_all(original_project_dir, temp_workspace)?;

        // Step 2: Reproduce failure
        let (initial_success, failure_report) = Self::run_cargo_test(temp_workspace).await?;
        if initial_success {
            return Err(HephaestusError::InvalidInput(
                "Initial project tests passed; no failure to repair".to_string(),
            ));
        }

        // Step 3: Read target file & generate patch
        let target_file_full = temp_workspace.join(target_file_rel_path);
        let original_code = fs::read_to_string(&target_file_full)?;

        let patch_candidate =
            Self::generate_patch_candidate(&original_code, target_file_rel_path, &failure_report)?;

        // Step 4: Apply patch to workspace copy only
        fs::write(&target_file_full, &patch_candidate.patched_file_content)?;

        // Step 5: Validate patch with cargo check, clippy, cargo test
        let val_start = Instant::now();
        let check_output = Command::new("cargo")
            .arg("check")
            .current_dir(temp_workspace)
            .output()
            .await?;

        let compiled = check_output.status.success();

        let clippy_output = Command::new("cargo")
            .arg("clippy")
            .current_dir(temp_workspace)
            .output()
            .await?;
        let clippy_passed = clippy_output.status.success();

        let (test_success, post_patch_test_report) = Self::run_cargo_test(temp_workspace).await?;

        let validation_report = ValidationReport {
            compiled,
            clippy_passed,
            tests_passed: test_success,
            duration_ms: val_start.elapsed().as_millis() as u64,
            stdout: post_patch_test_report.stdout.clone(),
            stderr: post_patch_test_report.stderr.clone(),
        };

        let overall_success = compiled && test_success;
        let unified_diff = Some(patch_candidate.diff.clone());

        // Step 6: Compute hash & store RepairGenome in SQLite
        let genome_hash = blake3::hash(original_code.as_bytes()).to_hex().to_string();
        let mut genome = RepairGenome::new(genome_hash.clone(), original_code);
        genome.telemetry_trigger = Some(failure_report.stderr.clone());
        genome.monkey_hypotheses = vec![patch_candidate.diff.clone()];
        if overall_success {
            genome.final_repaired_code = Some(patch_candidate.patched_file_content.clone());
            genome.narrative_summary = Some(format!(
                "Successfully repaired index out of bounds bug in {}. Failing test '{}' now passes.",
                target_file_rel_path.display(),
                failure_report.failing_test
            ));
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
    let mut diff = String::new();
    diff.push_str(&format!("--- a/{}\n", file_path.display()));
    diff.push_str(&format!("+++ b/{}\n", file_path.display()));

    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();

    diff.push_str("@@ -1,");
    diff.push_str(&old_lines.len().to_string());
    diff.push_str(" +1,");
    diff.push_str(&new_lines.len().to_string());
    diff.push_str(" @@\n");

    for line in &old_lines {
        if !new_lines.contains(line) {
            diff.push_str(&format!("-{}\n", line));
        }
    }
    for line in &new_lines {
        if !old_lines.contains(line) {
            diff.push_str(&format!("+{}\n", line));
        } else {
            diff.push_str(&format!(" {}\n", line));
        }
    }

    diff
}
