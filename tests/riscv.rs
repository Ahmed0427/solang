// SPDX-License-Identifier: Apache-2.0

//! Checks the object files the riscv target makes. Skipped without an `llc`
//! that has the RISC-V backend.

use solang::codegen::{OptimizationLevel, Options};
use solang::file_resolver::FileResolver;
use solang::{compile, Target};
use std::collections::BTreeSet;
use std::ffi::OsStr;

fn have_llc() -> bool {
    match solang::emit::riscv::codegen::find_llc() {
        Ok(_) => true,
        Err(err) => {
            eprintln!("skipping riscv test: {err}");
            false
        }
    }
}

fn compile_incrementer() -> Vec<u8> {
    let mut resolver = FileResolver::default();
    resolver.add_import_path(std::path::Path::new("tests/contract_testcases/riscv"));
    let opt = Options {
        opt_level: OptimizationLevel::Default,
        ..Default::default()
    };
    let (outputs, ns) = compile(
        OsStr::new("incrementer.sol"),
        &mut resolver,
        Target::Riscv,
        &opt,
        vec![],
        "0.0.1",
    );
    assert!(!ns.diagnostics.any_errors());
    outputs.into_iter().next().unwrap().0
}

fn u16_at(elf: &[u8], at: usize) -> usize {
    u16::from_le_bytes(elf[at..at + 2].try_into().unwrap()) as usize
}

fn u32_at(elf: &[u8], at: usize) -> usize {
    u32::from_le_bytes(elf[at..at + 4].try_into().unwrap()) as usize
}

fn u64_at(elf: &[u8], at: usize) -> usize {
    u64::from_le_bytes(elf[at..at + 8].try_into().unwrap()) as usize
}

fn c_str(elf: &[u8], at: usize) -> String {
    let end = elf[at..].iter().position(|b| *b == 0).unwrap();
    String::from_utf8(elf[at..at + end].to_vec()).unwrap()
}

/// The global symbols of an ELF64 object: (defined, undefined).
fn global_symbols(elf: &[u8]) -> (BTreeSet<String>, BTreeSet<String>) {
    let section = |i: usize| u64_at(elf, 0x28) + i * u16_at(elf, 0x3a);
    let symtab = (0..u16_at(elf, 0x3c))
        .map(section)
        .find(|sh| u32_at(elf, sh + 4) == 2)
        .expect("symbol table");
    let strtab = u64_at(elf, section(u32_at(elf, symtab + 0x28)) + 0x18);

    let (mut defined, mut undefined) = (BTreeSet::new(), BTreeSet::new());
    let start = u64_at(elf, symtab + 0x18);
    for sym in (start..start + u64_at(elf, symtab + 0x20)).step_by(24) {
        if elf[sym + 4] >> 4 != 1 {
            continue;
        }
        let name = c_str(elf, strtab + u32_at(elf, sym));
        if u16_at(elf, sym + 6) == 0 {
            undefined.insert(name);
        } else {
            defined.insert(name);
        }
    }
    (defined, undefined)
}

#[test]
fn object_is_rv64im() {
    if !have_llc() {
        return;
    }
    let elf = compile_incrementer();

    assert_eq!(&elf[..4], b"\x7fELF");
    assert_eq!(elf[4], 2, "64-bit");
    assert_eq!(u16_at(&elf, 0x10), 1, "relocatable object");
    assert_eq!(u16_at(&elf, 0x12), 243, "RISC-V");
    assert_eq!(u32_at(&elf, 0x30), 0, "e_flags: no compressed, soft-float");

    // .riscv.attributes names the extensions the code uses.
    let at = elf.windows(5).position(|w| w == b"rv64i").expect("arch");
    let arch = c_str(&elf, at);
    assert!(arch.starts_with("rv64i2p1_m2p0"), "{arch}");
    assert!(!arch.contains("_a") && !arch.contains("_c"), "{arch}");
}

#[test]
fn object_exports_entry_points_and_imports_syscalls() {
    if !have_llc() {
        return;
    }
    let (defined, undefined) = global_symbols(&compile_incrementer());

    let set = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(
        defined,
        set(&[
            "__ashlti3",
            "__ashrti3",
            "__lshrti3",
            "solang_call",
            "solang_deploy"
        ])
    );
    assert_eq!(
        undefined,
        set(&[
            "__sys_return",
            "__sys_revert",
            "__sys_sload",
            "__sys_sstore"
        ])
    );
}
