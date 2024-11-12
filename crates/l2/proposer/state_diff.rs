use std::collections::HashMap;

use bytes::Bytes;
use ethereum_types::{Address, H256, U256};

use super::errors::StateDiffError;

#[derive(Clone)]
pub struct AccountStateDiff {
    pub new_balance: Option<U256>,
    pub nonce_diff: Option<u16>,
    pub storage: Vec<(H256, U256)>,
    pub bytecode: Option<Bytes>,
    pub bytecode_hash: Option<H256>,
}

pub enum AccountStateDiffType {
    NewBalance = 1,
    NonceDiff = 2,
    Storage = 4,
    Bytecode = 8,
    BytecodeHash = 16,
}

#[derive(Clone)]
pub struct WithdrawalLog {
    pub address: Address,
    pub amount: U256,
    pub tx_hash: H256,
}

#[derive(Clone)]
pub struct DepositLog {
    pub address: Address,
    pub amount: U256,
}

#[derive(Clone)]
pub struct StateDiff {
    pub version: u8,
    pub modified_accounts: HashMap<Address, AccountStateDiff>,
    pub withdrawal_logs: Vec<WithdrawalLog>,
    pub deposit_logs: Vec<DepositLog>,
}

impl TryFrom<u8> for AccountStateDiffType {
    type Error = StateDiffError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(AccountStateDiffType::NewBalance),
            2 => Ok(AccountStateDiffType::NonceDiff),
            4 => Ok(AccountStateDiffType::Storage),
            8 => Ok(AccountStateDiffType::Bytecode),
            16 => Ok(AccountStateDiffType::BytecodeHash),
            _ => Err(StateDiffError::InvalidAccountStateDiffType(value)),
        }
    }
}

impl Default for StateDiff {
    fn default() -> Self {
        StateDiff {
            version: 1,
            modified_accounts: HashMap::new(),
            withdrawal_logs: Vec::new(),
            deposit_logs: Vec::new(),
        }
    }
}

impl StateDiff {
    pub fn encode(&self) -> Result<Bytes, StateDiffError> {
        if self.version != 1 {
            return Err(StateDiffError::UnsupportedVersion(self.version));
        }

        let mut encoded: Vec<u8> = Vec::new();
        encoded.push(self.version);
        encoded.extend((self.modified_accounts.len() as u16).to_be_bytes());

        for (address, diff) in &self.modified_accounts {
            let (r#type, diff_encoded) = diff.encode()?;
            encoded.extend(r#type.to_be_bytes());
            encoded.extend(address.0);
            encoded.extend(diff_encoded);
        }

        for withdrawal in self.withdrawal_logs.iter() {
            encoded.extend(withdrawal.address.0);
            let buf = &mut [0u8; 32];
            withdrawal.amount.to_big_endian(buf);
            encoded.extend_from_slice(buf);
            encoded.extend(&withdrawal.tx_hash.0);
        }

        for deposit in self.deposit_logs.iter() {
            encoded.extend(deposit.address.0);
            let buf = &mut [0u8; 32];
            deposit.amount.to_big_endian(buf);
            encoded.extend_from_slice(buf);
        }

        Ok(Bytes::from(encoded))
    }

    pub fn decode() -> Result<Self, String> {
        unimplemented!()
    }
}

impl AccountStateDiff {
    pub fn encode(&self) -> Result<(u8, Bytes), StateDiffError> {
        if self.bytecode.is_some() && self.bytecode_hash.is_some() {
            return Err(StateDiffError::BytecodeAndBytecodeHashSet);
        }

        let mut r#type = 0;
        let mut encoded: Vec<u8> = Vec::new();

        if let Some(new_balance) = self.new_balance {
            r#type += AccountStateDiffType::NewBalance as u8;
            let buf = &mut [0u8; 32];
            new_balance.to_big_endian(buf);
            encoded.extend_from_slice(buf);
        }

        if let Some(nonce_diff) = self.nonce_diff {
            r#type += AccountStateDiffType::NonceDiff as u8;
            encoded.extend(nonce_diff.to_be_bytes());
        }

        if !self.storage.is_empty() {
            r#type += AccountStateDiffType::Storage as u8;
            encoded.extend((self.storage.len() as u16).to_be_bytes());
            for (key, value) in &self.storage {
                encoded.extend_from_slice(&key.0);
                let buf = &mut [0u8; 32];
                value.to_big_endian(buf);
                encoded.extend_from_slice(buf);
            }
        }

        if let Some(bytecode) = &self.bytecode {
            r#type += AccountStateDiffType::Bytecode as u8;
            encoded.extend((bytecode.len() as u16).to_be_bytes());
            encoded.extend(bytecode);
        }

        if let Some(bytecode_hash) = &self.bytecode_hash {
            r#type += AccountStateDiffType::BytecodeHash as u8;
            encoded.extend(&bytecode_hash.0);
        }

        if r#type == 0 {
            return Err(StateDiffError::EmptyAccountDiff);
        }

        Ok((r#type, Bytes::from(encoded)))
    }
}