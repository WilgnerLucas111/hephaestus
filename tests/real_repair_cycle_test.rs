use hephaestus::error::Result;
use hephaestus::memory::genome_store::GenomeStore;
use hephaestus::orchestration::project_repair::ProjectRepairEngine;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_real_repair_cycle_end_to_end() -> Result<()> {
    // 1. Create a temporary sample Rust project with an Index Out of Bounds bug
    let sample_dir = tempfile::tempdir()?;
    let sample_path = sample_dir.path();

    fs::create_dir_all(sample_path.join("src"))?;

    let cargo_toml = r#"
[package]
name = "buggy_sample"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    fs::write(sample_path.join("Cargo.toml"), cargo_toml)?;

    let buggy_code = r#"
pub fn get_element(v: &[i32], idx: usize) -> Option<i32> {
    Some(v[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_out_of_bounds() {
        let data = vec![10, 20, 30];
        let res = get_element(&data, 5);
        assert_eq!(res, None);
    }
}
"#;
    fs::write(sample_path.join("src/lib.rs"), buggy_code)?;

    // 2. Set up SQLite GenomeStore
    let db_path = PathBuf::from(format!(
        "real_repair_test_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let genome_store = GenomeStore::open(&db_path)?;
    let store_arc = Arc::new(Mutex::new(genome_store));

    // 3. Execute the full end-to-end P1 repair cycle
    let result =
        ProjectRepairEngine::execute_repair_cycle(sample_path, Path::new("src/lib.rs"), &store_arc)
            .await?;

    // 4. Assertions on repair outcome
    if !result.success {
        eprintln!("REPAIR FAILURE DETAILS: {:#?}", result);
    }
    assert!(result.success, "Repair cycle should succeed");
    assert!(
        result
            .original_failure
            .failing_test
            .contains("test_out_of_bounds")
            || !result.original_failure.stderr.is_empty(),
        "Should capture original test failure details"
    );

    let patch = result.patch.expect("Should produce a patch candidate");
    assert!(
        patch.diff.contains("--- a/src/lib.rs") || patch.diff.contains("src/lib.rs"),
        "Should generate a unified diff"
    );

    let validation = result.validation.expect("Should produce validation report");
    assert!(validation.compiled, "Patched code should compile");
    assert!(validation.tests_passed, "Patched code tests should pass");

    // 5. Verify genome persistence in SQLite
    {
        let store = store_arc.lock().await;
        let retrieved = store.get_genome(&result.genome_hash)?;
        assert!(retrieved.is_some(), "Genome should be stored in SQLite");
        let g = retrieved.unwrap();
        assert!(
            g.final_repaired_code.is_some(),
            "Stored genome should include repaired code"
        );
    }

    // 6. Verify safety guarantee: Original project was NOT modified
    let original_content = fs::read_to_string(sample_path.join("src/lib.rs"))?;
    assert_eq!(
        original_content, buggy_code,
        "Original source file must remain untouched during repair cycle"
    );

    // Clean up
    let _ = fs::remove_file(&db_path);

    Ok(())
}
