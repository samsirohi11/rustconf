use coreconf_model::Result;
use serde_json::Value;

use crate::coap_types::Request;

/// Read-only state made available to a transaction participant.
///
/// The context is valid only while the participant callback is running.
/// `candidate_tree` is the complete tree that is published after all
/// pre-commit callbacks succeed, and is the committed tree in post-commit
/// callbacks.
pub struct TransactionContext<'a> {
    previous_tree: &'a Value,
    candidate_tree: &'a Value,
    changed_paths: &'a [String],
    request: &'a Request,
}

impl<'a> TransactionContext<'a> {
    pub(crate) fn new(
        previous_tree: &'a Value,
        candidate_tree: &'a Value,
        changed_paths: &'a [String],
        request: &'a Request,
    ) -> Self {
        Self {
            previous_tree,
            candidate_tree,
            changed_paths,
            request,
        }
    }

    /// Return the complete tree that was present before this transaction.
    pub fn previous_tree(&self) -> &Value {
        self.previous_tree
    }

    /// Return the complete candidate tree.
    pub fn candidate_tree(&self) -> &Value {
        self.candidate_tree
    }

    /// Return changed canonical paths, including predicates for keyed list
    /// instances when those predicates were part of the edit.
    pub fn changed_paths(&self) -> &[String] {
        self.changed_paths
    }

    /// Return the original request that created this transaction.
    pub fn request(&self) -> &Request {
        self.request
    }
}

/// A generic participant in root iPATCH transactions.
///
/// Participants are called in registration order.  A participant may reject
/// a candidate during `pre_commit`; `post_commit` is notification-only and
/// cannot veto or roll back a successful publication.
pub trait TransactionParticipant: Send {
    /// Validate the candidate before it is published.
    fn pre_commit(&self, _context: &TransactionContext<'_>) -> Result<()> {
        Ok(())
    }

    /// Observe a successfully committed transaction.
    fn post_commit(&self, _context: &TransactionContext<'_>) {}
}
