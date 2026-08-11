use crate::error::{HephaestusError, Result};
use crate::memory::evo_genome::{AAKEncoder, SemanticClusterer, WingManager};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Genome store for persisting RepairGenomes in SQLite.
///
/// This store provides safe persistence of RepairGenomes with protection against
/// SQL injection and path traversal attacks.
///
/// In Phase 9 (Evo-Genome), this store also implements:
/// - Semantic clustering of failures using Leiden clustering (via Graphify placeholder)
/// - Wing-based organization in the MemPalace (WingOfConcurrency, WingOfMemory, etc.)
/// - AAAK compression for high-density storage of repair solutions
pub struct GenomeStore {
    connection: Connection,
    db_path: PathBuf,
    /// Semantic clusterer for grouping similar failures
    semantic_clusterer: Mutex<SemanticClusterer>,
    /// Wing manager for organizing knowledge in the MemPalace
    wing_manager: Mutex<WingManager>,
    /// AAAK encoder for high-density storage
    aaak_encoder: Mutex<AAKEncoder>,
}

impl GenomeStore {
    /// Creates a new genome store at the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the SQLite database should be stored.
    ///   Must be a valid, non-empty path without directory traversal sequences.
    ///
    /// # Returns
    ///
    /// * `Ok(GenomeStore)` if the store was created successfully
    /// * `Err(HephaestusError)` if path validation fails or database initialization fails
    pub fn open<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let db_path = path.into();
        Self::validate_path(&db_path)?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open database connection
        let connection = Connection::open(&db_path)?;

        // Initialize schema
        Self::initialize_schema(&connection)?;

