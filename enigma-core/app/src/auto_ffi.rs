/* automatically generated by rust-bindgen */

#![allow(dead_code)]
use enigma_types::*;
use sgx_types::*;

extern "C" {
    pub fn ecall_get_registration_quote(
        eid: sgx_enclave_id_t,
        retval: *mut sgx_status_t,
        target_info: *const sgx_target_info_t,
        report: *mut sgx_report_t,
    ) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_run_tests(eid: sgx_enclave_id_t, db_ptr: *const RawPointer, result: *mut ResultStatus) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_deploy(
        eid: sgx_enclave_id_t,
        retval: *mut EnclaveReturn,
        bytecode: *const u8,
        bytecode_len: usize,
        construct: *const u8,
        construct_len: usize,
        args: *const u8,
        args_len: usize,
        address: *const ContractAddress,
        user_key: *mut [u8; 64usize],
        gas_limit: *const u64,
        db_ptr: *const RawPointer,
        result: *mut ExecuteResult,
    ) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_execute(
        eid: sgx_enclave_id_t,
        retval: *mut EnclaveReturn,
        bytecode: *const u8,
        bytecode_len: usize,
        callable: *const u8,
        callable_len: usize,
        callable_args: *const u8,
        callable_args_len: usize,
        pubkey: *mut [u8; 64usize],
        address: *const ContractAddress,
        gas_limit: *const u64,
        db_ptr: *const RawPointer,
        result: *mut ExecuteResult,
    ) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_get_signing_address(eid: sgx_enclave_id_t, arr: *mut [u8; 20usize]) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_ptt_req(
        eid: sgx_enclave_id_t,
        retval: *mut EnclaveReturn,
        sig: *mut [u8; 65usize],
        serialized_ptr: *mut u64,
    ) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_ptt_res(eid: sgx_enclave_id_t, retval: *mut EnclaveReturn, msg_ptr: *const u8, msg_len: usize) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_build_state(
        eid: sgx_enclave_id_t,
        retval: *mut EnclaveReturn,
        db_ptr: *const RawPointer,
        failed_ptr: *mut u64,
    ) -> sgx_status_t;
}
extern "C" {
    pub fn ecall_get_user_key(
        eid: sgx_enclave_id_t,
        retval: *mut EnclaveReturn,
        sig: *mut [u8; 65usize],
        pubkey: *mut [u8; 64usize],
        serialized_ptr: *mut u64,
    ) -> sgx_status_t;
}
