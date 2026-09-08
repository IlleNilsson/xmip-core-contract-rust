//! The Rust content contract — a technology of `xmip-core-contract`, as a
//! loadable module rather than a crate the runtime links.
//!
//! ADR-0042 decision 3: a contract may be authored in any declared language
//! over the C ABI, and this is the Rust one — the same entrypoint, contract
//! table and lifecycle as the C module, through the binding crate's mirrors of
//! the header (ADR-0012 clause 10). The Rust technologies beside it, `csv` and
//! the rest, implement the `Contract` trait and are linked by the runtime;
//! this one is loaded, which is the difference between a module the estate
//! ships and a module a third party ships (ADR-0023).
//!
//! What it claims: well-formedness is bytes — a Stream is read to its end and
//! held (ADR-0042 decision 1). A descriptor a Location binds is kept and
//! answered by `implies` under `descriptor`. A Rust module with a real
//! standard replaces [`judge`] and nothing else.

use core::ffi::c_void;
use xmip_core_abi::XMIP_ABI_VERSION;
use xmip_core_abi::ffi::{
    ContractVtable, Diagnostic, Host, Module, Reader, Str, VtableHeader, WireDescriptor, status,
};

/// One bound contract: the descriptor the Location named.
pub struct Contract {
    pub descriptor: Vec<u8>,
}

/// The module's state. Diagnostics are borrowed until the next call, so the
/// message lives here.
pub struct State {
    diagnostic: Diagnostic,
    message: String,
    error: String,
}

/// Judge a whole stream against a contract. `None` holds; `Some(why)`
/// refuses with that message. The identity contract holds everything.
#[must_use]
pub fn judge(_contract: &Contract, _bytes: &[u8]) -> Option<String> {
    None
}

fn str_of(text: &str) -> Str {
    Str {
        ptr: text.as_ptr(),
        len: text.len(),
    }
}

unsafe extern "C" fn configure(_state: *mut c_void, _toml: Str) -> i32 {
    status::OK
}
unsafe extern "C" fn start(_state: *mut c_void) -> i32 {
    status::OK
}
unsafe extern "C" fn stop(_state: *mut c_void) -> i32 {
    status::OK
}

unsafe extern "C" fn load(_state: *mut c_void, descriptor: Str, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return status::INVALID;
    }
    let bytes = if descriptor.len == 0 || descriptor.ptr.is_null() {
        Vec::new()
    } else {
        // SAFETY: the header guarantees ptr..ptr+len for the call's duration.
        unsafe { core::slice::from_raw_parts(descriptor.ptr, descriptor.len) }.to_vec()
    };
    let contract = Box::new(Contract { descriptor: bytes });
    // SAFETY: out is non-null, checked above.
    unsafe { *out = Box::into_raw(contract).cast() };
    status::OK
}

unsafe extern "C" fn release(_state: *mut c_void, contract: *mut c_void) {
    if !contract.is_null() {
        // SAFETY: only `load` produces these pointers, from Box::into_raw.
        drop(unsafe { Box::from_raw(contract.cast::<Contract>()) });
    }
}

unsafe extern "C" fn validate(
    state: *mut c_void,
    contract: *mut c_void,
    input: *const Reader,
    out: *mut *const Diagnostic,
    out_len: *mut usize,
) -> i32 {
    if state.is_null()
        || contract.is_null()
        || input.is_null()
        || out.is_null()
        || out_len.is_null()
    {
        return status::INVALID;
    }
    // SAFETY: every pointer was checked non-null; the header guarantees the
    // referents live for the call, and state/contract are ours.
    let (state, contract, reader) = unsafe {
        (
            &mut *state.cast::<State>(),
            &*contract.cast::<Contract>(),
            &*input,
        )
    };
    let Some(read) = reader.read else {
        return status::INVALID;
    };
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        // SAFETY: chunk is a valid, writable buffer of the given length.
        let got = unsafe { read(reader.ctx, chunk.as_mut_ptr(), chunk.len()) };
        if got < 0 {
            return i32::try_from(got).unwrap_or(status::IO);
        }
        if got == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..usize::try_from(got).unwrap_or(0)]);
    }
    // SAFETY: out and out_len are non-null.
    unsafe {
        *out = core::ptr::null();
        *out_len = 0;
    }
    match judge(contract, &bytes) {
        None => status::OK,
        Some(why) => {
            state.message = why;
            state.diagnostic = Diagnostic {
                code: status::CONTRACT,
                message: str_of(&state.message),
                location: Str::empty(),
                offset: u64::MAX,
            };
            // SAFETY: as above.
            unsafe {
                *out = &raw const state.diagnostic;
                *out_len = 1;
            }
            status::CONTRACT
        }
    }
}

unsafe extern "C" fn implies(
    _state: *mut c_void,
    contract: *mut c_void,
    key: Str,
    out: *mut Str,
) -> i32 {
    if contract.is_null() || out.is_null() || (key.ptr.is_null() && key.len > 0) {
        return status::INVALID;
    }
    // SAFETY: checked non-null; contract is ours.
    let (contract, key) = unsafe {
        (
            &*contract.cast::<Contract>(),
            core::slice::from_raw_parts(key.ptr, key.len),
        )
    };
    if key == b"descriptor" && !contract.descriptor.is_empty() {
        // SAFETY: out is non-null.
        unsafe {
            *out = Str {
                ptr: contract.descriptor.as_ptr(),
                len: contract.descriptor.len(),
            };
        }
        return status::OK;
    }
    status::NOT_FOUND
}

unsafe extern "C" fn last_error(state: *mut c_void) -> Str {
    if state.is_null() {
        return Str::empty();
    }
    // SAFETY: state is ours.
    str_of(&unsafe { &*state.cast::<State>() }.error)
}

unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        // SAFETY: produced by Box::into_raw in the entrypoint.
        drop(unsafe { Box::from_raw(state.cast::<State>()) });
    }
}

static VTABLE: ContractVtable = ContractVtable {
    header: VtableHeader {
        trait_major: 1,
        trait_minor: 0,
        configure: Some(configure),
        start: Some(start),
        stop: Some(stop),
    },
    load: Some(load),
    release: Some(release),
    validate: Some(validate),
    implies: Some(implies),
};

/// The one exported symbol, named by `XMIP_ENTRYPOINT`.
///
/// # Safety
/// Called by a host across the C boundary; `host` and `out` must be valid for
/// the call. A null pointer or a foreign version is refused, never
/// dereferenced into.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xmip_create_module_v1(host: *const Host, out: *mut Module) -> i32 {
    if host.is_null() || out.is_null() {
        return status::INVALID;
    }
    // SAFETY: host is non-null and valid for the call.
    if unsafe { (*host).abi_version } != XMIP_ABI_VERSION {
        return status::UNSUPPORTED;
    }
    let state = Box::new(State {
        diagnostic: Diagnostic {
            code: status::CONTRACT,
            message: Str::empty(),
            location: Str::empty(),
            offset: u64::MAX,
        },
        message: String::new(),
        error: String::new(),
    });
    // SAFETY: out is non-null and valid for the call.
    unsafe {
        *out = Module {
            descriptor: WireDescriptor {
                abi_version: XMIP_ABI_VERSION,
                provider: Str::from_static("core"),
                module: Str::from_static("contract"),
                standard: Str::from_static("rust"),
                trait_major: 1,
                trait_minor: 0,
                module_major: 0,
                module_minor: 1,
                module_patch: 0,
            },
            state: Box::into_raw(state).cast(),
            vtable: (&raw const VTABLE).cast(),
            last_error: Some(last_error),
            destroy: Some(destroy),
        };
    }
    status::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Source {
        bytes: &'static [u8],
        at: usize,
    }

    unsafe extern "C" fn read_source(ctx: *mut c_void, buf: *mut u8, len: usize) -> i64 {
        // SAFETY: the test owns ctx and buf.
        let source = unsafe { &mut *ctx.cast::<Source>() };
        let n = (source.bytes.len() - source.at).min(len).min(3);
        unsafe { core::ptr::copy_nonoverlapping(source.bytes[source.at..].as_ptr(), buf, n) };
        source.at += n;
        i64::try_from(n).unwrap_or(0)
    }

    fn host(version: u32) -> Host {
        Host {
            abi_version: version,
            ctx: core::ptr::null_mut(),
            log: None,
            cancelled: None,
            journey_id: None,
        }
    }

    fn blank() -> Module {
        Module {
            descriptor: WireDescriptor {
                abi_version: 0,
                provider: Str::empty(),
                module: Str::empty(),
                standard: Str::empty(),
                trait_major: 0,
                trait_minor: 0,
                module_major: 0,
                module_minor: 0,
                module_patch: 0,
            },
            state: core::ptr::null_mut(),
            vtable: core::ptr::null(),
            last_error: None,
            destroy: None,
        }
    }

    #[test]
    fn a_foreign_version_is_refused_and_out_left_untouched() {
        let mut module = blank();
        let foreign = host(99);
        assert_eq!(
            unsafe { xmip_create_module_v1(&raw const foreign, &raw mut module) },
            status::UNSUPPORTED
        );
        assert!(module.vtable.is_null());
    }

    #[test]
    fn the_module_drives_through_the_contract_table() {
        let mut module = blank();
        let ok = host(XMIP_ABI_VERSION);
        assert_eq!(
            unsafe { xmip_create_module_v1(&raw const ok, &raw mut module) },
            status::OK
        );
        let standard = unsafe {
            core::slice::from_raw_parts(
                module.descriptor.standard.ptr,
                module.descriptor.standard.len,
            )
        };
        assert_eq!(standard, b"rust");
        let table = unsafe { &*module.vtable.cast::<ContractVtable>() };
        let mut contract: *mut c_void = core::ptr::null_mut();
        let descriptor = Str::from_static("any");
        assert_eq!(
            unsafe { table.load.expect("load")(module.state, descriptor, &raw mut contract) },
            status::OK
        );
        let mut source = Source {
            bytes: b"xmip ping-pong",
            at: 0,
        };
        let reader = Reader {
            ctx: (&raw mut source).cast(),
            read: Some(read_source),
        };
        let mut out: *const Diagnostic = core::ptr::null();
        let mut out_len = 7usize;
        let verdict = unsafe {
            table.validate.expect("validate")(
                module.state,
                contract,
                &raw const reader,
                &raw mut out,
                &raw mut out_len,
            )
        };
        assert_eq!(verdict, status::OK);
        assert_eq!(out_len, 0);
        assert_eq!(
            source.at,
            source.bytes.len(),
            "read to the end in short reads"
        );
        let mut implied = Str::empty();
        assert_eq!(
            unsafe {
                table.implies.expect("implies")(
                    module.state,
                    contract,
                    Str::from_static("descriptor"),
                    &raw mut implied,
                )
            },
            status::OK
        );
        assert_eq!(
            unsafe { core::slice::from_raw_parts(implied.ptr, implied.len) },
            b"any"
        );
        assert_eq!(
            unsafe {
                table.implies.expect("implies")(
                    module.state,
                    contract,
                    Str::from_static("nothing"),
                    &raw mut implied,
                )
            },
            status::NOT_FOUND
        );
        unsafe { table.release.expect("release")(module.state, contract) };
        assert_eq!(
            unsafe { module.last_error.expect("last_error")(module.state) }.len,
            0
        );
        unsafe { module.destroy.expect("destroy")(module.state) };
    }
}
