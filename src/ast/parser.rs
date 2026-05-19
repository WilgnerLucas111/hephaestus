use crate::error::{HephaestusError, Result};
use blake3::Hasher;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Language, Parser};
use tree_sitter_c;
use tree_sitter_rust;

/// Represents a parsed AST genome with its hash and serialized content.
#[derive(Debug, Clone)]
pub struct Genome {
    pub id: String, // Hex-encoded BLAKE3 hash
    pub content: Vec<u8>,
}

/// Parser for converting source code to AST genomes.
pub struct AstParser {
    parsers: HashMap<Language, Parser>,
}

impl AstParser {
    /// Creates a new AstParser with no pre-loaded parsers.
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    /// Gets or creates a parser for the specified language.
    fn get_parser(&mut self, language_name: &str) -> Result<&mut Parser> {
        let language = Self::get_language(language_name)?;
        // If we don't have a parser for this language, create one
        if let std::collections::hash_map::Entry::Vacant(e) = self.parsers.entry(language) {
            let mut parser = Parser::new();
            parser.set_language(language).map_err(|e| {
                HephaestusError::InvalidInput(format!("Failed to set language: {}", e))
            })?;
            e.insert(parser);
        }
        // Safety: we just inserted if it didn't exist
        self
            .parsers
            .get_mut(&language)
            .ok_or_else(|| HephaestusError::NotFound("Parser language is missing".to_string()))
    }

    /// Converts a language name string to a tree-sitter Language.
    fn get_language(language_name: &str) -> Result<Language> {
        match language_name.to_lowercase().as_str() {
            "rust" => Ok(tree_sitter_rust::language()),
            "c" => Ok(tree_sitter_c::language()),
            _ => Err(HephaestusError::InvalidInput(format!(
                "Unsupported language: {}. Supported languages: rust, c",
                language_name
            ))),
        }
    }

    /// Serializes a tree-sitter node to a byte vector, skipping comments and errors.
    fn serialize_node(node: tree_sitter::Node, source: &[u8], buffer: &mut Vec<u8>) {
        let kind = node.kind();
        // Skip comment and error nodes
        if kind == "comment" || kind == "error" {
            return;
        }

        // Write node type length (4 bytes, little endian) followed by UTF-8 bytes
        let kind_bytes = kind.as_bytes();
        buffer.extend_from_slice(&(kind_bytes.len() as u32).to_le_bytes());
        buffer.extend_from_slice(kind_bytes);

        // Recursively serialize children
        let mut wc = node.walk();
        for child in node.children(&mut wc) {
            Self::serialize_node(child, source, buffer);
        }
    }

    /// Parses source code into an AST genome.
    ///
    /// # Arguments
    ///
    /// * `source` - Source code string to parse
    /// * `language_name` - Name of the language (e.g., "rust", "c")
    ///
    /// # Returns
    ///
    /// * `Ok(Genome)` containing the BLAKE3 hash and serialized AST
    /// * `Err(HephaestusError)` if parsing fails or language is unsupported
    pub fn parse_to_genome(&mut self, source: &str, language_name: &str) -> Result<Genome> {
        // Get parser for the language
        let parser = self.get_parser(language_name)?;

        // Parse the source code
        let tree = parser.parse(source, None).ok_or_else(|| {
            HephaestusError::InvalidInput("Failed to parse source code".to_string())
        })?;

        // Serialize the AST (skip comments and errors)
        let mut serialized = Vec::new();
        let root = tree.root_node();
        Self::serialize_node(root, source.as_bytes(), &mut serialized);

        // Check if serialization exceeded memory limit (45 MiB)
        const MAX_SIZE_BYTES: u64 = 45 * 1024 * 1024;
        if serialized.len() as u64 > MAX_SIZE_BYTES {
            return Err(HephaestusError::InvalidInput(format!(
                "Serialized AST size ({} bytes) exceeds 45 MiB limit",
                serialized.len()
            )));
        }

        // Compute BLAKE3 hash of the serialized AST
        let mut hasher = Hasher::new();
        hasher.update(&serialized);
        let hash = hasher.finalize();
        let id = hash.to_hex().to_string();

        Ok(Genome {
            id,
            content: serialized,
        })
    }

    /// Parses a file into an AST genome.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the source file
    /// * `language_name` - Name of the language (e.g., "rust", "c")
    ///
    /// # Returns
    ///
    /// * `Ok(Genome)` containing the BLAKE3 hash and serialized AST
    /// * `Err(HephaestusError)` if file cannot be read or parsing fails
    pub fn parse_file_to_genome<P: AsRef<Path>>(
        &mut self,
        path: P,
        language_name: &str,
    ) -> Result<Genome> {
        let source = std::fs::read_to_string(path).map_err(HephaestusError::Io)?;
        self.parse_to_genome(&source, language_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_code() -> Result<()> {
        let mut parser = AstParser::new();
        let source = r#"
            fn main() {
                let x = 1;
                println!("{}", x);
            }
        "#;
        let genome = parser.parse_to_genome(source, "rust")?;
        assert!(!genome.id.is_empty());
        assert!(!genome.content.is_empty());
        Ok(())
    }

    #[test]
    fn test_parse_c_code() -> Result<()> {
        let mut parser = AstParser::new();
        let source = r#"
            int main() {
                int x = 1;
                printf("%d\\n", x);
                return 0;
            }
        "#;
        let genome = parser.parse_to_genome(source, "c")?;
        assert!(!genome.id.is_empty());
        assert!(!genome.content.is_empty());
        Ok(())
    }

    #[test]
    fn test_unsupported_language() {
        let mut parser = AstParser::new();
        let result = parser.parse_to_genome("int x;", "javascript");
        assert!(result.is_err());
    }

    #[test]
    fn test_comment_skipping() -> Result<()> {
        let mut parser = AstParser::new();
        let source_with_comments = r#"
            // This is a comment
            int x = 1; /* Another comment */
        "#;
        let source_without_comments = r#"
            int x = 1;
        "#;
        let genome_with = parser.parse_to_genome(source_with_comments, "c")?;
        let genome_without = parser.parse_to_genome(source_without_comments, "c")?;
        // Should be identical because comments are skipped
        assert_eq!(genome_with.id, genome_without.id);
        assert_eq!(genome_with.content, genome_without.content);
        Ok(())
    }
}
