use hephaestus::memory::genome_store::RepairGenome;
use hephaestus::tribunal::{AngryMaster, NarrativeAgent, NeutralJudge, WildMonkey};
use std::time::{SystemTime, UNIX_EPOCH};
use hephaestus::error::Result;

#[tokio::test]
async fn test_tribunal_integration() -> Result<()> {
    // Create a test genome
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut genome = RepairGenome {
        hash: "test_tribunal_hash".to_string(),
        original_code: "fn main() { panic!(\"test\"); }".to_string(),
        telemetry_trigger: None,
        monkey_hypotheses: Vec::new(),
        rejected_patches: Vec::new(),
        final_repaired_code: None,
        narrative_summary: None,
        timestamp: now,
    };

    // Initialize tribunal actors
    let wild_monkey = WildMonkey;
    let neutral_judge = NeutralJudge;
    let angry_master = AngryMaster;
    let narrative_agent = NarrativeAgent;

    // Step 1: Wild Monkey generates patches
    let telemetry = "panic: test panic in main";
    let patches = wild_monkey
        .generate_patches(&mut genome, telemetry)
        .await?;
    
    assert_eq!(patches.len(), 2, "Should generate exactly 2 patches");
    assert_eq!(genome.monkey_hypotheses.len(), 2, "Genome should store hypotheses");

    // Step 2: Neutral Judge executes trials
    let trial_results = neutral_judge
        .execute_trials(patches.clone(), &genome)
        .await?;
    
    assert_eq!(trial_results.len(), 2, "Should have results for both patches");
    
    // Step 3: Neutral Judge inspects safeguards
    let _ = neutral_judge
        .inspect_safeguards(patches.clone())
        .await?;
    
    // Step 4: Angry Master applies penalties
    angry_master
        .apply_penalties(&mut genome, trial_results.clone())
        .await?;

    // Step 5: Narrative Agent records verdict
    let verdict = genome.final_repaired_code.is_some();
    narrative_agent
        .record_verdict(&mut genome, verdict, telemetry)
        .await?;

    // Verify final state
    assert!(genome.timestamp >= now, "Timestamp should be updated");
    assert!(genome.telemetry_trigger.is_some(), "Telemetry trigger should be set");
    assert!(genome.narrative_summary.is_some(), "Narrative summary should be set");
    
    // Either we have a final repaired code or we have rejected patches
    assert!(
        genome.final_repaired_code.is_some() || !genome.rejected_patches.is_empty(),
        "Should have either a final repair or rejections"
    );

    Ok(())
}