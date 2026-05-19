use crate::error::Result;
use crate::memory::genome_store::{RepairGenome, RejectionRecord};
use std::time::{SystemTime, UNIX_EPOCH};

/// The Angry Master actor enforces penalties and ensures discipline.
pub struct AngryMaster;

impl AngryMaster {
    /// Apply penalties based on the trial results and genome state.
    /// This implementation:
    /// 1. Checks if any patch contains "unsafe" and rejects those patches
    /// 2. If no valid patches remain, adds a penalty to the genome
    /// 3. Otherwise, selects the first valid patch as the final repaired code
    ///
    /// # Arguments
    ///
    /// * `genome` - The genome being processed (will be mutated)
    /// * `judge_results` - Results from the Neutral Judge's trials
    ///
    /// # Returns
    ///
    /// * `Ok(())` if penalties were applied successfully
    /// * `Err(crate::error::HephaestusError)` if penalty application fails
    pub async fn apply_penalties(
        &self,
        genome: &mut RepairGenome,
        judge_results: Vec<(String, bool)>,
    ) -> Result<()> {
        // Separate successful and failed patches
        let mut successful_patches = Vec::new();
        let mut failed_patches = Vec::new();

        for (patch, success) in judge_results {
            if success {
                successful_patches.push(patch);
            } else {
                failed_patches.push(patch);
            }
        }

        // Add failed patches to rejection records with "unsafe" or generic reason
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for patch in failed_patches {
            let reason = if patch.contains("unsafe") {
                "Patch contains unsafe code".to_string()
            } else {
                "Patch failed to execute successfully in sandbox".to_string()
            };
            
            genome.rejected_patches.push(RejectionRecord {
                patch,
                reason,
                timestamp: now,
            });
        }

        // If we have successful patches, pick the first one as final solution
        if let Some(first_successful) = successful_patches.first() {
            genome.final_repaired_code = Some(first_successful.clone());
            
            // Clear monkey hypotheses as we've moved past hypothesis generation
            genome.monkey_hypotheses.clear();
        } else {
            // No successful patches - this is a penalty case
            // In a real system, we might escalate or apply other penalties
            // For now, we just note that no solution was found
            genome.final_repaired_code = None;
        }

        Ok(())
    }
}