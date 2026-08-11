use crate::error::Result;
use crate::memory::genome_store::RepairGenome;
use crate::orchestration::project_repair::{FailureReport, PatchCandidate, ValidationReport};
use std::path::Path;

/// Interface for collecting failure context from execution runs
pub trait FailureCollector: Send + Sync {
    fn collect_failure(&self, stdout: &str, stderr: &str, exit_code: Option<i32>) -> FailureReport;
}

/// Interface for generating candidate patches given context and failure reports
pub trait PatchGenerator: Send + Sync {
    fn generate_patches(
        &self,
        original_code: &str,
        target_file: &Path,
        failure: &FailureReport,
    ) -> Result<Vec<PatchCandidate>>;
}

/// Interface for validating patches in isolated environments
pub trait PatchValidator: Send + Sync {
    fn validate_patch(
        &self,
        temp_workspace: &Path,
        patch: &PatchCandidate,
    ) -> Result<ValidationReport>;
}

/// Interface for persisting repair attempts and genomes
pub trait RepairRepository: Send + Sync {
    fn save_attempt(&self, genome: &mut RepairGenome) -> Result<()>;
    fn find_genome(&self, hash: &str) -> Result<Option<RepairGenome>>;
}