        Ok(GenomeStore {
            connection,
            db_path,
            semantic_clusterer: Mutex::new(SemanticClusterer::new()),
            wing_manager: Mutex::new(WingManager::new()),
            aaak_encoder: Mutex::new(AAKEncoder::new()),
        })
    }

    /// Validates that the database path is safe to use.
    ///
    /// # Checks
    ///
    /// * Path is not empty
    /// * Path does not contain directory traversal sequences (..)
    /// * Path is not absolute (we restrict to relative paths from working directory)
    /// * Path has a valid file extension (.db or .sqlite)
    ///
    /// # Arguments
    ///
    /// * `path` - Path to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` if path is valid
    /// * `Err(HephaestusError::InvalidInput)` if validation fails
    fn validate_path(path: &Path) -> Result<()> {
        // Guard clause: empty path
        if path.to_string_lossy().is_empty() {
            return Err(HephaestusError::InvalidInput(
                "Database path cannot be empty".to_string(),
            ));
        }

        // Guard clause: directory traversal
        let components: Vec<_> = path.components().collect();
        if components
            .iter()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(HephaestusError::InvalidInput(
                "Database path cannot contain directory traversal sequences".to_string(),
            ));
        }

        // Guard clause: absolute paths (restrict to relative paths from working directory)
        if path.is_absolute() {
            return Err(HephaestusError::InvalidInput(
                "Database path must be relative to working directory".to_string(),
            ));
        }

        // Guard clause: valid extension
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();
        if extension != "db" && extension != "sqlite" {
            return Err(HephaestusError::InvalidInput(
                "Database path must have .db or .sqlite extension".to_string(),
            ));
        }

        Ok(())
    }

    /// Initializes the database schema if it doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `connection` - Database connection to initialize
    ///
    /// # Returns
    ///
    /// * `Ok(())` if schema initialized successfully
    /// * `Err(HephaestusError)` if schema initialization fails
    fn initialize_schema(connection: &Connection) -> Result<()> {
        connection.execute(
            "
            CREATE TABLE IF NOT EXISTS genomes (
                id INTEGER PRIMARY KEY,
                hash TEXT NOT NULL UNIQUE,
                genome_json TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            ",
            params![],
        )?;

        // Create index on hash for faster lookups
        connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_genomes_hash ON genomes(hash)",
            params![],
        )?;

        Ok(())
    }

    /// Stores a genome in the database.
    ///
    /// Uses UPSERT pattern to insert new genomes or update existing ones by hash.
    /// Enforces the 45 MiB memory limit by checking database size before insertion.
    ///
    /// In Phase 9, this method also applies Evo-Genome enhancements:
    /// - Assigns semantic clusters using Leiden clustering (via Graphify placeholder)
    /// - Assigns wings in the MemPalace (WingOfConcurrency, WingOfMemory, etc.)
    /// - Encodes repair solutions using AAAK for high-density storage
    ///
    /// # Arguments
    ///
    /// * `genome` - The RepairGenome to store
    ///
    /// # Returns
    ///
    /// * `Ok(())` if genome was stored successfully
    /// * `Err(HephaestusError)` if storage fails or memory limit would be exceeded
    pub fn store_genome(&self, genome: &mut RepairGenome) -> Result<()> {
        // Guard clause: validate input
        let hash = &genome.hash;
        if hash.is_empty() {
            return Err(HephaestusError::InvalidInput(
                "Genome hash cannot be empty".to_string(),
            ));
        }

        // Apply Evo-Genome enhancements before storage
        // 1. Assign semantic cluster (using simple hash-based clustering as placeholder for Leiden)
        if genome.semantic_cluster.is_none() {
            genome.semantic_cluster = {
                let mut clusterer = self.semantic_clusterer.lock().unwrap();
                Some(clusterer.assign_cluster(genome))
            };
        }

        // 2. Assign wing in MemPalace
        if genome.wing.is_none() {
            genome.wing = {
                let mut wing_manager = self.wing_manager.lock().unwrap();
                Some(wing_manager.assign_wing(genome))
            };
        }

        // 3. Encode final repaired code using AAAK (if present)
        if let Some(ref mut repaired_code) = genome.final_repaired_code {
            // Only encode if we haven't already done so (aaak_compressed is None)
            if genome.aaak_compressed.is_none() {
                let encoder = self.aaak_encoder.lock().unwrap();
                genome.aaak_compressed = Some(encoder.encode(repaired_code.as_bytes()));
                // In a full implementation, we would store the compressed version and keep
                // the original for readability. For now, we'll keep both.
            }
        }

        // Calculate dependency density (placeholder - in future would use actual AST analysis)
        if genome.dependency_density.is_none() {
            // Simple heuristic: more functions/methods might indicate higher dependency density
            let dep_density = genome.original_code.matches("fn ").count() as f64 * 0.1
                + genome.original_code.matches("struct ").count() as f64 * 0.2
                + genome.original_code.matches("impl ").count() as f64 * 0.3;
            genome.dependency_density = Some(dep_density.min(1.0)); // Cap at 1.0
        }

        // Calculate AST topology hash (placeholder - in future would use actual AST hashing)
        if genome.ast_topology_hash.is_none() {
            // Simple hash of the original code as placeholder
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            genome.original_code.hash(&mut hasher);
            genome.ast_topology_hash = Some(hasher.finish());
        }

        // Serialize the enhanced genome to JSON
        let genome_json = serde_json::to_string(&genome).map_err(HephaestusError::Json)?;

        // Check memory limit (45 MiB = 45 * 1024 * 1024 bytes)
        const MAX_SIZE_BYTES: u64 = 45 * 1024 * 1024;
        let current_size = self.get_database_size()?;
        let json_size = genome_json.len() as u64;
        let new_size = current_size.checked_add(json_size).ok_or_else(|| {
            HephaestusError::InvalidInput("Genome size calculation overflow".to_string())
        })?;

        if new_size > MAX_SIZE_BYTES {
            return Err(HephaestusError::InvalidInput(format!(
                "Storing this genome would exceed the 45 MiB memory limit. \
                Current size: {} bytes, Required: {} bytes, Limit: {} bytes",
                current_size, json_size, MAX_SIZE_BYTES
            )));
        }

        // Use UPSERT to insert or update genome by hash
        self.connection.execute(
            "
            INSERT INTO genomes (hash, genome_json, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(hash) DO UPDATE SET
                genome_json = excluded.genome_json,
                updated_at = CURRENT_TIMESTAMP
            ",
            params![hash, genome_json],
        )?;

        Ok(())
    }

    /// Retrieves a genome by its hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - Hash of the genome to retrieve
    ///
    /// # Returns
    ///
    /// * `Ok(Some(genome))` if genome was found
    /// * `Ok(None)` if genome was not found
    /// * `Err(HephaestusError)` if retrieval fails
    pub fn get_genome(&self, hash: &str) -> Result<Option<RepairGenome>> {
        // Guard clause: validate input
        if hash.is_empty() {
            return Err(HephaestusError::InvalidInput(
                "Genome hash cannot be empty".to_string(),
            ));
        }

        let mut stmt = self
            .connection
            .prepare("SELECT genome_json FROM genomes WHERE hash = ?1")?;

        let mut rows = stmt.query_map(params![hash], |row| row.get::<_, String>(0))?;

        // Since hash is unique, we expect at most one row
        let genome_json = rows.next().transpose()?;
        let genome = if let Some(json) = genome_json {
            Some(serde_json::from_str(&json).map_err(HephaestusError::Json)?)
        } else {
            None
        };

        Ok(genome)
    }

    /// Deletes a genome by its hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - Hash of the genome to delete
    ///
    /// # Returns
    ///
    /// * `Ok(())` if genome was deleted successfully
    /// * `Err(HephaestusError)` if deletion fails
    pub fn delete_genome(&self, hash: &str) -> Result<()> {
        // Guard clause: validate input
        if hash.is_empty() {
            return Err(HephaestusError::InvalidInput(
                "Genome hash cannot be empty".to_string(),
            ));
        }

        let rows_deleted = self
            .connection
            .execute("DELETE FROM genomes WHERE hash = ?1", params![hash])?;

        if rows_deleted == 0 {
            return Err(HephaestusError::NotFound(format!(
                "Genome with hash {} not found",
                hash
            )));
        }

        Ok(())
    }

    /// Gets the current size of the database file in bytes.
    ///
    /// # Returns
    ///
    /// * `Ok(size)` - Size of database file in bytes
    /// * `Err(HephaestusError)` if file size cannot be determined
    fn get_database_size(&self) -> Result<u64> {
        let metadata = std::fs::metadata(&self.db_path)?;
        Ok(metadata.len())
    }

    /// Retrieves all successful genomes (those with a final repaired code).
    ///
    /// # Returns
    ///
    /// * `Ok(vec)` - Vector of successful RepairGenomes
    /// * `Err(HephaestusError)` if the query fails
    pub fn get_successful_genomes(&self) -> Result<Vec<RepairGenome>> {
        let mut stmt = self.connection.prepare("SELECT genome_json FROM genomes")?;

        let rows = stmt.query_map(params![], |row| row.get::<_, String>(0))?;

        let mut genomes = Vec::new();
        for row in rows {
            let genome_json = row?;
            let genome: RepairGenome =
                serde_json::from_str(&genome_json).map_err(HephaestusError::Json)?;
            if genome.final_repaired_code.is_some() {
                genomes.push(genome);
            }
        }

        Ok(genomes)
    }
}

