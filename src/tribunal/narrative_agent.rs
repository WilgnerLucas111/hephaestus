use crate::error::Result;
use crate::memory::genome_store::RepairGenome;
use std::time::{SystemTime, UNIX_EPOCH};

/// The Narrative Agent actor records the verdict and creates a narrative summary.
pub struct NarrativeAgent;

impl NarrativeAgent {
    /// Record the verdict and update the genome with the final state.
    /// Assembles a narrative summary of the tribunal proceedings.
    ///
    /// # Arguments
    ///
    /// * `genome` - The genome being processed (will be mutated)
    /// * `verdict` - Whether the tribunal found a successful repair
    /// * `telemetry_trigger` - The original telemetry that started the repair
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the verdict was recorded successfully
    /// * `Err(crate::error::HephaestusError)` if recording fails
    pub async fn record_verdict(
        &self,
        genome: &mut RepairGenome,
        verdict: bool,
        telemetry_trigger: &str,
    ) -> Result<()> {
        // Update the telemetry trigger if not already set
        if genome.telemetry_trigger.is_none() {
            genome.telemetry_trigger = Some(telemetry_trigger.to_string());
        }

        // Create a narrative summary based on the tribunal proceedings
        let mut summary_parts = Vec::new();

        summary_parts.push(format!(
            "Tribunal proceedings initiated at {}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ));

        summary_parts.push(format!(
            "Telemetry trigger: {}",
            telemetry_trigger
        ));

        if !genome.monkey_hypotheses.is_empty() {
            summary_parts.push(format!(
                "Wild Monkey generated {} hypotheses:",
                genome.monkey_hypotheses.len()
            ));
            for (i, hypothesis) in genome.monkey_hypotheses.iter().enumerate() {
                summary_parts.push(format!(
                    "  Hypothesis {}: {}...",
                    i + 1,
                    hypothesis.chars().take(50).collect::<String>()
                ));
            }
        }

        if !genome.rejected_patches.is_empty() {
            summary_parts.push(format!(
                "Neutral Judge rejected {} patches:",
                genome.rejected_patches.len()
            ));
            for (i, rejection) in genome.rejected_patches.iter().enumerate() {
                summary_parts.push(format!(
                    "  Rejection {}: {} (reason: {})",
                    i + 1,
                    rejection.patch.chars().take(30).collect::<String>(),
                    rejection.reason
                ));
            }
        }

        match (&genome.final_repaired_code, verdict) {
            (Some(code), true) => {
                summary_parts.push("Angry Master approved the final repair.".to_string());
                summary_parts.push(format!(
                    "Final repaired code ({} chars): {}...",
                    code.len(),
                    code.chars().take(50).collect::<String>()
                ));
            }
            (None, false) => {
                summary_parts.push("Angry Master found no viable repair after tribunal review.".to_string());
            }
            (Some(_), false) => {
                summary_parts.push("Angry Master found code but tribunal vetoed the repair.".to_string());
            }
            (None, true) => {
                summary_parts.push("Inconsistent state: verdict true but no code produced.".to_string());
            }
        }

        genome.narrative_summary = Some(summary_parts.join("\n"));
        genome.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }
}