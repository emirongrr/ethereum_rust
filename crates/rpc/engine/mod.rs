use ethereum_rust_core::{
    types::{validate_block_header, ExecutionPayloadV3, PayloadStatus},
    H256,
};
use ethereum_rust_evm::{evm_state, execute_block};
use ethereum_rust_storage::Store;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::RpcErr;

pub type ExchangeCapabilitiesRequest = Vec<String>;

pub struct NewPayloadV3Request {
    pub payload: ExecutionPayloadV3,
    pub expected_blob_versioned_hashes: Vec<H256>,
    pub parent_beacon_block_root: H256,
}

impl NewPayloadV3Request {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<NewPayloadV3Request> {
        let params = params.as_ref()?;
        if params.len() != 3 {
            return None;
        }
        Some(NewPayloadV3Request {
            payload: serde_json::from_value(params[0].clone()).ok()?,
            expected_blob_versioned_hashes: serde_json::from_value(params[1].clone()).ok()?,
            parent_beacon_block_root: serde_json::from_value(params[2].clone()).ok()?,
        })
    }
}

pub fn exchange_capabilities(capabilities: &ExchangeCapabilitiesRequest) -> Result<Value, RpcErr> {
    Ok(json!(capabilities))
}

pub fn forkchoice_updated_v3() -> Result<Value, RpcErr> {
    Ok(json!({
        "payloadId": null,
        "payloadStatus": {
            "latestValidHash": null,
            "status": "SYNCING",
            "validationError": null
        }
    }))
}

pub fn new_payload_v3(
    request: NewPayloadV3Request,
    storage: Store,
) -> Result<PayloadStatus, RpcErr> {
    let block_hash = request.payload.block_hash;
    info!("Received new payload with block hash: {block_hash}");

    let block = match request.payload.into_block(request.parent_beacon_block_root) {
        Ok(block) => block,
        Err(error) => return Ok(PayloadStatus::invalid_with_err(&error.to_string())),
    };

    // Payload Validation

    // Check timestamp does not fall within the time frame of the Cancun fork
    match storage.get_cancun_time()? {
        Some(cancun_time) if block.header.timestamp > cancun_time => {}
        _ => return Err(RpcErr::UnsuportedFork),
    }

    // Check that block_hash is valid
    let actual_block_hash = block.header.compute_block_hash();
    if block_hash != actual_block_hash {
        return Ok(PayloadStatus::invalid_with_err("Invalid block hash"));
    }
    info!("Block hash {block_hash} is valid");
    // Concatenate blob versioned hashes lists (tx.blob_versioned_hashes) of each blob transaction included in the payload, respecting the order of inclusion
    // and check that the resulting array matches expected_blob_versioned_hashes
    let blob_versioned_hashes: Vec<H256> = block
        .body
        .transactions
        .iter()
        .flat_map(|tx| tx.blob_versioned_hashes())
        .collect();
    if request.expected_blob_versioned_hashes != blob_versioned_hashes {
        return Ok(PayloadStatus::invalid_with_err(
            "Invalid blob_versioned_hashes",
        ));
    }
    // Check that the incoming block extends the current chain
    let last_block_number = storage.get_latest_block_number()?.ok_or(RpcErr::Internal)?;
    if block.header.number <= last_block_number {
        // Check if we already have this block stored
        if storage
            .get_block_number(block_hash)
            .map_err(|_| RpcErr::Internal)?
            .is_some_and(|num| num == block.header.number)
        {
            return Ok(PayloadStatus::valid_with_hash(block_hash));
        }
        warn!("Should start reorg but it is not supported yet");
        return Err(RpcErr::Internal);
    } else if block.header.number != last_block_number + 1 {
        return Ok(PayloadStatus::syncing());
    }

    // Fetch parent block header and validate current header
    if let Some(parent_header) = storage.get_block_header(block.header.number.saturating_sub(1))? {
        if !validate_block_header(&block.header, &parent_header) {
            return Ok(PayloadStatus::invalid_with_hash(
                parent_header.compute_block_hash(),
            ));
        }
    } else {
        return Ok(PayloadStatus::syncing());
    }

    // Execute and store the block
    info!("Executing payload with block hash: {block_hash}");
    execute_block(&block, &mut evm_state(storage.clone()))?;
    info!("Block with hash {block_hash} executed succesfully");
    info!("Block with hash {block_hash} added to storage");

    Ok(PayloadStatus::valid_with_hash(block_hash))
}
