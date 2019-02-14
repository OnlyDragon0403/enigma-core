use crate::km_t;
use enigma_runtime_t::ocalls_t as runtime_ocalls_t;
use enigma_runtime_t::{data::ContractState, eng_resolver, Runtime, RuntimeResult};
use enigma_tools_t::common::errors_t::EnclaveError;
use enigma_tools_t::common::utils_t::LockExpectMutex;
use enigma_crypto::{CryptoError, Encryption};
use enigma_types::{ContractAddress, RawPointer};
use parity_wasm::elements::{self, Deserialize};
use parity_wasm::io::Cursor;
use std::boxed::Box;
use std::string::String;
use std::string::ToString;
use std::vec::Vec;
use wasm_utils::rules;
use wasmi::{ImportsBuilder, Module, ModuleInstance};

/// Wasm cost table
pub struct WasmCosts {
    /// Default opcode cost
    pub regular: u32,
    /// Div operations multiplier.
    pub div: u32,
    /// Div operations multiplier.
    pub mul: u32,
    /// Memory (load/store) operations multiplier.
    pub mem: u32,
    /// General static query of U256 value from env-info
    pub static_u256: u32,
    /// General static query of Address value from env-info
    pub static_address: u32,
    /// Memory stipend. Amount of free memory (in 64kb pages) each contract can use for stack.
    pub initial_mem: u32,
    /// Grow memory cost, per page (64kb)
    pub grow_mem: u32,
    /// Memory copy cost, per byte
    pub memcpy: u32,
    /// Max stack height (native WebAssembly stack limiter)
    pub max_stack_height: u32,
    /// Cost of wasm opcode is calculated as TABLE_ENTRY_COST * `opcodes_mul` / `opcodes_div`
    pub opcodes_mul: u32,
    /// Cost of wasm opcode is calculated as TABLE_ENTRY_COST * `opcodes_mul` / `opcodes_div`
    pub opcodes_div: u32,
}

impl Default for WasmCosts {
    fn default() -> Self {
        WasmCosts {
            regular: 1,
            div: 16,
            mul: 4,
            mem: 2,
            static_u256: 64,
            static_address: 40,
            initial_mem: 4096,
            grow_mem: 8192,
            memcpy: 1,
            max_stack_height: 64 * 1024,
            opcodes_mul: 3,
            opcodes_div: 8,
        }
    }
}

fn gas_rules(wasm_costs: &WasmCosts) -> rules::Set {
    rules::Set::new(wasm_costs.regular, {
        let mut vals = ::std::collections::BTreeMap::new();
        vals.insert(rules::InstructionType::Load, rules::Metering::Fixed(wasm_costs.mem as u32));
        vals.insert(rules::InstructionType::Store, rules::Metering::Fixed(wasm_costs.mem as u32));
        vals.insert(rules::InstructionType::Div, rules::Metering::Fixed(wasm_costs.div as u32));
        vals.insert(rules::InstructionType::Mul, rules::Metering::Fixed(wasm_costs.mul as u32));
        vals
    })
    .with_grow_cost(wasm_costs.grow_mem)
    //.with_forbidden_floats()
}

fn create_module(code: &[u8]) -> Result<Box<Module>, EnclaveError> {
    let mut cursor = Cursor::new(&code[..]);
    let deserialized_module = elements::Module::deserialize(&mut cursor)?;
    if deserialized_module.memory_section().map_or(false, |ms| ms.entries().len() > 0) {
        // According to WebAssembly spec, internal memory is hidden from embedder and should not
        // be interacted with. So we disable this kind of modules at decoding level.
        return Err(EnclaveError::ExecutionError { code: "".to_string(), err: "Malformed wasm module: internal memory".to_string() });
    }
    let wasm_costs = WasmCosts::default();
    let contract_module = pwasm_utils::inject_gas_counter(deserialized_module, &gas_rules(&wasm_costs))?;
    let limited_module = pwasm_utils::stack_height::inject_limiter(contract_module, wasm_costs.max_stack_height)?;

    let module = wasmi::Module::from_parity_wasm_module(limited_module)?;
    Ok(Box::new(module))
}

fn execute(module: &Module, gas_limit: u64, state: ContractState,
           function_name: String, types: String, params: Vec<u8>) -> Result<RuntimeResult, EnclaveError> {
    let instantiation_resolver = eng_resolver::ImportResolver::with_limit(64);

    let imports = ImportsBuilder::new().with_resolver("env", &instantiation_resolver);

    // Instantiate a module
    let instance = ModuleInstance::new(module, &imports).expect("failed to instantiate wasm module").assert_no_start();

    let mut runtime = Runtime::new_with_state(gas_limit, instantiation_resolver.memory_ref(), params, state, function_name, types);

    match instance.invoke_export("call", &[], &mut runtime) {
        Ok(_v) => {
            let result = runtime.into_result()?;
            Ok(result)
        }
        Err(e) => {
            println!("Error in invocation of the external function: {}", e);
            // TODO: @moria This is not always deployment.
            Err(EnclaveError::ExecutionError { code: "deployment code".to_string(), err: e.to_string() })
        }
    }
}

pub fn execute_call(code: &[u8], gas_limit: u64, state: ContractState,
                    function_name: String, types: String, params: Vec<u8>) -> Result<RuntimeResult, EnclaveError>{
    let module = create_module(code)?;
    execute(&module, gas_limit, state, function_name, types, params)
}

pub fn execute_constructor(code: &[u8], gas_limit: u64, state: ContractState, params: Vec<u8>) -> Result<RuntimeResult, EnclaveError>{
    let module = create_module(code)?;
    execute(&module, gas_limit, state, "".to_string(), "".to_string(), params)
}

pub fn get_state(db_ptr: *const RawPointer, addr: ContractAddress) -> Result<ContractState, EnclaveError> {
    let guard = km_t::STATE_KEYS.lock_expect("State Keys");
    let key = guard.get(&addr).ok_or(CryptoError::MissingKeyError { key_type: "State Key" })?;

    let enc_state = runtime_ocalls_t::get_state(db_ptr, addr)?;
    let state = ContractState::decrypt(enc_state, key)?;

    Ok(state)
}

pub mod tests {

    use enigma_runtime_t::data::{ContractState, DeltasInterface};
    use enigma_crypto::hash::Sha256;
    use std::string::ToString;
    use std::vec::Vec;

