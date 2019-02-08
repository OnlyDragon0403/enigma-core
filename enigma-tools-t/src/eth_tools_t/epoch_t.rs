use ethabi::{Address, Event, EventParam, FixedBytes, Hash, ParamType, RawLog, Token, Uint};
use ethabi::token::{LenientTokenizer, Tokenizer};
use sgx_types::*;
use std::string::ToString;
use std::prelude::v1::Box;
use std::vec::Vec;
use std::panic;
use std::convert::TryFrom;
use serde_json as ser;
use eth_tools_t::keeper_types_t::{EventWrapper, Log};
use ethereum_types::{H160, U256, H256, U64};
use common::errors_t::EnclaveError;
use bigint;
use rlp::{Encodable, encode, RlpStream};
use enigma_crypto::hash::Keccak256;

pub trait IntoBigint<T> {
    fn bigint(self) -> T;
}

impl IntoBigint<bigint::U256> for U256 { fn bigint(self) -> bigint::U256 { bigint::U256(self.0) } }

impl IntoBigint<bigint::H256> for H256 { fn bigint(self) -> bigint::H256 { bigint::H256(self.0) } }

#[derive(Debug, Clone)]
struct WorkerSelectionToken {
    pub seed: U256,
    pub sc_addr: Hash,
    pub nonce: U256,
}

impl Encodable for WorkerSelectionToken {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3);
        s.append(&self.seed.bigint());
        s.append(&self.sc_addr.bigint());
        s.append(&self.nonce.bigint());
    }
}


#[derive(Debug, Clone)]
pub struct WorkerParams {
    pub block_number: U256,
    pub workers: Vec<Address>,
    pub balances: Vec<U256>,
    pub nonce: U256,
    pub seed: U256,
}

impl TryFrom<Log> for WorkerParams {
    type Error = EnclaveError;
    fn try_from(log: Log) -> Result<WorkerParams, EnclaveError> {
        println!("Parsing log: {:?}", log);
        let event = EventWrapper::workers_parameterized();
        let raw_log = RawLog { topics: log.topics, data: log.data };
        let log = match event.0.parse_log(raw_log) {
            Ok(log) => log,
            Err(err) => return Err(EnclaveError::WorkerAuthError { err: format!("Unable to parse the log: {:?}", err) }),
        };
        // Ugly deserialization from ABI tokens
        // TODO: not sure what the best pattern to handle errors here
        // TODO: do I really need to clone so much?
        let seed = log.params[0].value.clone().to_uint().unwrap();
        let block_number = log.params[1].value.clone().to_uint().unwrap();
        let workers = log.params[2].value.clone().to_array().unwrap().iter().map(|t| t.clone().to_address().unwrap()).collect::<Vec<H160>>();
        let balances = log.params[3].value.clone().to_array().unwrap().iter().map(|t| t.clone().to_uint().unwrap()).collect::<Vec<U256>>();
        let nonce = log.params[4].value.clone().to_uint().unwrap();

        Ok(Self { block_number, workers, balances, nonce, seed })
    }
}

impl WorkerParams {
    /// Discover the selected workers from the worker parameters
    pub fn get_selected_workers(&self, sc_addr: H256, group_size: Option<U64>) -> Result<Vec<Address>, EnclaveError> {
        let workers = self.workers.to_vec();
        let mut balance_sum: U256 = U256::from(0);
        for balance in self.balances.clone() {
            balance_sum = balance_sum + balance;
        }
        // Using the same type as the Enigma contract
        let mut nonce = U256::from(0);
        let mut selected_workers: Vec<H160> = Vec::new();
        while {
            let token = WorkerSelectionToken { seed: self.seed, sc_addr, nonce };
            // This is equivalent to encodePacked in Solidity
            let hash: [u8; 32] = encode(&token).keccak256().into();
            let mut rand_val: U256 = U256::from(hash) % balance_sum;
            println!("The initial random value: {:?}", rand_val);
            let mut selected_worker = self.workers[self.workers.len() - 1];
            for i in 0..self.workers.len() {
                let result = rand_val.overflowing_sub(self.balances[i]);
                if result.1 == true || result.0 == U256::from(0) {
                    selected_worker = self.workers[i];
                    break;
                }
                rand_val = result.0;
                println!("The next random value: {:?}", rand_val);
            }
            if !selected_workers.contains(&selected_worker) {
                selected_workers.push(selected_worker);
            }
            nonce = nonce + U256::from(1);
            let limit = match group_size {
                Some(size) => size,
                None => U64::from(1),
            };
            U64::from(selected_workers.len()) < limit
        } {}
        println!("The selected workers: {:?}", selected_workers);
        Ok(selected_workers)
    }
}

#[derive(Debug, Clone)]
pub struct Epoch {
    pub seed: U256,
    pub worker_params: Option<WorkerParams>,
    pub preverified_block_hash: Hash,
}

impl Epoch {
    pub fn set_worker_params(&mut self, params: WorkerParams) -> Result<(), EnclaveError> {
        if self.worker_params.is_some() {
            return Err(EnclaveError::WorkerAuthError {
                err: format!("Worker parameters already set for epoch: {:?}", self),
            });
        }
        // TODO: check that the seed of the WorkerParams matches the seed generated by the enclave
        println!("Comparing generated seed: {:?} with WorkerParams seed: {:?}", self.seed, params.seed);
        self.worker_params = Some(params);
        Ok(())
    }

    pub fn get_selected_workers(self, sc_addr: Hash) -> Result<Vec<Address>, EnclaveError> {
        let workers = match self.worker_params {
            Some(params) => params.get_selected_workers(sc_addr, None)?,
            None => {
                return Err(EnclaveError::WorkerAuthError {
                    err: format!("No worker parameters in epoch: {:?}", self),
                });
            }
        };
        Ok(workers)
    }
}

