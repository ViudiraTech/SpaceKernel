/*
 *
 *       user/desktop/src/xprobe.rs
 *       Standalone X11 connection, mapping and image upload diagnostic
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Minimal smoke probe that isolates X client setup and PutImage from the WM.

use crate::draw::{Canvas, Rect};
use crate::x11::{Result, X};
use std::thread::sleep;
use std::time::Duration;
use x11rb::protocol::xproto::EventMask;

pub fn run() -> Result<()> {
    let x = X::connect()?;
    eprintln!(
        "xprobe: connected screen={} size={}x{} depth={}",
        x.screen, x.width, x.height, x.depth
    );
    let window = x.generate_id()?;
    x.create_window(
        window,
        x.root,
        0,
        0,
        x.width,
        x.height,
        true,
        EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY,
        0x336699,
    )?;
    x.set_title(window, "SPUI X11 probe")?;
    x.map(window)?;
    x.flush()?;
    eprintln!("xprobe: mapped window={window:#x}");
    let canvas = Canvas::new(x.width as usize, x.height as usize, 0x336699);
    x.put_canvas(
        window,
        &canvas,
        Rect::new(0, 0, x.width as i32, x.height as i32),
    )?;
    x.flush()?;
    eprintln!("xprobe: uploaded {} pixels", canvas.width * canvas.height);

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        while let Some(event) = x.poll()? {
            if let x11rb::protocol::Event::Expose(expose) = event {
                if expose.window == window && expose.count == 0 {
                    x.put_canvas(
                        window,
                        &canvas,
                        Rect::new(0, 0, x.width as i32, x.height as i32),
                    )?;
                    x.flush()?;
                }
            }
        }
        sleep(Duration::from_millis(20));
    }
    x.destroy(window)?;
    x.flush()?;
    Ok(())
}
