use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use crate::ast::analyzer::ASTAnalyzer;
use crate::error::{HephaestusError, Result};
use crate::interceptor::interceptor::{Skill, SkillResult};
use crate::memory::genome_store::GenomeStore;
use crate::sandbox::executor::{PermissionMode, SandboxConfig, SandboxResult, execute_in_sandbox};

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
                  let ast_analyzer_clone = self.ast_analyzer.clone();
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
                        ast_analyzer_clone,
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



    /// Background repair pipeline (10 steps from HEPHAESTUS_BLUEPRINT)
    async fn run_repair_pipeline(
        skill: Skill,
        repair_trigger: Option<crate::interceptor::interceptor::RepairTrigger>,
        _genome_store: Arc<Mutex<GenomeStore>>,
        ast_analyzer: Arc<Mutex<ASTAnalyzer>>,
        _config: BifurcatedAgentConfig,
        repair_semaphore: &Arc<Semaphore>,
    ) -> crate::error::Result<RepairOutcome> {
        let start_time = std::time::Instant::now();
        let _permit = repair_semaphore.acquire().await;
        let skill_name = skill.name.clone();

        // Extract line and column from error message if available - simplified version without regex
        let (error_line, error_column) = if let Some(ref trigger) = repair_trigger {
            // Simple extraction: look for colon-separated numbers
            let mut line = 1u32;
            let mut _column: Option<u32> = None;
            
            // Very basic error line extraction
            for (i, ch) in trigger.error_message.char_indices() {
                if ch == ':' {
                    // Try to parse the following digits as line number
                    let rest = &trigger.error_message[i+1..];
                    if let Ok(num) = rest.split_whitespace().next().unwrap_or("").parse::<u32>() {
                        line = num;
                        break;
                    }
                }
            }
            
            (line, None)
        } else {
            (1, None)
        };

        // Step 1: Extract error context (from interceptor)
        // Use the actual repair trigger if we have one from interception, otherwise create a basic one
        let trigger = match repair_trigger.as_ref() {
            Some(t) => t.clone(),
            None => {
                let error_message = "Unknown error during skill execution".to_string();
                crate::interceptor::interceptor::RepairTrigger {
                    skill_name: skill_name.clone(),
                    error_message: error_message.clone(),
                    error_keywords: crate::interceptor::interceptor::extract_error_keywords(&error_message),
                    stack_trace: Vec::new(),
                    memory_snapshot: None,
                }
            }
        };

        // Step 2: AST diagnosis using the extracted line/column and error keywords from trigger
        let error_keywords = if let Some(ref trigger) = repair_trigger {
            &trigger.error_keywords
        } else {
            &vec![]
        };
        let mut analyzer: tokio::sync::MutexGuard<'_, ASTAnalyzer> = ast_analyzer.lock().await;
        let diagnosis = match analyzer.diagnose(&skill.code, error_line, error_column, error_keywords) {
            Ok(d) => d,
            Err(_) => {
                return Ok(RepairOutcome {
                    skill_name: skill.name.clone(),
                    success: false,
                    confidence: 0.0,
                    repair_time_ms: start_time.elapsed().as_millis() as u64,
                    mutation_applied: None,
                });
            }
        };

        // Step 3: Extract subgraph (token budgeting) - simplified
        // In a full implementation, we would extract a subgraph based on token budget
        let _subgraph = diagnosis.slim_nodes.clone();

        // Step 4: 7-phase investigation (with LLM) - simplified
        let investigation = match Self::run_investigation(&diagnosis, &trigger).await {
            Ok(inv) => inv,
            Err(_) => {
                return Ok(RepairOutcome {
                    skill_name: skill.name.clone(),
                    success: false,
                    confidence: 0.0,
                    repair_time_ms: start_time.elapsed().as_millis() as u64,
                    mutation_applied: None,
                });
            }
        };

        // Step 5: Propose mutations - simplified
        let proposals = match Self::propose_mutations(&diagnosis, &investigation).await {
            Ok(p) => p,
            Err(_) => {
                return Ok(RepairOutcome {
                    skill_name: skill.name.clone(),
                    success: false,
                    confidence: 0.0,
                    repair_time_ms: start_time.elapsed().as_millis() as u64,
                    mutation_applied: None,
                });
            }
        };

        let best_mutation = proposals.first().cloned();

        // Step 6-8: Sandbox validation → Execute repair
        let repair_success = if let Some(mutation) = best_mutation.clone() {
             let sandboxconfig = SandboxConfig {
                 permission_mode: PermissionMode::AutoRepair,
                 timeout_ms: 5000,
                 cwd: std::env::current_dir().unwrap_or_default(),
                 enable_network: false,
                 enable_user_namespace: true,
                 drop_capabilities: true,
                 danger_mode: false,
             };

            // Validate in sandbox
            let validation_result = match Self::validate_in_sandbox(&mutation, &sandboxconfig).await
            {
                Ok(v) => v,
                Err(_e) => {
                    return Ok(RepairOutcome {
                        skill_name: skill.name.clone(),
                        success: false,
                        confidence: 0.0,
                        repair_time_ms: start_time.elapsed().as_millis() as u64,
                        mutation_applied: None,
                    });
                }
            };

            if validation_result.success {
                // Execute repair (apply mutation)
                // In a full implementation, we would apply the mutation to the skill code
                // For now, we'll just record that we applied a mutation
                RepairOutcome {
                    skill_name: skill.name.clone(),
                    success: true,
                    confidence: 0.8, // Placeholder confidence
                    repair_time_ms: start_time.elapsed().as_millis() as u64,
                    mutation_applied: Some(mutation),
                }
            } else {
                RepairOutcome {
                    skill_name: skill.name.clone(),
                    success: false,
                    confidence: 0.0,
                    repair_time_ms: start_time.elapsed().as_millis() as u64,
                    mutation_applied: None,
                }
            }
        } else {
            RepairOutcome {
                skill_name: skill.name.clone(),
                success: false,
                confidence: 0.0,
                repair_time_ms: start_time.elapsed().as_millis() as u64,
                mutation_applied: None,
            }
        };

        // Step 9: Store Genome (rusqlite)
        if let Some(ref mutation) = repair_success.mutation_applied {
            // In a full implementation, we would store the repair genome
            // For now, we'll just log that we would store it
            let skill_name_for_log = skill.name.clone();
            println!(
                "Would store repair genome for skill {} with mutation {}",
                skill_name_for_log, mutation
            );
        }

        // Step 10: Emit Completion Event
        // In a full implementation, we would emit an event via the interceptor's event sender

        Ok(repair_success)
    }

    /// Run the 7-phase investigation protocol (simplified)
    async fn run_investigation(
        _diagnosis: &crate::ast::analyzer::ASTDiagnosis,
        _trigger: &crate::interceptor::interceptor::RepairTrigger,
    ) -> Result<crate::investigation::protocol::InvestigationOutput> {
        // In a full implementation, this would run the full 7-phase investigation
        // For now, we'll return a dummy investigation output
        Ok(crate::investigation::protocol::InvestigationOutput {
            problem: crate::investigation::protocol::ProblemDefinition {
                expected_behavior: "Should work".to_string(),
                observed_behavior: "But failed".to_string(),
                scope: "Test".to_string(),
                reproducible: true,
            },
            reproduction: crate::investigation::protocol::ReproductionAttempt {
                method: crate::investigation::protocol::ReproductionMethod::ExistingTest,
                steps: vec![],
                observed_result: "Failed".to_string(),
                consistent: true,
            },
            evidence: crate::investigation::protocol::EvidenceCollection {
                facts: vec![crate::investigation::protocol::Fact {
                    layer: "Entry".to_string(),
                    input_value: "test".to_string(),
                    output_value: "failed".to_string(),
                    transformed: false,
                    condition: "error".to_string(),
                }],
            },
            hypothesis: crate::investigation::protocol::Hypothesis {
                root_cause: "Null pointer".to_string(),
                evidence: "Stack trace shows crash".to_string(),
            },
            guard: crate::investigation::protocol::FailureGuard {
                guard_type: crate::investigation::protocol::GuardType::AutomatedTest,
                description: "Test passes before fix".to_string(),
                passes_before_fix: true,
                passes_after_fix: false,
            },
            fix: crate::investigation::protocol::CodeFix {
                original_code: "int x = *ptr;".to_string(),
                fixed_code: "if (ptr) { int x = *ptr; } else { int x = 0; }".to_string(),
                rationale: "Added null check".to_string(),
                changes_count: 2,
            },
            verification: crate::investigation::protocol::Verification {
                original_reproduction_still_fails: false,
                guard_now_passes: true,
                related_tests_pass: true,
                side_effects_none: true,
            },
        })
    }

    /// Propose mutations based on diagnosis and investigation (simplified)
    async fn propose_mutations(
        __diagnosis: &crate::ast::analyzer::ASTDiagnosis,
        _investigation: &crate::investigation::protocol::InvestigationOutput,
    ) -> Result<Vec<String>> {
        // In a full implementation, this would use an LLM to propose mutations
        // For now, we'll return a dummy mutation
        Ok(vec!["if (ptr == nullptr) { return 0; }".to_string()])
    }

    /// Validate a mutation in the sandbox (simplified)
    async fn validate_in_sandbox(mutation: &str, config: &SandboxConfig) -> Result<SandboxResult> {
        // In a full implementation, we would create a test script that applies the mutation and runs tests
        // For now, we'll just run a simple command to see if the sandbox works
        let code = format!(
            r#"
            echo "Validating mutation: {}"
            # In a real implementation, we would compile and test the mutated code here
            exit 0
            "#,
            mutation
        );

        execute_in_sandbox(&code, config)
            .await
            .map_err(HephaestusError::Sandbox)
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
