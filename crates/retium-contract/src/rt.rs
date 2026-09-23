//! Runtime bits a wasm contract needs: an allocator and a panic handler.
//!
//! The panic handler traps immediately. That is deliberate: a trap rolls the
//! whole call back, while a handler that loops would burn the caller's fuel to
//! the last drop before failing anyway.

#[global_allocator]
static ALLOCATOR: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
