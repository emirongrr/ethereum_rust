use ethereum_rust_core::rlp::{
    decode::RLPDecode,
    encode::RLPEncode,
    error::RLPDecodeError,
    structs::{Decoder, Encoder},
};

use super::node::{BranchNode, ExtensionNode, LeafNode, Node};

enum NodeType {
    Branch = 0,
    Extension = 1,
    Leaf = 2,
}

impl NodeType {
    fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Branch),
            1 => Some(Self::Extension),
            2 => Some(Self::Leaf),
            _ => None,
        }
    }
}

impl RLPEncode for BranchNode {
    fn encode(&self, buf: &mut dyn bytes::BufMut) {
        // TODO: choices encoded as vec due to conflicting trait impls for [T;N] & [u8;N], check if we can fix this later
        Encoder::new(buf)
            .encode_field(&self.hash)
            .encode_field(&self.choices.to_vec())
            .encode_field(&self.path)
            .finish()
    }
}

impl RLPEncode for ExtensionNode {
    fn encode(&self, buf: &mut dyn bytes::BufMut) {
        Encoder::new(buf)
            .encode_field(&self.hash)
            .encode_field(&self.prefix)
            .encode_field(&self.child)
            .finish()
    }
}

impl RLPEncode for LeafNode {
    fn encode(&self, buf: &mut dyn bytes::BufMut) {
        Encoder::new(buf)
            .encode_field(&self.hash)
            .encode_field(&self.path)
            .finish()
    }
}

impl RLPDecode for BranchNode {
    fn decode_unfinished(rlp: &[u8]) -> Result<(Self, &[u8]), RLPDecodeError> {
        const CHOICES_LEN_ERROR_MSG: &str =
            "Error decoding field 'choices' of type [H256;16]: Invalid Length";
        let decoder = Decoder::new(rlp)?;
        let (hash, decoder) = decoder.decode_field("hash")?;
        let (choices, decoder) = decoder.decode_field::<Vec<_>>("choices")?;
        let choices = choices
            .try_into()
            .map_err(|_| RLPDecodeError::Custom(CHOICES_LEN_ERROR_MSG.to_string()))?;
        let (path, decoder) = decoder.decode_field("path")?;
        Ok((
            Self {
                hash,
                choices,
                path,
            },
            decoder.finish()?,
        ))
    }
}

impl RLPDecode for ExtensionNode {
    fn decode_unfinished(rlp: &[u8]) -> Result<(Self, &[u8]), RLPDecodeError> {
        let decoder = Decoder::new(rlp)?;
        let (hash, decoder) = decoder.decode_field("hash")?;
        let (prefix, decoder) = decoder.decode_field("prefix")?;
        let (child, decoder) = decoder.decode_field("child")?;
        Ok((
            Self {
                hash,
                prefix,
                child,
            },
            decoder.finish()?,
        ))
    }
}

impl RLPDecode for LeafNode {
    fn decode_unfinished(rlp: &[u8]) -> Result<(Self, &[u8]), RLPDecodeError> {
        let decoder = Decoder::new(rlp)?;
        let (hash, decoder) = decoder.decode_field("hash")?;
        let (path, decoder) = decoder.decode_field("path")?;
        Ok((Self { hash, path }, decoder.finish()?))
    }
}

impl RLPEncode for Node {
    fn encode(&self, buf: &mut dyn bytes::BufMut) {
        let node_type = match self {
            Node::Branch(_) => NodeType::Branch,
            Node::Extension(_) => NodeType::Extension,
            Node::Leaf(_) => NodeType::Leaf,
        };
        buf.put_u8(node_type as u8);
        self.encode(buf)
    }
}

impl RLPDecode for Node {
    fn decode_unfinished(rlp: &[u8]) -> Result<(Self, &[u8]), RLPDecodeError> {
        let node_type = rlp.first().ok_or_else(|| RLPDecodeError::InvalidLength)?;
        let node_type =
            NodeType::from_u8(*node_type).ok_or_else(|| RLPDecodeError::MalformedData)?;
        let rlp = &rlp[1..];
        match node_type {
            NodeType::Branch => {
                BranchNode::decode_unfinished(rlp).map(|(node, rem)| (Node::Branch(node), rem))
            }
            NodeType::Extension => ExtensionNode::decode_unfinished(rlp)
                .map(|(node, rem)| (Node::Extension(node), rem)),
            NodeType::Leaf => {
                LeafNode::decode_unfinished(rlp).map(|(node, rem)| (Node::Leaf(node), rem))
            }
        }
    }
}