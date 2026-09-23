//! The functions the Retium runtime gives a contract.
//!
//! These are the only things a contract can do that reach outside its own
//! memory. Everything else in this crate is a safe wrapper around them.
//!
//! On a non-wasm target (your unit tests, `cargo doc`) the imports are replaced
//! by stubs that report "no chain here", so a handler can be tested without a
//! node.

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    pub fn write_state(key_ptr: *const u8, key_len: i32, val_ptr: *const u8, val_len: i32);
    pub fn read_state(key_ptr: *const u8, key_len: i32, out_ptr: *mut u8, out_max_len: i32) -> i32;
    pub fn emit_event(
        type_ptr: *const u8,
        type_len: i32,
        data_ptr: *const u8,
        data_len: i32,
    ) -> i32;
    pub fn list_keys(
        prefix_ptr: *const u8,
        prefix_len: i32,
        out_ptr: *mut u8,
        out_max_len: i32,
    ) -> i32;
    pub fn get_self_contract_id(out_ptr: *mut u8, out_max_len: i32) -> i32;
    pub fn get_block_height() -> i64;
    pub fn get_block_time() -> i64;
    pub fn sha256(in_ptr: *const u8, in_len: i32, out_ptr: *mut u8) -> i32;
    pub fn sig_verify(
        msg_ptr: *const u8,
        msg_len: i32,
        sig_ptr: *const u8,
        sig_len: i32,
        pk_ptr: *const u8,
        pk_len: i32,
    ) -> i32;
    pub fn call_contract(
        id_ptr: *const u8,
        id_len: i32,
        method_ptr: *const u8,
        method_len: i32,
        args_ptr: *const u8,
        args_len: i32,
        out_ptr: *mut u8,
        out_max_len: i32,
    ) -> i32;
    pub fn call_contract_value(
        id_ptr: *const u8,
        id_len: i32,
        method_ptr: *const u8,
        method_len: i32,
        args_ptr: *const u8,
        args_len: i32,
        amount_ptr: *const u8,
        amount_len: i32,
        out_ptr: *mut u8,
        out_max_len: i32,
    ) -> i32;
    pub fn get_contract_code_hash(
        id_ptr: *const u8,
        id_len: i32,
        out_ptr: *mut u8,
        out_max_len: i32,
    ) -> i32;
    pub fn get_contract_version(
        id_ptr: *const u8,
        id_len: i32,
        out_ptr: *mut u8,
        out_max_len: i32,
    ) -> i32;
    pub fn set_contract_version(ver_ptr: *const u8, ver_len: i32) -> i32;
    pub fn supports_capability(
        id_ptr: *const u8,
        id_len: i32,
        cap_ptr: *const u8,
        cap_len: i32,
    ) -> i32;
}

// ── Off-chain stubs, so handlers can be unit-tested and documented ──────────

#[cfg(not(target_arch = "wasm32"))]
mod offchain {
    pub unsafe fn write_state(_: *const u8, _: i32, _: *const u8, _: i32) {}
    pub unsafe fn read_state(_: *const u8, _: i32, _: *mut u8, _: i32) -> i32 {
        0
    }
    pub unsafe fn emit_event(_: *const u8, _: i32, _: *const u8, _: i32) -> i32 {
        0
    }
    pub unsafe fn list_keys(_: *const u8, _: i32, _: *mut u8, _: i32) -> i32 {
        0
    }
    pub unsafe fn get_self_contract_id(_: *mut u8, _: i32) -> i32 {
        -1
    }
    pub unsafe fn get_block_height() -> i64 {
        0
    }
    pub unsafe fn get_block_time() -> i64 {
        0
    }
    pub unsafe fn sha256(_: *const u8, _: i32, _: *mut u8) -> i32 {
        -1
    }
    pub unsafe fn sig_verify(
        _: *const u8,
        _: i32,
        _: *const u8,
        _: i32,
        _: *const u8,
        _: i32,
    ) -> i32 {
        -1
    }
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn call_contract(
        _: *const u8,
        _: i32,
        _: *const u8,
        _: i32,
        _: *const u8,
        _: i32,
        _: *mut u8,
        _: i32,
    ) -> i32 {
        -1
    }
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn call_contract_value(
        _: *const u8,
        _: i32,
        _: *const u8,
        _: i32,
        _: *const u8,
        _: i32,
        _: *const u8,
        _: i32,
        _: *mut u8,
        _: i32,
    ) -> i32 {
        -1
    }
    pub unsafe fn get_contract_code_hash(_: *const u8, _: i32, _: *mut u8, _: i32) -> i32 {
        -1
    }
    pub unsafe fn get_contract_version(_: *const u8, _: i32, _: *mut u8, _: i32) -> i32 {
        -1
    }
    pub unsafe fn set_contract_version(_: *const u8, _: i32) -> i32 {
        -1
    }
    pub unsafe fn supports_capability(_: *const u8, _: i32, _: *const u8, _: i32) -> i32 {
        -1
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use offchain::*;
