//! Atomic mutations produced by executing document commands.

use oo_schema::{BlockId, DocumentBlock, PageSetup};
use serde::{Deserialize, Serialize};

/// A block together with its original position in the serialized block array.
///
/// The position is not part of the public Document schema.  It only makes an inverse delete
/// restore the in-memory representation exactly, which is useful for journal replay and tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedBlock {
    pub position: usize,
    pub block: DocumentBlock,
}

/// Atomic state change emitted by a successful operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Mutation {
    Insert {
        block: DocumentBlock,
        parent_id: Option<BlockId>,
        index: usize,
    },
    Delete {
        block_id: BlockId,
        removed: Vec<RemovedBlock>,
        parent_id: Option<BlockId>,
        index: usize,
    },
    /// Inverse of an insert.  Keeping the full block lets an undo implementation restore the
    /// exact inserted record without looking at a second snapshot.
    RemoveInserted {
        block: DocumentBlock,
        parent_id: Option<BlockId>,
        index: usize,
    },
    /// Internal journal inverse for a subtree delete.  Unlike `Insert`, this keeps every
    /// removed record and its original model position, so nested deletes round-trip exactly.
    Restore {
        block_id: BlockId,
        removed: Vec<RemovedBlock>,
        parent_id: Option<BlockId>,
        index: usize,
    },
    Update {
        block_id: BlockId,
        before: Box<DocumentBlock>,
        after: Box<DocumentBlock>,
    },
    Move {
        block_id: BlockId,
        from_parent_id: Option<BlockId>,
        from_index: usize,
        to_parent_id: Option<BlockId>,
        to_index: usize,
    },
    SetPageSetup {
        before: Option<PageSetup>,
        after: Option<PageSetup>,
    },
}

impl Mutation {
    pub fn inverse(&self) -> Self {
        match self {
            Self::Insert {
                block,
                parent_id,
                index,
            } => Self::RemoveInserted {
                block: block.clone(),
                parent_id: parent_id.clone(),
                index: *index,
            },
            Self::Delete {
                block_id,
                removed,
                parent_id,
                index,
            } => Self::Restore {
                block_id: block_id.clone(),
                removed: removed.clone(),
                parent_id: parent_id.clone(),
                index: *index,
            },
            Self::Restore {
                block_id,
                removed,
                parent_id,
                index,
            } => Self::Delete {
                block_id: block_id.clone(),
                removed: removed.clone(),
                parent_id: parent_id.clone(),
                index: *index,
            },
            Self::RemoveInserted {
                block,
                parent_id,
                index,
            } => Self::Insert {
                block: block.clone(),
                parent_id: parent_id.clone(),
                index: *index,
            },
            Self::Update {
                block_id,
                before,
                after,
            } => Self::Update {
                block_id: block_id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            Self::Move {
                block_id,
                from_parent_id,
                from_index,
                to_parent_id,
                to_index,
            } => Self::Move {
                block_id: block_id.clone(),
                from_parent_id: to_parent_id.clone(),
                from_index: *to_index,
                to_parent_id: from_parent_id.clone(),
                to_index: *from_index,
            },
            Self::SetPageSetup { before, after } => Self::SetPageSetup {
                before: after.clone(),
                after: before.clone(),
            },
        }
    }
}
