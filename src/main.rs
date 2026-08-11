use clap::{Parser, Subcommand};
use hephaestus::memory::genome_store::GenomeStore;
use hephaestus::orchestration::project_repair::ProjectRepairEngine;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(
    name = "hephaestus",
    about = "Self-Repair Framework for Rust Projects",
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute an end-to-end sandboxed repair cycle on a target Rust project
    Repair {
        /// Path to the Rust project root
        #[arg(short, long, default_value = ".")]
        project: PathBuf,

        /// Relative path to the target source file
        #[arg(short, long, default_value = "src/lib.rs")]
        target: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    println!("⚡ Hephaestus: Self-Repair Framework for Rust Projects");
    println!("========================================================\n");

    match cli.command {
        Commands::Repair { project, target } => {
            println!("📁 Target Project: {}", project.display());
            println!("📄 Target File:    {}", target.display());
            println!("🔒 Isolation Mode: Sandboxed Temp Workspace");
            println!("========================================================\n");

            let db_path = PathBuf::from("hephaestus_genomes.db");
            let genome_store = GenomeStore::open(&db_path)?;
            let store_arc = Arc::new(Mutex::new(genome_store));

            println!("🔍 Step 1: Reproducing failure in sandboxed workspace copy...");
            let result =
                ProjectRepairEngine::execute_repair_cycle(&project, &target, &store_arc).await;

            match result {
                Ok(outcome) => {
                    println!("\n💥 Failure Captured:");
                    println!(
                        "   • Failing Test: {}",
                        outcome.original_failure.failing_test
                    );
                    println!(
                        "   • Reproduce Time: {} ms",
                        outcome.original_failure.duration_ms
                    );

                    if let Some(ref patch) = outcome.patch {
                        println!("\n🐒 [WildMonkey] Generated Candidate Patch:");
                        println!("   • Rationale: {}", patch.rationale);
                        println!("\n📜 Unified Diff:\n{}", patch.diff);
                    }

                    if let Some(ref val) = outcome.validation {
                        println!("⚖️ [NeutralJudge & AngryMaster] Empirical Validation:");
                        println!(
                            "   • Cargo Check:  {}",
                            if val.compiled {
                                "✅ PASSED"
                            } else {
                                "❌ FAILED"
                            }
                        );
                        println!(
                            "   • Cargo Clippy: {}",
                            if val.clippy_passed {
                                "✅ PASSED"
                            } else {
                                "❌ FAILED"
                            }
                        );
                        println!(
                            "   • Cargo Test:   {}",
                            if val.tests_passed {
                                "✅ PASSED"
                            } else {
                                "❌ FAILED"
                            }
                        );
                        println!(
                            "   • Unsafe Free:  {}",
                            if val.unsafe_free {
                                "✅ PASSED"
                            } else {
                                "❌ FAILED"
                            }
                        );
                        println!(
                            "   • Line Budget:  {}",
                            if val.line_budget_ok {
                                "✅ PASSED"
                            } else {
                                "❌ FAILED"
                            }
                        );
                        println!("   • Validation Time: {} ms", val.duration_ms);
                    }

                    println!("\n🏛️ [NarrativeAgent] Verdict:");
                    if outcome.success {
                        println!("   ✅ REPAIR APPROVED AND STORED IN REPAIR GENOME");
                        println!("   • Genome Hash: {}", outcome.genome_hash);
                        println!("   • Host Project Safety: ORIGINAL WORKSPACE REMAINED UNTOUCHED");
                    } else {
                        println!("   ❌ REPAIR VETOED BY TRIBUNAL");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Repair Cycle Failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
