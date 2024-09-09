use crate::{
    error::StoreError,
    trie::{
        db::TrieDB,
        hashing::{DelimitedHash, NodeHash, NodeHashRef, NodeHasher, Output},
        nibble::{Nibble, NibbleSlice, NibbleVec},
        node_ref::NodeRef,
        PathRLP, ValueRLP,
    },
};

use super::{ExtensionNode, LeafNode, Node};

/// Branch Node of an an Ethereum Compatible Patricia Merkle Trie
/// Contains the node's hash, value, path, and the references of its children nodes
#[derive(Debug, Clone)]
pub struct BranchNode {
    pub hash: NodeHash,
    pub choices: [NodeRef; 16],
    pub path: PathRLP,
    pub value: ValueRLP,
}

impl BranchNode {
    /// Creates a new branch node given its children, without any stored value
    pub fn new(choices: [NodeRef; 16]) -> Self {
        Self {
            choices,
            hash: Default::default(),
            path: Default::default(),
            value: Default::default(),
        }
    }

    /// Creates a new branch node given its children and stores the given (path, value) pair
    pub fn new_with_value(choices: [NodeRef; 16], path: PathRLP, value: ValueRLP) -> Self {
        Self {
            choices,
            hash: Default::default(),
            path,
            value,
        }
    }

    /// Updates the node's path and value
    pub fn update(&mut self, new_path: PathRLP, new_value: ValueRLP) {
        self.path = new_path;
        self.value = new_value;
    }

    /// Retrieves a value from the subtrie originating from this node given its path
    pub fn get(&self, db: &TrieDB, mut path: NibbleSlice) -> Result<Option<ValueRLP>, StoreError> {
        // If path is at the end, return to its own value if present.
        // Otherwise, check the corresponding choice and delegate accordingly if present.
        if let Some(choice) = path.next().map(usize::from) {
            // Delegate to children if present
            let child_ref = self.choices[choice];
            if child_ref.is_valid() {
                let child_node = db
                    .get_node(child_ref)?
                    .expect("inconsistent internal tree structure");
                child_node.get(db, path)
            } else {
                Ok(None)
            }
        } else {
            // Return internal value if present.
            Ok((!self.value.is_empty()).then_some(self.value.clone()))
        }
    }

    /// Inserts a value into the subtrie originating from this node and returns the new root of the subtrie
    pub fn insert(
        mut self,
        db: &mut TrieDB,
        mut path: NibbleSlice,
        value: ValueRLP,
    ) -> Result<Node, StoreError> {
        // If path is at the end, insert or replace its own value.
        // Otherwise, check the corresponding choice and insert or delegate accordingly.
        self.hash.mark_as_dirty();
        match path.next() {
            Some(choice) => match &mut self.choices[choice as usize] {
                // Create new child (leaf node)
                choice_ref if !choice_ref.is_valid() => {
                    let new_leaf = LeafNode::new(path.data(), value);
                    let child_ref = db.insert_node(new_leaf.into())?;
                    *choice_ref = child_ref;
                }
                // Insert into existing child and then update it
                choice_ref => {
                    let child_node = db
                        .get_node(*choice_ref)?
                        .expect("inconsistent internal tree structure");

                    let child_node = child_node.insert(db, path, value)?;
                    *choice_ref = db.insert_node(child_node)?;
                }
            },
            None => {
                // Insert into self
                self.update(path.data(), value);
            }
        };

        Ok(self.into())
    }

