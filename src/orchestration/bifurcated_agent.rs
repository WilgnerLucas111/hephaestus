use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use crate::ast::analyzer::ASTAnalyzer;
use crate::error::Result;
use crate::interceptor::interceptor::{Skill, SkillResult};
use crate::memory::genome_store::GenomeStore;

/// Outcome of a repair attempt
#[derive(Debug, Clone)]
pub struct RepairOutcome {
    pub skill_name: String,
    pub success: bool,
    pub confidence: f32,
    pub repair_time_ms: u64,
    pub mutation_applied: Option<String>,
}

/// Configuration for the bifurcated agent
#[derive(Debug, Clone)]
pub struct BifurcatedAgentConfig {
    pub max_parallel_repairs: usize,
    pub reinvoke_after_successful_repair: bool,
    pub log_dir: std::path::PathBuf,
}

/// Main skill runner (simplified)
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct SkillRunner {
    // In a full implementation, this would contain the actual skill functions
}


impl SkillRunner {
    pub async fn run_skill(&self, _skill: &Skill) -> Result<SkillResult> {
        // In a real implementation, this would invoke the skill function
        // For now, we'll simulate a skill that sometimes fails
        if _skill.code.contains("panic!") { panic!("Intentional panic"); } Ok(SkillResult::Success)
    }
}

/// Main bifurcated agent structure
pub struct BifurcatedAgent {
    /// Main skill runner
    skill_runner: SkillRunner,

    /// Background repair handles
    repair_tasks: Vec<JoinHandle<RepairOutcome>>,

    /// Configuration
    config: BifurcatedAgentConfig,

    /// Shared genome store
    genome_store: Arc<Mutex<GenomeStore>>,

    /// Shared AST analyzer
    ast_analyzer: Arc<Mutex<ASTAnalyzer>>,

    /// Semaphore to limit concurrent repairs
    repair_semaphore: Arc<Semaphore>,
}

impl BifurcatedAgent {
    /// Create a new bifurcated agent
    pub fn new(_config: BifurcatedAgentConfig, _genome_store: GenomeStore) -> Result<Self> {
        let genome_store = Arc::new(Mutex::new(_genome_store));
        let ast_analyzer = Arc::new(tokio::sync::Mutex::new(ASTAnalyzer::new(
            tree_sitter_rust::language(),
        )?));
        let repair_semaphore = Arc::new(Semaphore::new(_config.max_parallel_repairs));

        Ok(Self {
            skill_runner: SkillRunner::default(),
            repair_tasks: Vec::new(),
            config: _config,
            genome_store,
            ast_analyzer,
            repair_semaphore,
        })
    }

    /// Execute skill with bifurcated repair
    /// Main loop returns immediately; repair happens in background
    pub async fn executeskill_bifurcated(
        &mut self,
        skill: &Skill,
        interceptor: &crate::interceptor::interceptor::HephaestusInterceptor,
    ) -> Result<SkillResult> {
        // Main path: execute skill through interceptor to catch panics
        let skill_runner = self.skill_runner.clone();
        let skill_clone = skill.clone();
        let skill_result = interceptor.intercept_skill(skill, move || {
            // This closure contains the actual skill execution
            Box::pin(async move {
                skill_runner.run_skill(&skill_clone).await
            })
        }).await;
        
        // For now, we don't have a repair trigger from the interceptor since we're using the simpler intercept_skill method
        let repair_trigger = None;

        // If error occurred (either from interception or direct failure), spawn background repair (non-blocking)
        if !matches!(skill_result, SkillResult::Success)
            && self.repair_tasks.len() < self.config.max_parallel_repairs {
                 
                   let _interceptor_clone = interceptor.clone();
                   let skill_clone = skill.clone();
                   let genome_store_clone = self.genome_store.clone();
                   let _ast_analyzer_clone = self.ast_analyzer.clone();
                   let config_clone = self.config.clone();
                   let repair_semaphore_clone = self.repair_semaphore.clone();

                 // Spawn repair in background
                 let skill_name_forerror = skill.name.clone();
                 let start_time = std::time::Instant::now();
                 let repair_handle = tokio::spawn(async move {
                     match Self::run_repair_pipeline(
                         skill_clone,
                         repair_trigger,
                         genome_store_clone,
                         config_clone,
                         &repair_semaphore_clone,
                     )
                     .await
                     {
                         Ok(outcome) => outcome,
                         Err(_e) => {
                             // Log the error but still return a RepairOutcome
                             eprintln!("Error in repair pipeline: {}", _e);
                             RepairOutcome {
                                 skill_name: skill_name_forerror,
                                 success: false,
                                 confidence: 0.0,
                                 repair_time_ms: start_time.elapsed().as_millis() as u64,
                                 mutation_applied: None,
                             }
                         }
                     }
                 });

                self.repair_tasks.push(repair_handle);
            }

        Ok(skill_result)
    }



