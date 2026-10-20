// SPDX-License-Identifier: Apache-2.0

//! For now this targets SP1: a contract becomes an object file for an SP1
//! guest program to link. It exports two functions, each taking the calldata
//! pointer and length:
//!
//! * `solang_deploy`: runs the storage initializers and the constructor;
//! * `solang_call`: runs the public function the selector names.
//!
//! Both end by calling `__sys_return` or `__sys_revert`, and storage goes
//! through `__sys_sload` and `__sys_sstore`. The guest provides all four.

use crate::codegen::targets::riscv::dispatch::DispatchType;
use crate::codegen::{Options, STORAGE_INITIALIZER};
use crate::emit::binary::Binary;
use crate::emit::functions::emit_functions;
use crate::emit::riscv::target::RiscvTargetRuntime;
use crate::sema::ast::{Contract, Namespace};
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::values::{BasicMetadataValueEnum, FunctionValue};
use inkwell::AddressSpace;

pub mod codegen;
mod target;

pub struct RiscvTarget;

impl RiscvTarget {
    pub fn build<'a>(
        context: &'a Context,
        std_lib: &Module<'a>,
        contract: &'a Contract,
        ns: &'a Namespace,
        opt: &'a Options,
    ) -> Binary<'a> {
        let filename = ns.files[contract.loc.file_no()].file_name();
        let mut bin = Binary::new(
            context,
            ns,
            &contract.id.name,
            filename.as_str(),
            opt,
            std_lib,
            None,
        );

        target::declare_syscalls(&mut bin);
        emit_functions(&mut RiscvTargetRuntime, &mut bin, contract);

        Self::emit_deploy(&bin);
        Self::emit_call(&bin);

        Self::internalize(&bin);

        bin
    }

    /// Hide everything but the entry points from the guest it is linked into.
    /// (`Binary::internalize` hides nothing once there are two exports.)
    fn internalize(bin: &Binary) {
        const EXPORTS: [&str; 2] = ["solang_deploy", "solang_call"];
        // LLVM emits calls to these by name, so they must stay visible. Weak,
        // so that several contracts can be linked into one program.
        const HELPERS: [&str; 3] = ["__ashlti3", "__lshrti3", "__ashrti3"];

        let mut func = bin.module.get_first_function();
        while let Some(f) = func {
            let name = f.get_name().to_str().unwrap();
            if f.count_basic_blocks() > 0 {
                if HELPERS.contains(&name) {
                    f.set_linkage(Linkage::WeakODR);
                } else if !EXPORTS.contains(&name) {
                    f.set_linkage(Linkage::Internal);
                }
            }
            func = f.get_next_function();
        }

        let mut global = bin.module.get_first_global();
        while let Some(g) = global {
            if g.get_initializer().is_some() {
                g.set_linkage(Linkage::Internal);
            }
            global = g.get_next_global();
        }
    }

    fn emit_deploy(bin: &Binary) {
        let func = Self::begin_entry(bin, "solang_deploy");

        let storage_initializer = bin
            .functions
            .values()
            .find(|f| f.get_name().to_bytes() == STORAGE_INITIALIZER.as_bytes())
            .expect("storage initializer is always present");
        bin.builder
            .build_call(*storage_initializer, &[], "")
            .unwrap();

        Self::call_dispatch(bin, func, DispatchType::Deploy);

        let args: Vec<BasicMetadataValueEnum> = vec![
            bin.context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
            bin.context.i64_type().const_zero().into(),
        ];
        bin.builder
            .build_call(bin.module.get_function("__sys_return").unwrap(), &args, "")
            .unwrap();
        bin.builder.build_unreachable().unwrap();
    }

    /// The call dispatcher always ends with `__sys_return` or `__sys_revert`.
    fn emit_call(bin: &Binary) {
        let func = Self::begin_entry(bin, "solang_call");
        Self::call_dispatch(bin, func, DispatchType::Call);
        bin.builder.build_unreachable().unwrap();
    }

    fn begin_entry<'a>(bin: &Binary<'a>, name: &str) -> FunctionValue<'a> {
        let context = bin.context;
        let func = bin.module.add_function(
            name,
            context.void_type().fn_type(
                &[
                    context.ptr_type(AddressSpace::default()).into(),
                    context.i32_type().into(),
                ],
                false,
            ),
            None,
        );

        bin.builder
            .position_at_end(context.append_basic_block(func, "entry"));
        bin.builder
            .build_call(bin.module.get_function("__init_heap").unwrap(), &[], "")
            .unwrap();

        func
    }

    fn call_dispatch<'a>(bin: &Binary<'a>, entry: FunctionValue<'a>, ty: DispatchType) {
        let dispatch = bin
            .module
            .get_function(&ty.to_string())
            .expect("both dispatchers are emitted for every contract");
        bin.builder
            .build_call(
                dispatch,
                &[
                    entry.get_nth_param(0).unwrap().into(),
                    entry.get_nth_param(1).unwrap().into(),
                ],
                "",
            )
            .unwrap();
    }
}
