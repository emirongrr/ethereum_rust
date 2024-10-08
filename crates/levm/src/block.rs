use ethereum_types::{Address, H256, U256};

#[derive(Clone, Debug, Default)]
/// Collection of relevant pieces of information about the block, or "block header".
pub struct BlockEnv {
    /// The number of ancestor blocks of this block (block height).
    pub number: U256,
    /// Coinbase or miner or address that created and signed the block.
    ///
    /// This is the receiver address of all the gas spent in the block.
    pub coinbase: Address,
    /// The timestamp of the block in seconds since the UNIX epoch.
    pub timestamp: U256,
    /// The amount of wei to burn for each unit of gas consumed, added in the London upgrade with [EIP-1559].
    ///
    /// [EIP-1559]: https://eips.ethereum.org/EIPS/eip-1559
    pub base_fee_per_gas: U256,
    /// Current limit of gas expenditure per block.
    pub gas_limit: usize,
    // Chain ID of the EVM, it will be compared to the transaction's Chain ID.
    // Chain ID is introduced here https://eips.ethereum.org/EIPS/eip-155
    pub chain_id: usize,
    /// The output of the randomness beacon provided by the beacon chain.
    ///
    /// Replaces `difficulty` after the Paris (AKA the merge) upgrade with [EIP-4399].
    ///
    /// [EIP-4399]: https://eips.ethereum.org/EIPS/eip-4399
    pub prev_randao: Option<H256>,
    /// Running total of blob gas consumed in excess of the target, prior to the block. Blocks with above-target blob gas consumption increase this value, blocks with below-target blob gas consumption decrease it (bounded at 0).
    ///
    /// Incorporated as part of the Cancun upgrade via [EIP-4844].
    ///
    /// [EIP-4844]: https://eips.ethereum.org/EIPS/eip-4844#header-extension
    pub excess_blob_gas: Option<u64>,
    /// Total amount of blob gas consumed by the transactions within the block.
    ///
    /// Incorporated as part of the Cancun upgrade via [EIP-4844].
    ///
    /// [EIP-4844]: https://eips.ethereum.org/EIPS/eip-4844#header-extension
    pub blob_gas_used: Option<u64>,
}

pub const LAST_AVAILABLE_BLOCK_LIMIT: u64 = 256;
// EIP-4844 constants.
/// Minimum gas price for data blobs.
pub const MIN_BLOB_GASPRICE: u64 = 1;
/// Controls the maximum rate of change for blob gas price.
pub const BLOB_GASPRICE_UPDATE_FRACTION: u64 = 3338477;
/// Gas consumption of a single data blob (== blob byte size).
pub const GAS_PER_BLOB: u64 = 1 << 17;
/// Target number of the blob per block.
pub const TARGET_BLOB_NUMBER_PER_BLOCK: u64 = 3;
/// Target consumable blob gas for data blobs per block (for 1559-like pricing).
pub const TARGET_BLOB_GAS_PER_BLOCK: u64 = TARGET_BLOB_NUMBER_PER_BLOCK * GAS_PER_BLOB;

impl BlockEnv {
    /// Calculates the blob gas price from the header's excess blob gas field.
    ///
    /// See [the EIP-4844 helpers](https://eips.ethereum.org/EIPS/eip-4844#helpers)
    pub fn calculate_blob_gas_price(&self) -> U256 {
        let excess_blob_gas = self.calc_excess_blob_gas();
        U256::from(fake_exponential(
            MIN_BLOB_GASPRICE,
            excess_blob_gas,
            BLOB_GASPRICE_UPDATE_FRACTION,
        ))
    }

    /// Calculates the `excess_blob_gas` from the parent header's `blob_gas_used` and `excess_blob_gas`.
    ///
    /// See [the EIP-4844 helpers]<https://eips.ethereum.org/EIPS/eip-4844#helpers>
    pub fn calc_excess_blob_gas(&self) -> u64 {
        (self.excess_blob_gas.unwrap_or_default() + self.blob_gas_used.unwrap_or_default())
            .saturating_sub(TARGET_BLOB_GAS_PER_BLOCK)
    }
}

/// Approximates `factor * e ** (numerator / denominator)` using Taylor expansion.
///
/// This is used to calculate the blob price.
///
/// See also [the EIP-4844 helpers](https://eips.ethereum.org/EIPS/eip-4844#helpers)
/// (`fake_exponential`).
///
/// # Panics
///
/// This function panics if `denominator` is zero.
pub fn fake_exponential(factor: u64, numerator: u64, denominator: u64) -> u128 {
    assert_ne!(denominator, 0, "attempt to divide by zero");
    let factor = factor as u128;
    let numerator = numerator as u128;
    let denominator = denominator as u128;

    let mut i = 1;
    let mut output = 0;
    let mut numerator_accum = factor * denominator;
    while numerator_accum > 0 {
        output += numerator_accum;
        // Denominator is asserted as not zero at the start of the function.
        numerator_accum = (numerator_accum * numerator) / (denominator * i);
        i += 1;
    }
    output / denominator
}