    /// Removes a value from the subtrie originating from this node given its path
    /// Returns the new root of the subtrie (if any) and the removed value if it existed in the subtrie
    pub fn remove(
        mut self,
        db: &mut TrieDB,
        mut path: NibbleSlice,
    ) -> Result<(Option<Node>, Option<ValueRLP>), StoreError> {
        /* Possible flow paths:
            Step 1: Removal
                Branch { [ ... ], Path, Value } -> Branch { [...], None, None } (remove from self)
                Branch { [ childA, ... ], Path, Value } -> Branch { [childA', ... ], Path, Value } (remove from child)

            Step 2: Restructure
                [0 children]
                Branch { [], Path, Value } -> Leaf { Path, Value } (no children, with value)
                Branch { [], None, None } -> Branch { [], None, None } (no children, no value)
                [1 child]
                Branch { [ ExtensionChild], _ , _ } -> Extension { ChoiceIndex+ExtensionChildPrefx, ExtensionChildChild }
                Branch { [ BranchChild ], None, None } -> Extension { ChoiceIndex, BranchChild }
                Branch { [ LeafChild], None, None } -> LeafChild
                Branch { [LeafChild], Path, Value } -> Branch { [ LeafChild ], Path, Value }
                [+1 children]
                Branch { [childA, childB, ... ], None, None } ->   Branch { [childA, childB, ... ], None, None }
        */

        // Step 1: Remove value

        let path_offset = path.offset();
        // Check if the value is located in a child subtrie
        let value = match path.next() {
            Some(choice_index) => {
                if self.choices[choice_index as usize].is_valid() {
                    let child_node = db
                        .get_node(self.choices[choice_index as usize])?
                        .expect("inconsistent internal tree structure");
                    // Remove value from child node
                    let (child_node, old_value) = child_node.remove(db, path)?;
                    if let Some(child_node) = child_node {
                        // Update child node
                        self.choices[choice_index as usize] = db.insert_node(child_node)?;
                    } else {
                        // Remove child reference if the child subtrie was removed in the process
                        self.choices[choice_index as usize] = NodeRef::default();
                    }
                    old_value
                } else {
                    None
                }
            }
            None => {
                // Remove own value (if it has one) and return it
                if !self.path.is_empty() {
                    let value = self.value;
                    self.path = Default::default();
                    self.value = Default::default();

                    (!value.is_empty()).then_some(value)
                } else {
                    None
                }
            }
        };

        // Step 2: Restructure self

        // Check if self only has one child left

        // An `Err(_)` means more than one choice. `Ok(Some(_))` and `Ok(None)` mean a single and no
        // choices respectively.
        // If there is only one child choice_count will contain the choice index and the reference of the child node
        let choice_count = self
            .choices
            .iter_mut()
            .enumerate()
            .try_fold(None, |acc, (i, x)| {
                Ok(match (acc, x.is_valid()) {
                    (None, true) => Some((i, x)),
                    (None, false) => None,
                    (Some(_), true) => return Err(()),
                    (Some((i, x)), false) => Some((i, x)),
                })
            });

        if value.is_some() {
            self.hash.mark_as_dirty();
        }

        let child_ref = match choice_count {
            Ok(Some((choice_index, child_ref))) => {
                let choice_index = Nibble::try_from(choice_index as u8).unwrap();
                let child_node = db
                    .get_node(*child_ref)?
                    .expect("inconsistent internal tree structure");

                match child_node {
                    // Replace the child node  with an extension node leading to it
                    // The extension node will then replace self if self has no value
                    Node::Branch(_) => {
                        *child_ref = db.insert_node(
                            ExtensionNode::new(
                                NibbleVec::from_single(choice_index, path_offset % 2 != 0),
                                *child_ref,
                            )
                            .into(),
                        )?;
                    }
                    // Replace self with the child extension node, updating its path in the process
                    Node::Extension(mut extension_node) => {
                        debug_assert!(self.path.is_empty()); // Sanity check
                        extension_node.prefix.prepend(choice_index);
                        // Return node here so we don't have to update it in the DB and then fetch it
                        return Ok((Some(extension_node.into()), value));
                    }
                    _ => {}
                }

                Some(child_ref)
            }
            _ => None,
        };

        let new_node = match (child_ref, !self.path.is_empty()) {
            // If this node still has a child and value return the updated node
            (Some(_), true) => Some(self.into()),
            // If this node still has a value but no longer has children, convert it into a leaf node
            (None, true) => Some(LeafNode::new(self.path, self.value).into()),
            // If this node doesn't have a value, replace it with its child node
            (Some(x), false) => Some(
                db.get_node(*x)?
                    .expect("inconsistent internal tree structure"),
            ),
            // Return this node
            (None, false) => Some(self.into()),
        };

        Ok((new_node, value))
    }