    /// Background repair pipeline using the Tribunal architecture
    async fn run_repair_pipeline(
        skill: Skill,
        repair_trigger: Option<crate::interceptor::interceptor::RepairTrigger>,
        _genome_store: Arc<Mutex<GenomeStore>>,
        _config: BifurcatedAgentConfig,
        repair_semaphore: &Arc<Semaphore>,
    ) -> crate::error::Result<RepairOutcome> {
        let start_time = std::time::Instant::now();
        let _permit = repair_semaphore.acquire().await;
        let _skill_name = skill.name.clone();

          // Initialize an empty RepairGenome (Ledger)
          let mut genome = crate::memory::genome_store::RepairGenome {
              hash: "".to_string(), // Will be computed later if needed
              original_code: skill.code.clone(),
              telemetry_trigger: None,
              monkey_hypotheses: Vec::new(),
              rejected_patches: Vec::new(),
              final_repaired_code: None,
              narrative_summary: None,
              timestamp: 0,
              semantic_cluster: None,
              wing: None,
              aaak_compressed: None,
              dependency_density: None,
              ast_topology_hash: None,
          };

        // Extract telemetry from repair trigger or create basic telemetry
        let telemetry = match repair_trigger.as_ref() {
            Some(t) => t.error_message.clone(),
            None => "Unknown error during skill execution".to_string(),
        };

        // Create tribunal actors
        let wild_monkey = crate::tribunal::WildMonkey;
        let neutral_judge = crate::tribunal::NeutralJudge;
        let angry_master = crate::tribunal::AngryMaster;
        let narrative_agent = crate::tribunal::NarrativeAgent;

        // Phase 4/5: Wild Monkey generates patches based on telemetry
        let patches = wild_monkey
            .generate_patches(&mut genome, &telemetry, &_genome_store)
            .await
            .map_err(|e| crate::error::HephaestusError::Internal(format!(
                "Wild Monkey failed to generate patches: {}",
                e
            )))?;

        // Phase 6: Neutral Judge executes trials on the patches
        let trial_results = neutral_judge
            .execute_trials(patches.clone(), &genome)
            .await
            .map_err(|e| crate::error::HephaestusError::Internal(format!(
                "Neutral Judge failed to execute trials: {}",
                e
            )))?;

        // Also run safeguard inspection (though we don't use the result directly in this flow)
        let _ = neutral_judge
            .inspect_safeguards(patches.clone())
            .await
            .map_err(|e| crate::error::HephaestusError::Internal(format!(
                "Neutral Judge failed to inspect safeguards: {}",
                e
            )))?;

        // Phase 7: Angry Master applies penalties and selects final repair
        angry_master
            .apply_penalties(&mut genome, trial_results.clone())
            .await
            .map_err(|e| crate::error::HephaestusError::Internal(format!(
                "Angry Master failed to apply penalties: {}",
                e
            )))?;

        // Determine if we have a successful verdict
        let verdict = genome.final_repaired_code.is_some();

        // Phase 8: Narrative Agent records the verdict
        narrative_agent
            .record_verdict(&mut genome, verdict, &telemetry)
            .await
            .map_err(|e| crate::error::HephaestusError::Internal(format!(
                "Narrative Agent failed to record verdict: {}",
                e
            )))?;

        // Update timestamp and compute hash if we have a final repaired code
        genome.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // In a real implementation, we would compute a proper hash of the genome
        // For now, we'll use a simple placeholder
        if !genome.original_code.is_empty() {
            use std::hash::{Hash, Hasher};
            use std::collections::hash_map::DefaultHasher;
            let mut hasher = DefaultHasher::new();
            genome.original_code.hash(&mut hasher);
            genome.hash = hasher.finish().to_string();
        }

        // Store the populated RepairGenome in the genome store
        {
            let store = _genome_store.lock().await;
            store.store_genome(&mut genome)
                .map_err(|e| crate::error::HephaestusError::Internal(format!(
                    "Failed to store genome: {}",
                    e
                )))?;
        }

        // Determine the outcome based on whether we have a final repaired code
        let success = genome.final_repaired_code.is_some();
        let confidence = if success { 0.9 } else { 0.1 };
        let mutation_applied = genome.final_repaired_code.clone();

        Ok(RepairOutcome {
            skill_name: skill.name.clone(),
            success,
            confidence,
            repair_time_ms: start_time.elapsed().as_millis() as u64,
            mutation_applied,
        })
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_bifurcated_agent_creation() -> Result<()> {
        let config = BifurcatedAgentConfig {
            max_parallel_repairs: 2,
            reinvoke_after_successful_repair: true,
            log_dir: std::path::PathBuf::from("/tmp/logs"),
        };
        let genome_store = crate::memory::genome_store::GenomeStore::open("test.db")?;
        let agent = BifurcatedAgent::new(config, genome_store)?;
        assert_eq!(agent.config.max_parallel_repairs, 2);
        Ok(())
    }
}
