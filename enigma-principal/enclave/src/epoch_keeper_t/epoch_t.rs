use enigma_tools_m::keeper_types::{InputWorkerParams, RawEncodable};
use ethabi::Bytes;
use ethereum_types::{H160, H256, U256};
use std::string::ToString;
use std::vec::Vec;

use enigma_tools_t::common::errors_t::{
    EnclaveError::{self, SystemError},
    EnclaveSystemError,
};
use enigma_types::ContractAddress;
use super::nested_encoding::{ONE, NestedSerialization};

pub type EpochNonce = [u8; 32];
pub type EpochMarker = [u8; 64];

#[derive(Debug, Clone)]
pub struct Epoch {
    pub nonce: U256,
    pub seed: U256,
    pub worker_params: InputWorkerParams,
}

impl Epoch {
    pub fn get_selected_worker(&self, sc_addr: ContractAddress) -> Result<H160, EnclaveError> {
        self.worker_params
            .get_selected_worker(sc_addr, self.seed)
            .ok_or_else(|| SystemError(EnclaveSystemError::WorkerAuthError { err: "Worker selection returns nothing.".to_string() }))
    }

    pub fn encode_epoch(&self) -> Bytes {
        let mut encoding_len: usize = 0;
        let mut encoding: Vec<u8> = Vec::new();

        let (len_seed, seed_encoding) = self.seed.hash_encode();
        let (len_nonce, nonce_encoding) = self.nonce.hash_encode();
        let (len_workers, workers_encoding) = self.worker_params.workers.hash_encode();
        let (len_stakes, stakes_encoding) = self.worker_params.stakes.hash_encode();

        encoding_len = len_seed + len_nonce + len_workers + len_stakes;

        encoding.push(ONE);
        encoding.extend_from_slice(&encoding_len.to_be_bytes());
        encoding.extend_from_slice(&seed_encoding);
        encoding.extend_from_slice(&nonce_encoding);
        encoding.extend_from_slice(&workers_encoding);
        encoding.extend_from_slice(&stakes_encoding);

        encoding
    }
}
