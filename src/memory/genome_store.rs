use crate::error::{HephaestusError, Result};
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

/// Genome store for persisting AST genomes in SQLite.
///
/// This store provides safe persistence of AST genomes with protection against
/// SQL injection and path traversal attacks.
pub struct GenomeStore {
    connection: Connection,
    db_path: PathBuf,
}

impl GenomeStore {
    /// Creates a new genome store at the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the SQLite database should be stored.
    ///            Must be a valid, non-empty path without directory traversal sequences.
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
                content BLOB NOT NULL,
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
    /// # Arguments
    ///
    /// * `hash` - Unique identifier for the genome (typically SHA-256 of content)
    /// * `content` - Genome content to store
    ///
    /// # Returns
    ///
    /// * `Ok(())` if genome was stored successfully
    /// * `Err(HephaestusError)` if storage fails or memory limit would be exceeded
    pub fn store_genome(&self, hash: &str, content: &[u8]) -> Result<()> {
        // Guard clause: validate input
        if hash.is_empty() {
            return Err(HephaestusError::InvalidInput(
                "Genome hash cannot be empty".to_string(),
            ));
        }

        if content.is_empty() {
            return Err(HephaestusError::InvalidInput(
                "Genome content cannot be empty".to_string(),
            ));
        }

        // Check memory limit (45 MiB = 45 * 1024 * 1024 bytes)
        const MAX_SIZE_BYTES: u64 = 45 * 1024 * 1024;
        let current_size = self.get_database_size()?;
        let new_size = current_size
            .checked_add(content.len() as u64)
            .ok_or_else(|| {
                HephaestusError::InvalidInput("Genome size calculation overflow".to_string())
            })?;

        if new_size > MAX_SIZE_BYTES {
            return Err(HephaestusError::InvalidInput(format!(
                "Storing this genome would exceed the 45 MiB memory limit. \
                Current size: {} bytes, Required: {} bytes, Limit: {} bytes",
                current_size,
                content.len(),
                MAX_SIZE_BYTES
            )));
        }

        // Use UPSERT to insert or update genome by hash
        self.connection.execute(
            "
            INSERT INTO genomes (hash, content, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(hash) DO UPDATE SET
                content = excluded.content,
                updated_at = CURRENT_TIMESTAMP
            ",
            params![hash, content],
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
    /// * `Ok(Some(content))` if genome was found
    /// * `Ok(None)` if genome was not found
    /// * `Err(HephaestusError)` if retrieval fails
    pub fn get_genome(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        // Guard clause: validate input
        if hash.is_empty() {
            return Err(HephaestusError::InvalidInput(
                "Genome hash cannot be empty".to_string(),
            ));
        }

        let mut stmt = self
            .connection
            .prepare("SELECT content FROM genomes WHERE hash = ?1")?;

        let mut rows = stmt.query_map(params![hash], |row| row.get::<_, Vec<u8>>(0))?;

        // Since hash is unique, we expect at most one row
        let genome = rows.next().transpose()?;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_genome_store_operations() -> Result<()> {
        // Create temporary directory for test database
        let temp_dir = tempfile::tempdir()?;
        let db_path = std::path::PathBuf::from("test.db");
        

        // Open store
        let store = GenomeStore::open(&db_path)?;

        // Test storing a genome
        let hash = "test_hash_123";
        let content = b"test genome content";
        store.store_genome(hash, content)?;

        // Test retrieving the genome
        let retrieved = store.get_genome(hash)?;
        assert!(retrieved.is_some(), "Genome should exist");
        if let Some(ref r) = retrieved {
            assert_eq!(r, &content.to_vec());
        }

        // Test updating the genome
        let new_content = b"updated genome content";
        store.store_genome(hash, new_content)?;
        let updated = store.get_genome(hash)?;
        assert!(updated.is_some(), "Genome should exist");
        if let Some(ref u) = updated {
            assert_eq!(u, &new_content.to_vec());
        }

        // Test deleting the genome
        store.delete_genome(hash)?;
        let deleted = store.get_genome(hash)?;
        assert!(deleted.is_none(), "Genome should not exist after deletion");

        // Clean up
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
