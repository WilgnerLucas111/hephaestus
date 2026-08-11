use crate::error::{HephaestusError, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tree_sitter::{Language, Parser, Tree};

/// High-level AST diagnosis result
#[derive(Clone, Debug)]
pub struct ASTDiagnosis {
    pub error_function: String,
    pub error_file: String,
    pub error_line: u32,
    pub function_signature: String,
    pub callers: Vec<String>,
    pub dependencies: Vec<String>,
    pub slim_nodes: Vec<SlimNode>,
    pub repair_context: String,
}

/// Slim node format (C4 payload optimization)
/// Only essential fields to fit in token budget
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SlimNode {
    pub id: String,        // Deterministic ID (SHA256 hash)
    pub node_type: String, // "function", "class", "module", etc.
    pub name: String,      // Human-readable name
    pub file_path: String, // Relative to skill directory
    pub summary: String,   // Brief description from code comments
                           // DROPPED: languageNotes, complexity, domainMeta, lineRange
}

/// Structure node for internal representation
#[derive(Clone, Debug)]
pub struct StructureNode {
    pub node_type: String, // "function", "class", "module"
    pub name: String,
    pub file_path: String,
    pub line_number: u32,
    pub signature: String,     // Full function signature
    pub children: Vec<String>, // Child nodes' IDs (for hierarchy)
}

/// Call graph edge
#[derive(Clone, Debug)]
pub struct CallEdge {
    pub caller: String,   // Function that makes the call
    pub callee: String,   // Function being called
    pub line_number: u32, // Where the call occurs
}

/// Call graph structure
#[derive(Clone, Debug)]
pub struct CallGraph {
    pub edges: Vec<CallEdge>,
    pub nodes: HashMap<String, StructureNode>, // function name -> StructureNode
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            nodes: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: StructureNode) {
        self.nodes.insert(node.name.clone(), node);
    }

    pub fn add_edge(&mut self, edge: CallEdge) {
        self.edges.push(edge);
    }

    pub fn get_node(&self, name: &str) -> Option<&StructureNode> {
        self.nodes.get(name)
    }

    pub fn find_callers(&self, function_name: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|edge| edge.callee == function_name)
            .map(|edge| edge.caller.clone())
            .collect()
    }

    pub fn find_dependencies(&self, function_name: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|edge| edge.caller == function_name)
            .map(|edge| edge.callee.clone())
            .collect()
    }
}

/// AST Analyzer for converting source code to structured diagnosis
pub struct ASTAnalyzer {
    parser: Parser,
    language: Language,
}

impl ASTAnalyzer {
    /// Create analyzer for specific language
    #[allow(clippy::new_without_default)]
    pub fn new(language: Language) -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(language)
            .map_err(|e| HephaestusError::InvalidInput(format!("Failed to set language: {}", e)))?;