/// The four personalities of the Tribunal architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TribunalActor {
    WildMonkey,
    NeutralJudge,
    AngryMaster,
    NarrativeAgent,
}

/// A record of a rejected patch during the tribunal process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RejectionRecord {
    /// The patch that was rejected
    pub patch: String,
    /// The reason for rejection
    pub reason: String,
    /// Timestamp of rejection
    pub timestamp: u64,
}

/// The complete genome ledger entry, containing all metadata and state
/// for a code repair attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairGenome {
    /// Unique hash of the genome (typically SHA-256 of the original code)
    pub hash: String,
    /// The original code that needs repair
    pub original_code: String,
    /// Telemetry data that triggered the repair attempt
    pub telemetry_trigger: Option<String>,
    /// Hypotheses generated by the Wild Monkey actor
    pub monkey_hypotheses: Vec<String>,
    /// Patches that were rejected during tribunal trials
    pub rejected_patches: Vec<RejectionRecord>,
    /// The final repaired code, if any
    pub final_repaired_code: Option<String>,
    /// Narrative summary of the tribunal proceedings
    pub narrative_summary: Option<String>,
    /// Timestamp when this genome was last updated
    pub timestamp: u64,
    /// Simple semantic cluster ID for grouping similar failures (placeholder for Leiden clustering)
    pub semantic_cluster: Option<u64>,
    /// Wing assignment in the MemPalace (e.g., "WingOfConcurrency", "WingOfMemory")
    pub wing: Option<String>,
    /// Placeholder for AAAK compressed representation (for future high-density storage)
    pub aaak_compressed: Option<Vec<u8>>,
    /// Simple dependency density measure (placeholder for future enhancement)
    pub dependency_density: Option<f64>,
    /// Simple AST topology hash (placeholder for future enhancement)
    pub ast_topology_hash: Option<u64>,
}

