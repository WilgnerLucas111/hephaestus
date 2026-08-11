#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Initializing Hephaestus OS AI...");

    // Import necessary types
    use hephaestus::interceptor::interceptor::Skill;
    use hephaestus::interceptor::interceptor::{
        HephaestusEvent, HephaestusInterceptor, InterceptorConfig, PermissionMode,
    };
    use hephaestus::memory::genome_store::GenomeStore;
    use hephaestus::orchestration::bifurcated_agent::{BifurcatedAgent, BifurcatedAgentConfig};

    // Initialize the genome store (using a file for persistence)
    let genome_store = GenomeStore::open("hephaestus_genomes.db")?;
    println!("Genome store initialized.");

    // Configure the bifurcated agent
    let config = BifurcatedAgentConfig {
        max_parallel_repairs: 2,
        reinvoke_after_successful_repair: true,
        log_dir: std::path::PathBuf::from("/tmp/logs"),
    };
    println!("Bifurcated agent configuration loaded.");

    // Create the bifurcated agent
    let mut agent = BifurcatedAgent::new(config, genome_store)?;
    println!("Bifurcated agent created.");

    // Set up the interceptor for skill execution monitoring
    let (event_tx, _event_rx) = std::sync::mpsc::channel::<HephaestusEvent>();
    let interceptor_config = InterceptorConfig {
        permission_mode: PermissionMode::AutoRepair,
        max_repair_wait_ms: 5000,
        reinvoke_after_repair: true,
        repair_log_path: std::path::PathBuf::from("/tmp/repair.log"),
    };
    let interceptor = HephaestusInterceptor::new(event_tx, interceptor_config);
    println!("Interceptor initialized.");

    // Create a test skill (a simple Rust function that will succeed)
    let skill = Skill {
        name: "test_skill".to_string(),
        code: r#"
            fn main() {
                println!("Hello from test skill!");
            }
        "#
        .to_string(),
    };
    println!("Test skill created: {}", skill.name);

    // Execute the skill with the bifurcated agent (monitored by interceptor)
    let result = agent.executeskill_bifurcated(&skill, &interceptor).await?;
    println!("Skill execution result: {:?}", result);

    // Optionally, we could process events from the interceptor here
    // For now, we'll just note that the agent is ready for further skills
    println!("Hephaestus OS AI is ready and operational.");

    Ok(())
}
