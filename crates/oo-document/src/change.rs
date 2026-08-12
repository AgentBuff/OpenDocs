//! Change summaries and transaction-level mutation history.

use oo_schema::BlockId;
use serde::{Deserialize, Serialize};

use crate::mutation::Mutation;

/// Minimal invalidation information plus the exact mutations required for undo/collaboration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    pub revision: u64,
    pub changed_blocks: Vec<BlockId>,
    /// Containers whose child order changed. Root changes have no synthetic container ID and
    /// are represented by `structure_changed`.
    pub changed_containers: Vec<BlockId>,
    pub structure_changed: bool,
    pub mutations: Vec<Mutation>,
}

/// The journal stores mutations grouped by committed transaction.  It is intentionally separate
/// from the schema snapshot so undo/redo and CRDT adapters can consume the same atomic changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MutationJournal {
    entries: Vec<Vec<Mutation>>,
    redo_entries: Vec<Vec<Mutation>>,
}

impl MutationJournal {
    pub(crate) fn push(&mut self, mutations: Vec<Mutation>) {
        self.entries.push(mutations);
        self.redo_entries.clear();
    }

    pub(crate) fn push_undo(&mut self, mutations: Vec<Mutation>) {
        self.entries.push(mutations);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn last(&self) -> Option<&[Mutation]> {
        self.entries.last().map(Vec::as_slice)
    }

    pub fn can_undo(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_entries.is_empty()
    }

    pub(crate) fn pop_undo(&mut self) -> Option<Vec<Mutation>> {
        self.entries.pop()
    }

    pub(crate) fn restore_undo(&mut self, entry: Vec<Mutation>) {
        self.entries.push(entry);
    }

    pub(crate) fn push_redo(&mut self, entry: Vec<Mutation>) {
        self.redo_entries.push(entry);
    }

    pub(crate) fn pop_redo(&mut self) -> Option<Vec<Mutation>> {
        self.redo_entries.pop()
    }

    pub(crate) fn restore_redo(&mut self, entry: Vec<Mutation>) {
        self.redo_entries.push(entry);
    }

    pub fn iter(&self) -> impl Iterator<Item = &[Mutation]> {
        self.entries.iter().map(Vec::as_slice)
    }

    /// Returns the inverse mutations for the latest transaction in replay order.
    pub fn last_inverse(&self) -> Option<Vec<Mutation>> {
        self.entries
            .last()
            .map(|entry| entry.iter().rev().map(Mutation::inverse).collect())
    }
}