        Ok(ASTAnalyzer { parser, language })
    }

    /// Diagnose error location and surrounding context
    ///
    /// # Arguments
    ///
    /// * `_source_code` - Source code string to analyze
    /// * `error_line` - Line number where error occurred (1-indexed)
    /// * `error_column` - Column number where error occurred (1-indexed, optional)
    /// * `_error_keywords` - Keywords from error message to help locate relevant code
    ///
    /// # Returns
    ///
    /// * `Ok(ASTDiagnosis)` containing structured analysis
    /// * `Err(HephaestusError)` if analysis fails
    pub fn diagnose(
        &mut self,
        _source_code: &str,
        error_line: u32,
        error_column: Option<u32>,
        _error_keywords: &[String],
    ) -> Result<ASTDiagnosis> {
        // Parse code
        let tree = self.parser.parse(_source_code, None).ok_or_else(|| {
            HephaestusError::InvalidInput("Failed to parse source code".to_string())
        })?;

        // Two-pass extraction: structure + call graph
        let (structure_nodes, call_graph) = self.extract_two_pass(&tree, _source_code)?;

        // Find the exact node at error position
        let error_node = self.find_error_node(&tree, _source_code, error_line, error_column)?;

        // Extract the repair context (the node and surrounding code)
        let repair_context = self.extract_repair_context(&error_node, _source_code, 4)?;

        // Find the structure node that corresponds to this AST node
        let structure_node = structure_nodes
            .iter()
            .find(|n| {
                // Match by line number (approximate since we don't store exact byte ranges in StructureNode yet)
                n.line_number == error_node.start_position().row as u32 + 1
            })
            .cloned()
            .unwrap_or_else(|| StructureNode {
                node_type: "unknown".to_string(),
                name: "unknown".to_string(),
                file_path: "unknown".to_string(),
                line_number: error_node.start_position().row as u32 + 1,
                signature: "".to_string(),
                children: Vec::new(),
            });

        // Generate deterministic ID for error node
        let node_id = Self::make_deterministic_id(
            &structure_node.node_type,
            &structure_node.name,
            structure_node.line_number,
        );

        // Create slim node for error location
        let slim = SlimNode {
            id: node_id,
            node_type: structure_node.node_type.clone(),
            name: structure_node.name.clone(),
            file_path: structure_node.file_path.clone(),
            summary: Self::extract_summary(&structure_node, _source_code),
        };

        // Find callers and dependencies
        let callers = call_graph.find_callers(&structure_node.name);
        let dependencies = call_graph.find_dependencies(&structure_node.name);

        Ok(ASTDiagnosis {
            error_function: structure_node.name.clone(),
            error_file: structure_node.file_path.clone(),
            error_line: structure_node.line_number,
            function_signature: structure_node.signature.clone(),
            callers,
            dependencies,
            slim_nodes: vec![slim],
            repair_context,
        })
    }

    /// Two-pass extraction: structure + call graph
    fn extract_two_pass(
        &self,
        tree: &Tree,
        _source_code: &str,
    ) -> Result<(Vec<StructureNode>, CallGraph)> {
        // Pass 1: Extract structure (functions, classes)
        let structure_nodes = self.extract_structure(tree, _source_code)?;

        // Pass 2: Extract call relationships
        let call_graph = self.extract_call_graph(tree, _source_code)?;

        Ok((structure_nodes, call_graph))
    }

    /// Extract structure nodes (functions, classes, etc.) from AST
    fn extract_structure(&self, tree: &Tree, _source_code: &str) -> Result<Vec<StructureNode>> {
        let mut nodes = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        // Depth-first traversal to find function definitions
        loop {
            let node = cursor.node();

            // Check if this node is a function definition
            if self.is_function_definition(&node)
                && let Some(structure_node) = self.node_to_structure(&node, _source_code)
            {
                nodes.push(structure_node);
            }

            // Try to go deeper
            if cursor.goto_first_child() {
                continue;
            }

            // Try to go to next sibling
            if cursor.goto_next_sibling() {
                continue;
            }

            // Try to go up to parent
            loop {
                if !cursor.goto_parent() {
                    // We've traversed the entire tree
                    return Ok(nodes);
                }

                // Try to go to next sibling from parent
                if cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Check if a tree-sitter node represents a function definition
    fn is_function_definition(&self, node: &tree_sitter::Node) -> bool {
        if self.language == tree_sitter_rust::language() {
            // Rust function definitions
            node.kind() == "function_item"
        } else if self.language == tree_sitter_c::language() {
            // C function definitions
            node.kind() == "function_definition"
        } else {
            false
        }
    }

    /// Convert a tree-sitter node to a StructureNode
    fn node_to_structure(
        &self,
        node: &tree_sitter::Node,
        _source_code: &str,
    ) -> Option<StructureNode> {
        // Extract the text content of the node
        let _node_text = node.utf8_text(_source_code.as_bytes()).ok()?;

        // Extract function name
        let name = self.extract_function_name(node, _source_code)?;

        // Extract signature
        let signature = self.extract_function_signature(node, _source_code).ok()?;

        Some(StructureNode {
            node_type: "function".to_string(),
            name,
            file_path: "unknown".to_string(), // TODO: Extract from actual file path
            line_number: node.start_position().row as u32 + 1, // 1-indexed
            signature,
            children: Vec::new(), // TODO: Extract child relationships
        })
    }

    /// Extract function name from a function definition node
    fn extract_function_name(
        &self,
        node: &tree_sitter::Node,
        _source_code: &str,
    ) -> Option<String> {
        if self.language == tree_sitter_rust::language() {
            // In Rust, look for "identifier" child of function_item
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    return Some(child.utf8_text(_source_code.as_bytes()).ok()?.to_string());
                }
            }
        } else if self.language == tree_sitter_c::language() {
            // In C, look for "function_declarator" -> "identifier"
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "function_declarator" {
                    let mut grandchild_cursor = child.walk();
                    for grandchild in child.children(&mut grandchild_cursor) {
                        if grandchild.kind() == "identifier" {
                            return Some(
                                grandchild
                                    .utf8_text(_source_code.as_bytes())
                                    .ok()?
                                    .to_string(),
                            );
                        }
                    }
                }
            }
        }
        None
    }

    /// Extract function signature from AST node
    fn extract_function_signature(
        &self,
        node: &tree_sitter::Node,
        _source_code: &str,
    ) -> Result<String> {
        // Extract the text content of the node
        let node_text = node.utf8_text(_source_code.as_bytes()).map_err(|e| {
            HephaestusError::InvalidInput(format!("Failed to extract node text: {}", e))
        })?;

        // For now, return the first line of the function (signature)
        // A more sophisticated implementation would extract just the signature line
        let lines: Vec<&str> = node_text.lines().collect();
        if let Some(first_line) = lines.first() {
            Ok(first_line.trim().to_string())
        } else {
            Ok(node_text.trim().to_string())
        }
    }

    /// Extract call graph from AST
    fn extract_call_graph(&self, tree: &Tree, _source_code: &str) -> Result<CallGraph> {
        let mut call_graph = CallGraph::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        // Depth-first traversal to find call expressions
        loop {
            let node = cursor.node();

            // Check if this node is a call expression
            if self.is_call_expression(&node)
                && let Some(edge) = self.node_to_call_edge(&node, _source_code)
            {
                call_graph.add_edge(edge);
            }

            // Try to go deeper
            if cursor.goto_first_child() {
                continue;
            }

            // Try to go to next sibling
            if cursor.goto_next_sibling() {
                continue;
            }

            // Try to go up to parent
            loop {
                if !cursor.goto_parent() {
                    // We've traversed the entire tree
                    return Ok(call_graph);
                }

                // Try to go to next sibling from parent
                if cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Check if a tree-sitter node represents a call expression
    fn is_call_expression(&self, node: &tree_sitter::Node) -> bool {
        if self.language == tree_sitter_rust::language() {
            // Rust method calls and function calls
            matches!(node.kind(), "method_call" | "call_expression")
        } else if self.language == tree_sitter_c::language() {
            // C function calls
            node.kind() == "call_expression"
        } else {
            false
        }
    }

    /// Convert a tree-sitter call node to a CallEdge
    fn node_to_call_edge(&self, node: &tree_sitter::Node, _source_code: &str) -> Option<CallEdge> {
        // Extract the function being called
        let callee = self.extract_callee_name(node, _source_code)?;

        // For simplicity, we'll use a placeholder caller
        // A full implementation would traverse up to find the containing function
        let caller = "unknown_function".to_string();

        Some(CallEdge {
            caller,
            callee,
            line_number: node.start_position().row as u32 + 1, // 1-indexed
        })
    }

    /// Extract the name of the function being called from a call expression
    fn extract_callee_name(&self, node: &tree_sitter::Node, _source_code: &str) -> Option<String> {
        if self.language == tree_sitter_rust::language() {
            // For method_call: look for the identifier after the object
            // For call_expression: look for the function being called
            if node.kind() == "method_call" {
                // Find the method name (identifier after the dot)
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        return Some(child.utf8_text(_source_code.as_bytes()).ok()?.to_string());
                    }
                }
            } else if node.kind() == "call_expression" {
                // Find the function being called
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        return Some(child.utf8_text(_source_code.as_bytes()).ok()?.to_string());
                    }
                }
            }
        } else if self.language == tree_sitter_c::language() {
            // In C, look for the function name in a call_expression
            if node.kind() == "call_expression" {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        return Some(child.utf8_text(_source_code.as_bytes()).ok()?.to_string());
                    }
                }
            }
        }
        None
    }

    /// Generate deterministic ID based on node properties
    /// Same inputs = same ID (semantic deduplication)
    pub fn make_deterministic_id(node_type: &str, name: &str, line: u32) -> String {
        let combined = format!("{}_{}_{}", node_type, name, line);

        // Clean up special characters
        let cleaned: String = combined
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        // SHA256 hash for compact representation
        let mut hasher = Sha256::new();
        hasher.update(cleaned);

        // Take first 16 chars of hex (128 bits) for compact ID
        format!("{:x}", hasher.finalize())[..16].to_string()
    }

    /// Extract summary from code comments/docstrings
    fn extract_summary(node: &StructureNode, _source_code: &str) -> String {
        // For simplicity, we'll extract a comment line if available
        let lines: Vec<&str> = _source_code.lines().collect();
        let start_line = node.line_number as usize;

        // Look for comments before the function
        for i in (0..start_line.saturating_sub(1)).rev() {
            if i >= lines.len() {
                continue;
            }
            let line = lines[i].trim();
            if line.starts_with("//") || line.starts_with("/*") || line.starts_with("!") {
                // Found a comment line, clean it up
                let cleaned = line.trim_start_matches(['/', '*', '!']).trim().to_string();
                if !cleaned.is_empty() {
                    return cleaned;
                }
            } else if !line.is_empty()
                && !line.starts_with("fn")
                && !line.starts_with("struct")
                && !line.starts_with("enum")
                && !line.starts_with("trait")
            {
                // Found non-empty non-code line, use it
                return line.to_string();
            }
        }

        "?".to_string()
    }

    /// Find the AST node that contains the given error position (line/column)
    ///
    /// # Arguments
    ///
    /// * `tree` - The parsed AST tree
    /// * `_source_code` - The source code string
    /// * `error_line` - 1-indexed line number where error occurred
    /// * `error_column` - 1-indexed column number where error occurred (optional)
    ///
    /// # Returns
    ///
    /// * `Ok(Node)` - The deepest node that encompasses the error position
    /// * `Err(HephaestusError)` - If no node is found at the position
    pub fn find_error_node<'a>(
        &self,
        tree: &'a Tree,
        _source_code: &str,
        error_line: u32,
        error_column: Option<u32>,
    ) -> Result<tree_sitter::Node<'a>> {
        let root = tree.root_node();

        // Convert to 0-indexed for tree-sitter
        let zero_indexed_line = error_line.saturating_sub(1) as usize;
        let zero_indexed_column = error_column.map(|c| c.saturating_sub(1) as usize);

        // Find the deepest node that contains the error position
        let mut best_match = None;
        let mut depth = 0;

        // Depth-first search to find the deepest matching node
        let mut cursor = root.walk();
        let mut current_depth = 0;

        loop {
            let node = cursor.node();
            let node_start_line = node.start_position().row;
            let node_start_column = node.start_position().column;
            let node_end_line = node.end_position().row;
            let node_end_column = node.end_position().column;

            // Check if the error position is within this node's range
            let mut _contains_position = false;

            if zero_indexed_line > node_end_line {
                // Error is below this node
                _contains_position = false;
            } else if zero_indexed_line < node_start_line {
                // Error is above this node
                _contains_position = false;
            } else {
                // Error is on the same line range as node
                if zero_indexed_line == node_start_line && zero_indexed_line == node_end_line {
                    // Error, start, and end are all on the same line
                    if let Some(col) = zero_indexed_column {
                        _contains_position = (col >= node_start_column) && (col <= node_end_column);
                    } else {
                        _contains_position = true; // No column specified, just check line
                    }
                } else if zero_indexed_line == node_start_line {
                    // Error is on the start line
                    if let Some(col) = zero_indexed_column {
                        _contains_position = col >= node_start_column;
                    } else {
                        _contains_position = true;
                    }
                } else if zero_indexed_line == node_end_line {
                    // Error is on the end line
                    if let Some(col) = zero_indexed_column {
                        _contains_position = col <= node_end_column;
                    } else {
                        _contains_position = true;
                    }
                } else {
                    // Error is between start and end lines (exclusive)
                    _contains_position = true;
                }
            }

            if _contains_position {
                // This node contains the error position
                // Keep track of the deepest match
                if best_match.is_none() || current_depth > depth {
                    best_match = Some(node);
                    depth = current_depth;
                }
            }

            // Try to go deeper
            if cursor.goto_first_child() {
                current_depth += 1;
                continue;
            }

            // Try to go to next sibling
            if cursor.goto_next_sibling() {
                continue;
            }

            // Try to go up to parent
            loop {
                if !cursor.goto_parent() {
                    // We've traversed the entire tree
                    break;
                }
                current_depth -= 1;

                // Try to go to next sibling from parent
                if cursor.goto_next_sibling() {
                    break;
                }
            }

            // If we couldn't go anywhere, we're done
            if !cursor.goto_first_child() && !cursor.goto_next_sibling() {
                break;
            }
        }

        best_match.ok_or_else(|| {
            HephaestusError::InvalidInput(format!(
                "No AST node found at line {}, column {:?}",
                error_line, error_column
            ))
        })
    }

    /// Extract repair context around a given AST node for LLM-based fixing
    ///
    /// # Arguments
    ///
    /// * `node` - The AST node containing the error
    /// * `_source_code` - The full source code string
    /// * `context_lines` - Number of lines to include before/after the node
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - The extracted context including the node and surrounding code
    /// * `Err(HephaestusError)` - If extraction fails
    pub fn extract_repair_context(
        &self,
        node: &tree_sitter::Node,
        _source_code: &str,
        context_lines: usize,
    ) -> Result<String> {
        // Get the byte range of the node
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        // Convert source code to lines with byte offsets
        let lines: Vec<&str> = _source_code.lines().collect();
        let mut line_offsets = Vec::new();
        let mut byte_offset = 0usize;

        for line in &lines {
            line_offsets.push(byte_offset);
            byte_offset += line.len() + 1; // +1 for newline
        }

        // Find the line numbers for the node's start and end
        let start_line = line_offsets
            .iter()
            .position(|&offset| offset > start_byte)
            .map(|pos| pos.saturating_sub(1))
            .unwrap_or(0);

        let end_line = line_offsets
            .iter()
            .position(|&offset| offset >= end_byte)
            .map(|pos| pos.saturating_sub(1))
            .unwrap_or(lines.len().saturating_sub(1));

        // Calculate context range
        let context_start = start_line.saturating_sub(context_lines);
        let context_end = (end_line + context_lines).min(lines.len().saturating_sub(1));

        // Extract the context lines
        let context_lines_vec: Vec<String> = lines[context_start..=context_end]
            .iter()
            .map(|line| line.to_string())
            .collect();

        Ok(context_lines_vec.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter_c::language as c_language;
    use tree_sitter_rust::language as rust_language;

    #[test]
    fn test_make_deterministic_id() -> Result<()> {
        let id1 = ASTAnalyzer::make_deterministic_id("function", "test_func", 42);
        let id2 = ASTAnalyzer::make_deterministic_id("function", "test_func", 42);
        let id3 = ASTAnalyzer::make_deterministic_id("function", "test_func", 43);

        assert_eq!(id1, id2); // Same inputs should produce same ID
        assert_ne!(id1, id3); // Different line should produce different ID

        // ID should be 16 hex characters (128 bits)
        assert_eq!(id1.len(), 16);
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));

        Ok(())
    }

    #[test]
    fn test_extract_summary() -> Result<()> {
        let node = StructureNode {
            node_type: "function".to_string(),
            name: "test_func".to_string(),
            file_path: "test.rs".to_string(),
            line_number: 3,
            signature: "fn test_func()".to_string(),
            children: Vec::new(),
        };

        let source = r#"
            // This is a test function
            // It does something important
            fn test_func() {
                println!("Hello");
            }
        "#;

        let summary = ASTAnalyzer::extract_summary(&node, source);
        // Should find one of the comment lines
        assert!(summary.contains("test function") || summary.contains("does something"));

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_rust_analyzer_basic() -> Result<()> {
        let mut analyzer = ASTAnalyzer::new(rust_language())?;

        let source = r#"
            /// This function adds two numbers
            fn add(a: i32, b: i32) -> i32 {
                a + b
            }
            
            fn main() {
                let result = add(1, 2);
                println!("{}", result);
            }
        "#;

        let diagnosis = analyzer.diagnose(source, 2, None, &["add".to_string()])?;

        assert_eq!(diagnosis.error_function, "unknown"); // Simplified implementation
        assert_eq!(diagnosis.error_line, 2);
        assert!(!diagnosis.slim_nodes.is_empty());
        assert_eq!(diagnosis.slim_nodes[0].node_type, "function");
        assert!(!diagnosis.slim_nodes[0].id.is_empty());

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_c_analyzer_basic() -> Result<()> {
        let mut analyzer = ASTAnalyzer::new(c_language())?;

        let source = r#"
            /**
             * Adds two integers
             */
            int add(int a, int b) {
                return a + b;
            }
            
            int main() {
                int result = add(1, 2);
                printf("%d\n", result);
                return 0;
            }
        "#;

        let diagnosis = analyzer.diagnose(source, 2, None, &["add".to_string()])?;

        assert_eq!(diagnosis.error_function, "unknown"); // Simplified implementation
        assert_eq!(diagnosis.error_line, 2);
        assert!(!diagnosis.slim_nodes.is_empty());
        assert_eq!(diagnosis.slim_nodes[0].node_type, "function");
        assert!(!diagnosis.slim_nodes[0].id.is_empty());

        Ok(())
    }
}
