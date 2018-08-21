extern crate wasmi;
use std::cell::RefCell;
use std::borrow::ToOwned;

use wasmi::{FuncInstance, Signature, FuncRef, Error, ModuleImportResolver, MemoryInstance, memory_units, MemoryRef, MemoryDescriptor};

pub mod ids {
    pub const EXTERNAL_FUNC: usize = 0;
    pub const RET_FUNC: usize = 1;
}

pub mod signatures {
    use wasmi::{self, ValueType};
    use wasmi::ValueType::*;

    pub struct StaticSignature(pub &'static [ValueType], pub Option<ValueType>);

    pub const EXTERNAL: StaticSignature = StaticSignature(
        &[],
        Some(I32),
    );

    pub const RET: StaticSignature = StaticSignature(
        &[I32, I32],
        None,
    );

    impl Into<wasmi::Signature> for StaticSignature {
        fn into(self) -> wasmi::Signature {
            wasmi::Signature::new(self.0, self.1)
        }
    }
}

/// Import resolver for wasmi
/// Maps all functions that runtime support to the corresponding contract import
/// entries.
/// Also manages initial memory request from the runtime.
#[derive(Default)]
pub struct ImportResolver {
    max_memory: u32,
    memory: RefCell<Option<MemoryRef>>,
}

impl ImportResolver {
    /// New import resolver with specifed maximum amount of inital memory (in wasm pages = 64kb)
    pub fn with_limit(max_memory: u32) -> ImportResolver {
        ImportResolver {
            max_memory: max_memory,
            memory: RefCell::new(None),
        }
    }

    /// Returns memory that was instantiated during the contract module
    /// start. If contract does not use memory at all, the dummy memory of length (0, 0)
    /// will be created instead. So this method always returns memory instance
    /// unless errored.
    pub fn memory_ref(&self) -> MemoryRef {
        {
            let mut mem_ref = self.memory.borrow_mut();
            if mem_ref.is_none() {
                *mem_ref = Some(
                    MemoryInstance::alloc(
                        memory_units::Pages(0),
                        Some(memory_units::Pages(0)),
                    ).expect("Memory allocation (0, 0) should not fail; qed")
                );
            }
        }

        self.memory.borrow().clone().expect("it is either existed or was created as (0, 0) above; qed")
    }

    /// Returns memory size module initially requested
    pub fn memory_size(&self) -> Result<u32, Error> {
        Ok(self.memory_ref().current_size().0 as u32)
    }
}


impl ModuleImportResolver for ImportResolver {
    fn resolve_func(&self, field_name: &str, _signature: &Signature) -> Result<FuncRef, Error> {
        let func_ref = match field_name {
           // "moria" => 	FuncInstance::alloc_host(signatures::EXTERNAL.into(), ids::EXTERNAL_FUNC),
            "ret" => FuncInstance::alloc_host(signatures::RET.into(), ids::RET_FUNC),
            _ => {
                return Err(wasmi::Error::Instantiation(
                    format!("Export {} not found", field_name),
                ))
            }
        };

        Ok(func_ref)
    }

    fn resolve_memory(
        &self,
        field_name: &str,
        descriptor: &MemoryDescriptor,
    ) -> Result<MemoryRef, Error> {
        if field_name == "memory" {
            let effective_max = descriptor.maximum().unwrap_or(self.max_memory + 1);
            if descriptor.initial() > self.max_memory || effective_max > self.max_memory
                {
                    Err(Error::Instantiation("Module requested too much memory".to_owned()))
                } else {
                let mem = MemoryInstance::alloc(
                    memory_units::Pages(descriptor.initial() as usize),
                    descriptor.maximum().map(|x| memory_units::Pages(x as usize)),
                )?;
                *self.memory.borrow_mut() = Some(mem.clone());
                Ok(mem)
            }
        } else {
            Err(Error::Instantiation("Memory imported under unknown name".to_owned()))
        }
    }
}