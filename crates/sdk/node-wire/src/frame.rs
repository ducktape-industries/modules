//! The signed frame every write and query carries: core
//! `crates/kernel/node/src/frame.rs`, field for field.
use borsh::{BorshDeserialize, BorshSerialize};
use commonware_cryptography::{Signer as _, ed25519};
use keyscheme::KeyScheme;

use crate::Tip;

/// What a frame's proof signs under.
pub const FRAME_NAMESPACE: &[u8] = b"ducktape:frame";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Body {
    pub scheme: KeyScheme,
    pub signer: Vec<u8>,
    /// The network's name (`Status::network`), not the chain id.
    pub network: Vec<u8>,
    pub anchor: Tip,
    pub salt: Option<u64>,
    pub target: String,
    pub payload: Vec<u8>,
}

impl Body {
    /// What the signer signs under [`FRAME_NAMESPACE`].
    pub fn preimage(&self) -> Vec<u8> {
        abi::encode(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Frame {
    pub body: Body,
    pub proof: Vec<u8>,
}

impl Frame {
    /// `payload` to `target`, signed by the Ed25519 `key`.
    pub fn sign(
        key: &ed25519::PrivateKey,
        network: &[u8],
        anchor: Tip,
        salt: Option<u64>,
        target: &str,
        payload: Vec<u8>,
    ) -> Frame {
        let body = Body {
            scheme: KeyScheme::Ed25519,
            signer: key.public_key().as_ref().to_vec(),
            network: network.to_vec(),
            anchor,
            salt,
            target: target.to_owned(),
            payload,
        };
        let proof = key
            .sign(FRAME_NAMESPACE, &body.preimage())
            .as_ref()
            .to_vec();
        Frame { body, proof }
    }

    pub fn encode(&self) -> Vec<u8> {
        abi::encode(self)
    }
}
