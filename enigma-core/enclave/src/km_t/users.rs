use crate::SIGNINING_KEY;
use enigma_tools_t::common::errors_t::EnclaveError;
use enigma_tools_t::common::utils_t::LockExpectMutex;
use enigma_tools_t::cryptography_t::asymmetric::KeyPair;
use std::{vec::Vec, sync::SgxMutex};
use enigma_tools_t::km_primitives::{UserMessage, PubKey};
use std::collections::HashMap;

lazy_static! { pub static ref DH_KEYS: SgxMutex< HashMap<Vec<u8>, [u8; 32]> > = SgxMutex::new(HashMap::new()); }

pub(crate) unsafe fn ecall_get_user_key_internal(sig: &mut [u8; 65], user_pubkey: &PubKey) -> Result<Vec<u8>, EnclaveError> {
    let keys = KeyPair::new()?;
    let req = UserMessage::new(keys.get_pubkey());
    let msg = req.to_message()?;
    *sig = SIGNINING_KEY.sign(&msg[..])?;
    let enc_key = keys.get_aes_key(&user_pubkey)?;
    DH_KEYS.lock_expect("DH Keys").insert(user_pubkey.to_vec(), enc_key);
    Ok(msg)
}