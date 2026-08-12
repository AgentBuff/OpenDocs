//! Mutable block storage and indexes used by the document transaction engine.
//!
//! `DocumentModel` remains the serialization boundary, but the engine no longer clones it
//! before every transaction.  The index is updated alongside each mutation; rebuilding it is
//! only required while loading a snapshot (or when a debug caller explicitly asks for one).

use std::collections::HashMap;

use oo_schema::{BlockId, DocumentBlock, DocumentModel};

use crate::DocumentEngineError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BlockStore {
    pub(crate) model: DocumentModel,
    pub(crate) index: DocumentIndex,
}

impl BlockStore {
    pub(crate) fn new(model: DocumentModel) -> Result<Self, DocumentEngineError> {
        let index = DocumentIndex::build(&model)?;
        Ok(Self { model, index })
    }

    pub(crate) fn model(&self) -> &DocumentModel {
        &self.model
    }

    pub(crate) fn block(&self, id: &str) -> Result<&DocumentBlock, DocumentEngineError> {
        let position = self.index.position(id)?;
        self.model
            .blocks
            .get(position)
            .ok_or_else(|| DocumentEngineError::MissingBlock(id.to_string()))
    }

    pub(crate) fn block_mut(
        &mut self,
        id: &str,
    ) -> Result<&mut DocumentBlock, DocumentEngineError> {
        let position = self.index.position(id)?;
        self.model
            .blocks
            .get_mut(position)
            .ok_or_else(|| DocumentEngineError::MissingBlock(id.to_string()))
    }

    /// Updates positions after removing records from the model's stable Vec representation.
    /// Parent links are untouched because deleting a subtree only removes links owned by the
    /// deleted records; the owning parent link is edited by the caller.
    pub(crate) fn remove_positions(&mut self, removed: &[(usize, BlockId)]) {
        for (_, id) in removed {
            self.index.positions.remove(id);
            self.index.parents.remove(id);
        }

        for position in self.index.positions.values_mut() {
            let shift = removed
                .iter()
                .filter(|(removed_position, _)| *removed_position < *position)
                .count();
            *position -= shift;
        }
    }

    pub(crate) fn insert_position(
        &mut self,
        id: BlockId,
        position: usize,
        parent: Option<BlockId>,
    ) {
        self.index.positions.insert(id.clone(), position);
        self.index.parents.insert(id, parent);
    }

    pub(crate) fn position(&self, id: &str) -> Result<usize, DocumentEngineError> {
        self.index.position(id)
    }

    /// Finds the direct container and the child's position in that container.
    pub(crate) fn container_position(
        &self,
        id: &str,
    ) -> Result<(Option<BlockId>, usize), DocumentEngineError> {
        let parent = self.index.parent(id);
        let children = match parent.as_deref() {
            None => &self.model.root,
            Some(parent_id) => {
                &self
                    .model
                    .blocks
                    .get(self.index.position(parent_id)?)
                    .ok_or_else(|| DocumentEngineError::MissingBlock(parent_id.to_string()))?
                    .children
            }
        };
        let index = children
            .iter()
            .position(|child| child == id)
            .ok_or_else(|| DocumentEngineError::MissingBlock(id.to_string()))?;
        Ok((parent, index))
    }
}

/// Derived index for the serialized `DocumentModel`.
///
/// It is deliberately kept separate from the schema model: IDs and parent links are indexes,
/// not a second source of truth. Unlike the old implementation, commands update this map in
/// place rather than rebuilding it after each command.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DocumentIndex {
    pub(crate) positions: HashMap<BlockId, usize>,
    pub(crate) parents: HashMap<BlockId, Option<BlockId>>,
}

impl DocumentIndex {
    pub(crate) fn build(model: &DocumentModel) -> Result<Self, DocumentEngineError> {
        model.validate()?;
        let mut index = Self::default();
        for (position, block) in model.blocks.iter().enumerate() {
            index.positions.insert(block.id.clone(), position);
        }
        for block in &model.blocks {
            for child in &block.children {
                index.parents.insert(child.clone(), Some(block.id.clone()));
            }
        }
        for root in &model.root {
            index.parents.insert(root.clone(), None);
        }
        Ok(index)
    }

    pub(crate) fn position(&self, id: &str) -> Result<usize, DocumentEngineError> {
        self.positions
            .get(id)
            .copied()
            .ok_or_else(|| DocumentEngineError::MissingBlock(id.to_string()))
    }

    pub(crate) fn parent(&self, id: &str) -> Option<BlockId> {
        self.parents.get(id).cloned().flatten()
    }
}
