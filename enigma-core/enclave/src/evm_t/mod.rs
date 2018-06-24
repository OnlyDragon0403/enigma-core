pub mod evm;
pub mod abi;
pub mod error;
pub mod rlp;
use ring::digest;
use std::vec::Vec;


pub enum EvmResult{
    SUCCESS=0,
    FAULT,
}

pub fn get_key() -> Vec<u8> {
    digest::digest(&digest::SHA256, b"EnigmaMPC").as_ref().iter().cloned().collect()
}

pub mod preprocessor{
    use std::vec::Vec;
    use sgx_trts::trts::rsgx_read_rand;
    // TODO: Implement Errors
    pub fn run(pre_sig: &str) -> Vec<u8> {
        match pre_sig {
            "rand()" | "rand" => rand(),
            _ => panic!()
        }
    }
    fn rand() -> Vec<u8> {
        let mut r: [u8; 16] = [0; 16];
        match rsgx_read_rand(&mut r) {
            Ok(_) => r.to_vec(),
            Err(err) => panic!(err)
        }
    }

}
