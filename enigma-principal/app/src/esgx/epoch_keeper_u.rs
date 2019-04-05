use enigma_tools_m::keeper_types::InputWorkerParams;
use failure::Error;
use sgx_types::{sgx_enclave_id_t, sgx_status_t};
use web3::types::{Bytes, U256, H256};

use common_u::errors::EnclaveFailError;
use enigma_types::{EnclaveReturn, traits::SliceCPtr};
use epoch_u::epoch_types::{encode, EpochState};

extern "C" {
    fn ecall_set_worker_params(
        eid: sgx_enclave_id_t, retval: &mut EnclaveReturn, worker_params_rlp: *const u8, worker_params_rlp_len: usize,
        seed_in: &[u8; 32], nonce_in: &[u8; 32],
        rand_out: &mut [u8; 32], nonce_out: &mut [u8; 32], sig_out: &mut [u8; 65],
    ) -> sgx_status_t;
}

/// Returns an EpochState object containing the 32 bytes signed random seed and an incremented account nonce.
/// # Examples
/// ```
/// let enclave = esgx::general::init_enclave().unwrap();
/// let result = self.contract.get_active_workers(block_number)?;
/// let worker_params: InputWorkerParams = InputWorkerParams { block_number, workers: result.0, stakes: result.1 };
/// let sig = set_worker_params(enclave.geteid(), worker_params).unwrap();
/// ```
pub fn set_worker_params(eid: sgx_enclave_id_t, worker_params: &InputWorkerParams, epoch_state: Option<EpochState>) -> Result<EpochState, Error> {
    let mut retval: EnclaveReturn = EnclaveReturn::Success;
    let (nonce_in, mut seed_in) = match epoch_state.clone() {
        Some(e) => (H256::from(e.nonce).0, H256::from(e.seed).0),
        None => ([0; 32], [0; 32])
    };
    let (mut nonce_out, mut rand_out) = ([0; 32], [0; 32]);
    println!("Calling enclave set_worker_params with nonce/seed: {:?} {:?}", nonce_out, rand_out);
    let mut sig_out: [u8; 65] = [0; 65];
    // Serialize the InputWorkerParams into RLP
    let worker_params_rlp = encode(worker_params);
    let status = unsafe {
        ecall_set_worker_params(
            eid,
            &mut retval,
            worker_params_rlp.as_c_ptr() as *const u8,
            worker_params_rlp.len(),
            &seed_in,
            &nonce_in,
            &mut rand_out,
            &mut nonce_out,
            &mut sig_out,
        )
    };
    if retval != EnclaveReturn::Success || status != sgx_status_t::SGX_SUCCESS {
        return Err(EnclaveFailError { err: retval, status }.into());
    }
    let epoch_state_out = match epoch_state {
        Some(epoch_state) => epoch_state,
        None => {
            let seed = U256::from_big_endian(&rand_out);
            let sig = Bytes(sig_out.to_vec());
            let nonce = U256::from_big_endian(&nonce_out);
            EpochState::new(seed, sig, nonce)
        }
    };
    Ok(epoch_state_out)
}

#[cfg(test)]
pub mod tests {
    use ethabi::Uint;
    use web3::types::Address;

    use esgx::general::init_enclave_wrapper;

    use super::*;

    pub(crate) fn set_mock_worker_params(eid: sgx_enclave_id_t) -> (EpochState) {
        let worker_params = InputWorkerParams {
            block_number: U256::from(1),
            workers: vec![Address::from("f25186B5081Ff5cE73482AD761DB0eB0d25abfBF")],
            stakes: vec![U256::from(1)],
        };
        set_worker_params(eid, &worker_params, None).unwrap()
    }

    #[test]
    fn test_set_mock_worker_params() {
        let enclave = init_enclave_wrapper().unwrap();
        let epoch_seed = set_mock_worker_params(enclave.geteid());
        println!("Got epoch seed params: {:?}", epoch_seed);
        assert_eq!(epoch_seed.nonce, Uint::from(0));

        enclave.destroy();
    }
}