impl RepairGenome {
    /// Helper constructor to instantiate RepairGenome with default optional fields.
    pub fn new<S: Into<String>>(hash: S, original_code: S) -> Self {
        Self {
            hash: hash.into(),
            original_code: original_code.into(),
            telemetry_trigger: None,
            monkey_hypotheses: Vec::new(),
            rejected_patches: Vec::new(),
            final_repaired_code: None,
            narrative_summary: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            semantic_cluster: None,
            wing: None,
            aaak_compressed: None,
            dependency_density: None,
            ast_topology_hash: None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_genome_store_operations() -> Result<()> {
        // Create temporary directory for test database
        let temp_dir = tempfile::tempdir()?;
        let db_path = std::path::PathBuf::from("test.db");

        // Open store
        let store = GenomeStore::open(&db_path)?;

        // Create a test genome
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let genome = RepairGenome {
            hash: "test_hash_123".to_string(),
            original_code: "fn main() {};".to_string(),
            telemetry_trigger: Some("panic: index out of bounds".to_string()),
            monkey_hypotheses: vec![
                "fn main() { let x = 0; }".to_string(),
                "fn main() { let x = 1; }".to_string(),
            ],
            rejected_patches: vec![RejectionRecord {
                patch: "fn main() { let x = 2; }".to_string(),
                reason: "still panics".to_string(),
                timestamp: now,
            }],
            final_repaired_code: Some("fn main() { let x = 0; }".to_string()),
            narrative_summary: Some("The Wild Monkey generated two hypotheses. The Neutral Judge found the first hypothesis safe and effective. The Angry Master applied no penalties. The Narrative Agent recorded the verdict.".to_string()),
            timestamp: now,
            semantic_cluster: Some(1),
            wing: Some("WingOfPanics".to_string()),
            aaak_compressed: Some(vec![1, 2, 3]),
            dependency_density: Some(0.5),
            ast_topology_hash: Some(12345),
        };

        // Test storing a genome
        store.store_genome(&mut genome.clone())?;

        // Test retrieving the genome
        let retrieved = store.get_genome(&genome.hash)?;
        assert!(retrieved.is_some(), "Genome should exist");
        if let Some(ref r) = retrieved {
            assert_eq!(r.hash, genome.hash);
            assert_eq!(r.original_code, genome.original_code);
            assert_eq!(r.telemetry_trigger, genome.telemetry_trigger);
            assert_eq!(r.monkey_hypotheses, genome.monkey_hypotheses);
            assert_eq!(r.rejected_patches, genome.rejected_patches);
            assert_eq!(r.final_repaired_code, genome.final_repaired_code);
            assert_eq!(r.narrative_summary, genome.narrative_summary);
            assert_eq!(r.timestamp, genome.timestamp);
            assert_eq!(r.semantic_cluster, genome.semantic_cluster);
            assert_eq!(r.wing, genome.wing);
            assert_eq!(r.aaak_compressed, genome.aaak_compressed);
            assert_eq!(r.dependency_density, genome.dependency_density);
            assert_eq!(r.ast_topology_hash, genome.ast_topology_hash);
        }

        // Test updating the genome
        let mut updated_genome = genome.clone();
        updated_genome.final_repaired_code = Some("fn main() { let x = 42; }".to_string());
        updated_genome.timestamp = now + 1;
        updated_genome.semantic_cluster = Some(2);
        updated_genome.wing = Some("WingOfMemory".to_string());
        store.store_genome(&mut updated_genome)?;
        let retrieved_updated = store.get_genome(&genome.hash)?;
        assert!(
            retrieved_updated.is_some(),
            "Genome should exist after update"
        );
        if let Some(ref r) = retrieved_updated {
            assert_eq!(
                r.final_repaired_code,
                Some("fn main() { let x = 42; }".to_string())
            );
            assert_eq!(r.timestamp, now + 1);
            assert_eq!(r.semantic_cluster, Some(2));
            assert_eq!(r.wing, Some("WingOfMemory".to_string()));
        }

        // Test deleting the genome
        store.delete_genome(&genome.hash)?;
        let deleted = store.get_genome(&genome.hash)?;
        assert!(deleted.is_none(), "Genome should not exist after deletion");

        // Clean up
        let _ = std::fs::remove_file(&db_path);
        temp_dir.close()?;
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_path_validation() -> Result<()> {
        // Test empty path
        assert!(GenomeStore::open("").is_err());

        // Test path with directory traversal
        assert!(GenomeStore::open("../../etc/passwd.db").is_err());

        // Test absolute path (should fail in our implementation)
        assert!(GenomeStore::open("/tmp/test.db").is_err());

        // Test invalid extension
        assert!(GenomeStore::open("test.txt").is_err());

        // Test valid relative path
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("valid.db");

        // Convert to relative path from current directory for validation
        let relative_path = if let Ok(cwd) = std::env::current_dir() {
            db_path
                .strip_prefix(&cwd)
                .unwrap_or(db_path.as_path())
                .to_path_buf()
        } else {
            db_path
        };

        let result = GenomeStore::open(&relative_path);
        assert!(result.is_ok());
        temp_dir.close()?;
        Ok(())
    }
}
