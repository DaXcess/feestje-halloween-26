#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

mod panic;
mod serial;

use core::fmt;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use embassy_executor::{Spawner, task};
use embassy_time::{Duration, Timer};
use embedded_graphics::Drawable;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::FONT_5X7;
use embedded_graphics::pixelcolor::RgbColor;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Alignment, Text};
use esp_hal::Async;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Pin;
use esp_hal::time::{Instant, Rate};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::usb::usb_serial_jtag::UsbSerialJtag;
use esp_hub75::framebuffer::bitplane::plain::row::DmaFrameBuffer;
use esp_hub75::framebuffer::compute_rows;
use esp_hub75::{Color, Hub75, Hub75Config, Hub75Pins16, hub75_dma_descriptors};
use heapless::String;
use log::{info, warn};
use static_cell::StaticCell;

use crate::serial::usb_driver_task;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write($val);
        x
    }};
}

static RENDER_MS: AtomicU32 = AtomicU32::new(0);
static SIMPLE_COUNTER: AtomicU32 = AtomicU32::new(0);

const ROWS: usize = 32;
const COLS: usize = 64;
const NROWS: usize = compute_rows(ROWS);

const PLANES: usize = 6;

type FBType = DmaFrameBuffer<NROWS, COLS, PLANES>;

const RATE: Rate = Rate::from_mhz(20);

static HUB75_CELL: StaticCell<Hub75<Async, FBType>> = StaticCell::new();
static HUB75: AtomicPtr<Hub75<Async, FBType>> = AtomicPtr::new(null_mut());

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32c6 -o esp32c6-wroom-1 -o unstable-hal

    #[cfg(feature = "log")]
    esp_println::logger::init_logger_from_env();

    info!("Starting firmware");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO4
    // - GPIO5
    // - GPIO8
    // - GPIO9
    // - GPIO15
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO24;
    let _ = peripherals.GPIO25;
    let _ = peripherals.GPIO26;
    let _ = peripherals.GPIO27;
    let _ = peripherals.GPIO28;
    let _ = peripherals.GPIO29;
    let _ = peripherals.GPIO30;

    // Start embassy
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // Initialize USB driver
    let usb = UsbSerialJtag::new(peripherals.USB_DEVICE).into_async();
    spawner.spawn(usb_driver_task(usb).unwrap());

    info!("init framebuffers");
    let fb0 = mk_static!(FBType, FBType::new());
    let fb1 = mk_static!(FBType, FBType::new());

    let tx_descriptors = hub75_dma_descriptors!(FBType);
    info!(
        "DMA descriptors: {} ({} bytes)",
        tx_descriptors.len(),
        core::mem::size_of_val(tx_descriptors)
    );

    let pins = Hub75Pins16 {
        red1: peripherals.GPIO19.degrade(),
        grn1: peripherals.GPIO20.degrade(),
        blu1: peripherals.GPIO21.degrade(),
        red2: peripherals.GPIO22.degrade(),
        grn2: peripherals.GPIO23.degrade(),
        blu2: peripherals.GPIO15.degrade(),
        addr0: peripherals.GPIO10.degrade(),
        addr1: peripherals.GPIO8.degrade(),
        addr2: peripherals.GPIO1.degrade(),
        addr3: peripherals.GPIO0.degrade(),
        addr4: peripherals.GPIO11.degrade(),
        blank: peripherals.GPIO5.degrade(),
        clock: peripherals.GPIO7.degrade(),
        latch: peripherals.GPIO6.degrade(),
    };

    let hub75 = Hub75::new_async(
        peripherals.PARL_IO,
        pins,
        peripherals.DMA_CH0,
        tx_descriptors,
        Hub75Config::new().with_frequency(RATE),
        &*fb0,
    )
    .expect("failed to create Hub75");

    let hub75 = HUB75_CELL.uninit().write(hub75);
    HUB75.store(hub75 as *mut _, Ordering::Release);

    if panic::has_panicked() {
        warn!("Previous execution cycle panicked");

        Timer::after(Duration::from_secs(3)).await;
    }

    spawner.spawn(display_task(hub75, fb1).unwrap());

    loop {
        if SIMPLE_COUNTER.fetch_add(1, Ordering::Relaxed) >= 99999 {
            SIMPLE_COUNTER.store(0, Ordering::Relaxed);
        }
        Timer::after(Duration::from_millis(100)).await;
    }

    // let mut fb = fb1;

    // let text_style = MonoTextStyleBuilder::new()
    //     .font(&FONT_5X7)
    //     .text_color(Color::YELLOW)
    //     .background_color(Color::BLACK)
    //     .build();

    // let mut render_ms = 0;
    // let mut simple_counter = 0;
    // let mut counter_start = Instant::now();

    // let app_start = Instant::now();

    // loop {
    //     let render_start = Instant::now();

    //     fb.erase();

    //     const LINE1: i32 = ROWS as i32 - 1 - 14;
    //     const LINE2: i32 = ROWS as i32 - 1 - 7;
    //     const LINE3: i32 = ROWS as i32 - 1;

    //     let mut buffer: String<64> = String::new();

    //     fmt::write(&mut buffer, format_args!("Refresh: {:4}", REFRESH_RATE)).unwrap();
    //     Text::with_alignment(
    //         buffer.as_str(),
    //         Point::new(0, LINE3),
    //         text_style,
    //         Alignment::Left,
    //     )
    //     .draw(fb)
    //     .unwrap();

    //     buffer.clear();
    //     fmt::write(&mut buffer, format_args!("Render: {:>3}ms", render_ms)).unwrap();
    //     Text::with_alignment(
    //         buffer.as_str(),
    //         Point::new(0, LINE2),
    //         text_style,
    //         Alignment::Left,
    //     )
    //     .draw(fb)
    //     .unwrap();

    //     buffer.clear();
    //     fmt::write(&mut buffer, format_args!("Simple: {:5}", simple_counter)).unwrap();
    //     Text::with_alignment(
    //         buffer.as_str(),
    //         Point::new(0, LINE1),
    //         text_style,
    //         Alignment::Left,
    //     )
    //     .draw(fb)
    //     .unwrap();

    //     if (app_start.elapsed().as_millis() / 500) % 2 == 0 {
    //         for y in 0..8 {
    //             for x in 0..8 {
    //                 const ERROR: [u8; 16] = [
    //                     0b10000000, 0b00010101, 0b00100001, 0b01000000, 0b00001000, 0b00010101,
    //                     0b00010010, 0b01000000, 0b00010001, 0b10000101, 0b01000100, 0b00100000,
    //                     0b01000100, 0b01001000, 0b01000100, 0b01000010,
    //                 ];

    //                 let pidx = y * 8 + x;
    //                 let byte = ERROR[(pidx / 4) as usize];
    //                 let shift = 6 - (pidx % 4) * 2;

    //                 match (byte >> shift) & 0b11 {
    //                     0 => {}
    //                     1 => fb.set_pixel(Point::new(COLS as i32 - 8 - 1 + x, 1 + y), Color::WHITE),
    //                     2 => fb.set_pixel(Point::new(COLS as i32 - 8 - 1 + x, 1 + y), Color::RED),
    //                     _ => {}
    //                 }
    //             }
    //         }
    //     }

    //     render_ms = render_start.elapsed().as_millis();

    //     swap!(hub75, fb);

    //     if counter_start.elapsed() >= Duration::from_millis(100) {
    //         simple_counter += 1;
    //         simple_counter %= 100000;

    //         counter_start = Instant::now();
    //     }
    // }

    // let buffer: String<16> = String::try_from("Blefgh :(").unwrap();

    // draw_frame(1, 2, fb);

    // Text::with_alignment(
    //     buffer.as_str(),
    //     Point::new(32, 16),
    //     text_style,
    //     Alignment::Center,
    // )
    // .draw(fb)
    // .unwrap();

    // swap!(hub75, fb);

    // loop {}
}

