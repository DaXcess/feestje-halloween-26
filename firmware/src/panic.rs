use core::panic::PanicInfo;

use esp_hal::ram;
use log::error;

const PANIC_MAGIC: u32 = 0x50414E49;

#[ram(unstable(rtc_fast, persistent))]
static mut RESET_MARKER: u32 = 0;

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    error!("Panic occured, resetting board: {info:?}");

    unsafe {
        RESET_MARKER = PANIC_MAGIC;
    }

    esp_hal::system::software_reset()
}

pub fn has_panicked() -> bool {
    unsafe {
        let panicked = RESET_MARKER == PANIC_MAGIC;
        RESET_MARKER = 0;

        panicked
    }
}