    /// Computes the node's hash given the offset in the path traversed before reaching this node
    pub fn compute_hash(&self, db: &TrieDB, path_offset: usize) -> Result<NodeHashRef, StoreError> {
        if let Some(hash) = self.hash.extract_ref() {
            return Ok(hash);
        };
        let hash_choice = |node_ref: NodeRef| -> Result<DelimitedHash, StoreError> {
            if node_ref.is_valid() {
                let child_node = db
                    .get_node(node_ref)?
                    .expect("inconsistent internal tree structure");

                let mut target = Output::default();
                let target_len = match child_node.compute_hash(db, path_offset + 1)? {
                    NodeHashRef::Inline(x) => {
                        target[..x.len()].copy_from_slice(&x);
                        x.len()
                    }
                    NodeHashRef::Hashed(x) => {
                        target.copy_from_slice(&x);
                        x.len()
                    }
                };

                Ok(DelimitedHash(target, target_len))
            } else {
                Ok(DelimitedHash(Output::default(), 0))
            }
        };
        let children = self
            .choices
            .iter()
            .map(|node_ref| hash_choice(*node_ref))
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap();

        let encoded_value = (!self.value.is_empty()).then_some(&self.value[..]);

        Ok(compute_branch_hash::<DelimitedHash>(
            &self.hash,
            &children,
            encoded_value,
        ))
    }
}

