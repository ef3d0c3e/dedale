//! The sandbox's seccomp filter: a bot may not change its CPU affinity, so it
//! stays on the CPU Dedale pinned it to and cannot take its opponent's.
//!
//! A classic BPF program, as bubblewrap's `--seccomp` loads it:
//! `sched_setaffinity` fails with `EPERM`, and so does every system call made
//! through another architecture's interface (32-bit x86 through `int 0x80`,
//! x32), which would otherwise get around the check. Every other call is
//! allowed.

// Instructions (linux/bpf_common.h): load a word of the call's data, jump
// if equal or greater, return.
const LD_W_ABS: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
const JEQ_K: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
const JGE_K: u16 = 0x35; // BPF_JMP | BPF_JGE | BPF_K
const RET_K: u16 = 0x06; // BPF_RET | BPF_K

// Offsets in `struct seccomp_data`.
const NR: u32 = 0;
const ARCH: u32 = 4;

const RET_ALLOW: u32 = 0x7fff_0000;
const RET_ERRNO: u32 = 0x0005_0000;
const EPERM: u32 = 1;
/// Set in the number of an x32 system call.
const X32_BIT: u32 = 0x4000_0000;

/// What the filter needs to know about an architecture.
#[derive(Debug, Clone, Copy)]
struct Native {
    /// Its `AUDIT_ARCH_*` value.
    audit: u32,
    sched_setaffinity: u32,
    /// Whether x32 calls share its audit value (numbers with `X32_BIT`).
    x32: bool,
}

#[cfg(any(test, target_arch = "x86_64"))]
const X86_64: Native = Native {
    audit: 0xc000_003e,
    sched_setaffinity: 203,
    x32: true,
};

#[cfg(any(test, target_arch = "aarch64"))]
const AARCH64: Native = Native {
    audit: 0xc000_00b7,
    sched_setaffinity: 122,
    x32: false,
};

#[cfg(target_arch = "x86_64")]
const NATIVE: Option<Native> = Some(X86_64);
#[cfg(target_arch = "aarch64")]
const NATIVE: Option<Native> = Some(AARCH64);
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const NATIVE: Option<Native> = None;

/// The filter for the architecture Dedale runs on, as bubblewrap reads it
/// (`struct sock_filter`s, native byte order); `None` where Dedale does not
/// know the architecture's system call numbers.
#[must_use]
pub(crate) fn filter() -> Option<Vec<u8>> {
    NATIVE.map(|native| encode(&instructions(native)))
}

#[derive(Debug, Clone, Copy)]
enum To {
    Next,
    Deny,
}

#[derive(Debug, Clone, Copy)]
struct Instruction {
    code: u16,
    jt: To,
    jf: To,
    k: u32,
}

impl Instruction {
    fn load(offset: u32) -> Self {
        Self {
            code: LD_W_ABS,
            jt: To::Next,
            jf: To::Next,
            k: offset,
        }
    }

    fn jump(code: u16, k: u32, jt: To, jf: To) -> Self {
        Self { code, jt, jf, k }
    }

    fn ret(k: u32) -> Self {
        Self {
            code: RET_K,
            jt: To::Next,
            jf: To::Next,
            k,
        }
    }
}

fn instructions(native: Native) -> Vec<Instruction> {
    let mut program = vec![
        Instruction::load(ARCH),
        Instruction::jump(JEQ_K, native.audit, To::Next, To::Deny),
        Instruction::load(NR),
    ];
    if native.x32 {
        program.push(Instruction::jump(JGE_K, X32_BIT, To::Deny, To::Next));
    }
    program.push(Instruction::jump(
        JEQ_K,
        native.sched_setaffinity,
        To::Deny,
        To::Next,
    ));
    program.push(Instruction::ret(RET_ALLOW));
    program.push(Instruction::ret(RET_ERRNO | EPERM));
    program
}

