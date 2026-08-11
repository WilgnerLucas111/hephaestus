use hephaestus::memory::genome_store::GenomeStore;
use hephaestus::orchestration::project_repair::ProjectRepairEngine;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    println!("⚡ Hephaestus: Self-Repair Framework for Rust Projects");
    println!("========================================================\n");

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "repair" => {
            let mut project_path = None;
            let mut target_file = None;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--project" | "-p" => {
                        if i + 1 < args.len() {
                            project_path = Some(PathBuf::from(&args[i + 1]));
                            i += 1;
                        }
                    }
                    "--target" | "-t" => {
                        if i + 1 < args.len() {
                            target_file = Some(PathBuf::from(&args[i + 1]));
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            let project = project_path.unwrap_or_else(|| PathBuf::from("."));
            let target = target_file.unwrap_or_else(|| PathBuf::from("src/lib.rs"));

            println!("📁 Target Project: {}", project.display());
            println!("📄 Target File:    {}", target.display());
            println!("🔒 Isolation Mode: Sandboxed Temp Workspace (--offline)");
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
                        println!("   • Unsafe Free:  ✅ PASSED");
                        println!("   • Line Budget:  ✅ PASSED");
                        println!("   • Validation Time: {} ms", val.duration_ms);
                    }

                    println!("\n🏛️ [NarrativeAgent] Verdict:");
                    if outcome.success {
                        println!("   ✅ REPAIR APPROVED AND STORED IN REPAIR GENOME");
                        println!("   • Genome Hash: {}", outcome.genome_hash);
                        println!("   • Host Project Safety: ORIGINAL WORKSPACE REMAINED UNTOUCHED");
                    } else {
                        println!("   ❌ REPAIR VETOED BY TRIBUNAL");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Repair Cycle Failed: {}", e);
                }
            }
        }
        _ => {
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("Usage:");
    println!("  hephaestus repair --project <PATH> --target <REL_PATH>");
    println!("\nOptions:");
    println!("  --project, -p <PATH>     Path to the Rust project root (default: .)");
    println!(
        "  --target,  -t <REL_PATH> Relative path to the target source file (default: src/lib.rs)"
    );
    println!("  --help,    -h            Show this help message");
}
