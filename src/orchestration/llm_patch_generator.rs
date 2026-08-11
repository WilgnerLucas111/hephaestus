use crate::error::{HephaestusError, Result};
use crate::orchestration::project_repair::{FailureReport, PatchCandidate, ProjectRepairEngine};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_OPENROUTER_KEY: &str = "";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmPatchResponse {
    pub hypothesis: String,
    pub patch: String,
    pub files_changed: Vec<String>,
    pub risk: String,
}

pub struct LlmPatchGenerator {
    pub api_key: String,
    pub model: String,
}

impl Default for LlmPatchGenerator {
    fn default() -> Self {
        Self {
            api_key: std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
            model: "google/gemini-2.0-flash-lite-001:free".to_string(),
        }
    }
}

impl LlmPatchGenerator {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    pub async fn generate_patch(
        &self,
        original_code: &str,
        target_file: &Path,
        failure: &FailureReport,
    ) -> Result<PatchCandidate> {
        // Heuristic candidate as primary/fallback
        let fallback =
            ProjectRepairEngine::generate_patch_candidate(original_code, target_file, failure)?;

        if self.api_key.is_empty() {
            return Ok(fallback);
        }

        // Format prompt context for OpenRouter
        let prompt = format!(
            "You are a Rust software repair assistant. Fix the following code failure:\n\n\
            File: {}\n\
            Failing Test: {}\n\
            Error Output:\n{}\n\n\
            Source Code:\n```rust\n{}\n```\n\n\
            Return JSON:\n\
            {{\n  \"hypothesis\": \"...\",\n  \"patch\": \"...full repaired source code...\",\n  \"files_changed\": [\"{}\"],\n  \"risk\": \"low\"\n}}",
            target_file.display(),
            failure.failing_test,
            failure.stderr,
            original_code,
            target_file.display()
        );

        match self.call_openrouter_api(&prompt).await {
            Ok(llm_resp) => {
                let diff = crate::orchestration::project_repair::generate_unified_diff_pub(
                    original_code,
                    &llm_resp.patch,
                    target_file,
                );
                Ok(PatchCandidate {
                    diff,
                    patched_file_content: llm_resp.patch,
                    target_file: target_file.to_path_buf(),
                    rationale: format!("LLM Hypothesis: {}", llm_resp.hypothesis),
                })
            }
            Err(_) => Ok(fallback),
        }
    }

    async fn call_openrouter_api(&self, _prompt: &str) -> Result<LlmPatchResponse> {
        Err(HephaestusError::Internal(
            "LLM offline fallback".to_string(),
        ))
    }
}
