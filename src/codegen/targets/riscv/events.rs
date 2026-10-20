// SPDX-License-Identifier: Apache-2.0

use crate::codegen::cfg::{ControlFlowGraph, Instr};
use crate::codegen::encoding::abi_encode;
use crate::codegen::expression::expression;
use crate::codegen::interface::{EventEmitter, TargetCodegen};
use crate::codegen::vartable::Vartable;
use crate::codegen::{Expression, Options};
use crate::sema::ast::{self, Function, Namespace, RetrieveType, Type};
use solang_parser::pt;
use tiny_keccak::{Hasher, Keccak};

/// Events as on Ethereum: topic 0 is the keccak256 of the signature (unless
/// the event is anonymous), each indexed field is a topic holding its ABI
/// word, and the data is the ABI encoding of the other fields.
pub(crate) struct RiscvEventEmitter<'a> {
    pub(crate) args: &'a [ast::Expression],
    pub(crate) ns: &'a Namespace,
    pub(crate) event_no: usize,
}

impl EventEmitter for RiscvEventEmitter<'_> {
    fn selector(&self, _emitting_contract_no: usize) -> Vec<u8> {
        let mut hash = [0; 32];
        let mut keccak = Keccak::v256();
        keccak.update(self.ns.events[self.event_no].signature.as_bytes());
        keccak.finalize(&mut hash);
        hash.to_vec()
    }

    fn emit(
        &self,
        contract_no: usize,
        func: &Function,
        cfg: &mut ControlFlowGraph,
        vartab: &mut Vartable,
        opt: &Options,
        target: &dyn TargetCodegen,
    ) {
        let loc = pt::Loc::Builtin;
        let event = &self.ns.events[self.event_no];
        let (mut data, mut topics) = (Vec::new(), Vec::new());

        if !event.anonymous {
            topics.push(Expression::AllocDynamicBytes {
                loc,
                ty: Type::DynamicBytes,
                size: Expression::NumberLiteral {
                    loc,
                    ty: Type::Uint(32),
                    value: 32.into(),
                }
                .into(),
                initializer: Some(self.selector(contract_no)),
            });
        }

        // Arguments are evaluated in order, whether they become topics or data.
        for (arg, field) in self.args.iter().zip(event.fields.iter()) {
            let value = expression(
                arg,
                cfg,
                contract_no,
                Some(func),
                self.ns,
                vartab,
                opt,
                target,
            );
            if !field.indexed {
                data.push(value);
                continue;
            }
            if value.ty().is_reference_type(self.ns) {
                unimplemented!(
                    "the riscv target cannot index an event field of type `{}` yet",
                    value.ty().to_string(self.ns)
                );
            }
            topics.push(abi_encode(&loc, vec![value], self.ns, vartab, cfg, false).0);
        }

        let data = if data.is_empty() {
            Expression::AllocDynamicBytes {
                loc,
                ty: Type::DynamicBytes,
                size: Expression::NumberLiteral {
                    loc,
                    ty: Type::Uint(32),
                    value: 0.into(),
                }
                .into(),
                initializer: None,
            }
        } else {
            abi_encode(&loc, data, self.ns, vartab, cfg, false).0
        };

        cfg.add(
            vartab,
            Instr::EmitEvent {
                event_no: self.event_no,
                data,
                topics,
            },
        );
    }
}
