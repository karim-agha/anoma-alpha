#![cfg_attr(target_family = "wasm", no_std)]

#[cfg(target_family = "wasm")]
#[panic_handler]
pub unsafe fn panic(_info: &core::panic::PanicInfo) -> ! {
  syscall_terminate();
  loop {}
}

#[cfg(target_family = "wasm")]
extern "C" {
  pub fn syscall_terminate();
  pub fn syscall_read_account(_: u32) -> u32;
}

#[cfg(not(target_family = "wasm"))]
mod build;

#[cfg(not(target_family = "wasm"))]
pub use build::configure_build;
