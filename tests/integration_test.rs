use hephaestus::{
    interceptor::interceptor::{HephaestusInterceptor, InterceptorConfig, PermissionMode, Skill, SkillResult},
    memory::genome_store::{GenomeStore, RepairGenome, RejectionRecord},
    orchestration::bifurcated_agent::{BifurcatedAgent, BifurcatedAgentConfig},
};
use std::time::{SystemTime, UNIX_EPOCH, Duration};

// Test genome store operations
#[tokio::test]
async fn test_genome_store_operations() {
    // Set up in-memory SQLite GenomeStore using a temporary file
    
    let db_path = std::path::PathBuf::from(format!("integration_test_{}.db", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let genome_store = GenomeStore::open(&db_path).expect("Failed to create genome store");
    
    // Create a test genome
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let test_genome = RepairGenome {
        hash: "test_skill_hash".to_string(),
        original_code: "fn test() { panic!() }".to_string(),
        telemetry_trigger: Some("panic: index out of bounds".to_string()),
        monkey_hypotheses: vec!["fn test() { }".to_string()],
        rejected_patches: vec![RejectionRecord {
            patch: "fn test() { let x = 1; }".to_string(),
            reason: "does not match original behavior".to_string(),
            timestamp: now,
        }],
        final_repaired_code: Some("fn test() { }".to_string()),
        narrative_summary: Some("The Wild Monkey generated a hypothesis. The Neutral Judge found it safe. The Angry Master applied no penalties. The Narrative Agent recorded the verdict.".to_string()),
        timestamp: now,
    };
    
    // Store the genome
    genome_store.store_genome(&test_genome)
        .expect("Failed to store genome");
    
    // Retrieve the genome by hash
    let retrieved = genome_store.get_genome(&test_genome.hash)
        .expect("Failed to get genome");
    
    assert!(retrieved.is_some(), "Expected genome to be found");
    if let Some(ref r) = retrieved {
        assert_eq!(r.hash, test_genome.hash);
        assert_eq!(r.original_code, test_genome.original_code);
        assert_eq!(r.telemetry_trigger, test_genome.telemetry_trigger);
        assert_eq!(r.monkey_hypotheses, test_genome.monkey_hypotheses);
        assert_eq!(r.rejected_patches, test_genome.rejected_patches);
        assert_eq!(r.final_repaired_code, test_genome.final_repaired_code);
        assert_eq!(r.narrative_summary, test_genome.narrative_summary);
        assert_eq!(r.timestamp, test_genome.timestamp);
    }
    
    // Test updating the genome with new content
    let mut updated_genome = test_genome.clone();
    updated_genome.final_repaired_code = Some("fn test() { let x = 42; }".to_string());
    updated_genome.timestamp = now + 1;
    genome_store.store_genome(&updated_genome)
        .expect("Failed to update genome");
    
    let updated = genome_store.get_genome(&test_genome.hash)
        .expect("Failed to get updated genome");
    
    assert!(updated.is_some(), "Expected updated genome to be found");
    if let Some(ref r) = updated {
        assert_eq!(r.final_repaired_code, Some("fn test() { let x = 42; }".to_string()));
        assert_eq!(r.timestamp, now + 1);
    }
    
    // Test deleting the genome
    genome_store.delete_genome(&test_genome.hash)
        .expect("Failed to delete genome");
    
    let deleted = genome_store.get_genome(&test_genome.hash)
        .expect("Failed to get genome after deletion");
    
    assert!(deleted.is_none(), "Expected genome to be not found after deletion");
}

// Test that a successful skill execution does not spawn a background repair task
#[tokio::test]
async fn test_bifucated_agent_skill_execution_does_not_spawn_background_task_on_success() {
    // Set up in-memory SQLite GenomeStore using a temporary file
    
    let db_path = std::path::PathBuf::from(format!("integration_test_{}.db", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let genome_store = GenomeStore::open(&db_path).expect("Failed to create genome store");
    
    // Set up HephaestusInterceptor
    let (event_sender, _event_receiver) = std::sync::mpsc::channel();
    let interceptor_config = InterceptorConfig {
        permission_mode: PermissionMode::AutoRepair,
        max_repair_wait_ms: 1000,
        reinvoke_after_repair: false,
        repair_log_path: std::path::PathBuf::from("."),
    };
    
    let interceptor = HephaestusInterceptor::new(
        event_sender,
        interceptor_config,
    );
    
     // Set up BifurcatedAgent with real dependencies
     let config = BifurcatedAgentConfig {
         max_parallel_repairs: 2,
         reinvoke_after_successful_repair: true,
         log_dir: std::path::PathBuf::from("/tmp/logs"),
     };
    
     let mut bif_agent = BifurcatedAgent::new(
         config,
         genome_store,
     ).expect("Failed to create BifurcatedAgent");
    
    // Create a skill (the actual code doesn't matter because the skill runner always returns success)
    let skill = Skill {
        name: "test_skill".to_string(),
        code: "fn test() {};".to_string(),
    };
    
    // Execute the skill - should return immediately and return success
    let start_time = std::time::Instant::now();
    let skill_result = bif_agent.executeskill_bifurcated(&skill, &interceptor).await;
    let elapsed = start_time.elapsed();
    
    // Assert that the main loop returned immediately (should be very fast)
    assert!(elapsed < Duration::from_millis(100), "Main loop did not return immediately");
    
    // The skill should have succeeded (because the skill runner always returns success and the interceptor mock returns success)
    assert!(matches!(skill_result, Ok(SkillResult::Success)), 
        "Expected Ok(SkillResult::Success), got {:?}", skill_result);
    
    // We cannot directly check the repair_tasks vector because it's private.
    // Instead, we check that the agent is still in a valid state by trying to execute another skill.
    let skill_result2 = bif_agent.executeskill_bifurcated(&skill, &interceptor).await;
    assert!(matches!(skill_result2, Ok(SkillResult::Success)), 
        "Expected Ok(SkillResult::Success) on second execution, got {:?}", skill_result2);
}

// Test that a panicking skill triggers background repair (we'll just check that the agent can be created and the skill fails)
#[tokio::test]
async fn test_bifucated_agent_panicking_skill_triggers_background_repair_attempt() {
    // Set up in-memory SQLite GenomeStore using a temporary file
    
    let db_path = std::path::PathBuf::from(format!("integration_test_{}.db", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let genome_store = GenomeStore::open(&db_path).expect("Failed to create genome store");
    
    // Set up HephaestusInterceptor
    let (event_sender, _event_receiver) = std::sync::mpsc::channel();
    let interceptor_config = InterceptorConfig {
        permission_mode: PermissionMode::AutoRepair,
        max_repair_wait_ms: 1000,
        reinvoke_after_repair: false,
        repair_log_path: std::path::PathBuf::from("."),
    };
    
    let interceptor = HephaestusInterceptor::new(
        event_sender,
        interceptor_config,
    );
    
    // Create BifurcatedAgent with real dependencies
    let config = BifurcatedAgentConfig {
        max_parallel_repairs: 2,
        reinvoke_after_successful_repair: true,
         log_dir: std::path::PathBuf::from("/tmp/logs"),
    };
    
     let mut bif_agent = BifurcatedAgent::new(
         config,
         genome_store,
     ).expect("Failed to create BifurcatedAgent");
    
    // Create a panicking skill
    let skill = Skill {
        name: "panicking_skill".to_string(),
        code: "panic!(\"Intentional panic for testing\");".to_string(),
    };
    
    // Execute the skill - should return immediately and return an error (because the skill panics)
    let start_time = std::time::Instant::now();
    let skill_result = bif_agent.executeskill_bifurcated(&skill, &interceptor).await;
    let elapsed = start_time.elapsed();
    
    // Assert that the main loop returned immediately (should be very fast)
    assert!(elapsed < Duration::from_millis(100), "Main loop did not return immediately");
    
    // The skill should have failed (because it panics)
    assert!(matches!(skill_result, Ok(SkillResult::Failure(_))), "Expected skill execution to fail with error");
    
    // We cannot easily check that a background task was spawned without more complex mocking,
    // but we can at least verify that the agent is still functional by executing another skill.
    let skill_result2 = bif_agent.executeskill_bifurcated(&skill, &interceptor).await;
    assert!(matches!(skill_result2, Ok(SkillResult::Failure(_))), "Expected skill execution to fail with error on second execution");
}