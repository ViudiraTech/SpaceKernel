/*
 *
 *       user/desktop/src/apps/settings.rs
 *       SPUI 4.0 Settings: live theme/wallpaper sync, xrandr display modes and About
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! System Settings application with live `_SPUI_SETTINGS` appearance broadcasts,
//! visual `picsum.photos` wallpaper thumbnails, live `xrandr` resolution
//! switching and real kernel/hardware telemetry.

use crate::app::{App, UiEvent};
use crate::draw::{Canvas, Rect, Sprite};
use crate::icons::{self, Icon};
use crate::settings_store::{ThemeMode, ACCENT_PRESETS, WALLPAPERS, WALLPAPER_DIR};
use crate::text::{Face, TextRenderer};
use crate::theme::{Theme, RADIUS_CONTROL, TITLEBAR_HEIGHT};
use crate::ui;
use crate::x11::Result;
use std::ffi::CStr;
use std::fs;
use std::path::Path;
use std::process::Command;
use x11rb::protocol::xproto::ConnectionExt;

const SIDEBAR_W: i32 = 186;
const CONTENT_X: i32 = SIDEBAR_W + 22;
const THUMB_W: usize = 128;
const THUMB_H: usize = 74;

const RESOLUTIONS: [(&str, &str); 5] = [
    ("1024x768", "1024 × 768"),
    ("1280x800", "1280 × 800"),
    ("1440x900", "1440 × 900"),
    ("1600x900", "1600 × 900"),
    ("1920x1080", "1920 × 1080"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Appearance,
    Display,
    About,
}

const NAV: [(Page, &str, Icon); 3] = [
    (Page::Appearance, "外观与壁纸", Icon::Appearance),
    (Page::Display, "显示与分辨率", Icon::Display),
    (Page::About, "关于系统", Icon::About),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum HoverItem {
    None,
    Nav(usize),
    Mode(ThemeMode),
    Accent(usize),
    Wallpaper(usize),
    Resolution(usize),
}

struct SystemInfo {
    os_name: String,
    kernel: String,
    arch: String,
    cpu: String,
    memory: String,
}

impl SystemInfo {
    fn gather() -> Self {
        let os_raw = fs::read_to_string("/etc/os-release").unwrap_or_default();
        let os_name = os_raw
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME="))
            .map(|v| v.trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "SpaceOS 9".to_string());

        let (kernel, arch) = read_uname();
        let cpu = read_cpu_summary();
        let memory = read_mem_summary();

        Self {
            os_name,
            kernel,
            arch,
            cpu,
            memory,
        }
    }
}

fn read_uname() -> (String, String) {
    unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) == 0 {
            let sys = CStr::from_ptr(uts.sysname.as_ptr()).to_string_lossy();
            let rel = CStr::from_ptr(uts.release.as_ptr()).to_string_lossy();
            let mach = CStr::from_ptr(uts.machine.as_ptr()).to_string_lossy();
            return (format!("{sys} {rel}"), mach.into_owned());
        }
    }
    ("SpaceKernel".to_string(), "x86_64".to_string())
}

fn read_cpu_summary() -> String {
    let raw = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let mut model = String::new();
    let mut cores = 0usize;
    for line in raw.lines() {
        if line.starts_with("processor") {
            cores += 1;
        } else if model.is_empty() {
            if let Some((_, val)) = line.split_once(':') {
                let key = line.split(':').next().unwrap_or("").trim();
                if matches!(key, "model name" | "Hardware" | "Processor" | "uarch") {
                    model = val.trim().to_string();
                }
            }
        }
    }
    let cores = cores.max(1);
    if model.is_empty() {
        format!("x86_64 处理器 ({cores} 核)")
    } else {
        format!("{model} ({cores} 核)")
    }
}

fn read_mem_summary() -> String {
    let raw = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let parse_kib = |key: &str| -> Option<u64> {
        raw.lines().find_map(|line| {
            let rest = line.strip_prefix(key)?;
            rest.split_whitespace().next()?.parse::<u64>().ok()
        })
    };
    let Some(total_kib) = parse_kib("MemTotal:") else {
        return "未知".to_string();
    };
    let avail_kib = parse_kib("MemAvailable:")
        .or_else(|| parse_kib("MemFree:"))
        .unwrap_or(0);
    let total_mib = total_kib / 1024;
    let used_mib = total_kib.saturating_sub(avail_kib) / 1024;
    format!("已用 {used_mib} MiB / 总计 {total_mib} MiB")
}

fn load_wallpaper_thumbnails() -> [Option<Sprite>; 4] {
    std::array::from_fn(|i| {
        let path = format!("{WALLPAPER_DIR}/{}", WALLPAPERS[i].file);
        let decoded = icons::decode_jpeg(Path::new(&path))?;
        let opaque = decoded.scale_cover_opaque(THUMB_W, THUMB_H);
        let pixels = opaque.into_iter().map(|px| 0xff00_0000 | px).collect();
        Some(Sprite {
            width: THUMB_W as u32,
            height: THUMB_H as u32,
            pixels,
        })
    })
}

fn nav_rect(i: usize) -> Rect {
    Rect::new(10, TITLEBAR_HEIGHT + 40 + i as i32 * 40, SIDEBAR_W - 20, 34)
}

fn mode_card_rect(idx: usize) -> Rect {
    Rect::new(
        CONTENT_X + 16 + idx as i32 * 164,
        TITLEBAR_HEIGHT + 86,
        150,
        44,
    )
}

fn accent_swatch_rect(idx: usize) -> Rect {
    Rect::new(
        CONTENT_X + 16 + idx as i32 * 96,
        TITLEBAR_HEIGHT + 166,
        86,
        30,
    )
}

fn wallpaper_card_rect(idx: usize) -> Rect {
    let col = (idx % 2) as i32;
    let row = (idx / 2) as i32;
    Rect::new(
        CONTENT_X + 16 + col * 246,
        TITLEBAR_HEIGHT + 256 + row * 108,
        232,
        98,
    )
}

fn resolution_btn_rect(idx: usize) -> Rect {
    let col = (idx % 3) as i32;
    let row = (idx / 3) as i32;
    Rect::new(
        CONTENT_X + 16 + col * 164,
        TITLEBAR_HEIGHT + 196 + row * 48,
        152,
        36,
    )
}

fn hit_test(page: Page, x: i32, y: i32) -> HoverItem {
    if y < TITLEBAR_HEIGHT {
        return HoverItem::None;
    }
    for i in 0..NAV.len() {
        if nav_rect(i).contains(x, y) {
            return HoverItem::Nav(i);
        }
    }
    match page {
        Page::Appearance => {
            if mode_card_rect(0).contains(x, y) {
                return HoverItem::Mode(ThemeMode::Dark);
            }
            if mode_card_rect(1).contains(x, y) {
                return HoverItem::Mode(ThemeMode::Light);
            }
            for i in 0..ACCENT_PRESETS.len() {
                if accent_swatch_rect(i).contains(x, y) {
                    return HoverItem::Accent(i);
                }
            }
            for i in 0..WALLPAPERS.len() {
                if wallpaper_card_rect(i).contains(x, y) {
                    return HoverItem::Wallpaper(i);
                }
            }
        }
        Page::Display => {
            for i in 0..RESOLUTIONS.len() {
                if resolution_btn_rect(i).contains(x, y) {
                    return HoverItem::Resolution(i);
                }
            }
        }
        Page::About => {}
    }
    HoverItem::None
}

fn query_root_resolution(app: &App) -> (u16, u16) {
    app.x
        .conn
        .get_geometry(app.x.root)
        .ok()
        .and_then(|c| c.reply().ok())
        .map_or((app.x.width, app.x.height), |g| (g.width, g.height))
}

fn apply_settings_live(app: &mut App) {
    app.theme = Theme::from_settings(&app.settings);
    let _ = app.settings.save();
    let _ = app.x.publish_settings(&app.settings);
    app.needs_redraw = true;
}

pub fn run() -> Result<()> {
    let mut app = App::new("系统设置", Icon::Settings, 760, 540)?;
    app.min_width = 720;
    app.min_height = 520;

    let mut page = Page::Appearance;
    let mut hover = HoverItem::None;
    let mut status = String::new();
    let mut screen_size = query_root_resolution(&app);
    let thumbnails = load_wallpaper_thumbnails();
    let sys_info = SystemInfo::gather();

    loop {
        if app.close {
            break;
        }
        for event in app.pump()? {
            match event {
                UiEvent::Close => app.request_close(),
                UiEvent::Expose | UiEvent::Configure { .. } | UiEvent::ThemeChanged => {
                    screen_size = query_root_resolution(&app);
                    app.needs_redraw = true;
                }
                UiEvent::Motion { x, y, .. } => {
                    let next = hit_test(page, x, y);
                    if next != hover {
                        hover = next;
                        app.needs_redraw = true;
                    }
                }
                UiEvent::Button {
                    x,
                    y,
                    button: 1,
                    down: true,
                    ..
                } if y >= TITLEBAR_HEIGHT => match hit_test(page, x, y) {
                    HoverItem::Nav(i) => {
                        page = NAV[i].0;
                        status.clear();
                        if page == Page::Display {
                            screen_size = query_root_resolution(&app);
                        }
                        app.needs_redraw = true;
                    }
                    HoverItem::Mode(mode) => {
                        if app.settings.mode != mode {
                            app.settings.mode = mode;
                            apply_settings_live(&mut app);
                            status = format!(
                                "已切换至{}",
                                match mode {
                                    ThemeMode::Dark => "石墨深色主题",
                                    ThemeMode::Light => "清爽浅色主题",
                                }
                            );
                        }
                    }
                    HoverItem::Accent(i) => {
                        let (rgb, label) = ACCENT_PRESETS[i];
                        if app.settings.accent != rgb {
                            app.settings.accent = rgb;
                            apply_settings_live(&mut app);
                            status = format!("已应用强调色：{label}");
                        }
                    }
                    HoverItem::Wallpaper(i) => {
                        let wp = &WALLPAPERS[i];
                        if app.settings.wallpaper != wp.file {
                            app.settings.wallpaper = wp.file.to_string();
                            apply_settings_live(&mut app);
                            status = format!("已切换桌面壁纸：{}", wp.label);
                        }
                    }
                    HoverItem::Resolution(i) => {
                        let (mode_arg, label) = RESOLUTIONS[i];
                        let res = Command::new("/usr/bin/xrandr")
                            .args(["--output", "Virtual-1", "--mode", mode_arg])
                            .status();
                        match res {
                            Ok(st) if st.success() => {
                                screen_size = query_root_resolution(&app);
                                status = format!("分辨率已切换至 {label}");
                            }
                            Ok(st) => {
                                status = format!("xrandr 返回状态码 {:?}", st.code());
                            }
                            Err(e) => {
                                status = format!("调用 xrandr 失败: {e}");
                            }
                        }
                        app.needs_redraw = true;
                    }
                    HoverItem::None => {}
                },
                _ => {}
            }
        }

        if app.needs_redraw {
            draw_settings(
                &mut app.canvas,
                &mut app.text,
                &app.theme,
                &app.settings,
                page,
                hover,
                &thumbnails,
                screen_size,
                app.x.has_shm,
                &sys_info,
                &status,
            );
            app.draw_chrome();
            app.present()?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn draw_settings(
    canvas: &mut Canvas,
    text: &mut TextRenderer,
    theme: &Theme,
    settings: &crate::settings_store::SpuiSettings,
    page: Page,
    hover: HoverItem,
    thumbnails: &[Option<Sprite>; 4],
    screen_size: (u16, u16),
    has_shm: bool,
    sys_info: &SystemInfo,
    status: &str,
) {
    let w = canvas.width as i32;
    let h = canvas.height as i32;
    canvas.clear(theme.window);

    // Left sidebar
    let body_h = (h - TITLEBAR_HEIGHT).max(0);
    canvas.fill(
        Rect::new(0, TITLEBAR_HEIGHT, SIDEBAR_W, body_h),
        theme.window_alt,
    );
    canvas.fill(
        Rect::new(SIDEBAR_W - 1, TITLEBAR_HEIGHT, 1, body_h),
        theme.window_border,
    );
    text.draw(
        canvas,
        "系统偏好",
        18.0,
        (TITLEBAR_HEIGHT + 26) as f32,
        11.0,
        theme.text_muted,
        Face::Bold,
    );

    for (i, (id, label, icon)) in NAV.iter().enumerate() {
        let r = nav_rect(i);
        let active = page == *id;
        if active {
            canvas.rounded_rect(r, RADIUS_CONTROL, theme.accent_soft, 1.0);
            canvas.fill(Rect::new(r.x, r.y + 7, 3, r.h - 14), theme.accent);
        } else if hover == HoverItem::Nav(i) {
            canvas.rounded_rect(r, RADIUS_CONTROL, theme.panel_hover, 1.0);
        }
        icons::draw(canvas, *icon, r.x + 10, r.y + (r.h - 18) / 2, 18, None);
        text.draw(
            canvas,
            label,
            (r.x + 36) as f32,
            (r.y + 22) as f32,
            12.5,
            if active { theme.text } else { theme.text_dim },
            Face::Ui,
        );
    }

    let card_w = (w - CONTENT_X - 22).max(500);

    match page {
        Page::Appearance => {
            text.draw(
                canvas,
                "外观与壁纸",
                CONTENT_X as f32,
                (TITLEBAR_HEIGHT + 32) as f32,
                16.5,
                theme.text,
                Face::Bold,
            );

            // Card 1: Theme mode & Accent
            let c1 = Rect::new(CONTENT_X, TITLEBAR_HEIGHT + 46, card_w, 162);
            ui::card(canvas, theme, c1);
            text.draw(
                canvas,
                "色彩模式",
                (CONTENT_X + 16) as f32,
                (TITLEBAR_HEIGHT + 72) as f32,
                12.5,
                theme.text,
                Face::Bold,
            );

            for (idx, (mode, title, preview_bg, preview_fg)) in [
                (ThemeMode::Dark, "石墨深色", 0x18181b, 0xf4f4f5),
                (ThemeMode::Light, "清爽浅色", 0xf4f4f6, 0x18181b),
            ]
            .into_iter()
            .enumerate()
            {
                let r = mode_card_rect(idx);
                let selected = settings.mode == mode;
                let bg = if selected {
                    theme.accent_soft
                } else if hover == HoverItem::Mode(mode) {
                    theme.panel_hover
                } else {
                    theme.panel
                };
                canvas.rounded_rect(r, RADIUS_CONTROL, bg, 1.0);
                canvas.rounded_border(
                    r,
                    RADIUS_CONTROL,
                    if selected {
                        theme.accent
                    } else {
                        theme.window_border
                    },
                    1.0,
                    if selected { 2 } else { 1 },
                );
                // Mini theme swatch
                let sw = Rect::new(r.x + 10, r.y + 10, 28, 24);
                canvas.rounded_rect(sw, 4, preview_bg, 1.0);
                canvas.rounded_border(sw, 4, theme.panel_border, 1.0, 1);
                canvas.fill(Rect::new(sw.x + 4, sw.y + 6, 14, 3), settings.accent);
                canvas.fill(Rect::new(sw.x + 4, sw.y + 13, 20, 2), preview_fg);
                text.draw(
                    canvas,
                    title,
                    (r.x + 48) as f32,
                    (r.y + 27) as f32,
                    12.5,
                    theme.text,
                    Face::Ui,
                );
            }

            text.draw(
                canvas,
                "强调色",
                (CONTENT_X + 16) as f32,
                (TITLEBAR_HEIGHT + 154) as f32,
                12.5,
                theme.text,
                Face::Bold,
            );
            for (i, (color, label)) in ACCENT_PRESETS.iter().enumerate() {
                let r = accent_swatch_rect(i);
                let selected = settings.accent == *color;
                let bg = if selected {
                    theme.accent_soft
                } else if hover == HoverItem::Accent(i) {
                    theme.panel_hover
                } else {
                    theme.panel
                };
                canvas.rounded_rect(r, RADIUS_CONTROL, bg, 1.0);
                canvas.rounded_border(
                    r,
                    RADIUS_CONTROL,
                    if selected {
                        *color
                    } else {
                        theme.window_border
                    },
                    1.0,
                    if selected { 2 } else { 1 },
                );
                canvas.rounded_rect(Rect::new(r.x + 7, r.y + 7, 16, 16), 4, *color, 1.0);
                text.draw(
                    canvas,
                    label,
                    (r.x + 29) as f32,
                    (r.y + 19) as f32,
                    11.5,
                    theme.text,
                    Face::Ui,
                );
            }

            // Card 2: Wallpapers 2x2 thumbnails
            let c2 = Rect::new(CONTENT_X, TITLEBAR_HEIGHT + 218, card_w, 254);
            ui::card(canvas, theme, c2);
            text.draw(
                canvas,
                "桌面壁纸",
                (CONTENT_X + 16) as f32,
                (TITLEBAR_HEIGHT + 244) as f32,
                12.5,
                theme.text,
                Face::Bold,
            );

            for (i, wp) in WALLPAPERS.iter().enumerate() {
                let r = wallpaper_card_rect(i);
                let selected = settings.wallpaper == wp.file;
                let bg = if selected {
                    theme.accent_soft
                } else if hover == HoverItem::Wallpaper(i) {
                    theme.panel_hover
                } else {
                    theme.panel
                };
                canvas.rounded_rect(r, 6, bg, 1.0);
                canvas.rounded_border(
                    r,
                    6,
                    if selected {
                        theme.accent
                    } else {
                        theme.window_border
                    },
                    1.0,
                    if selected { 2 } else { 1 },
                );

                let thumb_box = Rect::new(r.x + 8, r.y + 12, THUMB_W as i32, THUMB_H as i32);
                if let Some(sprite) = &thumbnails[i] {
                    canvas.blit_sprite(thumb_box.x, thumb_box.y, sprite);
                    canvas.border_1px(thumb_box, theme.window_border);
                } else {
                    canvas.rounded_rect(thumb_box, 4, theme.bg_bottom, 1.0);
                    icons::draw(
                        canvas,
                        Icon::Wallpaper,
                        thumb_box.x + (thumb_box.w - 24) / 2,
                        thumb_box.y + (thumb_box.h - 24) / 2,
                        24,
                        None,
                    );
                }

                let tx = (thumb_box.right() + 10) as f32;
                text.draw(
                    canvas,
                    wp.label,
                    tx,
                    (r.y + 42) as f32,
                    12.5,
                    theme.text,
                    Face::Bold,
                );
                text.draw(
                    canvas,
                    if selected {
                        "当前使用"
                    } else {
                        "点击应用"
                    },
                    tx,
                    (r.y + 63) as f32,
                    11.0,
                    if selected {
                        theme.accent
                    } else {
                        theme.text_muted
                    },
                    Face::Ui,
                );
            }
        }
        Page::Display => {
            text.draw(
                canvas,
                "显示与分辨率",
                CONTENT_X as f32,
                (TITLEBAR_HEIGHT + 32) as f32,
                16.5,
                theme.text,
                Face::Bold,
            );

            let c1 = Rect::new(CONTENT_X, TITLEBAR_HEIGHT + 46, card_w, 96);
            ui::card(canvas, theme, c1);
            icons::draw(
                canvas,
                Icon::Display,
                CONTENT_X + 18,
                TITLEBAR_HEIGHT + 70,
                40,
                None,
            );
            text.draw(
                canvas,
                "Virtual-1 · VirtIO-GPU KMS",
                (CONTENT_X + 72) as f32,
                (TITLEBAR_HEIGHT + 84) as f32,
                13.5,
                theme.text,
                Face::Bold,
            );
            let cur_res = format!(
                "当前活动分辨率：{} × {}  |  MIT-SHM 零拷贝：{}",
                screen_size.0,
                screen_size.1,
                if has_shm { "已启用" } else { "回退模式" }
            );
            text.draw(
                canvas,
                &cur_res,
                (CONTENT_X + 72) as f32,
                (TITLEBAR_HEIGHT + 108) as f32,
                12.0,
                theme.text_dim,
                Face::Ui,
            );

            let c2 = Rect::new(CONTENT_X, TITLEBAR_HEIGHT + 156, card_w, 152);
            ui::card(canvas, theme, c2);
            text.draw(
                canvas,
                "屏幕分辨率 (RandR 实时切换)",
                (CONTENT_X + 16) as f32,
                (TITLEBAR_HEIGHT + 182) as f32,
                12.5,
                theme.text,
                Face::Bold,
            );

            let active_mode = format!("{}x{}", screen_size.0, screen_size.1);
            for (i, (mode_arg, label)) in RESOLUTIONS.iter().enumerate() {
                let r = resolution_btn_rect(i);
                let is_current = active_mode == *mode_arg;
                ui::button(
                    canvas,
                    text,
                    theme,
                    r,
                    label,
                    None,
                    is_current,
                    hover == HoverItem::Resolution(i),
                );
            }
        }
        Page::About => {
            text.draw(
                canvas,
                "关于系统",
                CONTENT_X as f32,
                (TITLEBAR_HEIGHT + 32) as f32,
                16.5,
                theme.text,
                Face::Bold,
            );

            let hero = Rect::new(CONTENT_X, TITLEBAR_HEIGHT + 46, card_w, 96);
            ui::card(canvas, theme, hero);
            icons::draw(
                canvas,
                Icon::SpaceLogo,
                CONTENT_X + 18,
                TITLEBAR_HEIGHT + 66,
                56,
                None,
            );
            text.draw(
                canvas,
                &sys_info.os_name,
                (CONTENT_X + 90) as f32,
                (TITLEBAR_HEIGHT + 86) as f32,
                18.0,
                theme.text,
                Face::Bold,
            );
            text.draw(
                canvas,
                "SPUI 4.0 桌面环境 · ViudiraTech",
                (CONTENT_X + 90) as f32,
                (TITLEBAR_HEIGHT + 110) as f32,
                12.5,
                theme.text_dim,
                Face::Ui,
            );

            let specs_card = Rect::new(CONTENT_X, TITLEBAR_HEIGHT + 156, card_w, 224);
            ui::card(canvas, theme, specs_card);
            let rows: [(&str, String); 6] = [
                ("操作系统", sys_info.os_name.clone()),
                ("内核版本", sys_info.kernel.clone()),
                ("体系结构", sys_info.arch.clone()),
                ("处理器", sys_info.cpu.clone()),
                ("物理内存", sys_info.memory.clone()),
                (
                    "图形渲染",
                    format!(
                        "Xorg modesetting · Breeze SVG · {}",
                        if has_shm { "MIT-SHM" } else { "ZPixmap" }
                    ),
                ),
            ];
            for (idx, (k, v)) in rows.iter().enumerate() {
                let ry = TITLEBAR_HEIGHT + 172 + idx as i32 * 34;
                if idx > 0 {
                    canvas.fill(
                        Rect::new(CONTENT_X + 14, ry - 8, card_w - 28, 1),
                        theme.window_border,
                    );
                }
                text.draw(
                    canvas,
                    k,
                    (CONTENT_X + 18) as f32,
                    (ry + 14) as f32,
                    12.0,
                    theme.text_muted,
                    Face::Ui,
                );
                let shown_v = text.truncate(v, (card_w - 140) as f32, 12.0, Face::Ui);
                text.draw(
                    canvas,
                    &shown_v,
                    (CONTENT_X + 114) as f32,
                    (ry + 14) as f32,
                    12.0,
                    theme.text,
                    Face::Ui,
                );
            }
        }
    }

    if !status.is_empty() {
        text.draw(
            canvas,
            status,
            CONTENT_X as f32,
            (h - 14) as f32,
            11.5,
            theme.accent,
            Face::Ui,
        );
    }
}