/// Helper method to compute the hash of a branch node
fn compute_branch_hash<'a, T>(
    hash: &'a NodeHash,
    choices: &[T; 16],
    value: Option<&[u8]>,
) -> NodeHashRef<'a>
where
    T: AsRef<[u8]>,
{
    let mut children_len: usize = choices
        .iter()
        .map(|x| match x.as_ref().len() {
            0 => 1,
            32 => NodeHasher::bytes_len(32, x.as_ref()[0]),
            x => x,
        })
        .sum();

    if let Some(value) = value {
        children_len +=
            NodeHasher::bytes_len(value.len(), value.first().copied().unwrap_or_default());
    } else {
        children_len += 1;
    }

    let mut hasher = NodeHasher::new(hash);
    hasher.write_list_header(children_len);
    choices.iter().for_each(|x| match x.as_ref().len() {
        0 => hasher.write_bytes(&[]),
        32 => hasher.write_bytes(x.as_ref()),
        _ => hasher.write_raw(x.as_ref()),
    });
    match value {
        Some(value) => hasher.write_bytes(value),
        None => hasher.write_bytes(&[]),
    }
    hasher.finalize()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{pmt_node, trie::Trie};

    #[test]
    fn new() {
        let node = BranchNode::new({
            let mut choices = [Default::default(); 16];

            choices[2] = NodeRef::new(2);
            choices[5] = NodeRef::new(5);

            choices
        });

        assert_eq!(
            node.choices,
            [
                Default::default(),
                Default::default(),
                NodeRef::new(2),
                Default::default(),
                Default::default(),
                NodeRef::new(5),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
        );
    }

    #[test]
    fn get_some() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x12, 0x34, 0x56, 0x78] },
                1 => leaf { vec![0x10] => vec![0x34, 0x56, 0x78, 0x9A] },
            }
        };

        assert_eq!(
            node.get(&trie.db, NibbleSlice::new(&[0x00])).unwrap(),
            Some(vec![0x12, 0x34, 0x56, 0x78]),
        );
        assert_eq!(
            node.get(&trie.db, NibbleSlice::new(&[0x10])).unwrap(),
            Some(vec![0x34, 0x56, 0x78, 0x9A]),
        );
    }

    #[test]
    fn get_none() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x12, 0x34, 0x56, 0x78] },
                1 => leaf { vec![0x10] => vec![0x34, 0x56, 0x78, 0x9A] },
            }
        };

        assert_eq!(node.get(&trie.db, NibbleSlice::new(&[0x20])).unwrap(), None,);
    }

    #[test]
    fn insert_self() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x12, 0x34, 0x56, 0x78] },
                1 => leaf { vec![0x10] => vec![0x34, 0x56, 0x78, 0x9A] },
            }
        };
        let path = NibbleSlice::new(&[0x2]);
        let value = vec![0x3];

        let node = node
            .insert(&mut trie.db, path.clone(), value.clone())
            .unwrap();

        assert!(matches!(node, Node::Branch(_)));
        assert_eq!(node.get(&trie.db, path).unwrap(), Some(value));
    }

    #[test]
    fn insert_choice() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x12, 0x34, 0x56, 0x78] },
                1 => leaf { vec![0x10] => vec![0x34, 0x56, 0x78, 0x9A] },
            }
        };

        let path = NibbleSlice::new(&[0x20]);
        let value = vec![0x21];

        let node = node
            .insert(&mut trie.db, path.clone(), value.clone())
            .unwrap();

        assert!(matches!(node, Node::Branch(_)));
        assert_eq!(node.get(&trie.db, path).unwrap(), Some(value));
    }

    #[test]
    fn insert_passthrough() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x12, 0x34, 0x56, 0x78] },
                1 => leaf { vec![0x10] => vec![0x34, 0x56, 0x78, 0x9A] },
            }
        };

        // The extension node is ignored since it's irrelevant in this test.
        let mut path = NibbleSlice::new(&[0x00]);
        path.offset_add(2);
        let value = vec![0x1];

        let new_node = node
            .clone()
            .insert(&mut trie.db, path.clone(), value.clone())
            .unwrap();

        let new_node = match new_node {
            Node::Branch(x) => x,
            _ => panic!("expected a branch node"),
        };

        assert_eq!(new_node.choices, node.choices);
        assert_eq!(new_node.path, path.data());
        assert_eq!(new_node.value, value);
    }

    #[test]
    fn remove_choice_into_inner() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x00] },
                1 => leaf { vec![0x10] => vec![0x10] },
            }
        };

        let (node, value) = node
            .remove(&mut trie.db, NibbleSlice::new(&[0x00]))
            .unwrap();

        assert!(matches!(node, Some(Node::Leaf(_))));
        assert_eq!(value, Some(vec![0x00]));
    }

    #[test]
    fn remove_choice() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x00] },
                1 => leaf { vec![0x10] => vec![0x10] },
                2 => leaf { vec![0x10] => vec![0x10] },
            }
        };

        let (node, value) = node
            .remove(&mut trie.db, NibbleSlice::new(&[0x00]))
            .unwrap();

        assert!(matches!(node, Some(Node::Branch(_))));
        assert_eq!(value, Some(vec![0x00]));
    }

    #[test]
    fn remove_choice_into_value() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x00] },
            } with_leaf { vec![0x01] => vec![0xFF] }
        };

        let (node, value) = node
            .remove(&mut trie.db, NibbleSlice::new(&[0x00]))
            .unwrap();

        assert!(matches!(node, Some(Node::Leaf(_))));
        assert_eq!(value, Some(vec![0x00]));
    }

    #[test]
    fn remove_value_into_inner() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x00] },
            } with_leaf { vec![0x1] => vec![0xFF] }
        };

        let (node, value) = node.remove(&mut trie.db, NibbleSlice::new(&[])).unwrap();

        assert!(matches!(node, Some(Node::Leaf(_))));
        assert_eq!(value, Some(vec![0xFF]));
    }

    #[test]
    fn remove_value() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0 => leaf { vec![0x00] => vec![0x00] },
                1 => leaf { vec![0x10] => vec![0x10] },
            } with_leaf { vec![0x1] => vec![0xFF] }
        };

        let (node, value) = node.remove(&mut trie.db, NibbleSlice::new(&[])).unwrap();

        assert!(matches!(node, Some(Node::Branch(_))));
        assert_eq!(value, Some(vec![0xFF]));
    }

    #[test]
    fn compute_hash_two_choices() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                2 => leaf { vec![0x20] => vec![0x20] },
                4 => leaf { vec![0x40] => vec![0x40] },
            }
        };

        assert_eq!(
            node.compute_hash(&trie.db, 0).unwrap().as_ref(),
            &[
                0xD5, 0x80, 0x80, 0xC2, 0x30, 0x20, 0x80, 0xC2, 0x30, 0x40, 0x80, 0x80, 0x80, 0x80,
                0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
            ],
        );
    }

    #[test]
    fn compute_hash_all_choices() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0x0 => leaf { vec![0x00] => vec![0x00] },
                0x1 => leaf { vec![0x10] => vec![0x10] },
                0x2 => leaf { vec![0x20] => vec![0x20] },
                0x3 => leaf { vec![0x30] => vec![0x30] },
                0x4 => leaf { vec![0x40] => vec![0x40] },
                0x5 => leaf { vec![0x50] => vec![0x50] },
                0x6 => leaf { vec![0x60] => vec![0x60] },
                0x7 => leaf { vec![0x70] => vec![0x70] },
                0x8 => leaf { vec![0x80] => vec![0x80] },
                0x9 => leaf { vec![0x90] => vec![0x90] },
                0xA => leaf { vec![0xA0] => vec![0xA0] },
                0xB => leaf { vec![0xB0] => vec![0xB0] },
                0xC => leaf { vec![0xC0] => vec![0xC0] },
                0xD => leaf { vec![0xD0] => vec![0xD0] },
                0xE => leaf { vec![0xE0] => vec![0xE0] },
                0xF => leaf { vec![0xF0] => vec![0xF0] },
            }
        };

        assert_eq!(
            node.compute_hash(&trie.db, 0).unwrap().as_ref(),
            &[
                0x0A, 0x3C, 0x06, 0x2D, 0x4A, 0xE3, 0x61, 0xEC, 0xC4, 0x82, 0x07, 0xB3, 0x2A, 0xDB,
                0x6A, 0x3A, 0x3F, 0x3E, 0x98, 0x33, 0xC8, 0x9C, 0x9A, 0x71, 0x66, 0x3F, 0x4E, 0xB5,
                0x61, 0x72, 0xD4, 0x9D,
            ],
        );
    }

    #[test]
    fn compute_hash_one_choice_with_value() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                2 => leaf { vec![0x20] => vec![0x20] },
                4 => leaf { vec![0x40] => vec![0x40] },
            } with_leaf { vec![0x1] => vec![0x1] }
        };

        assert_eq!(
            node.compute_hash(&trie.db, 0).unwrap().as_ref(),
            &[
                0xD5, 0x80, 0x80, 0xC2, 0x30, 0x20, 0x80, 0xC2, 0x30, 0x40, 0x80, 0x80, 0x80, 0x80,
                0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01,
            ],
        );
    }

    #[test]
    fn compute_hash_all_choices_with_value() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            branch {
                0x0 => leaf { vec![0x00] => vec![0x00] },
                0x1 => leaf { vec![0x10] => vec![0x10] },
                0x2 => leaf { vec![0x20] => vec![0x20] },
                0x3 => leaf { vec![0x30] => vec![0x30] },
                0x4 => leaf { vec![0x40] => vec![0x40] },
                0x5 => leaf { vec![0x50] => vec![0x50] },
                0x6 => leaf { vec![0x60] => vec![0x60] },
                0x7 => leaf { vec![0x70] => vec![0x70] },
                0x8 => leaf { vec![0x80] => vec![0x80] },
                0x9 => leaf { vec![0x90] => vec![0x90] },
                0xA => leaf { vec![0xA0] => vec![0xA0] },
                0xB => leaf { vec![0xB0] => vec![0xB0] },
                0xC => leaf { vec![0xC0] => vec![0xC0] },
                0xD => leaf { vec![0xD0] => vec![0xD0] },
                0xE => leaf { vec![0xE0] => vec![0xE0] },
                0xF => leaf { vec![0xF0] => vec![0xF0] },
            } with_leaf { vec![0x1] => vec![0x1] }
        };

        assert_eq!(
            node.compute_hash(&trie.db, 0).unwrap().as_ref(),
            &[
                0x2A, 0x85, 0x67, 0xC5, 0x63, 0x4A, 0x87, 0xBA, 0x19, 0x6F, 0x2C, 0x65, 0x15, 0x16,
                0x66, 0x37, 0xE0, 0x9A, 0x34, 0xE6, 0xC9, 0xB0, 0x4D, 0xA5, 0x6F, 0xC4, 0x70, 0x4E,
                0x38, 0x61, 0x7D, 0x8E
            ],
        );
    }
}