#[task]
async fn display_task(hub75: &'static mut Hub75<Async, FBType>, mut fb: &'static mut FBType) {
    info!("display_task: starting!");

    let fps_style = MonoTextStyleBuilder::new()
        .font(&FONT_5X7)
        .text_color(Color::YELLOW)
        .background_color(Color::BLACK)
        .build();

    loop {
        let render_start = Instant::now();

        fb.erase();

        const STEP: u8 = (256 / COLS) as u8;
        const LINE1: i32 = ROWS as i32 - 1 - 14;
        const LINE2: i32 = ROWS as i32 - 1 - 7;
        const LINE3: i32 = ROWS as i32 - 1;
        const NBARS: i32 = NROWS as i32 / 8;

        for x in 0..COLS {
            let brightness = (x as u8) * STEP;
            for y in 0..NBARS {
                fb.set_pixel(Point::new(x as i32, y), Color::new(brightness, 0, 0));
                fb.set_pixel(
                    Point::new(x as i32, y + NBARS),
                    Color::new(0, brightness, 0),
                );
                fb.set_pixel(
                    Point::new(x as i32, y + 2 * NBARS),
                    Color::new(0, 0, brightness),
                );
            }
        }

        let mut buffer: String<64> = String::new();

        buffer.clear();
        fmt::write(
            &mut buffer,
            format_args!("Render: {:>3}ms", RENDER_MS.load(Ordering::Relaxed)),
        )
        .unwrap();
        Text::with_alignment(
            buffer.as_str(),
            Point::new(0, LINE2),
            fps_style,
            Alignment::Left,
        )
        .draw(fb)
        .unwrap();

        buffer.clear();
        fmt::write(
            &mut buffer,
            format_args!("Simple: {:5}", SIMPLE_COUNTER.load(Ordering::Relaxed)),
        )
        .unwrap();
        Text::with_alignment(
            buffer.as_str(),
            Point::new(0, LINE1),
            fps_style,
            Alignment::Left,
        )
        .draw(fb)
        .unwrap();

        // Time taken to draw this frame into the framebuffer (erase, gradient
        // and status text). The value shown above is therefore from the
        // previous frame, which is indistinguishable at panel refresh rates.
        RENDER_MS.store(render_start.elapsed().as_millis() as u32, Ordering::Relaxed);

        let mut xfer = hub75.swap(fb).expect("swap already in flight");
        xfer.wait_for_done().await;
        fb = xfer.wait().expect("DMA transfer failed");
    }
}

fn draw_frame(margin: u32, thickness: u32, fb: &mut FBType) {
    // Top
    fb.fill_solid(
        &Rectangle::new(
            Point::new(margin as _, margin as _),
            Size::new(COLS as u32 - margin * 2, thickness),
        ),
        Color::RED,
    );

    // Bottom
    fb.fill_solid(
        &Rectangle::new(
            Point::new(margin as _, ROWS as i32 - thickness as i32 - margin as i32),
            Size::new(COLS as u32 - margin * 2, thickness),
        ),
        Color::RED,
    );

    // Left
    fb.fill_solid(
        &Rectangle::new(
            Point::new(margin as _, (margin + thickness) as i32),
            Size::new(thickness, ROWS as u32 - margin * 2 - thickness * 2),
        ),
        Color::RED,
    );

    // Right
    fb.fill_solid(
        &Rectangle::new(
            Point::new(
                COLS as i32 - margin as i32 - thickness as i32,
                (margin + thickness) as i32,
            ),
            Size::new(thickness, ROWS as u32 - margin * 2 - thickness * 2),
        ),
        Color::RED,
    );
}
