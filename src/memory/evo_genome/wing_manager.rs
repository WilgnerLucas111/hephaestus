use crate::memory::genome_store::RepairGenome;
use std::collections::HashMap;

/// Manages the assignment of genomes to different "Wings" in the MemPalace.
/// Each Wing represents a technical domain (e.g., Concurrency, Memory Overflow).
pub struct WingManager {
    /// Mapping from wing name to list of genome hashes in that wing
    wings: HashMap<String, Vec<String>>,
}

impl WingManager {
    /// Create a new WingManager.
    pub fn new() -> Self {
        Self {
            wings: HashMap::new(),
        }
    }

    /// Assign a genome to a wing based on its telemetry trigger and other features.
    /// Returns the wing name assigned.
    pub fn assign_wing(&mut self, genome: &RepairGenome) -> String {
        // Determine wing based on keywords in telemetry trigger and original code.
        let wing = if let Some(ref telemetry) = genome.telemetry_trigger {
            let telemetry_lower = telemetry.to_lowercase();
            if telemetry_lower.contains("panic") || telemetry_lower.contains("unwrap") {
                "WingOfPanics".to_string()
            } else if telemetry_lower.contains("deadlock") || telemetry_lower.contains("mutex") {
                "WingOfConcurrency".to_string()
            } else if telemetry_lower.contains("overflow") || telemetry_lower.contains("oom") {
                "WingOfMemory".to_string()
            } else if telemetry_lower.contains("index") || telemetry_lower.contains("bounds") {
                "WingOfIndexing".to_string()
            } else {
                "WingOfGeneral".to_string()
            }
        } else {
            "WingOfUnknown".to_string()
        };

        // Add the genome hash to the wing's list
        self.wings.entry(wing.clone()).or_insert_with(Vec::new).push(genome.hash.clone());

        wing
    }

    /// Get the wing name for a genome (if already assigned).
    pub fn get_wing(&self, genome: &RepairGenome) -> Option<String> {
        for (wing, members) in &self.wings {
            if members.iter().any(|h| h == &genome.hash) {
                return Some(wing.clone());
            }
        }
        None
    }

    /// Get all genomes in a given wing.
    pub fn get_wing_members(&self, wing: &str) -> Option<Vec<String>> {
        self.wings.get(wing).cloned()
    }

    /// List all wings.
    pub fn list_wings(&self) -> Vec<String> {
        self.wings.keys().cloned().collect()
    }
}