/// Jumps are relative to the next instruction; `Deny` is the last one.
fn encode(program: &[Instruction]) -> Vec<u8> {
    let deny = program.len() - 1;
    let mut bytes = Vec::with_capacity(program.len() * 8);
    for (i, instruction) in program.iter().enumerate() {
        let offset = |to| match to {
            To::Next => 0,
            To::Deny => u8::try_from(deny - i - 1).expect("a program of a few instructions"),
        };
        bytes.extend(instruction.code.to_ne_bytes());
        bytes.push(offset(instruction.jt));
        bytes.push(offset(instruction.jf));
        bytes.extend(instruction.k.to_ne_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::{
        AARCH64, ARCH, EPERM, JEQ_K, JGE_K, LD_W_ABS, NR, Native, RET_ALLOW, RET_ERRNO, RET_K,
        X32_BIT, X86_64, encode, filter, instructions,
    };
    use proptest::prelude::*;

    const DENIED: u32 = RET_ERRNO | EPERM;

    /// What the kernel decides for a call: the program run on its data.
    fn run(program: &[u8], arch: u32, nr: u32) -> u32 {
        let mut pc = 0;
        let mut a = 0;
        loop {
            let at = &program[pc * 8..pc * 8 + 8];
            let code = u16::from_ne_bytes([at[0], at[1]]);
            let (jt, jf) = (usize::from(at[2]), usize::from(at[3]));
            let k = u32::from_ne_bytes([at[4], at[5], at[6], at[7]]);
            pc += 1;
            match code {
                LD_W_ABS if k == NR => a = nr,
                LD_W_ABS if k == ARCH => a = arch,
                JEQ_K => pc += if a == k { jt } else { jf },
                JGE_K => pc += if a >= k { jt } else { jf },
                RET_K => return k,
                _ => panic!("unexpected instruction {code:#x} {k:#x}"),
            }
        }
    }

    fn program(native: Native) -> Vec<u8> {
        encode(&instructions(native))
    }

    #[test]
    fn sched_setaffinity_is_refused() {
        assert_eq!(run(&program(X86_64), X86_64.audit, 203), DENIED);
        assert_eq!(run(&program(AARCH64), AARCH64.audit, 122), DENIED);
        // getpid, write, clone.
        for nr in [39, 1, 56] {
            assert_eq!(run(&program(X86_64), X86_64.audit, nr), RET_ALLOW);
        }
        assert_eq!(run(&program(AARCH64), AARCH64.audit, 172), RET_ALLOW);
    }

    #[test]
    fn other_interfaces_are_refused() {
        // 32-bit x86 (AUDIT_ARCH_I386) and 32-bit Arm, whatever the call.
        assert_eq!(run(&program(X86_64), 0x4000_0003, 20), DENIED);
        assert_eq!(run(&program(AARCH64), 0x4000_0028, 20), DENIED);
        // x32's sched_setaffinity.
        assert_eq!(
            run(
                &program(X86_64),
                X86_64.audit,
                X32_BIT | X86_64.sched_setaffinity
            ),
            DENIED
        );
    }

    #[test]
    fn the_filter_fits_this_machine() {
        if let Some(bytes) = filter() {
            assert_eq!(bytes.len() % 8, 0);
            assert!(bytes.len() / 8 <= 7);
        }
    }

    proptest! {
        #[test]
        fn x86_64_allows_every_other_call(nr in 0..X32_BIT) {
            let program = program(X86_64);
            let expected = if nr == 203 { DENIED } else { RET_ALLOW };
            prop_assert_eq!(run(&program, X86_64.audit, nr), expected);
            prop_assert_eq!(run(&program, X86_64.audit, nr | X32_BIT), DENIED);
        }

        #[test]
        fn aarch64_allows_every_other_call(nr: u32) {
            let expected = if nr == 122 { DENIED } else { RET_ALLOW };
            prop_assert_eq!(run(&program(AARCH64), AARCH64.audit, nr), expected);
        }

        #[test]
        fn other_architectures_are_refused(arch: u32, nr: u32) {
            prop_assume!(arch != X86_64.audit && arch != AARCH64.audit);
            prop_assert_eq!(run(&program(X86_64), arch, nr), DENIED);
            prop_assert_eq!(run(&program(AARCH64), arch, nr), DENIED);
        }
    }
}