    pub fn test_execute_contract() {
        let addr = b"enigma".sha256();
        let bytecode: Vec<u8> = vec![0, 97, 115, 109, 1, 0, 0, 0, 1, 42, 8, 96, 4, 127, 127, 127, 127, 0, 96, 2, 127, 127, 1, 127, 96, 2, 127, 127, 0, 96, 0, 0, 96, 3, 127, 127, 127, 0, 96, 1, 127, 0, 96, 1, 127, 1, 127, 96, 1, 127, 1, 126, 2, 69, 4, 3, 101, 110, 118, 11, 119, 114, 105, 116, 101, 95, 115, 116, 97, 116, 101, 0, 0, 3, 101, 110, 118, 10, 114, 101, 97, 100, 95, 115, 116, 97, 116, 101, 0, 1, 3, 101, 110, 118, 11, 102, 114, 111, 109, 95, 109, 101, 109, 111, 114, 121, 0, 2, 3, 101, 110, 118, 6, 109, 101, 109, 111, 114, 121, 2, 1, 17, 32, 3, 17, 16, 3, 4, 5, 3, 3, 6, 5, 6, 3, 5, 5, 5, 2, 2, 5, 7, 4, 5, 1, 112, 1, 3, 3, 6, 9, 1, 127, 1, 65, 128, 128, 192, 0, 11, 7, 8, 1, 4, 99, 97, 108, 108, 0, 3, 9, 8, 1, 0, 65, 1, 11, 2, 17, 18, 10, 164, 43, 16, 65, 1, 1, 127, 35, 0, 65, 16, 107, 34, 0, 36, 0, 65, 128, 128, 192, 0, 65, 4, 65, 132, 128, 192, 0, 65, 3, 16, 0, 32, 0, 65, 128, 128, 192, 0, 65, 4, 16, 4, 2, 64, 32, 0, 40, 2, 4, 69, 13, 0, 32, 0, 40, 2, 0, 16, 5, 11, 32, 0, 65, 16, 106, 36, 0, 11, 74, 0, 2, 64, 32, 1, 32, 2, 16, 1, 34, 1, 65, 127, 76, 13, 0, 2, 64, 2, 64, 32, 1, 69, 13, 0, 32, 1, 16, 8, 34, 2, 13, 1, 0, 0, 11, 65, 1, 33, 2, 11, 32, 2, 32, 1, 16, 2, 32, 0, 32, 1, 54, 2, 4, 32, 0, 32, 2, 54, 2, 0, 32, 0, 65, 0, 54, 2, 8, 15, 11, 16, 6, 0, 11, 160, 7, 1, 5, 127, 32, 0, 65, 120, 106, 34, 1, 32, 0, 65, 124, 106, 40, 2, 0, 34, 2, 65, 120, 113, 34, 0, 106, 33, 3, 2, 64, 2, 64, 32, 2, 65, 1, 113, 13, 0, 32, 2, 65, 3, 113, 69, 13, 1, 32, 1, 40, 2, 0, 34, 2, 32, 0, 106, 33, 0, 2, 64, 2, 64, 2, 64, 65, 0, 40, 2, 172, 131, 64, 32, 1, 32, 2, 107, 34, 1, 70, 13, 0, 32, 2, 65, 255, 1, 75, 13, 1, 32, 1, 40, 2, 12, 34, 4, 32, 1, 40, 2, 8, 34, 5, 70, 13, 2, 32, 5, 32, 4, 54, 2, 12, 32, 4, 32, 5, 54, 2, 8, 12, 3, 11, 32, 3, 40, 2, 4, 34, 2, 65, 3, 113, 65, 3, 71, 13, 2, 65, 0, 32, 0, 54, 2, 164, 131, 64, 32, 3, 65, 4, 106, 32, 2, 65, 126, 113, 54, 2, 0, 32, 1, 32, 0, 65, 1, 114, 54, 2, 4, 32, 1, 32, 0, 106, 32, 0, 54, 2, 0, 15, 11, 32, 1, 16, 14, 12, 1, 11, 65, 0, 65, 0, 40, 2, 148, 128, 64, 65, 126, 32, 2, 65, 3, 118, 119, 113, 54, 2, 148, 128, 64, 11, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 32, 3, 40, 2, 4, 34, 2, 65, 2, 113, 13, 0, 65, 0, 40, 2, 176, 131, 64, 32, 3, 70, 13, 1, 65, 0, 40, 2, 172, 131, 64, 32, 3, 70, 13, 2, 32, 2, 65, 120, 113, 34, 4, 32, 0, 106, 33, 0, 32, 4, 65, 255, 1, 75, 13, 3, 32, 3, 40, 2, 12, 34, 4, 32, 3, 40, 2, 8, 34, 3, 70, 13, 4, 32, 3, 32, 4, 54, 2, 12, 32, 4, 32, 3, 54, 2, 8, 12, 5, 11, 32, 3, 65, 4, 106, 32, 2, 65, 126, 113, 54, 2, 0, 32, 1, 32, 0, 65, 1, 114, 54, 2, 4, 32, 1, 32, 0, 106, 32, 0, 54, 2, 0, 12, 7, 11, 65, 0, 32, 1, 54, 2, 176, 131, 64, 65, 0, 65, 0, 40, 2, 168, 131, 64, 32, 0, 106, 34, 0, 54, 2, 168, 131, 64, 32, 1, 32, 0, 65, 1, 114, 54, 2, 4, 2, 64, 32, 1, 65, 0, 40, 2, 172, 131, 64, 71, 13, 0, 65, 0, 65, 0, 54, 2, 164, 131, 64, 65, 0, 65, 0, 54, 2, 172, 131, 64, 11, 65, 0, 40, 2, 204, 131, 64, 32, 0, 79, 13, 7, 2, 64, 32, 0, 65, 41, 73, 13, 0, 65, 188, 131, 192, 0, 33, 0, 3, 64, 2, 64, 32, 0, 40, 2, 0, 34, 3, 32, 1, 75, 13, 0, 32, 3, 32, 0, 40, 2, 4, 106, 32, 1, 75, 13, 2, 11, 32, 0, 40, 2, 8, 34, 0, 13, 0, 11, 11, 65, 0, 33, 1, 65, 0, 40, 2, 196, 131, 64, 34, 0, 69, 13, 4, 3, 64, 32, 1, 65, 1, 106, 33, 1, 32, 0, 40, 2, 8, 34, 0, 13, 0, 11, 32, 1, 65, 255, 31, 32, 1, 65, 255, 31, 75, 27, 33, 1, 12, 5, 11, 65, 0, 32, 1, 54, 2, 172, 131, 64, 65, 0, 65, 0, 40, 2, 164, 131, 64, 32, 0, 106, 34, 0, 54, 2, 164, 131, 64, 32, 1, 32, 0, 65, 1, 114, 54, 2, 4, 32, 1, 32, 0, 106, 32, 0, 54, 2, 0, 15, 11, 32, 3, 16, 14, 12, 1, 11, 65, 0, 65, 0, 40, 2, 148, 128, 64, 65, 126, 32, 2, 65, 3, 118, 119, 113, 54, 2, 148, 128, 64, 11, 32, 1, 32, 0, 65, 1, 114, 54, 2, 4, 32, 1, 32, 0, 106, 32, 0, 54, 2, 0, 32, 1, 65, 0, 40, 2, 172, 131, 64, 71, 13, 2, 65, 0, 32, 0, 54, 2, 164, 131, 64, 15, 11, 65, 255, 31, 33, 1, 11, 65, 0, 65, 127, 54, 2, 204, 131, 64, 65, 0, 32, 1, 54, 2, 212, 131, 64, 15, 11, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 32, 0, 65, 255, 1, 75, 13, 0, 32, 0, 65, 3, 118, 34, 3, 65, 3, 116, 65, 156, 128, 192, 0, 106, 33, 0, 65, 0, 40, 2, 148, 128, 64, 34, 2, 65, 1, 32, 3, 65, 31, 113, 116, 34, 3, 113, 69, 13, 1, 32, 0, 65, 8, 106, 33, 2, 32, 0, 40, 2, 8, 33, 3, 12, 2, 11, 32, 1, 32, 0, 16, 15, 65, 0, 65, 0, 40, 2, 212, 131, 64, 65, 127, 106, 34, 1, 54, 2, 212, 131, 64, 32, 1, 13, 4, 65, 0, 40, 2, 196, 131, 64, 34, 0, 69, 13, 2, 65, 0, 33, 1, 3, 64, 32, 1, 65, 1, 106, 33, 1, 32, 0, 40, 2, 8, 34, 0, 13, 0, 11, 32, 1, 65, 255, 31, 32, 1, 65, 255, 31, 75, 27, 33, 1, 12, 3, 11, 65, 0, 32, 2, 32, 3, 114, 54, 2, 148, 128, 64, 32, 0, 65, 8, 106, 33, 2, 32, 0, 33, 3, 11, 32, 2, 32, 1, 54, 2, 0, 32, 3, 32, 1, 54, 2, 12, 32, 1, 32, 0, 54, 2, 12, 32, 1, 32, 3, 54, 2, 8, 15, 11, 65, 255, 31, 33, 1, 11, 65, 0, 32, 1, 54, 2, 212, 131, 64, 11, 11, 5, 0, 16, 7, 0, 11, 10, 0, 65, 236, 131, 192, 0, 16, 12, 0, 11, 128, 27, 2, 9, 127, 1, 126, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 32, 0, 65, 244, 1, 75, 13, 0, 65, 0, 40, 2, 148, 128, 64, 34, 1, 65, 16, 32, 0, 65, 11, 106, 65, 120, 113, 32, 0, 65, 11, 73, 27, 34, 2, 65, 3, 118, 34, 3, 65, 31, 113, 34, 4, 118, 34, 0, 65, 3, 113, 69, 13, 1, 32, 0, 65, 127, 115, 65, 1, 113, 32, 3, 106, 34, 2, 65, 3, 116, 34, 4, 65, 164, 128, 192, 0, 106, 40, 2, 0, 34, 0, 65, 8, 106, 33, 5, 32, 0, 40, 2, 8, 34, 3, 32, 4, 65, 156, 128, 192, 0, 106, 34, 4, 70, 13, 2, 32, 3, 32, 4, 54, 2, 12, 32, 4, 65, 8, 106, 32, 3, 54, 2, 0, 12, 3, 11, 65, 0, 33, 3, 32, 0, 65, 64, 79, 13, 28, 32, 0, 65, 11, 106, 34, 0, 65, 120, 113, 33, 2, 65, 0, 40, 2, 152, 128, 64, 34, 6, 69, 13, 9, 65, 0, 33, 7, 2, 64, 32, 0, 65, 8, 118, 34, 0, 69, 13, 0, 65, 31, 33, 7, 32, 2, 65, 255, 255, 255, 7, 75, 13, 0, 32, 2, 65, 38, 32, 0, 103, 34, 0, 107, 65, 31, 113, 118, 65, 1, 113, 65, 31, 32, 0, 107, 65, 1, 116, 114, 33, 7, 11, 65, 0, 32, 2, 107, 33, 3, 32, 7, 65, 2, 116, 65, 164, 130, 192, 0, 106, 40, 2, 0, 34, 0, 69, 13, 6, 65, 0, 33, 4, 32, 2, 65, 0, 65, 25, 32, 7, 65, 1, 118, 107, 65, 31, 113, 32, 7, 65, 31, 70, 27, 116, 33, 1, 65, 0, 33, 5, 3, 64, 2, 64, 32, 0, 40, 2, 4, 65, 120, 113, 34, 8, 32, 2, 73, 13, 0, 32, 8, 32, 2, 107, 34, 8, 32, 3, 79, 13, 0, 32, 8, 33, 3, 32, 0, 33, 5, 32, 8, 69, 13, 6, 11, 32, 0, 65, 20, 106, 40, 2, 0, 34, 8, 32, 4, 32, 8, 32, 0, 32, 1, 65, 29, 118, 65, 4, 113, 106, 65, 16, 106, 40, 2, 0, 34, 0, 71, 27, 32, 4, 32, 8, 27, 33, 4, 32, 1, 65, 1, 116, 33, 1, 32, 0, 13, 0, 11, 32, 4, 69, 13, 5, 32, 4, 33, 0, 12, 7, 11, 32, 2, 65, 0, 40, 2, 164, 131, 64, 77, 13, 8, 32, 0, 69, 13, 2, 32, 0, 32, 4, 116, 65, 2, 32, 4, 116, 34, 0, 65, 0, 32, 0, 107, 114, 113, 34, 0, 65, 0, 32, 0, 107, 113, 104, 34, 3, 65, 3, 116, 34, 5, 65, 164, 128, 192, 0, 106, 40, 2, 0, 34, 0, 40, 2, 8, 34, 4, 32, 5, 65, 156, 128, 192, 0, 106, 34, 5, 70, 13, 10, 32, 4, 32, 5, 54, 2, 12, 32, 5, 65, 8, 106, 32, 4, 54, 2, 0, 12, 11, 11, 65, 0, 32, 1, 65, 126, 32, 2, 119, 113, 54, 2, 148, 128, 64, 11, 32, 0, 32, 2, 65, 3, 116, 34, 2, 65, 3, 114, 54, 2, 4, 32, 0, 32, 2, 106, 34, 0, 32, 0, 40, 2, 4, 65, 1, 114, 54, 2, 4, 32, 5, 15, 11, 65, 0, 40, 2, 152, 128, 64, 34, 0, 69, 13, 5, 32, 0, 65, 0, 32, 0, 107, 113, 104, 65, 2, 116, 65, 164, 130, 192, 0, 106, 40, 2, 0, 34, 1, 40, 2, 4, 65, 120, 113, 32, 2, 107, 33, 3, 32, 1, 33, 4, 32, 1, 40, 2, 16, 34, 0, 69, 13, 20, 65, 0, 33, 9, 12, 21, 11, 65, 0, 33, 3, 32, 0, 33, 5, 12, 2, 11, 32, 5, 13, 2, 11, 65, 0, 33, 5, 65, 2, 32, 7, 65, 31, 113, 116, 34, 0, 65, 0, 32, 0, 107, 114, 32, 6, 113, 34, 0, 69, 13, 2, 32, 0, 65, 0, 32, 0, 107, 113, 104, 65, 2, 116, 65, 164, 130, 192, 0, 106, 40, 2, 0, 34, 0, 69, 13, 2, 11, 3, 64, 32, 0, 40, 2, 4, 65, 120, 113, 34, 4, 32, 2, 79, 32, 4, 32, 2, 107, 34, 8, 32, 3, 73, 113, 33, 1, 2, 64, 32, 0, 40, 2, 16, 34, 4, 13, 0, 32, 0, 65, 20, 106, 40, 2, 0, 33, 4, 11, 32, 0, 32, 5, 32, 1, 27, 33, 5, 32, 8, 32, 3, 32, 1, 27, 33, 3, 32, 4, 33, 0, 32, 4, 13, 0, 11, 32, 5, 69, 13, 1, 11, 65, 0, 40, 2, 164, 131, 64, 34, 0, 32, 2, 73, 13, 1, 32, 3, 32, 0, 32, 2, 107, 73, 13, 1, 11, 2, 64, 2, 64, 2, 64, 2, 64, 65, 0, 40, 2, 164, 131, 64, 34, 3, 32, 2, 79, 13, 0, 65, 0, 40, 2, 168, 131, 64, 34, 0, 32, 2, 77, 13, 1, 65, 0, 32, 0, 32, 2, 107, 34, 3, 54, 2, 168, 131, 64, 65, 0, 65, 0, 40, 2, 176, 131, 64, 34, 0, 32, 2, 106, 34, 4, 54, 2, 176, 131, 64, 32, 4, 32, 3, 65, 1, 114, 54, 2, 4, 32, 0, 32, 2, 65, 3, 114, 54, 2, 4, 32, 0, 65, 8, 106, 15, 11, 65, 0, 40, 2, 172, 131, 64, 33, 0, 32, 3, 32, 2, 107, 34, 4, 65, 16, 79, 13, 1, 65, 0, 65, 0, 54, 2, 172, 131, 64, 65, 0, 65, 0, 54, 2, 164, 131, 64, 32, 0, 32, 3, 65, 3, 114, 54, 2, 4, 32, 0, 32, 3, 106, 34, 3, 65, 4, 106, 33, 2, 32, 3, 40, 2, 4, 65, 1, 114, 33, 3, 12, 2, 11, 65, 0, 33, 3, 32, 2, 65, 175, 128, 4, 106, 34, 4, 65, 16, 118, 64, 0, 34, 0, 65, 127, 70, 13, 20, 32, 0, 65, 16, 116, 34, 1, 69, 13, 20, 65, 0, 65, 0, 40, 2, 180, 131, 64, 32, 4, 65, 128, 128, 124, 113, 34, 8, 106, 34, 0, 54, 2, 180, 131, 64, 65, 0, 65, 0, 40, 2, 184, 131, 64, 34, 3, 32, 0, 32, 0, 32, 3, 73, 27, 54, 2, 184, 131, 64, 65, 0, 40, 2, 176, 131, 64, 34, 3, 69, 13, 9, 65, 188, 131, 192, 0, 33, 0, 3, 64, 32, 0, 40, 2, 0, 34, 4, 32, 0, 40, 2, 4, 34, 5, 106, 32, 1, 70, 13, 11, 32, 0, 40, 2, 8, 34, 0, 13, 0, 12, 19, 11, 11, 65, 0, 32, 4, 54, 2, 164, 131, 64, 65, 0, 32, 0, 32, 2, 106, 34, 1, 54, 2, 172, 131, 64, 32, 1, 32, 4, 65, 1, 114, 54, 2, 4, 32, 0, 32, 3, 106, 32, 4, 54, 2, 0, 32, 2, 65, 3, 114, 33, 3, 32, 0, 65, 4, 106, 33, 2, 11, 32, 2, 32, 3, 54, 2, 0, 32, 0, 65, 8, 106, 15, 11, 32, 5, 16, 14, 32, 3, 65, 15, 75, 13, 2, 32, 5, 32, 3, 32, 2, 106, 34, 0, 65, 3, 114, 54, 2, 4, 32, 5, 32, 0, 106, 34, 0, 32, 0, 40, 2, 4, 65, 1, 114, 54, 2, 4, 12, 12, 11, 65, 0, 32, 1, 65, 126, 32, 3, 119, 113, 54, 2, 148, 128, 64, 11, 32, 0, 65, 8, 106, 33, 4, 32, 0, 32, 2, 65, 3, 114, 54, 2, 4, 32, 0, 32, 2, 106, 34, 1, 32, 3, 65, 3, 116, 34, 3, 32, 2, 107, 34, 2, 65, 1, 114, 54, 2, 4, 32, 0, 32, 3, 106, 32, 2, 54, 2, 0, 65, 0, 40, 2, 164, 131, 64, 34, 0, 69, 13, 3, 32, 0, 65, 3, 118, 34, 5, 65, 3, 116, 65, 156, 128, 192, 0, 106, 33, 3, 65, 0, 40, 2, 172, 131, 64, 33, 0, 65, 0, 40, 2, 148, 128, 64, 34, 8, 65, 1, 32, 5, 65, 31, 113, 116, 34, 5, 113, 69, 13, 1, 32, 3, 40, 2, 8, 33, 5, 12, 2, 11, 32, 5, 32, 2, 65, 3, 114, 54, 2, 4, 32, 5, 32, 2, 106, 34, 0, 32, 3, 65, 1, 114, 54, 2, 4, 32, 0, 32, 3, 106, 32, 3, 54, 2, 0, 32, 3, 65, 255, 1, 75, 13, 5, 32, 3, 65, 3, 118, 34, 3, 65, 3, 116, 65, 156, 128, 192, 0, 106, 33, 2, 65, 0, 40, 2, 148, 128, 64, 34, 4, 65, 1, 32, 3, 65, 31, 113, 116, 34, 3, 113, 69, 13, 7, 32, 2, 65, 8, 106, 33, 4, 32, 2, 40, 2, 8, 33, 3, 12, 8, 11, 65, 0, 32, 8, 32, 5, 114, 54, 2, 148, 128, 64, 32, 3, 33, 5, 11, 32, 3, 65, 8, 106, 32, 0, 54, 2, 0, 32, 5, 32, 0, 54, 2, 12, 32, 0, 32, 3, 54, 2, 12, 32, 0, 32, 5, 54, 2, 8, 11, 65, 0, 32, 1, 54, 2, 172, 131, 64, 65, 0, 32, 2, 54, 2, 164, 131, 64, 32, 4, 15, 11, 2, 64, 2, 64, 65, 0, 40, 2, 208, 131, 64, 34, 0, 69, 13, 0, 32, 0, 32, 1, 77, 13, 1, 11, 65, 0, 32, 1, 54, 2, 208, 131, 64, 11, 65, 0, 33, 0, 65, 0, 32, 8, 54, 2, 192, 131, 64, 65, 0, 32, 1, 54, 2, 188, 131, 64, 65, 0, 65, 255, 31, 54, 2, 212, 131, 64, 65, 0, 65, 0, 54, 2, 200, 131, 64, 3, 64, 32, 0, 65, 164, 128, 192, 0, 106, 32, 0, 65, 156, 128, 192, 0, 106, 34, 3, 54, 2, 0, 32, 0, 65, 168, 128, 192, 0, 106, 32, 3, 54, 2, 0, 32, 0, 65, 8, 106, 34, 0, 65, 128, 2, 71, 13, 0, 11, 32, 1, 32, 8, 65, 88, 106, 34, 0, 65, 1, 114, 54, 2, 4, 65, 0, 32, 1, 54, 2, 176, 131, 64, 65, 0, 65, 128, 128, 128, 1, 54, 2, 204, 131, 64, 65, 0, 32, 0, 54, 2, 168, 131, 64, 32, 1, 32, 0, 106, 65, 40, 54, 2, 4, 12, 9, 11, 32, 0, 40, 2, 12, 69, 13, 1, 12, 7, 11, 32, 0, 32, 3, 16, 15, 12, 3, 11, 32, 1, 32, 3, 77, 13, 5, 32, 4, 32, 3, 75, 13, 5, 32, 0, 65, 4, 106, 32, 5, 32, 8, 106, 54, 2, 0, 65, 0, 40, 2, 176, 131, 64, 34, 0, 65, 15, 106, 65, 120, 113, 34, 3, 65, 120, 106, 34, 4, 65, 0, 40, 2, 168, 131, 64, 32, 8, 106, 34, 1, 32, 3, 32, 0, 65, 8, 106, 107, 107, 34, 3, 65, 1, 114, 54, 2, 4, 65, 0, 65, 128, 128, 128, 1, 54, 2, 204, 131, 64, 65, 0, 32, 4, 54, 2, 176, 131, 64, 65, 0, 32, 3, 54, 2, 168, 131, 64, 32, 0, 32, 1, 106, 65, 40, 54, 2, 4, 12, 6, 11, 65, 0, 32, 4, 32, 3, 114, 54, 2, 148, 128, 64, 32, 2, 65, 8, 106, 33, 4, 32, 2, 33, 3, 11, 32, 4, 32, 0, 54, 2, 0, 32, 3, 32, 0, 54, 2, 12, 32, 0, 32, 2, 54, 2, 12, 32, 0, 32, 3, 54, 2, 8, 11, 32, 5, 65, 8, 106, 33, 3, 12, 4, 11, 65, 1, 33, 9, 11, 3, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 32, 9, 14, 11, 0, 1, 2, 4, 5, 6, 8, 9, 10, 7, 3, 3, 11, 32, 0, 40, 2, 4, 65, 120, 113, 32, 2, 107, 34, 1, 32, 3, 32, 1, 32, 3, 73, 34, 1, 27, 33, 3, 32, 0, 32, 4, 32, 1, 27, 33, 4, 32, 0, 34, 1, 40, 2, 16, 34, 0, 13, 10, 65, 1, 33, 9, 12, 17, 11, 32, 1, 65, 20, 106, 40, 2, 0, 34, 0, 13, 10, 65, 2, 33, 9, 12, 16, 11, 32, 4, 16, 14, 32, 3, 65, 16, 79, 13, 10, 65, 10, 33, 9, 12, 15, 11, 32, 4, 32, 3, 32, 2, 106, 34, 0, 65, 3, 114, 54, 2, 4, 32, 4, 32, 0, 106, 34, 0, 32, 0, 40, 2, 4, 65, 1, 114, 54, 2, 4, 12, 13, 11, 32, 4, 32, 2, 65, 3, 114, 54, 2, 4, 32, 4, 32, 2, 106, 34, 2, 32, 3, 65, 1, 114, 54, 2, 4, 32, 2, 32, 3, 106, 32, 3, 54, 2, 0, 65, 0, 40, 2, 164, 131, 64, 34, 0, 69, 13, 9, 65, 4, 33, 9, 12, 13, 11, 32, 0, 65, 3, 118, 34, 5, 65, 3, 116, 65, 156, 128, 192, 0, 106, 33, 1, 65, 0, 40, 2, 172, 131, 64, 33, 0, 65, 0, 40, 2, 148, 128, 64, 34, 8, 65, 1, 32, 5, 65, 31, 113, 116, 34, 5, 113, 69, 13, 9, 65, 5, 33, 9, 12, 12, 11, 32, 1, 40, 2, 8, 33, 5, 12, 9, 11, 65, 0, 32, 8, 32, 5, 114, 54, 2, 148, 128, 64, 32, 1, 33, 5, 65, 6, 33, 9, 12, 10, 11, 32, 1, 65, 8, 106, 32, 0, 54, 2, 0, 32, 5, 32, 0, 54, 2, 12, 32, 0, 32, 1, 54, 2, 12, 32, 0, 32, 5, 54, 2, 8, 65, 7, 33, 9, 12, 9, 11, 65, 0, 32, 2, 54, 2, 172, 131, 64, 65, 0, 32, 3, 54, 2, 164, 131, 64, 65, 8, 33, 9, 12, 8, 11, 32, 4, 65, 8, 106, 15, 11, 65, 0, 33, 9, 12, 6, 11, 65, 0, 33, 9, 12, 5, 11, 65, 3, 33, 9, 12, 4, 11, 65, 7, 33, 9, 12, 3, 11, 65, 9, 33, 9, 12, 2, 11, 65, 6, 33, 9, 12, 1, 11, 65, 8, 33, 9, 12, 0, 11, 11, 65, 0, 65, 0, 40, 2, 208, 131, 64, 34, 0, 32, 1, 32, 0, 32, 1, 73, 27, 54, 2, 208, 131, 64, 32, 1, 32, 8, 106, 33, 4, 65, 188, 131, 192, 0, 33, 0, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 3, 64, 32, 0, 40, 2, 0, 32, 4, 70, 13, 1, 32, 0, 40, 2, 8, 34, 0, 13, 0, 12, 2, 11, 11, 32, 0, 40, 2, 12, 69, 13, 1, 11, 65, 188, 131, 192, 0, 33, 0, 2, 64, 3, 64, 2, 64, 32, 0, 40, 2, 0, 34, 4, 32, 3, 75, 13, 0, 32, 4, 32, 0, 40, 2, 4, 106, 34, 4, 32, 3, 75, 13, 2, 11, 32, 0, 40, 2, 8, 33, 0, 12, 0, 11, 11, 32, 1, 32, 8, 65, 88, 106, 34, 0, 65, 1, 114, 54, 2, 4, 32, 1, 32, 0, 106, 65, 40, 54, 2, 4, 32, 3, 32, 4, 65, 96, 106, 65, 120, 113, 65, 120, 106, 34, 5, 32, 5, 32, 3, 65, 16, 106, 73, 27, 34, 5, 65, 27, 54, 2, 4, 65, 0, 32, 1, 54, 2, 176, 131, 64, 65, 0, 65, 128, 128, 128, 1, 54, 2, 204, 131, 64, 65, 0, 32, 0, 54, 2, 168, 131, 64, 65, 0, 41, 2, 188, 131, 64, 33, 10, 32, 5, 65, 16, 106, 65, 0, 41, 2, 196, 131, 64, 55, 2, 0, 32, 5, 32, 10, 55, 2, 8, 65, 0, 32, 8, 54, 2, 192, 131, 64, 65, 0, 32, 1, 54, 2, 188, 131, 64, 65, 0, 32, 5, 65, 8, 106, 54, 2, 196, 131, 64, 65, 0, 65, 0, 54, 2, 200, 131, 64, 32, 5, 65, 28, 106, 33, 0, 3, 64, 32, 0, 65, 7, 54, 2, 0, 32, 4, 32, 0, 65, 4, 106, 34, 0, 75, 13, 0, 11, 32, 5, 32, 3, 70, 13, 3, 32, 5, 32, 5, 40, 2, 4, 65, 126, 113, 54, 2, 4, 32, 3, 32, 5, 32, 3, 107, 34, 0, 65, 1, 114, 54, 2, 4, 32, 5, 32, 0, 54, 2, 0, 2, 64, 32, 0, 65, 255, 1, 75, 13, 0, 32, 0, 65, 3, 118, 34, 4, 65, 3, 116, 65, 156, 128, 192, 0, 106, 33, 0, 65, 0, 40, 2, 148, 128, 64, 34, 1, 65, 1, 32, 4, 65, 31, 113, 116, 34, 4, 113, 69, 13, 2, 32, 0, 40, 2, 8, 33, 4, 12, 3, 11, 32, 3, 32, 0, 16, 15, 12, 3, 11, 32, 0, 32, 1, 54, 2, 0, 32, 0, 32, 0, 40, 2, 4, 32, 8, 106, 54, 2, 4, 32, 1, 32, 2, 65, 3, 114, 54, 2, 4, 32, 1, 32, 2, 106, 33, 0, 32, 4, 32, 1, 107, 32, 2, 107, 33, 2, 65, 0, 40, 2, 176, 131, 64, 32, 4, 70, 13, 4, 65, 0, 40, 2, 172, 131, 64, 32, 4, 70, 13, 5, 32, 4, 40, 2, 4, 34, 3, 65, 3, 113, 65, 1, 71, 13, 9, 32, 3, 65, 120, 113, 34, 5, 65, 255, 1, 75, 13, 6, 32, 4, 40, 2, 12, 34, 8, 32, 4, 40, 2, 8, 34, 7, 70, 13, 7, 32, 7, 32, 8, 54, 2, 12, 32, 8, 32, 7, 54, 2, 8, 12, 8, 11, 65, 0, 32, 1, 32, 4, 114, 54, 2, 148, 128, 64, 32, 0, 33, 4, 11, 32, 0, 65, 8, 106, 32, 3, 54, 2, 0, 32, 4, 32, 3, 54, 2, 12, 32, 3, 32, 0, 54, 2, 12, 32, 3, 32, 4, 54, 2, 8, 11, 65, 0, 33, 3, 65, 0, 40, 2, 168, 131, 64, 34, 0, 32, 2, 77, 13, 0, 65, 0, 32, 0, 32, 2, 107, 34, 3, 54, 2, 168, 131, 64, 65, 0, 65, 0, 40, 2, 176, 131, 64, 34, 0, 32, 2, 106, 34, 4, 54, 2, 176, 131, 64, 32, 4, 32, 3, 65, 1, 114, 54, 2, 4, 32, 0, 32, 2, 65, 3, 114, 54, 2, 4, 32, 0, 65, 8, 106, 15, 11, 32, 3, 15, 11, 65, 0, 32, 0, 54, 2, 176, 131, 64, 65, 0, 65, 0, 40, 2, 168, 131, 64, 32, 2, 106, 34, 2, 54, 2, 168, 131, 64, 32, 0, 32, 2, 65, 1, 114, 54, 2, 4, 12, 5, 11, 32, 0, 65, 0, 40, 2, 164, 131, 64, 32, 2, 106, 34, 2, 65, 1, 114, 54, 2, 4, 65, 0, 32, 0, 54, 2, 172, 131, 64, 65, 0, 32, 2, 54, 2, 164, 131, 64, 32, 0, 32, 2, 106, 32, 2, 54, 2, 0, 12, 4, 11, 32, 4, 16, 14, 12, 1, 11, 65, 0, 65, 0, 40, 2, 148, 128, 64, 65, 126, 32, 3, 65, 3, 118, 119, 113, 54, 2, 148, 128, 64, 11, 32, 5, 32, 2, 106, 33, 2, 32, 4, 32, 5, 106, 33, 4, 11, 32, 4, 32, 4, 40, 2, 4, 65, 126, 113, 54, 2, 4, 32, 0, 32, 2, 65, 1, 114, 54, 2, 4, 32, 0, 32, 2, 106, 32, 2, 54, 2, 0, 2, 64, 2, 64, 2, 64, 32, 2, 65, 255, 1, 75, 13, 0, 32, 2, 65, 3, 118, 34, 3, 65, 3, 116, 65, 156, 128, 192, 0, 106, 33, 2, 65, 0, 40, 2, 148, 128, 64, 34, 4, 65, 1, 32, 3, 65, 31, 113, 116, 34, 3, 113, 69, 13, 1, 32, 2, 65, 8, 106, 33, 4, 32, 2, 40, 2, 8, 33, 3, 12, 2, 11, 32, 0, 32, 2, 16, 15, 12, 2, 11, 65, 0, 32, 4, 32, 3, 114, 54, 2, 148, 128, 64, 32, 2, 65, 8, 106, 33, 4, 32, 2, 33, 3, 11, 32, 4, 32, 0, 54, 2, 0, 32, 3, 32, 0, 54, 2, 12, 32, 0, 32, 2, 54, 2, 12, 32, 0, 32, 3, 54, 2, 8, 11, 32, 1, 65, 8, 106, 11, 13, 0, 32, 0, 40, 2, 8, 16, 10, 26, 16, 11, 0, 11, 21, 0, 2, 64, 32, 0, 69, 13, 0, 32, 0, 15, 11, 65, 132, 132, 192, 0, 16, 12, 0, 11, 90, 1, 1, 127, 65, 1, 33, 0, 2, 64, 2, 64, 2, 64, 65, 0, 40, 2, 136, 128, 64, 65, 1, 71, 13, 0, 65, 0, 65, 0, 40, 2, 140, 128, 64, 65, 1, 106, 34, 0, 54, 2, 140, 128, 64, 32, 0, 65, 3, 73, 13, 1, 12, 2, 11, 65, 0, 66, 129, 128, 128, 128, 16, 55, 3, 136, 128, 64, 11, 65, 0, 40, 2, 144, 128, 64, 65, 127, 76, 13, 0, 32, 0, 65, 2, 73, 26, 11, 0, 0, 11, 104, 2, 1, 127, 3, 126, 35, 0, 65, 48, 107, 34, 1, 36, 0, 32, 0, 41, 2, 16, 33, 2, 32, 0, 41, 2, 8, 33, 3, 32, 0, 41, 2, 0, 33, 4, 32, 1, 65, 20, 106, 65, 0, 54, 2, 0, 32, 1, 32, 4, 55, 3, 24, 32, 1, 66, 1, 55, 2, 4, 32, 1, 65, 252, 132, 192, 0, 54, 2, 16, 32, 1, 32, 1, 65, 24, 106, 54, 2, 0, 32, 1, 32, 3, 55, 3, 32, 32, 1, 32, 2, 55, 3, 40, 32, 1, 32, 1, 65, 32, 106, 16, 16, 0, 11, 7, 0, 32, 0, 16, 9, 0, 11, 205, 2, 1, 5, 127, 32, 0, 40, 2, 24, 33, 1, 2, 64, 2, 64, 2, 64, 2, 64, 32, 0, 40, 2, 12, 34, 2, 32, 0, 70, 13, 0, 32, 0, 40, 2, 8, 34, 3, 32, 2, 54, 2, 12, 32, 2, 32, 3, 54, 2, 8, 32, 1, 13, 1, 12, 2, 11, 2, 64, 32, 0, 65, 20, 65, 16, 32, 0, 65, 20, 106, 34, 2, 40, 2, 0, 34, 4, 27, 106, 40, 2, 0, 34, 3, 69, 13, 0, 32, 2, 32, 0, 65, 16, 106, 32, 4, 27, 33, 4, 2, 64, 3, 64, 32, 4, 33, 5, 2, 64, 32, 3, 34, 2, 65, 20, 106, 34, 4, 40, 2, 0, 34, 3, 69, 13, 0, 32, 3, 13, 1, 12, 2, 11, 32, 2, 65, 16, 106, 33, 4, 32, 2, 40, 2, 16, 34, 3, 13, 0, 11, 11, 32, 5, 65, 0, 54, 2, 0, 32, 1, 13, 1, 12, 2, 11, 65, 0, 33, 2, 32, 1, 69, 13, 1, 11, 2, 64, 2, 64, 32, 0, 40, 2, 28, 34, 4, 65, 2, 116, 65, 164, 130, 192, 0, 106, 34, 3, 40, 2, 0, 32, 0, 70, 13, 0, 32, 1, 65, 16, 65, 20, 32, 1, 40, 2, 16, 32, 0, 70, 27, 106, 32, 2, 54, 2, 0, 32, 2, 13, 1, 12, 2, 11, 32, 3, 32, 2, 54, 2, 0, 32, 2, 69, 13, 2, 11, 32, 2, 32, 1, 54, 2, 24, 2, 64, 32, 0, 40, 2, 16, 34, 3, 69, 13, 0, 32, 2, 32, 3, 54, 2, 16, 32, 3, 32, 2, 54, 2, 24, 11, 32, 0, 65, 20, 106, 40, 2, 0, 34, 3, 69, 13, 0, 32, 2, 65, 20, 106, 32, 3, 54, 2, 0, 32, 3, 32, 2, 54, 2, 24, 11, 15, 11, 65, 0, 65, 0, 40, 2, 152, 128, 64, 65, 126, 32, 4, 119, 113, 54, 2, 152, 128, 64, 11, 196, 2, 1, 4, 127, 65, 0, 33, 2, 2, 64, 32, 1, 65, 8, 118, 34, 3, 69, 13, 0, 65, 31, 33, 2, 32, 1, 65, 255, 255, 255, 7, 75, 13, 0, 32, 1, 65, 38, 32, 3, 103, 34, 2, 107, 65, 31, 113, 118, 65, 1, 113, 65, 31, 32, 2, 107, 65, 1, 116, 114, 33, 2, 11, 32, 0, 32, 2, 54, 2, 28, 32, 0, 66, 0, 55, 2, 16, 32, 2, 65, 2, 116, 65, 164, 130, 192, 0, 106, 33, 3, 2, 64, 2, 64, 2, 64, 2, 64, 2, 64, 65, 0, 40, 2, 152, 128, 64, 34, 4, 65, 1, 32, 2, 65, 31, 113, 116, 34, 5, 113, 69, 13, 0, 32, 3, 40, 2, 0, 34, 4, 40, 2, 4, 65, 120, 113, 32, 1, 71, 13, 1, 32, 4, 33, 2, 12, 2, 11, 65, 0, 32, 4, 32, 5, 114, 54, 2, 152, 128, 64, 32, 3, 32, 0, 54, 2, 0, 32, 0, 32, 3, 54, 2, 24, 12, 3, 11, 32, 1, 65, 0, 65, 25, 32, 2, 65, 1, 118, 107, 65, 31, 113, 32, 2, 65, 31, 70, 27, 116, 33, 3, 3, 64, 32, 4, 32, 3, 65, 29, 118, 65, 4, 113, 106, 65, 16, 106, 34, 5, 40, 2, 0, 34, 2, 69, 13, 2, 32, 3, 65, 1, 116, 33, 3, 32, 2, 33, 4, 32, 2, 40, 2, 4, 65, 120, 113, 32, 1, 71, 13, 0, 11, 11, 32, 2, 40, 2, 8, 34, 3, 32, 0, 54, 2, 12, 32, 2, 32, 0, 54, 2, 8, 32, 0, 32, 2, 54, 2, 12, 32, 0, 32, 3, 54, 2, 8, 32, 0, 65, 0, 54, 2, 24, 15, 11, 32, 5, 32, 0, 54, 2, 0, 32, 0, 32, 4, 54, 2, 24, 11, 32, 0, 32, 0, 54, 2, 12, 32, 0, 32, 0, 54, 2, 8, 11, 74, 2, 1, 127, 1, 126, 35, 0, 65, 32, 107, 34, 2, 36, 0, 32, 1, 41, 2, 0, 33, 3, 32, 2, 65, 20, 106, 32, 1, 41, 2, 8, 55, 2, 0, 32, 2, 65, 156, 132, 192, 0, 54, 2, 4, 32, 2, 65, 252, 132, 192, 0, 54, 2, 0, 32, 2, 32, 0, 54, 2, 8, 32, 2, 32, 3, 55, 2, 12, 32, 2, 16, 13, 0, 11, 2, 0, 11, 13, 0, 66, 206, 198, 236, 164, 153, 193, 165, 217, 192, 0, 11, 11, 193, 5, 9, 0, 65, 128, 128, 192, 0, 11, 4, 99, 111, 100, 101, 0, 65, 132, 128, 192, 0, 11, 3, 49, 53, 55, 0, 65, 136, 128, 192, 0, 11, 208, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 65, 216, 131, 192, 0, 11, 19, 108, 105, 98, 97, 108, 108, 111, 99, 47, 114, 97, 119, 95, 118, 101, 99, 46, 114, 115, 0, 65, 236, 131, 192, 0, 11, 64, 44, 2, 16, 0, 17, 0, 0, 0, 216, 1, 16, 0, 19, 0, 0, 0, 245, 2, 0, 0, 5, 0, 0, 0, 61, 2, 16, 0, 43, 0, 0, 0, 104, 2, 16, 0, 17, 0, 0, 0, 89, 1, 0, 0, 21, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 65, 172, 132, 192, 0, 11, 17, 99, 97, 112, 97, 99, 105, 116, 121, 32, 111, 118, 101, 114, 102, 108, 111, 119, 0, 65, 189, 132, 192, 0, 11, 43, 99, 97, 108, 108, 101, 100, 32, 96, 79, 112, 116, 105, 111, 110, 58, 58, 117, 110, 119, 114, 97, 112, 40, 41, 96, 32, 111, 110, 32, 97, 32, 96, 78, 111, 110, 101, 96, 32, 118, 97, 108, 117, 101, 0, 65, 232, 132, 192, 0, 11, 17, 108, 105, 98, 99, 111, 114, 101, 47, 111, 112, 116, 105, 111, 110, 46, 114, 115, 0, 65, 252, 132, 192, 0, 11, 0];
        let initial_state = ContractState::new(addr);
        match super::execute(
            &super::create_module(&bytecode).unwrap(),
            100_000,
            initial_state.clone(),
            "call".to_string(),
            "".to_string(),
            Vec::new(),
        ) {
            Ok(v) => {
                let mut after = super::ContractState {
                    contract_address: b"Enigma".sha256(),
                    json: json!({ "code" : 157 }),
                    .. Default::default()
                };
                let delta = super::ContractState::generate_delta_and_update_state(&initial_state, &mut after).unwrap();
                assert_eq!(v.state_delta.unwrap(), delta);
            }
            Err(_) => assert!(true),
        };
    }

}
