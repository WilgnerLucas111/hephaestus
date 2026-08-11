use crate::memory::genome_store::RepairGenome;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// A semantic clusterer for grouping similar failures.
/// This is a placeholder for Leiden clustering via Graphify.
/// In the future, this will be replaced by a proper Leiden clustering implementation
/// that uses AST topology and dependency density to group similar failures.
pub struct SemanticClusterer {
    /// Map from cluster ID to list of genome hashes in that cluster
    clusters: HashMap<u64, Vec<String>>,
    /// Next available cluster ID
    next_cluster_id: u64,
}

impl Default for SemanticClusterer {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticClusterer {
    /// Create a new semantic clusterer.
    pub fn new() -> Self {
        Self {
            clusters: HashMap::new(),
            next_cluster_id: 0,
        }
    }

    /// Assign a genome to a cluster based on its semantic features.
    /// Returns the cluster ID assigned.
    ///
    /// In the future, this will use Leiden clustering on a graph of genomes
    /// where edges are weighted by similarity of AST topology and dependency density.
    /// For now, we use a simple hash of the telemetry trigger and AST topology hash.
    pub fn assign_cluster(&mut self, genome: &RepairGenome) -> u64 {
        // For now, we use a simple hash of the telemetry trigger and ast topology hash.
        // In the future, we will use a more sophisticated method (Leiden clustering).
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(ref telemetry) = genome.telemetry_trigger {
            telemetry.hash(&mut hasher);
        }
        if let Some(ref ast_hash) = genome.ast_topology_hash {
            ast_hash.hash(&mut hasher);
        }
        let _hash = hasher.finish();

        // Check if we already have a cluster for this hash
        for (&cluster_id, members) in &mut self.clusters {
            if members.iter().any(|h| h == &genome.hash) {
                return cluster_id;
            }
        }

        // If not, create a new cluster
        let cluster_id = self.next_cluster_id;
        self.next_cluster_id += 1;
        self.clusters.insert(cluster_id, vec![genome.hash.clone()]);
        cluster_id
    }

    /// Get the cluster ID for a genome (if already assigned).
    pub fn get_cluster(&self, genome: &RepairGenome) -> Option<u64> {
        for (&cluster_id, members) in &self.clusters {
            if members.iter().any(|h| h == &genome.hash) {
                return Some(cluster_id);
            }
        }
        None
    }

    /// Get all genomes in a given cluster.
    pub fn get_cluster_members(&self, cluster_id: u64) -> Option<Vec<String>> {
        self.clusters.get(&cluster_id).cloned()
    }

    /// Merge two clusters (for when we find they are similar).
    pub fn merge_clusters(&mut self, id1: u64, id2: u64) {
        if id1 == id2 {
            return;
        }
        let mut members = Vec::new();
        if let Some(m1) = self.clusters.remove(&id1) {
            members.extend(m1);
        }
        if let Some(m2) = self.clusters.remove(&id2) {
            members.extend(m2);
        }
        if !members.is_empty() {
            let min_id = id1.min(id2);
            self.clusters.insert(min_id, members);
            // Note: we don't reuse the max id for simplicity
        }
    }
}
