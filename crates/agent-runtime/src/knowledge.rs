//! Knowledge retrieval abstraction.
//!
//! Defines the retriever trait that an `Agent` invokes when it decides to
//! query an external knowledge base (RAG). The concrete implementation lives
//! in `agent-assembly` and is wired in through [`crate::RuntimeServices`].

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AgentResult;

/// Result of a single knowledge query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeQueryResult {
    /// Assembled context text, ready to be injected into a prompt.
    pub context_text: String,
    /// Number of chunks that matched the query.
    pub hit_count: usize,
}

/// Knowledge retriever trait.
///
/// Invoked by `Agent` when it handles a `PlannerDecision::QueryKnowledge`
/// step. The platform-side implementation usually delegates to `rag-core`.
#[async_trait]
pub trait AgentKnowledgeRetriever: Send + Sync + 'static {
    /// Run a knowledge query.
    ///
    /// # Parameters
    ///
    /// * `kb_ids` - Knowledge base IDs to search.
    /// * `query` - Query text.
    /// * `user_id` - User ID, used for permission filtering.
    /// * `top_k` - Maximum number of chunks to return.
    async fn query(
        &self,
        kb_ids: &[i64],
        query: &str,
        user_id: i64,
        top_k: usize,
    ) -> AgentResult<KnowledgeQueryResult>;
}

/// No-op retriever, used in tests and when knowledge retrieval is disabled.
pub struct NoopKnowledgeRetriever;

#[async_trait]
impl AgentKnowledgeRetriever for NoopKnowledgeRetriever {
    async fn query(
        &self,
        _kb_ids: &[i64],
        _query: &str,
        _user_id: i64,
        _top_k: usize,
    ) -> AgentResult<KnowledgeQueryResult> {
        Ok(KnowledgeQueryResult {
            context_text: "knowledge retrieval disabled".to_string(),
            hit_count: 0,
        })
    }
}
