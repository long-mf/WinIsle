#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! WinIsle Windows 
//! Rust + eframe/egui(glow) +  Win32/DWM FFI + (tray-icon)
//!  / 
mod platform;
mod tray;
mod smtc;
mod lyrics;
mod netease_mem;
mod soda_lyrics;
mod cdp_lyric;
mod soda_patch;
mod update;
mod paths;
mod weather;
mod audio_meter;
mod soda_now;

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tray::{Tray, TrayAction};

// / 
const W_COLLAPSED: f32 = 150.0;
const W_COLLAPSED_MUSIC: f32 = 356.0;
const W_COLLAPSED_FACE: f32 = 72.0;
///  draw_robot_face expr 
const FACES: [&str; 14] = [
    "平静", "开心", "难过", "生气", "哭泣", "爱心眼", "害羞",
    "思考", "眨眼", "酷", "亲亲", "自信", "困倦", "困惑",
];
/// /FACES 41213
const FACE_REMOVED: [bool; 14] = [
    false, false, false, false, true, false, false,
    true, false, false, false, false, true, true,
];
/// 
fn face_kept_indices() -> Vec<usize> {
    (0..FACES.len()).filter(|i| !FACE_REMOVED[*i]).collect()
}
const UPD_BADGE_W: f32 = 40.0;
const UPD_BADGE_H: f32 = 15.3;
const UPD_BADGE_GAP: f32 = 8.0;
const CW_SPECTRUM: f32 = 13.0;
const CW_LYRICS: f32 = 200.0;
const CW_FACE: f32 = 48.0;
const CW_CLOCK: f32 = 36.0; // HH:MM 兜底宽度（实际按文字实测自适应，含秒）
const CW_WEATHER: f32 = 90.0;
/// 
const FACE_EYE_GAP_REDUCE: f32 = 2.0;
const CB_EDGE: f32 = 5.0;
const CB_OUTER_EXTRA: f32 = 5.0;
const CB_GAP: f32 = 10.0;
///  lyric_font 
const LYRIC_COLLAPSED_PT: f32 = 13.0;
/// = 
const CW_MAX: f32 = W_EXPANDED;
const H_COLLAPSED: f32 = 30.0;
const W_EXPANDED: f32 = 520.0;
const H_EXPANDED: f32 = 300.0;
/// )
/// /?
const MARGIN: f32 = 6.0;
// 
//  1px 
const TOP_OVERLAP: f32 = 3.0;
const CONTENT_PAD_X: f32 = 16.0;
const CONTENT_PAD_T: f32 = 16.0;
const CONTENT_PAD_B: f32 = 14.0;

// ============================  ============================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DockPosition {
    TopCenter,
    TopLeft,
    TopRight,
    LeftCenter,
    RightCenter,
    BottomCenter,
}

impl DockPosition {
    const ALL: [DockPosition; 6] = [
        DockPosition::TopCenter,
        DockPosition::TopLeft,
        DockPosition::TopRight,
        DockPosition::LeftCenter,
        DockPosition::RightCenter,
        DockPosition::BottomCenter,
    ];

    fn label(&self) -> &'static str {
        match self {
            DockPosition::TopCenter => "顶部居中",
            DockPosition::TopLeft => "左上",
            DockPosition::TopRight => "右上",
            DockPosition::LeftCenter => "左侧居中",
            DockPosition::RightCenter => "右侧居中",
            DockPosition::BottomCenter => "底部居中",
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            DockPosition::TopCenter => "top_center",
            DockPosition::TopLeft => "top_left",
            DockPosition::TopRight => "top_right",
            DockPosition::LeftCenter => "left_center",
            DockPosition::RightCenter => "right_center",
            DockPosition::BottomCenter => "bottom_center",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.trim() {
            "top_left" => DockPosition::TopLeft,
            "top_right" => DockPosition::TopRight,
            "left_center" => DockPosition::LeftCenter,
            "right_center" => DockPosition::RightCenter,
            "bottom_center" => DockPosition::BottomCenter,
            _ => DockPosition::TopCenter,
        }
    }


    fn corner_radius(&self, r: f32) -> egui::CornerRadius {
        let r = r.max(0.0) as u8;
        match self {
            DockPosition::TopCenter | DockPosition::TopLeft | DockPosition::TopRight => {
                egui::CornerRadius { nw: 0, ne: 0, sw: r, se: r }
            }
            DockPosition::BottomCenter => egui::CornerRadius { nw: r, ne: r, sw: 0, se: 0 },
            DockPosition::LeftCenter => egui::CornerRadius { nw: 0, ne: r, sw: 0, se: r },
            DockPosition::RightCenter => egui::CornerRadius { nw: r, ne: 0, sw: r, se: 0 },
        }
    }
}

/// ???
fn dock_origin(pos: DockPosition, mx: f32, my: f32, w: f32, h: f32) -> egui::Pos2 {
    let (x, y) = match pos {
        DockPosition::TopCenter => ((mx - w) * 0.5, 0.0),
        DockPosition::TopLeft => (0.0, 0.0),
        DockPosition::TopRight => (mx - w, 0.0),
        DockPosition::BottomCenter => ((mx - w) * 0.5, my - h),
        DockPosition::LeftCenter => (0.0, (my - h) * 0.5),
        DockPosition::RightCenter => (mx - w, (my - h) * 0.5),
    };
    egui::pos2(x.max(0.0), y.max(0.0))
}

// ============================  ============================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Theme {
    Dark,
    Light,
    System,
}

impl Theme {
    fn label(&self) -> &'static str {
        match self {
            Theme::Dark => "深色",
            Theme::Light => "浅色",
            Theme::System => "跟随系统",
        }
    }
    fn as_str(&self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::System => "system",
        }
    }
    fn from_str(s: &str) -> Self {
        match s.trim() {
            "light" => Theme::Light,
            "system" => Theme::System,
            _ => Theme::Dark,
        }
    }
    fn is_light(&self) -> bool {
        match self {
            Theme::Light => true,
            Theme::Dark => false,
            Theme::System => system_prefers_light(),
        }
    }
}

///  Windows ?
fn system_prefers_light() -> bool {
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(idx) = s.find("REG_DWORD") {
            let tail = &s[idx + "REG_DWORD".len()..];
            if let Some(tok) = tail.split_whitespace().next() {
                return tok.trim() == "0x1";
            }
        }
    }
    false
}

// ============================ ?============================
#[derive(Clone, Copy)]
struct Colors {
    bg: egui::Color32,
    card: egui::Color32,
    card2: egui::Color32,
    line: egui::Color32,
    line_soft: egui::Color32,
    text: egui::Color32,
    muted: egui::Color32,
    muted2: egui::Color32,
    accent: egui::Color32,
    accent_strong: egui::Color32,
    accent_dim: egui::Color32,
    danger: egui::Color32,
    danger_dim: egui::Color32,
}

fn palette(light: bool) -> Colors {
    let rgb = egui::Color32::from_rgb;
    let rgba = egui::Color32::from_rgba_unmultiplied;
    if light {
        Colors {
            bg: rgb(246, 247, 249),
            card: rgb(255, 255, 255),
            card2: rgb(240, 241, 244),
            line: rgb(223, 226, 232),
            line_soft: rgb(233, 235, 240),
            text: rgb(28, 30, 36),
            muted: rgb(91, 98, 115),
            muted2: rgb(139, 145, 161),
            accent: rgb(16, 185, 129),
            accent_strong: rgb(16, 185, 129),
            accent_dim: rgba(16, 185, 129, 28),
            danger: rgb(217, 75, 75),
            danger_dim: rgba(217, 75, 75, 24),
        }
    } else {
        Colors {
            bg: rgb(23, 24, 29),
            card: rgb(30, 32, 39),
            card2: rgb(35, 38, 46),
            line: rgb(42, 45, 54),
            line_soft: rgb(34, 36, 43),
            text: rgb(233, 234, 238),
            muted: rgb(154, 161, 176),
            muted2: rgb(107, 113, 128),
            accent: rgb(52, 211, 153),
            accent_strong: rgb(16, 185, 129),
            accent_dim: rgba(52, 211, 153, 40),
            danger: rgb(244, 114, 114),
            danger_dim: rgba(244, 114, 114, 30),
        }
    }
}

fn apply_style(ctx: &egui::Context, light: bool) {
    let c = palette(light);
    // egui 0.31 Styledark_style / light_style
    //
    // Windows egui 
    //  Visuals144,209,255
    // 
    let theme = if light { egui::Theme::Light } else { egui::Theme::Dark };
    let mut style = (*ctx.style_of(theme)).clone();
    style.visuals = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    style.visuals.panel_fill = c.bg;
    style.visuals.window_fill = c.bg;
    style.visuals.extreme_bg_color = c.card2;
    style.visuals.faint_bg_color = c.card;
    style.visuals.widgets.noninteractive.bg_fill = c.card;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, c.line_soft);
    style.visuals.widgets.inactive.bg_fill = c.card2;
    style.visuals.widgets.inactive.weak_bg_fill = c.card2;
    style.visuals.widgets.hovered.bg_fill = c.card2;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, c.line);
    style.visuals.widgets.active.bg_fill = c.accent_dim;
    style.visuals.selection.bg_fill = c.accent_dim;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, c.accent);
    style.visuals.hyperlink_color = c.accent;
    style.visuals.override_text_color = Some(c.text);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.slider_width = 150.0;
    ctx.set_style_of(theme, style);
    //  1px feathering     //  feathering 
ctx.tessellation_options_mut(|o| {
        o.feathering = true;
        o.feathering_size_in_pixels = 1.5;
        o.round_rects_to_pixels = false;
    });
}

// ============================  ============================
#[derive(Clone, PartialEq, Debug)]
struct Config {
    position: DockPosition,
    /// ?0..1)?
    x_frac: f32,
   /// ..1
    y_frac: f32,
    /// X
    mon_x: f32,
   /// Y
    mon_y: f32,
    font: f32,
    delay_ms: u32,
    bg: [u8; 3],
    alpha: u8,
    radius: f32,
    show_seconds: bool,
    show_date: bool,
    autostart: bool,
    theme: Theme,
   ///
    lyric_shift_ms: f32,
    /// 
    roll_anim: bool,
    /// 
    roll_ms: f32,
   ///
    char_green: bool,
    /// 
    marquee: bool,
    ///  / 
    edge_fade: bool,
    /// 
    side_snap: bool,
    /// 
    lyric_font: f32,
    /// 
    spectrum: bool,
    /// 
    water_spectrum: bool,
    /// 0==
    collapsed_mode: u8,
    /// ..FACES.len()-1
    idle_face: u8,
    ///  `+`  / `-`     /// "+spectrum,+lyrics,-face,+clock,-weather"collapsed_mode 
    collapsed_layout: String,
    /// 
    upd_seen_rail: String,
    /// 
    upd_seen_row: String,
   /// ?yyyy-mm-dd
    upd_checked_day: String,

    weather_city: String,

    weather_city_name: String,

    weather_unit: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            position: DockPosition::TopCenter,
            x_frac: 0.5,
            y_frac: 0.5,
            mon_x: 0.0,
            mon_y: 0.0,
            font: 21.0,
            delay_ms: 200,
            bg: [28, 28, 34],
            alpha: 255,
            radius: 16.0,
            show_seconds: false,
            show_date: false,
            autostart: false,
            theme: Theme::Dark,
            lyric_shift_ms: 0.0,
            roll_anim: true,
            roll_ms: 500.0,
            char_green: true,
            marquee: true,
            edge_fade: true,
            side_snap: true,
            lyric_font: 13.0,
            spectrum: true,
            water_spectrum: true,
            collapsed_mode: 0,
            idle_face: 1,
            collapsed_layout: String::new(),
            upd_seen_rail: String::new(),
            upd_seen_row: String::new(),
            upd_checked_day: String::new(),
   weather_city: String::new(), // "= 自��定位（音次拉取时探测扢在城市，
            weather_city_name: String::new(),
            weather_unit: 0,
        }
    }
}

fn config_path() -> PathBuf {
    crate::paths::ensure().join("config.ini")
}

fn trigger_path() -> PathBuf {
    config_path().with_file_name(".show_trigger")
}

fn trigger_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(trigger_path())
        .ok()
        .and_then(|m| m.modified().ok())
}

impl Config {
    fn load() -> Self {
        let mut cfg = Config::default();
        if let Ok(text) = std::fs::read_to_string(config_path()) {
            let text = text.trim_start_matches('\u{feff}');
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((k, v)) = line.split_once('=') else { continue };
                let k = k.trim();
                let v = v.trim();
                match k {
                    "position" => cfg.position = DockPosition::from_str(v),
                    "x_frac" => cfg.x_frac = v.parse().unwrap_or(cfg.x_frac),
                    "y_frac" => cfg.y_frac = v.parse().unwrap_or(cfg.y_frac),
                    "mon_x" => cfg.mon_x = v.parse().unwrap_or(cfg.mon_x),
                    "mon_y" => cfg.mon_y = v.parse().unwrap_or(cfg.mon_y),
                    "font" => cfg.font = v.parse().unwrap_or(cfg.font),
                    "delay_ms" => cfg.delay_ms = v.parse().unwrap_or(cfg.delay_ms),
                    "bg" => {
                        let p: Vec<u8> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                        if p.len() == 3 {
                            cfg.bg = [p[0], p[1], p[2]];
                        }
                    }
                    "alpha" => cfg.alpha = v.parse().unwrap_or(cfg.alpha),
                    "radius" => cfg.radius = v.parse().unwrap_or(cfg.radius),
                    "show_seconds" => cfg.show_seconds = v == "1" || v == "true",
                    "show_date" => cfg.show_date = v == "1" || v == "true",
                    "collapsed_mode" => cfg.collapsed_mode = v.parse().unwrap_or(cfg.collapsed_mode),
                    "idle_face" => cfg.idle_face = v.parse().unwrap_or(cfg.idle_face),
                    "collapsed_layout" => cfg.collapsed_layout = v.to_string(),
                    "autostart" => cfg.autostart = v == "1" || v == "true",
                    "theme" => cfg.theme = Theme::from_str(v),
                    "lyric_shift_ms" => cfg.lyric_shift_ms = v.parse().unwrap_or(cfg.lyric_shift_ms),
                    "roll_anim" => cfg.roll_anim = v == "1" || v == "true",
                    "marquee" => cfg.marquee = v == "1" || v == "true",
                    "edge_fade" => cfg.edge_fade = v == "1" || v == "true",
                    "side_snap" => cfg.side_snap = v == "1" || v == "true",
                    "lyric_font" => cfg.lyric_font = v.parse().unwrap_or(cfg.lyric_font),
                    "spectrum" => cfg.spectrum = v == "1" || v == "true",
                    "water_spectrum" => cfg.water_spectrum = v == "1" || v == "true",
                    "upd_seen_rail" => cfg.upd_seen_rail = v.to_string(),
                    "upd_seen_row" => cfg.upd_seen_row = v.to_string(),
                    "upd_checked_day" => cfg.upd_checked_day = v.to_string(),
                    "weather_city" => cfg.weather_city = v.to_string(),
                    "weather_city_name" => cfg.weather_city_name = v.to_string(),
                    "weather_unit" => cfg.weather_unit = v.parse().unwrap_or(cfg.weather_unit),
                    _ => {}
                }
            }
        }
        //   /  /  / TopLeft/TopRight/BottomCenter
        cfg.position = match cfg.position {
            DockPosition::LeftCenter => DockPosition::LeftCenter,
            DockPosition::RightCenter => DockPosition::RightCenter,
            _ => DockPosition::TopCenter,
        };
        cfg.x_frac = cfg.x_frac.clamp(0.0, 1.0);
        cfg.y_frac = cfg.y_frac.clamp(0.0, 1.0);
        // 
if (cfg.idle_face as usize) >= FACES.len() || FACE_REMOVED[cfg.idle_face as usize] {
            cfg.idle_face = face_kept_indices().first().copied().unwrap_or(0) as u8;
        }
        // collapsed_mode 
        if cfg.collapsed_layout.trim().is_empty() {
            cfg.collapsed_layout = if cfg.collapsed_mode == 1 {
                "-spectrum,-lyrics,+face,-clock,-weather".to_string()
            } else {
                "+spectrum,+lyrics,-face,+clock,-weather".to_string()
            };
        }
        //         cfg.weather_city.clear();
        cfg.weather_city_name.clear();
        cfg
    }

    fn save(&self) {
        let text = format!(
            "# WinIsle 配置\nposition={}\nx_frac={}\ny_frac={}\nfont={}\ndelay_ms={}\nbg={},{},{}\nalpha={}\nradius={}\nshow_seconds={}\nshow_date={}\nautostart={}\ntheme={}\nlyric_shift_ms={}\nroll_anim={}\nroll_ms={}\nchar_green={}\nmarquee={}\nedge_fade={}\nside_snap={}\nlyric_font={}\nspectrum={}\nwater_spectrum={}\ncollapsed_mode={}\nidle_face={}\ncollapsed_layout={}\nupd_seen_rail={}\nupd_seen_row={}\nupd_checked_day={}\nweather_city={}\nweather_city_name={}\nweather_unit={}\nmon_x={}\nmon_y={}\n",
            self.position.as_str(),
            self.x_frac,
            self.y_frac,
            self.font,
            self.delay_ms,
            self.bg[0],
            self.bg[1],
            self.bg[2],
            self.alpha,
            self.radius,
            self.show_seconds as u8,
            self.show_date as u8,
            self.autostart as u8,
            self.theme.as_str(),
            self.lyric_shift_ms,
            self.roll_anim as u8,
            self.roll_ms,
            self.char_green as u8,
            self.marquee as u8,
            self.edge_fade as u8,
            self.side_snap as u8,
            self.lyric_font,
            self.spectrum as u8,
            self.water_spectrum as u8,
            self.collapsed_mode,
            self.idle_face,
            self.collapsed_layout,
            self.upd_seen_rail,
            self.upd_seen_row,
            self.upd_checked_day,
            self.weather_city,
            self.weather_city_name,
            self.weather_unit,
            self.mon_x,
            self.mon_y,
        );
        let _ = std::fs::write(config_path(), text);
    }
}

///  HKCU\...\Run--autostart
fn set_autostart(on: bool) {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    let _ = std::process::Command::new("reg")
        .args(["delete", key, "/v", "MyIsle", "/f"])
        .status();
    if on {
        let val = format!("\"{}\" --autostart", exe);
        let _ = std::process::Command::new("reg")
            .args(["add", key, "/v", "WinIsle", "/t", "REG_SZ", "/d", &val, "/f"])
            .status();
    } else {
        let _ = std::process::Command::new("reg")
            .args(["delete", key, "/v", "WinIsle", "/f"])
            .status();
    }
}

// ============================  ============================
fn bold_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(Arc::from("bold")))
}

/// Segoe MDL2 AssetsWindows 
fn icon_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(Arc::from("icons")))
}

/// HSV -> Color32
fn hsv(h: f32, s: f32, v: f32, a: u8) -> egui::Color32 {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let to = |f: f32| (((f + m) * 255.0).clamp(0.0, 255.0)) as u8;
    egui::Color32::from_rgba_unmultiplied(to(r), to(g), to(b), a)
}

fn lerp_color(a: egui::Color32, b: egui::Color32, f: f32) -> egui::Color32 {
    let f = f.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f) as u8;
    egui::Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// rect  pivot  ang 
fn draw_tex_rotated(painter: &egui::Painter, id: egui::TextureId, rect: egui::Rect, pivot: egui::Pos2, ang: f32, a: u8) {
    let (s, c) = (ang.sin(), ang.cos());
    let corners = [
        (0.0f32, 0.0f32, 0.0f32, 0.0f32),
        (1.0, 0.0, 1.0, 0.0),
        (0.0, 1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0, 1.0),
    ];
    let mut mesh = egui::Mesh::with_texture(id);
    for (fx, fy, u, v) in corners {
        let p = egui::pos2(rect.left() + fx * rect.width(), rect.top() + fy * rect.height());
        let (dx, dy) = (p.x - pivot.x, p.y - pivot.y);
        let np = egui::pos2(pivot.x + dx * c - dy * s, pivot.y + dx * s + dy * c);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: np,
            uv: egui::pos2(u, v),
            color: egui::Color32::from_rgba_unmultiplied(255, 255, 255, a),
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// radius ang 
/// mesh base base+ang ang 
fn draw_rotating_circle_tex(
    painter: &egui::Painter,
    id: egui::TextureId,
    dc: egui::Pos2,
    radius: f32,
    ang: f32,
    a: u8,
) {
    let col = egui::Color32::from_rgba_unmultiplied(255, 255, 255, a);
    let mut mesh = egui::Mesh::with_texture(id);
    mesh.vertices.push(egui::epaint::Vertex { pos: dc, uv: egui::pos2(0.5, 0.5), color: col });
    let n = 160usize;
    for i in 0..n {
        let base = (i as f32) / (n as f32) * std::f32::consts::TAU;
        let th = base + ang;
        let (s, c) = (th.sin(), th.cos());
        let (sb, cb) = (base.sin(), base.cos());
        mesh.vertices.push(egui::epaint::Vertex {
            pos: egui::pos2(dc.x + radius * c, dc.y + radius * s),
            uv: egui::pos2(0.5 + 0.5 * cb, 0.5 + 0.5 * sb),
            color: col,
        });
    }
    for i in 0..n {
        let a1 = 1 + i as u32;
        let a2 = 1 + ((i + 1) % n) as u32;
        mesh.indices.extend_from_slice(&[0, a1, a2]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// app_id exe  WinRT AUMID> 
fn src_label(app_id: &str) -> String {
    let s = app_id.to_ascii_lowercase();
    if s.contains("cloudmusic") || app_id.contains("网易") {
        "网易云音".to_string()
    } else if s.contains("soda") || app_id.contains("汽水") {
        "汽水音乐".to_string()
    } else if s.contains("qqmusic") {
        "QQ音乐".to_string()
    } else if s.contains("spotify") {
        "Spotify".to_string()
    } else if s.contains("kugou") {
        "酷狗音乐".to_string()
    } else if s.contains("kuwo") || s.contains("kwmusic") {
        "酷我音乐".to_string()
    } else if s.contains("applemusic") || s.contains("appleinc.applemusicwin") {
        "Apple Music".to_string()
    } else if s.contains("bodian") {
        "波点音乐".to_string()
    } else {
        app_id.to_string()
    }
}

fn card_begin(ui: &mut egui::Ui, c: &Colors, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(c.card)
        .stroke(egui::Stroke::new(1.0, c.line_soft))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, add);
}

fn section_label(ui: &mut egui::Ui, c: &Colors, text: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(text).size(11.0).color(c.muted2).strong());
    ui.add_space(4.0);
}

/// iOS 
fn switch(ui: &mut egui::Ui, on: &mut bool, c: &Colors) -> egui::Response {
    let desired = egui::vec2(42.0, 24.0);
    let (rect, mut resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = ui.painter();
    let r = rect.height() * 0.5;
    let bg = if *on { c.accent_strong } else { c.card2 };
    p.rect_filled(rect, egui::CornerRadius::same(r as u8), bg);
    if !*on {
        p.rect_stroke(rect, egui::CornerRadius::same(r as u8), egui::Stroke::new(1.0, c.line), egui::StrokeKind::Inside);
    }
    let knob_x = if *on { rect.right() - r } else { rect.left() + r };
    p.circle_filled(egui::pos2(knob_x, rect.center().y), r - 3.0, egui::Color32::WHITE);
    resp
}

/// 
fn kv_row(ui: &mut egui::Ui, c: &Colors, title: &str, desc: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.add_space(1.0);
            ui.label(egui::RichText::new(title).size(13.5).color(c.text));
            if !desc.is_empty() {
                ui.label(egui::RichText::new(desc).size(11.0).color(c.muted2));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
}

fn hairline(ui: &mut egui::Ui, c: &Colors) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, c.line_soft);
}

/// ?
fn position_grid(ui: &mut egui::Ui, sel: &mut DockPosition, c: &Colors) {
    let gap = 8.0;
    let avail = ui.available_width();
    let tile_w = ((avail - 2.0 * gap) / 3.0).max(60.0);
    let tile_h = 54.0;
    egui::Grid::new("pos_grid")
        .num_columns(3)
        .spacing(egui::vec2(gap, gap))
        .show(ui, |ui| {
            for (i, &p) in DockPosition::ALL.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(tile_w, tile_h), egui::Sense::click());
                let is_sel = *sel == p;
                let painter = ui.painter_at(rect);
                let fill = if is_sel { c.accent_dim } else { c.card2 };
                painter.rect_filled(rect, egui::CornerRadius::same(10), fill);
                painter.rect_stroke(
                    rect,
                    egui::CornerRadius::same(10),
                    egui::Stroke::new(if is_sel { 1.6 } else { 1.0 }, if is_sel { c.accent } else { c.line }),
                    egui::StrokeKind::Inside,
                );
                // 
                let screen = egui::Rect::from_min_size(
                    rect.min + egui::vec2(12.0, 9.0),
                    egui::vec2(rect.width() - 24.0, 24.0),
                );
                painter.rect_filled(screen, egui::CornerRadius::same(4), egui::Color32::from_rgb(18, 19, 23));
                // ?
                let pw = 18.0;
                let ph = 3.6;
                let (px, py) = match p {
                    DockPosition::TopCenter => (screen.center().x - pw / 2.0, screen.top() + 2.0),
                    DockPosition::TopLeft => (screen.left() + 3.0, screen.top() + 2.0),
                    DockPosition::TopRight => (screen.right() - pw - 3.0, screen.top() + 2.0),
                    DockPosition::LeftCenter => (screen.left() + 2.0, screen.center().y - ph / 2.0),
                    DockPosition::RightCenter => (screen.right() - pw - 2.0, screen.center().y - ph / 2.0),
                    DockPosition::BottomCenter => (screen.center().x - pw / 2.0, screen.bottom() - ph - 2.0),
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(pw, ph)),
                    egui::CornerRadius::same(2),
                    c.accent,
                );
                // 
                painter.text(
                    egui::pos2(rect.center().x, rect.bottom() - 7.0),
                    egui::Align2::CENTER_BOTTOM,
                    p.label(),
                    egui::FontId::proportional(10.5),
                    if is_sel { c.accent } else { c.muted },
                );
                if resp.clicked() {
                    *sel = p;
                }
                if (i + 1) % 3 == 0 {
                    ui.end_row();
                }
            }
        });
}

// ============================ App ============================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Module {
    Music,
    Schedule,
    Todo,
    Reminder,
    Weather,
}

impl Module {
    /// 0 / =1 / =2 / =3
fn rail_idx(self) -> f32 {
        match self {
            Module::Music => 0.0,
            Module::Todo => 1.0,
            Module::Reminder => 2.0,
            _ => 3.0,
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DbgView {
    Settings,
    Schedule,
    Todo,
    Reminder,
    Weather,
}

/// `data/todo.json`
#[derive(Clone)]
struct Todo {
    id: u64,
    title: String,
    done: bool,
    archived: bool,
    /// ?`YYYY-MM-DD`
    created: String,
    ///  = 
    done_seq: u64,
}

fn todo_path() -> PathBuf {
    crate::paths::file("todo.json")
}

fn load_todos() -> (Vec<Todo>, u64, u64) {
    let mut todos: Vec<Todo> = Vec::new();
    if let Ok(text) = std::fs::read_to_string(todo_path()) {
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&text) {
            for it in arr {
                let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if title.is_empty() {
                    continue;
                }
                todos.push(Todo {
                    id: it.get("id").and_then(|x| x.as_u64()).unwrap_or(0),
                    title,
                    done: it.get("done").and_then(|x| x.as_bool()).unwrap_or(false),
                    archived: it.get("archived").and_then(|x| x.as_bool()).unwrap_or(false),
                    created: it.get("created").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    done_seq: it.get("done_seq").and_then(|x| x.as_u64()).unwrap_or(0),
                });
            }
        }
    }
    let id = todos.iter().map(|t| t.id).max().unwrap_or(0);
    let dseq = todos.iter().map(|t| t.done_seq).max().unwrap_or(0);
    (todos, id, dseq)
}

fn save_todos(todos: &[Todo]) {
    let arr: Vec<serde_json::Value> = todos
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "title": t.title,
                "done": t.done,
                "archived": t.archived,
                "created": t.created,
                "done_seq": t.done_seq,
            })
        })
        .collect();
    let text = serde_json::Value::Array(arr).to_string();
    let _ = std::fs::write(todo_path(), text);
}

// ===== /  /  /  / =====

/// `data/reminders.json`/// `icon`= 1= 2= 3= 4=
/// `interval_min`
/// `window_start/window_end`0 
#[derive(Clone)]
struct Reminder {
    id: u64,
    name: String,
    icon: u8,
    interval_min: u32,
    window_start: u32,
    window_end: u32,
    days: u8,
    enabled: bool,
    custom: bool,
    /// true= false=
    repeat: bool,
    /// Unix =
    next_due: i64,
}

/// None=
#[derive(Clone)]
struct RemindForm {
    id: u64, // 0="新建"
    name: String,
    icon: u8,
    interval_min: u32,
    /// 
    interval_text: String,
    window_start: u32,
    window_end: u32,
    days: u8,
    repeat: bool,
    sound: bool,
}

fn reminders_path() -> PathBuf {
    crate::paths::file("reminders.json")
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

///  5 /
fn default_reminders() -> Vec<Reminder> {
    let now = now_secs();
    let mk = |id: u64, name: &str, icon: u8, interval: u32, ws: u32, we: u32, days: u8, enabled: bool, custom: bool| Reminder {
        id,
        name: name.to_string(),
        icon,
        interval_min: interval,
        window_start: ws,
        window_end: we,
        days,
        enabled,
        custom,
        repeat: true,
        next_due: 0,
    };
    vec![
        mk(1, "久坐提醒", 0, 45, 9 * 60, 20 * 60, 0b1111110, true, false),
        mk(2, "喝水提醒", 1, 30, 9 * 60, 20 * 60, 0b1111111, true, false),
        mk(3, "护眼提醒", 2, 60, 9 * 60, 22 * 60, 0b1111111, true, false),
    ]
}

fn load_reminders() -> (Vec<Reminder>, u64) {
    let mut out: Vec<Reminder> = Vec::new();
    let mut dropped_legacy = false;
    if let Ok(text) = std::fs::read_to_string(reminders_path()) {
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&text) {
            for it in arr {
                let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if name.is_empty() {
                    continue;
                }
                let rid = it.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
                //  / 
                if (rid == 4 && name == "吃药提醒") || (rid == 5 && name == "站起来拉伸") {
                    dropped_legacy = true;
                    continue;
                }
                out.push(Reminder {
                    id: rid,
                    name,
                    icon: it.get("icon").and_then(|x| x.as_u64()).unwrap_or(0) as u8,
                    interval_min: it.get("interval_min").and_then(|x| x.as_u64()).unwrap_or(30) as u32,
                    window_start: it.get("window_start").and_then(|x| x.as_u64()).unwrap_or(540) as u32,
                    window_end: it.get("window_end").and_then(|x| x.as_u64()).unwrap_or(1200) as u32,
                    days: it.get("days").and_then(|x| x.as_u64()).unwrap_or(127) as u8,
                    enabled: it.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
                    custom: it.get("custom").and_then(|x| x.as_bool()).unwrap_or(false),
                    repeat: it.get("repeat").and_then(|x| x.as_bool()).unwrap_or(true),
                    next_due: it.get("next_due").and_then(|x| x.as_i64()).unwrap_or(0),
                });
            }
        }
    }
    if out.is_empty() {
        out = default_reminders();
        save_reminders(&out);
    } else if dropped_legacy {
        save_reminders(&out);
    }
    let max_id = out.iter().map(|r| r.id).max().unwrap_or(0);
    (out, max_id)
}

fn save_reminders(list: &[Reminder]) {
    let arr: Vec<serde_json::Value> = list
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "name": r.name,
                "icon": r.icon,
                "interval_min": r.interval_min,
                "window_start": r.window_start,
                "window_end": r.window_end,
                "days": r.days,
                "enabled": r.enabled,
                "custom": r.custom,
                "repeat": r.repeat,
                "next_due": r.next_due,
            })
        })
        .collect();
    let text = serde_json::Value::Array(arr).to_string();
    let _ = std::fs::write(reminders_path(), text);
}

fn days_contains(days: u8, wd: u8) -> bool {
    days & (1u8 << (wd & 7)) != 0
}

fn next_grid_due(r: &Reminder, now: i64) -> i64 {
    let st = platform::local_time();
    let step = (r.interval_min.max(1) as i64) * 60;
    let sod = st.wHour as i64 * 3600 + st.wMinute as i64 * 60 + st.wSecond as i64;
    let today0 = now - sod;
    for d in 0..8i64 {
        let day0 = today0 + d * 86400;
        let wd = ((st.wDayOfWeek as i64 + d) % 7) as u8;
        if !days_contains(r.days, wd) {
            continue;
        }
        let base = day0 + r.window_start as i64 * 60;
        let end = day0 + r.window_end as i64 * 60;
        if end <= base {
            continue;
        }
        let t = if now <= base || now - base < 60 {
            base
        } else {
            let k = (now - base + step - 1) / step;
            base + k * step
        };
        if t < end {
            return t;
        }
    }
    now + step
}

fn remind_color(icon: u8) -> [u8; 3] {
    match icon {
        0 => [230, 178, 60],
        1 => [57, 199, 255],
        2 => [52, 211, 153],
        3 => [242, 112, 92],
        5 => [167, 139, 250],
        _ => [167, 139, 250],
    }
}

fn remind_verb(icon: u8) -> &'static str {
    match icon {
        0 => "起身活动",
        1 => "喝水",
        2 => "让眼睛歇会儿",
        3 => "吃药",
        _ => "伸展一下",
    }
}

fn remind_tip(icon: u8) -> &'static str {
    match icon {
        0 => "该起来活动一下了",
        1 => "喝水时间到",
        2 => "远眺一下，放松眼睛",
        3 => "记得吃药",
        _ => "起身拉伸一下",
    }
}

/// DATA.desc 
fn remind_desc(r: &Reminder) -> String {
    match r.icon {
        0 => format!("已连续久坐{} 分钟 · 起身走走", r.interval_min),
        1 => format!("距上次喝水{} 分钟 · 起来喝口水", r.interval_min),
        2 => format!("用眼 {} 分钟 · 远眺 20 秒", r.interval_min),
        3 => "按计划服药 · 别忘记".to_string(),
        _ => "肩颈放松 · 活动 1 分钟".to_string(),
    }
}

///  "HH:MM"
fn hhmm(min_of_day: u32) -> String {
    format!("{:02}:{:02}", (min_of_day / 60) % 24, min_of_day % 60)
}

/// ?`YYYY-MM-DD`?
fn today_str() -> String {
    let st = platform::local_time();
    format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
}

/// `YYYY-MM-DD` MD
fn fmt_cn_date(d: &str) -> String {
    let p: Vec<&str> = d.split('-').collect();
    if p.len() == 3 {
        if let (Ok(m), Ok(day)) = (p[1].parse::<u32>(), p[2].parse::<u32>()) {
            return format!("{}月{}", m, day);
        }
    }
    d.to_string()
}

struct IslandApp {
    hwnd: isize,
    initialized: bool,
    expanded: f32,
    target: f32,
    collapse_at: Option<Instant>,
    monitor: egui::Vec2,
   /// pixels_per_point
    monitor_rect: egui::Rect,
    last_geom: (f32, f32, f32, f32),
    last_tick: std::time::Instant,
    cfg: Config,
    settings_open: bool,
    confirm_open: bool,
    settings_hwnd: isize,
    settings_shown: bool,
    island_visible: bool,
    last_trigger: Option<std::time::SystemTime>,
    tray: Option<Tray>,
    focus_settings: bool,
    // 
    dragging: bool,
    drag_off: egui::Vec2,
    /// ..1    /// 
   /// :1
    drag_frac: egui::Vec2,
    drag_last_btn: bool,
   /// "" (, ,
    settle: Option<(egui::Pos2, egui::Pos2, Instant)>,
    /// (false 
    /// 9
    full: bool,
    /// ""?
    hover_hold: Option<Instant>,

    click_through: bool,
    ///  (l,t,r,b,rad)SetWindowRgn
    region: (i32, i32, i32, i32, i32),
    /// SetWindowRgn 
    perf_region_ms: f32,
    /// 
    perf_netease_ms: f32,
    /// SMTC 
    music: smtc::Track,
    /// SMTC ? smtc.rs
    tracker: smtc::Tracker,
    /// 
    lyr_tx: std::sync::mpsc::Sender<(String, String, String, f64, bool, bool)>,
    lyr_rx: std::sync::mpsc::Receiver<(String, Option<Vec<lyrics::LrcLine>>, Option<Vec<u8>>, bool)>,
    lyr_key: String,
    lyr_dur: f64, // "当初请求歌词时采用的“期望时长，用于发现同名不同版本(时长不的")"后重" 
    lyr_lines: Vec<lyrics::LrcLine>,
    /// 
    lyr_done: bool,
    ///  /  / ?
    lyr_settled: bool,
    /// SMTC position  0
    clock_base: f64,
    clock_start: Instant,
    clock_playing: bool,
    prev_playing: bool,
    from_smtc: bool,
    ui_playing: bool,
    // SMTC ""
    override_play: Option<(bool, std::time::Instant)>,
    /// ?
    /// 
    pause_pending_at: Option<std::time::Instant>,
    /// SMTC  Position position lock
    spos: f64,
    spos_at: Instant,
    spos_valid: bool,
   ///  SMTC position   /// position
    smtc_rep_pos: f64,
    /// position    /// 0.5~1s     ///  2.30 / 2.31 450ms 
   ///
    netease_prev_raw: f64,
    netease_raw_move_at: Instant,
   /// CDP ?
    last_soda_heal: Instant,
   ///
    soda_boot_checked: bool,
    /// ?/
    soda_tick_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
   /// CDP soda_running+cdp_up
    soda_live_cached: bool,
    /// Some(true)=Some(false)=
   /// None=SMTC /
    soda_playing: Option<bool>,
    soda_live_at: Instant,
   /// 205ms
    soda_live_rx: Option<std::sync::mpsc::Receiver<bool>>,
   ///
    soda_pos_at: Instant,
   /// CDP  UI
    soda_prog_rx: Option<
        std::sync::mpsc::Receiver<(
            Option<(f64, f64, f64)>,
            Option<bool>,
            Option<crate::cdp_lyric::SodaTrialMeta>,
        )>,
    >,
   /// SMTC
    soda_dur: f64,
   /// track.duration
    soda_full_dur: f64,
   ///  VIP
    soda_clip_start: f64,
   ///  VIP
    soda_clip_dur: f64,
   /// 30
    soda_trial_active: bool,
    ///  DOM 
    soda_trial_hint: bool,
    /// DOM  1    /// 
    soda_last_p: f64,
    /// 
    lyr_retry_at: Instant,
    lyr_retry_n: u8,
    lyr_inflight: bool,
    disc_angle: f32,
    arm_angle: f32,
    /// 
    drag_pending: Option<egui::Pos2>,
   /// / HTML
    tt_play: Option<egui::TextureHandle>,
    tt_pause: Option<egui::TextureHandle>,
    tt_base: Option<egui::TextureHandle>,
    tt_arm0: Option<egui::TextureHandle>,
    cover_tex: Option<egui::TextureHandle>,
    /// 
    fallback_cover_tex: Option<(u64, egui::TextureHandle)>,
    soda_cover_hash: u64,
    upd_badge_tex: Option<egui::TextureHandle>,
    /// :1  Otto  FACES 
    face_frames: Vec<Option<Vec<egui::TextureHandle>>>,
    ///  UV =
    face_content_uv: Vec<Option<(f32, f32, f32, f32)>>,
    /// SMTC  - 
    media_title_at: Instant,
    media_title_cache: Option<(String, String, String)>,
    /// EnumWindows 
    media_title_rx: Option<std::sync::mpsc::Receiver<Option<(String, String, String)>>>,
    /// 50ms+ 
    player_running_cached: bool,
    player_running_at: Instant,
   ///
    music_lost_since: Option<Instant>,
    /// music_present  400ms     ///  false music_present 
    music_was_present: bool,
    /// /
    last_title: Option<(String, String, String)>,
    last_title_at: Instant,
    ///  0..1//
    music_amt: f32,
    ///  music_amt  60fps
    ///  false 16.7fps CPU
    music_motion: bool,
    /// 
    lyr_idx_last: i32,
    /// 
    lyr_anim_at: Option<Instant>,
    lyr_anim_from: Option<usize>,
    /// =
    lyr_drag_off: f32,
    /// `lyr_drag_off` 
    lyr_scroll_target: f32,
    /// 
    spec_phase: f64,
    /// 
    spec_speed: f32,
    /// 
    spec_last_t: f64,
   /// 0..127   /// water-reflection-test.html fakeBins
    wave_idle: [f32; 128],
    /// 
    wave_play: [f32; 128],
    wave_disp: [f32; 128],
    /// /=1=
    wave_mix: f32,
    ///  60fps 5fps 
    wave_active: bool,
    /// 0 0.35s
    wave_onset: f32,
    ///  60fps200Hzvsync 200fps
    frame_last: Instant,
    /// (
   ///  request_repaint_after 60fps
    prev_req_ms: f32,
    /// /// 600ms     ///  CPU 
    last_busy: Instant,
   ///  Some 0
    intro_start: Option<Instant>,
    /// ?
    boot_at: Instant,
   /// /
    intro_done: bool,
   /// DWM cloak
    cloaked: bool,
    /// cloak  apply_geometry 
    cloak_frames: u8,
   /// ///?0..1   ///
    cb_anim: f32,
   ///
    exit_start: Option<Instant>,
   ///
    lyr_ci_anim: f32,
    lyr_release_at: Option<Instant>,
    /// 
    lyr_return_from: f32,
    lyr_return_start: Option<Instant>,
   /// VIP = 0
    trial_start: f32,
   /// ,  <60s
    trial_probe: Option<(Instant, u32)>,
    /// freeTrialInfo
    trial_since: String,
    ///  300ms 
    pending_title: Option<(String, Instant)>,
    netease_at: Instant,
    /// =20 
    netease_dur: f64,
   ///
    ctrl_hit: Vec<egui::Rect>,
    /// //
    active: Module,
   ///
    show_settings: bool,
    settings_scroll: f32,
    ///  settings_scroll 
    settings_scroll_target: f32,

    settings_scroll_vis: f32,
    /// 
    settings_scroll_act: std::time::Instant,
    /// WINISLE_PAGE=settings/schedule/weather 
    dbg_page: Option<DbgView>,
    ///   / 
    rail_anim_at: Option<std::time::Instant>,
    rail_from: f32,
    last_active: Module,
    /// None=
    upd_ver: Option<update::Manifest>,
    /// 
    upd_msg: String,
    upd_checking: bool,
    upd_busy: bool,
    upd_last_check: Instant,
    upd_rx: Option<std::sync::mpsc::Receiver<Result<Option<update::Manifest>, String>>>,
    upd_inst_rx: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    /// UI ?
    upd_dl_pct: std::sync::Arc<std::sync::atomic::AtomicU8>,
    // =====  =====

    w_data: Option<weather::Weather>,
    /// 
    w_rx: Option<std::sync::mpsc::Receiver<Result<weather::Weather, String>>>,

    w_inflight: bool,
    /// 
    w_last_fetch: Instant,
    /// UI 
    w_err: Option<String>,
    ///  / 
    w_notice: String,
    w_notice_err: bool,
    w_notice_until: Option<Instant>,

    w_fetch_label: String,

    w_scroll: f32,
    w_scroll_target: f32,
    /// 天气温度文本实测宽度：天气槽宽 = 图标 20 + 间距 6 + 温度文字宽度。
    /// 模块左右各 5px 由外层统一间距提供，避免槽内留白把相邻模块推远。
    w_temp_w: f32,
    /// 时间文本实测宽度（含秒/不含秒按实际格式）：时间槽宽只包住文字，避免固定槽宽裁字或留白。
    w_clock_w: f32,
    /// ?0..11.2 
    w_scroll_vis: f32,
    /// 
    w_scroll_act: std::time::Instant,

    w_city_search: bool,

    w_city_search_from_weather: bool,

    w_search_need_focus: bool,
   ///
    w_search_focus_tries: u8,
    /// 
    w_activatable_last: bool,

    w_city_search_anim: f32,

    w_search_query: String,
    w_search_results: Vec<weather::CityHit>,

    w_search_results_at: Option<Instant>,
    w_search_rx: Option<std::sync::mpsc::Receiver<Vec<weather::CityHit>>>,
    w_search_inflight: bool,
    w_search_last_edit: Instant,
    w_search_last_query: String,
    // =====  =====
    /// ?/ ?/ ?data/todo.json
    todos: Vec<Todo>,
    ///  id ?
    todo_next_id: u64,
    /// 
    todo_done_seq: u64,
    ///  tab?= 1=
    todo_tab: u8,
    /// 0= 1=2=
    todo_filter: u8,
    /// 
    todo_menu_open: bool,
   ///
    todo_editing: bool,
    ///  
    todo_debug_edit: bool,
    /// ?
    todo_input: String,
    /// ?/ 
    todo_need_focus: bool,
    todo_focus_tries: u8,
    /// 
    /// lost_focus 
    todo_was_focused: bool,
    /// 
    /// ointer 
    todo_addrow_hover: bool,
    /// 调试用：FAKE_DELETE 仅删除一次
    todo_fake_deleted: bool,
    /// ?+ egui time?
    /// / 
    todo_scroll: f32,
    todo_scroll_target: f32,
    ///  1.2s 
    todo_scroll_vis: f32,
    todo_scroll_act: std::time::Instant,
    /// tab /  + 
    todo_anim: f32,

    // =====  =====
    /// data/reminders.json
    reminders: Vec<Reminder>,
    remind_next_id: u64,
    /// idNone=/ 
    remind_alert: Option<u64>,
    /// 15s 
    remind_alert_at: Option<Instant>,
    ///  id
    remind_dot: Option<u64>,
    remind_dot_at: Option<Instant>,
   /// None=
    remind_form: Option<RemindForm>,
    /// 
    remind_form_focus: bool,
    remind_focus_tries: u32,
    remind_interval_tries: u32,
        remind_interval_cursor: usize,
        remind_active_field: u8,
    remind_soft_hover: bool,
    ///  
    remind_scroll: f32,
    remind_scroll_target: f32,
    ///  1.2s 
    remind_scroll_vis: f32,
    remind_scroll_act: std::time::Instant,
    /// 
    remind_form_open: bool,
        remind_list_anim: f32,
        remind_lista_force: Option<f32>,
        remind_anim: f32,
    /// 0..1=1=
    remind_alert_anim: f32,
    sim_click_stage: u8,
    remind_glow_tex: Option<egui::TextureHandle>,
    remind_glow_blue: Option<egui::TextureHandle>,
    remind_glow_amber: Option<egui::TextureHandle>,
    remind_glow_mint: Option<egui::TextureHandle>,
    remind_glow18_blue: Option<egui::TextureHandle>,
    remind_glow18_amber: Option<egui::TextureHandle>,
    remind_glow18_mint: Option<egui::TextureHandle>,
    remind_glow_phases: Vec<Option<egui::TextureHandle>>,
    /// Some(0)=Some(1)=
    remind_tp_open: Option<u8>,
    remind_tp_hour: u32,
    remind_tp_min: u32,
    remind_tp_hs: f32,
        remind_tp_hs_t: f32,
    remind_tp_ms: f32,
        remind_tp_ms_t: f32,
    ///  hover
    remind_dbg_hover: Option<u64>,
    /// 待处理提醒队列（仅存 id）
    remind_queue: Vec<u64>,
    remind_mlist_sig: u64,
    remind_mlist_anim: f32,
    remind_closing: bool,
    remind_mlist_mode: bool,
    remind_morph: f32,
}

impl IslandApp {
    fn new(tray: Option<Tray>, open_panel: bool) -> Self {
        let cfg = Config::load();
        let (lyr_tx, lyr_req_rx) =
            std::sync::mpsc::channel::<(String, String, String, f64, bool, bool)>();
        let (lyr_res_tx, lyr_rx) =
            std::sync::mpsc::channel::<(String, Option<Vec<lyrics::LrcLine>>, Option<Vec<u8>>, bool)>();
        std::thread::spawn(move || {
            while let Ok((key, title, artist, dur, soda, light)) = lyr_req_rx.recv() {
                // light=trueCDP 
                // light=falseCDP +  +  +                // settled=true ? / / 
                // false ??
let (r, cov, settled) = if light {
                    match lyrics::soda_probe_cdp(&title, &artist) {
                        lyrics::SodaProbeResult::Got(v) => (Some(v), None, true),
                        lyrics::SodaProbeResult::NoLyric => (None, None, true),
                        lyrics::SodaProbeResult::Unknown => (None, None, false),
                    }
                } else {
                    let (r, cov) = lyrics::fetch_full(&title, &artist, dur, soda);
                    //                    //  CDP settled=false
                    let settled = r.is_some() || !soda;
                    (r, cov, settled)
                };
                // ?
                {
                    use std::io::Write;
                    let p = crate::paths::file("lyrics_debug.log");
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
                        let n = r.as_ref().map(|v| v.len()).unwrap_or(0);
                        let cb = cov.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = writeln!(f, "{} - {} => lines={} cover={}B", title, artist, n, cb);
                    }
                }
                if lyr_res_tx.send((key, r, cov, settled)).is_err() {
                    break;
                }
            }
        });



        let dbg_page = match std::env::var("WINISLE_PAGE").ok().as_deref() {
            Some("settings") => Some(DbgView::Settings),
            Some("schedule") => Some(DbgView::Schedule),
            Some("todo") => Some(DbgView::Todo),
            Some("reminder") => Some(DbgView::Reminder),
            Some("weather") => Some(DbgView::Weather),
            _ => None,
        };
        // /
        let w_data = weather::load_cache(&cfg.weather_city);
        let w_fetch_label = if w_data.is_some() { "缓存".to_string() } else { String::new() };
        // ?data/todo.json 
        let (todos, todo_next_id, todo_done_seq) = load_todos();
        let (reminders, remind_next_id) = load_reminders();
        //   =  +
        let mut reminders = reminders;
        let boot_now = now_secs();
        for r in reminders.iter_mut() {
            if r.enabled {
                r.next_due = boot_now + r.interval_min.max(1) as i64 * 60;
            } else {
                r.next_due = 0;
            }
        }
        save_reminders(&reminders);
        let mut app = Self {
            hwnd: 0,
            initialized: false,
            expanded: 0.0,
            target: 0.0,
            collapse_at: None,
            monitor: egui::vec2(1920.0, 1080.0),
            monitor_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1080.0)),
            last_geom: (0.0, 0.0, -9999.0, -9999.0),
            last_tick: std::time::Instant::now(),
            cfg,
            settings_open: open_panel,
            confirm_open: false,
            settings_hwnd: 0,
            settings_shown: false,
            island_visible: true,
            last_trigger: trigger_mtime(),
            tray,
            focus_settings: open_panel,
            dragging: false,
            drag_off: egui::vec2(0.0, 0.0),
            drag_frac: egui::vec2(0.5, 0.5),
            drag_last_btn: false,
            settle: None,
            full: false,
            hover_hold: None,
            region: (i32::MIN, 0, 1, 1, 0),
            perf_region_ms: 0.0,
            perf_netease_ms: 0.0,
            click_through: false,
            music: smtc::Track::default(),
            tracker: smtc::Tracker::new(),
            lyr_tx,
            lyr_rx,
            lyr_key: String::new(),
            lyr_dur: 0.0,
            lyr_lines: Vec::new(),
            lyr_done: false,
            lyr_settled: false,
            clock_base: 0.0,
            clock_start: Instant::now(),
            clock_playing: false,
            prev_playing: false,
            from_smtc: false,
            ui_playing: true,
            override_play: None,
            pause_pending_at: None,
            spos: 0.0,
            spos_at: Instant::now(),
            spos_valid: false,
            smtc_rep_pos: -1.0,
            netease_prev_raw: -1.0,
            netease_raw_move_at: Instant::now(),
            last_soda_heal: Instant::now() - Duration::from_secs(3600),
            soda_boot_checked: false,
            soda_tick_busy: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            soda_live_cached: false,
            soda_playing: None,
            soda_live_at: Instant::now() - Duration::from_secs(3600),
            soda_live_rx: None,
            soda_pos_at: Instant::now() - Duration::from_secs(3600),
            soda_prog_rx: None,
            soda_dur: 0.0,
            soda_full_dur: 0.0,
            soda_clip_start: 0.0,
            soda_clip_dur: 0.0,
            soda_trial_active: false,
            soda_trial_hint: false,
            soda_last_p: -1.0,
            lyr_retry_at: Instant::now() - Duration::from_secs(3600),
            lyr_retry_n: 0,
            lyr_inflight: false,
            disc_angle: 0.0,
            arm_angle: -22.0,
            drag_pending: None,
            tt_play: None,
            tt_pause: None,
            tt_base: None,
            tt_arm0: None,
            cover_tex: None,
            fallback_cover_tex: None,
            soda_cover_hash: 0,
        upd_badge_tex: None,
            face_frames: (0..FACES.len()).map(|_| None).collect(),
            face_content_uv: (0..FACES.len()).map(|_| None).collect(),
            media_title_at: Instant::now() - Duration::from_secs(10),
            media_title_cache: None,
            media_title_rx: None,
            player_running_cached: false,
            player_running_at: Instant::now() - Duration::from_secs(10),
            music_lost_since: None,
            music_was_present: false,
            last_title: None,
            last_title_at: Instant::now() - Duration::from_secs(60),
            music_amt: 0.0,
            music_motion: false,
            lyr_idx_last: -1,
            lyr_anim_at: None,
            lyr_anim_from: None,
            lyr_drag_off: 0.0,
            lyr_scroll_target: 0.0,
            spec_phase: 0.0,
            spec_speed: 5.2,
            spec_last_t: 0.0,
            wave_idle: [0.52; 128],
            wave_play: [0.0; 128],
        wave_disp: [0.52; 128],
            wave_mix: 0.0,
        wave_active: false,
        wave_onset: 0.0,
            frame_last: Instant::now() - Duration::from_millis(20),
            prev_req_ms: 16.0,
            last_busy: Instant::now(),
            intro_start: None,
            boot_at: Instant::now(),
            //             // / falseintro_dy 14.0draw  0.16/0.34 
            intro_done: true,
            cloaked: false,
            cloak_frames: 0,
            cb_anim: 0.0,
            exit_start: None,
            lyr_ci_anim: -1.0,
            lyr_release_at: None,
            lyr_return_from: 0.0,
            lyr_return_start: None,
            trial_start: 0.0,
            trial_probe: None,
            trial_since: String::new(),
            pending_title: None,
            netease_at: Instant::now(),
        netease_dur: 0.0,
            // netease reader started below
            ctrl_hit: Vec::new(),
            active: Module::Music,
            show_settings: false,
            settings_scroll: 0.0,
            settings_scroll_target: 0.0,
            settings_scroll_vis: 0.0,
            settings_scroll_act: std::time::Instant::now() - std::time::Duration::from_secs(10),
            dbg_page,
            rail_anim_at: None,
            rail_from: 0.0,
            last_active: Module::Music,
            upd_ver: None,
            upd_msg: String::new(),
            upd_checking: false,
            upd_busy: false,
            upd_last_check: Instant::now() - Duration::from_secs(5 * 3600),
            upd_rx: None,
            upd_inst_rx: None,
            upd_dl_pct: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            w_data,
            w_rx: None,
            w_inflight: false,
            w_last_fetch: Instant::now() - Duration::from_secs(3600),
            w_err: None,
            w_notice: String::new(),
            w_notice_err: false,
            w_notice_until: None,
            w_fetch_label,
            w_scroll: 0.0,
            w_scroll_target: 0.0,
            w_temp_w: 18.0,
            w_clock_w: CW_CLOCK,
            w_scroll_vis: 0.0,
            w_scroll_act: std::time::Instant::now() - std::time::Duration::from_secs(10),
            w_city_search: false,
            w_city_search_from_weather: false,
            w_search_need_focus: false,
            w_search_focus_tries: 0,
            w_activatable_last: false,
            w_city_search_anim: 0.0,
            w_search_query: String::new(),
            w_search_results: Vec::new(),
            w_search_results_at: None,
            w_search_rx: None,
            w_search_inflight: false,
            w_search_last_edit: Instant::now(),
            w_search_last_query: String::new(),
            todos,
            todo_next_id,
            todo_done_seq,
            todo_tab: 0,
            todo_filter: 0,
            todo_menu_open: false,
            todo_editing: false,
            todo_debug_edit: false,
            todo_input: String::new(),
            todo_need_focus: false,
            todo_focus_tries: 0,
            todo_was_focused: false,
            todo_addrow_hover: false,
            todo_fake_deleted: false,
            todo_scroll: 0.0,
            todo_scroll_target: 0.0,
            todo_scroll_vis: 0.0,
            todo_scroll_act: std::time::Instant::now() - std::time::Duration::from_secs(10),
            todo_anim: 1.0,
            reminders,
            remind_next_id,
            remind_alert: None,
            remind_alert_at: None,
            remind_dot: None,
            remind_dot_at: None,
            remind_form: None,
            remind_form_focus: false,
    remind_focus_tries: 0,
    remind_interval_tries: 0,
            remind_interval_cursor: 0,
            remind_active_field: 0,
    remind_soft_hover: false,
            remind_scroll: 0.0,
            remind_scroll_target: 0.0,
            remind_scroll_vis: 0.0,
            remind_scroll_act: std::time::Instant::now() - std::time::Duration::from_secs(10),
            remind_form_open: false,
            remind_list_anim: 1.0,
            remind_lista_force: std::env::var("WINISLE_REMIND_LISTA").ok().and_then(|v| v.parse::<f32>().ok()),
            remind_anim: 0.0,
    remind_alert_anim: 0.0,
    sim_click_stage: 0,
    remind_glow_tex: None,
    remind_glow_blue: None,
    remind_glow_amber: None,
    remind_glow_mint: None,
    remind_glow18_blue: None,
    remind_glow18_amber: None,
    remind_glow18_mint: None,
    remind_glow_phases: vec![None; 44],
            remind_tp_open: None,
            remind_tp_hour: 9,
            remind_tp_min: 0,
            remind_tp_hs: 0.0,
            remind_tp_hs_t: 0.0,
            remind_tp_ms: 0.0,
            remind_tp_ms_t: 0.0,
            remind_dbg_hover: None,
            remind_queue: Vec::new(),
            remind_mlist_sig: 0,
            remind_mlist_anim: 1.0,
            remind_closing: false,
            remind_mlist_mode: false,
            remind_morph: 1.0,
        };
        if let Ok(v) = std::env::var("WINISLE_CITY_SEARCH") {
            app.show_settings = true;
            app.w_city_search = true;
            app.w_search_need_focus = true;
            app.w_search_focus_tries = 5;
            app.w_city_search_anim = 1.0;
            if v == "weather" {
                app.active = Module::Weather;
                app.last_active = Module::Weather;
                app.rail_from = 1.0;
                app.w_city_search_from_weather = true;
            }
        }
        if let Ok(q) = std::env::var("WINISLE_SEARCH_QUERY") {
            app.w_search_query = q;
            app.w_search_last_edit = Instant::now() - Duration::from_millis(500);
        }
        //  tab / / 
        if let Ok(v) = std::env::var("WINISLE_TODO_TAB") {
            if v == "archive" { app.todo_tab = 1; }
        }
        if let Ok(v) = std::env::var("WINISLE_TODO_FILTER") {
            app.todo_filter = match v.as_str() { "todo" => 1, "done" => 2, _ => 0 };
        }
        // 1= 2= 3= 4= 5=
if let Ok(v) = std::env::var("WINISLE_REMIND_FIRE") {
            if let Ok(icon) = v.parse::<u8>() {
                if let Some(r) = app.reminders.iter().find(|r| r.icon == icon) {
                    app.remind_alert = Some(r.id);
                    app.remind_alert_at = Some(Instant::now());
                }
            }
        }
        if std::env::var_os("WINISLE_REMIND_MULTI").is_some() {
            let ids: Vec<u64> = app.reminders.iter().take(4).map(|r| r.id).collect();
            app.remind_queue = ids.clone();
            if let Some(f) = ids.first() {
                app.remind_alert = Some(*f);
                app.remind_alert_at = Some(Instant::now());
            }
        }
        // debug: 列表粘滞（仅剩 1 条但仍按列表渲染）
        if std::env::var_os("WINISLE_MLIST_ONE").is_some() {
            if let Some(r0) = app.reminders.iter().find(|r| r.icon <= 4).map(|r| r.id) {
                app.remind_queue = vec![r0];
                app.remind_alert = Some(r0);
                app.remind_alert_at = Some(Instant::now());
                app.remind_mlist_mode = true;
            }
        }
        // debug: 进行中的淡出（队列已空、closing）
        if std::env::var_os("WINISLE_MLIST_CLOSE").is_some() {
            if let Some(r0) = app.reminders.iter().find(|r| r.icon <= 4).map(|r| r.id) {
                app.remind_queue.clear();
                app.remind_alert = Some(r0);
                app.remind_alert_at = None;
                app.remind_closing = true;
                app.remind_mlist_mode = true;
                app.remind_alert_anim = 0.55;
            }
        }
        // 
        if std::env::var_os("WINISLE_REMIND_FORM").is_some() {
            let nm = std::env::var("WINISLE_REMIND_FORM").unwrap_or_default();
            app.remind_form = Some(RemindForm {
                id: 0,
                name: if nm == "empty" { String::new() } else { "站起来拉伸".to_string() },
                icon: 4,
                interval_min: 90,
                interval_text: "90".to_string(),
                window_start: 540,
                window_end: 1200,
                days: 0b0111110,
                repeat: true,
                sound: true,
            });
            app.remind_form_focus = true;
            app.remind_focus_tries = 0;
            app.remind_form_open = true;
            app.remind_anim = 0.0;
        }
        if let Ok(v) = std::env::var("WINISLE_REMIND_EDIT") {
            if let Ok(eid) = v.parse::<u64>() {
                if let Some(r) = app.reminders.iter().find(|r| r.id == eid) {
                    app.remind_form = Some(RemindForm {
                        id: r.id,
                        name: r.name.clone(),
                        icon: r.icon,
                        interval_min: r.interval_min,
                        interval_text: r.interval_min.to_string(),
                        window_start: r.window_start,
                        window_end: r.window_end,
                        days: r.days,
                        repeat: r.repeat,
                        sound: true,
                    });
                    app.remind_form_open = true;
                    app.remind_anim = 1.0;
                }
            }
        }
        // ==
if let Ok(v) = std::env::var("WINISLE_REMIND_TP") {
            if app.remind_form.is_none() {
                app.remind_form = Some(RemindForm {
                    id: 0,
                    name: "站起来拉伸".to_string(),
                    icon: 4,
                    interval_min: 90,
                    interval_text: "90".to_string(),
                    window_start: 540,
                    window_end: 1200,
                    days: 0b0111110,
                    repeat: true,
                    sound: true,
                });
            }
            if let Some(f) = app.remind_form.as_ref() {
                let which: u8 = v.parse().unwrap_or(0);
                let m = if which == 0 { f.window_start } else { f.window_end };
                app.remind_tp_open = Some(which);
                app.remind_tp_hour = m / 60;
                app.remind_tp_min = m % 60;
                app.remind_tp_hs = app.remind_tp_hour as f32 * 26.0;
                app.remind_tp_ms = ((app.remind_tp_min / 5) as f32) * 26.0;
                app.remind_form_open = true;
                app.remind_anim = 0.0;
            }
        }
        //  hover 0 
        if let Ok(v) = std::env::var("WINISLE_REMIND_HOVER") {
            if let Ok(idx) = v.parse::<usize>() {
                if let Some(r) = app.reminders.get(idx) {
                    app.remind_dbg_hover = Some(r.id);
                }
            }
        }
        // 
if std::env::var_os("WINISLE_REMIND_DOT").is_some() {
            if let Some(r) = app.reminders.first() {
                app.remind_dot = Some(r.id);
                app.remind_dot_at = Some(Instant::now());
            }
        }
        // 
        if let Ok(v) = std::env::var("WINISLE_TODO_EDIT") {
            app.todo_debug_edit = true;
            app.todo_input = v;
        }
        // 
        if std::env::var_os("WINISLE_TODO_EMPTYEDIT").is_some() {
            app.todo_debug_edit = true;
            app.todo_input.clear();
        }
        // egui
        if std::env::var("WINISLE_TODO_DEBUG_FOCUS").is_ok() {
            app.todo_debug_edit = true;
            app.todo_need_focus = true;
            app.todo_focus_tries = 30;
        }
        // 
        if std::env::var_os("WINISLE_TODO_FAKE_ENTER").is_some() {
            app.todo_debug_edit = true;
            app.todo_input = "验证连续添加".to_string();
            app.todo_was_focused = true;
        }
        app
    }

    fn ensure_window(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.initialized {
            return;
        }
        if let Ok(handle) = frame.window_handle() {
            if let RawWindowHandle::Win32(w) = handle.as_raw() {
                self.hwnd = w.hwnd.get();
                platform::set_main_hwnd(self.hwnd);
                platform::make_island(self.hwnd);
                platform::install_erase_guard(self.hwnd);
                platform::start_click_through_poller(self.hwnd);
                // WS_EX_LAYERED)WS_EX_TRANSPARENT 
                // WS_EX_TRANSPARENT                // SetLayeredWindowAttributes(alpha=255)  GL/DWM alpha 
if std::env::var_os("WINISLE_NO_LAYERED").is_none() {
                    platform::set_layered(self.hwnd);
                }
                // 
                platform::fill_dark_now(self.hwnd);
                // eframe                 // **ShowWindowalpha=0/cloak**eframe 
                //  set_visible(true)eframe-0.31.1/src/native/epi_integration.rs:308-310 glow_integration.rs:158               // show/cloak
                self.refresh_monitor(ctx);
                self.apply_geometry(ctx);
                // DWM cloak：避免首帧闪白
                platform::set_cloak(self.hwnd, true);
                self.cloaked = true;
                self.cloak_frames = 0;
                self.initialized = true;
            }
        }
    }

    fn drive_animation(&mut self, ctx: &egui::Context) {
        if !self.island_visible {
            self.target = 0.0;
            self.expanded = 0.0;
            self.collapse_at = None;
            return;
        }
        let now = Instant::now();
        if std::env::var_os("WINISLE_FORCE_EXPAND").is_some() {

            self.target = 1.0;
            self.collapse_at = None;
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            let k = 1.0 - (-15.0 * dt).exp();
            self.expanded += (1.0 - self.expanded) * k;
            if (1.0 - self.expanded).abs() > 0.002 {
                ctx.request_repaint();
            }
            return;
        }
        if std::env::var_os("WINISLE_CYCLE").is_some() {
            let t = ctx.input(|i| i.time);
            self.target = if (t % 4.0) < 2.0 { 1.0 } else { 0.0 };
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            let k = 1.0 - (-15.0 * dt).exp();
            self.expanded += (self.target - self.expanded) * k;
            if (self.target - self.expanded).abs() > 0.002 {
                ctx.request_repaint();
            }
            return;
        }
        if self.dragging || self.settle.is_some() {
            self.target = 0.0;
            self.collapse_at = None;
        }
        let hover = if std::env::var_os("WINISLE_NO_HOVER").is_some() {
            false
        } else if self.dragging || self.settle.is_some() {
            false
        } else if let Some(t) = self.hover_hold {
            if now < t {
                false
            } else {
                self.hover_hold = None;
                self.hover(ctx, self.hwnd)
            }
        } else {
            self.hover(ctx, self.hwnd)
        };
        if hover {
            self.target = 1.0;
            self.collapse_at = None;
        } else if self.target > 0.5 {
            match self.collapse_at {
                None => self.collapse_at = Some(now + Duration::from_millis(self.cfg.delay_ms as u64)),
                Some(t) if now >= t => {
                    self.target = 0.0;
                    self.collapse_at = None;
                }
                _ => {}
            }
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let k = 1.0 - (-15.0 * dt).exp();
        self.expanded += (self.target - self.expanded) * k;
        if (self.expanded - self.target).abs() < 0.002 {
            self.expanded = self.target;
        } else {
            ctx.request_repaint();
        }
    }

    /// >= 
    fn expanded_size(&self) -> egui::Vec2 {
        egui::vec2(W_EXPANDED, H_EXPANDED)
    }

    /// 8 ease-out     /// 0=
fn intro_dy(&self) -> f32 {
        if !self.island_visible {
            return 0.0;
        }
        let t0 = match self.intro_start {
            Some(t) => t,
            None => return 0.0,
        };
        let el = t0.elapsed().as_secs_f32();
        let dur = 0.34_f32;
        if el >= dur {
            return 0.0;
        }
        let p = (el / dur).clamp(0.0, 1.0);
        // ease-out-cubic
        let e = 1.0 - (1.0 - p).powi(3);
        // 10px
        -10.0 * (1.0 - e)
    }

    /// = 0.18s
    fn exit_dy(&self) -> f32 {
        if let Some(t0) = self.exit_start {
            let p = (t0.elapsed().as_secs_f32() / 0.18).clamp(0.0, 1.0);
            // ease-in
            -24.0 * (p * p)
        } else {
            0.0
        }
    }

    /// 0    /// ?
fn step_cb_anim(&mut self, ctx: &egui::Context) {
        let target = if self.island_visible && self.expanded < 0.03 && self.target < 0.5 {
            1.0
        } else {
            0.0
        };
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let k = 1.0 - (-dt / 0.09).exp();
        self.cb_anim += (target - self.cb_anim) * k;
        if (self.cb_anim - target).abs() < 0.002 {
            self.cb_anim = target;
        } else {
            // [FIX] 16ms /  ~200fps 
ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn comp_order(&self) -> Vec<(String, bool)> {
        const KNOWN: [&str; 5] = ["spectrum", "lyrics", "face", "clock", "weather"];
        let mut out: Vec<(String, bool)> = Vec::new();
        for tok in self.cfg.collapsed_layout.split(',') {
            let t = tok.trim();
            if t.is_empty() {
                continue;
            }
            let (en, id) = if let Some(r) = t.strip_prefix('+') {
                (true, r.trim())
            } else if let Some(r) = t.strip_prefix('-') {
                (false, r.trim())
            } else {
                (true, t)
            };
            if KNOWN.contains(&id) && !out.iter().any(|(x, _)| x == id) {
                out.push((id.to_string(), en));
            }
        }
        for k in KNOWN {
            if !out.iter().any(|(x, _)| x == k) {
                out.push((k.to_string(), false));
            }
        }
        out
    }

    fn set_comp_order(&mut self, order: &[(String, bool)]) {
        let s = order
            .iter()
            .map(|(id, on)| format!("{}{}", if *on { "+" } else { "-" }, id))
            .collect::<Vec<_>>()
            .join(",");
        self.cfg.collapsed_layout = s;
    }

    fn comp_label(id: &str) -> &'static str {
        match id {
            "spectrum" => "频谱",
            "lyrics" => "歌词",
            "face" => "表情",
            "clock" => "时间",
            "weather" => "天气",
            _ => "?",
        }
    }

    /// 
fn comp_base_w(&self, id: &str) -> f32 {
        match id {
            "spectrum" => CW_SPECTRUM,
            "lyrics" => CW_LYRICS,
            "face" => self.face_slot_w(),
            "clock" => {
                let mut w = self.w_clock_w.max(CW_CLOCK);
                if self.show_time_update() {
                    w += UPD_BADGE_W + UPD_BADGE_GAP;
                }
                w
            }
            "weather" => self.weather_slot_w(),
            _ => 0.0,
        }
    }

    fn comp_eff_w(&self, id: &str) -> f32 {
        let base = self.comp_base_w(id);
        match id {
            "spectrum" => base * self.music_amt.clamp(0.0, 1.0),
            "lyrics" => base * self.music_amt.clamp(0.0, 1.0),
            "weather" => {
                if self.w_data.is_none() {
                    0.0
                } else {
                    base
                }
            }
            _ => base,
        }
    }

    /// /music_amt     /// 10px 
fn comp_gap(&self, prev: &str, cur: &str) -> f32 {
        let amt = self.music_amt.clamp(0.0, 1.0);
        let m = |id: &str| id == "spectrum" || id == "lyrics";
        if m(prev) && m(cur) {
            CB_GAP * amt
        } else if m(prev) || m(cur) {
            CB_GAP * (1.0 + amt) * 0.5
        } else {
            CB_GAP
        }
    }

    /// 0 (id, )
    fn active_comps(&self) -> Vec<(String, f32)> {
        let mut out = Vec::new();
        for (id, on) in self.comp_order() {
            if !on {
                continue;
            }
            let w = self.comp_eff_w(&id);
            //  music_amt 
            let is_music = id == "spectrum" || id == "lyrics";
            if w > 0.5 || (is_music && self.music_amt.clamp(0.0, 1.0) > 0.002) {
                out.push((id, w));
            }
        }
        out
    }

    /// [72, CW_MAX]
    fn collapsed_w(&self) -> f32 {
        let comps = self.active_comps();
        if comps.is_empty() {
            return W_COLLAPSED_FACE;
        }
        //  5px
        if comps.len() == 1 && comps[0].0 == "face" {
            return self.face_slot_w() + CB_EDGE * 2.0 + CB_OUTER_EXTRA * 2.0;
        }
        let mut w = CB_EDGE * 2.0;
        let mut prev: Option<&str> = None;
        for (id, cw) in comps.iter() {
            if let Some(p) = prev {
                w += self.comp_gap(p, id);
            }
            w += *cw;
            prev = Some(id.as_str());
        }
        // 首模块左边距 10px、末模块右边距 10px：模块内 5px + 胶囊外侧 5px。
        w += CB_OUTER_EXTRA * 2.0;
        w.clamp(W_COLLAPSED_FACE, CW_MAX)
    }

    fn order_base_width(&self, order: &[(String, bool)]) -> f32 {
        let widths: Vec<f32> = order
            .iter()
            .filter(|(_, on)| *on)
            .map(|(id, _)| self.comp_base_w(id))
            .collect();
        if widths.is_empty() {
            return 0.0;
        }
        let mut w = CB_EDGE * 2.0;
        for (i, cw) in widths.iter().enumerate() {
            if i > 0 {
                w += CB_GAP;
            }
            w += *cw;
        }
        w += CB_OUTER_EXTRA * 2.0;
        w
    }

    fn face_mode_active(&self) -> bool {
        self.comp_order().iter().any(|(id, on)| *on && id == "face")
    }

    /// MFAN (w, h, , )    /// OnceLock 
fn face_anim_meta() -> &'static Vec<(u16, u16, u16, usize)> {
        use std::sync::OnceLock;
        static CACHE: OnceLock<Vec<(u16, u16, u16, usize)>> = OnceLock::new();
        CACHE.get_or_init(|| {
            const ANIM: &[u8] = include_bytes!("assets/faces_anim.bin");
            let mut out = Vec::new();
            if ANIM.len() < 8 || &ANIM[0..4] != b"MFAN" {
                return out;
            }
            let count = u16::from_le_bytes([ANIM[6], ANIM[7]]) as usize;
            let mut p = 8usize;
            for _ in 0..count {
                if p + 6 > ANIM.len() {
                    break;
                }
                let w = u16::from_le_bytes([ANIM[p], ANIM[p + 1]]);
                let h = u16::from_le_bytes([ANIM[p + 2], ANIM[p + 3]]);
                let fc = u16::from_le_bytes([ANIM[p + 4], ANIM[p + 5]]);
                p += 6;
                let off = p;
                p += (w as usize) * (h as usize) * (fc as usize);
                out.push((w, h, fc, off));
            }
            out
        })
    }

    /// UV (u0,v0,u1,v1)    /// self
fn face_content_uv_compute(idx: usize) -> (f32, f32, f32, f32) {
        let metas = Self::face_anim_meta();
        let full = (0.0, 0.0, 1.0, 1.0);
        let (w, h, fc, off) = match metas.get(idx) {
            Some(m) => *m,
            None => return full,
        };
        const ANIM: &[u8] = include_bytes!("assets/faces_anim.bin");
        let (wu, hu, fcu) = (w as usize, h as usize, fc as usize);
        let stride = wu * hu;
        let (mut minx, mut miny, mut maxx, mut maxy) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for f in 0..fcu {
            let base = off + f * stride;
            if base + stride > ANIM.len() {
                break;
            }
            for y in 0..hu {
                let row = base + y * wu;
                for x in 0..wu {
                    if ANIM[row + x] > 16 {
                        if x < minx { minx = x; }
                        if x > maxx { maxx = x; }
                        if y < miny { miny = y; }
                        if y > maxy { maxy = y; }
                    }
                }
            }
        }
        if minx == usize::MAX {
            full
        } else {
            (
                minx as f32 / wu as f32,
                miny as f32 / hu as f32,
                (maxx + 1) as f32 / wu as f32,
                (maxy + 1) as f32 / hu as f32,
            )
        }
    }

    fn face_slot_w(&self) -> f32 {
        let aspect = Self::face_content_aspect(self.cfg.idle_face as usize);
        let max_h = (H_COLLAPSED * 1.2).max(10.0);
        (max_h * aspect).min(CW_FACE).max(8.0)
    }

    /// 20 +  6 +     /// 5px 
fn weather_slot_w(&self) -> f32 {
        if self.w_data.is_none() {
            return CW_WEATHER;
        }
        20.0 + 6.0 + self.w_temp_w.max(10.0)
    }

    ///    /// + idx 
fn face_content_aspect(idx: usize) -> f32 {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        static CACHE: OnceLock<Mutex<HashMap<usize, f32>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(v) = cache.lock().unwrap().get(&idx) {
            return *v;
        }
        let metas = Self::face_anim_meta();
        let (w, h) = metas
            .get(idx)
            .map(|m| (m.0 as f32, m.1 as f32))
            .unwrap_or((77.0, 80.0));
        let (u0, v0, u1, v1) = Self::face_content_uv_compute(idx);
        let aspect = ((w * (u1 - u0)) / (h * (v1 - v0)).max(1.0)).max(0.2);
        cache.lock().unwrap().insert(idx, aspect);
        aspect
    }

    fn face_content_uv(&mut self, idx: usize) -> (f32, f32, f32, f32) {
        let metas = Self::face_anim_meta();
        if self.face_content_uv.len() != metas.len() {
            self.face_content_uv = (0..metas.len()).map(|_| None).collect();
        }
        if let Some(uv) = self.face_content_uv.get(idx).and_then(|o| *o) {
            return uv;
        }
        let uv = Self::face_content_uv_compute(idx);
        if let Some(slot) = self.face_content_uv.get_mut(idx) {
            *slot = Some(uv);
        }
        uv
    }

    /// Otto + 
fn ensure_face_frames(&mut self, ctx: &egui::Context, idx: usize) {
        const ANIM: &[u8] = include_bytes!("assets/faces_anim.bin");
        let metas = Self::face_anim_meta();
        if self.face_frames.len() != metas.len() {
            self.face_frames = (0..metas.len()).map(|_| None).collect();
        }
        if idx >= metas.len() || self.face_frames[idx].is_some() {
            return;
        }
        let (w, h, fc, off) = metas[idx];
        let (w, h, fc) = (w as usize, h as usize, fc as usize);
        let stride = w * h;
        let mut frames: Vec<egui::TextureHandle> = Vec::with_capacity(fc);
        for f in 0..fc {
            let base = off + f * stride;
            if base + stride > ANIM.len() {
                break;
            }
            let src = &ANIM[base..base + stride];
            let mut rgba = Vec::with_capacity(stride * 4);
            for &a in src {
                rgba.push(255);
                rgba.push(255);
                rgba.push(255);
                rgba.push(a);
            }
            let ci = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
            let tex = ctx.load_texture(
                format!("face_fr_{}_{}", idx, f),
                ci,
                egui::TextureOptions::LINEAR,
            );
            frames.push(tex);
        }
        if !frames.is_empty() {
            self.face_frames[idx] = Some(frames);
        }
    }


    /// SMTC     /// 1 
fn player_is_running(&mut self) -> bool {
        if self.player_running_at.elapsed() >= Duration::from_secs(1) {
            self.player_running_cached = platform::player_running();
            self.player_running_at = Instant::now();
        }
        self.player_running_cached
    }

    fn is_playing(&self) -> bool {
        if let Some((v, t)) = self.override_play {
            if std::time::Instant::now() < t {
                return v;
            }
        }
        //   SMTC
        if self.app_is_soda() {
            if let Some(p) = self.soda_playing {
                return p;
            }
        }
        if self.from_smtc { self.music.playing } else { self.ui_playing }
    }

    /// =   /// 
    /// 0 = 
    /// 1 =     /// 2 =  CDP 
    fn music_link_state(&self) -> u8 {
        if std::env::var_os("WINISLE_LINK_BAD").is_some() {
            return 2;
        }
        if self.music.title.is_empty() {
            return 2;
        }
        let a = self.music.app_id.to_ascii_lowercase();
        let is_soda = a.contains("soda") || a.contains("qishui") || self.music.app_id.contains("汽水");
        // CDP CDP        //  SMTCfrom_smtc CDP SMTC 
        //  CDP ?
let conn_ok = if is_soda { self.soda_live_cached || self.from_smtc } else { true };
        if !conn_ok {
            return 2;
        }
        let lyrics_ok = !self.lyr_lines.is_empty() || !self.lyr_done;
        if lyrics_ok { 0 } else { 1 }
    }

    fn current_pos(&self) -> f64 {
        if self.spos_valid {
            if self.is_playing() {
                self.spos + self.spos_at.elapsed().as_secs_f64()
            } else {
                self.spos
            }
        } else if self.clock_playing {
            self.clock_base + self.clock_start.elapsed().as_secs_f64()
        } else {
            self.clock_base
        }
    }

    /// VIP 
    fn lyric_time(&self) -> f64 {
        self.current_pos() + self.trial_start as f64
    }

    ///     /// 
    fn display_pos(&self) -> f64 {
        if self.app_is_soda() {
            self.lyric_time()
        } else {
            self.current_pos()
        }
    }

    fn app_is_soda(&self) -> bool {
        let a = self.music.app_id.to_ascii_lowercase();
        a.contains("汽水") || a.contains("soda") || a.contains("qishui")
    }

    fn upd_version(&self) -> Option<String> {
        self.upd_ver.as_ref().map(|m| m.version.clone())
    }

    fn show_time_update(&self) -> bool {
        match self.upd_version() {
            Some(v) => v != self.cfg.upd_seen_rail,
            None => false,
        }
    }

    fn show_rail_update(&self) -> bool {
        self.show_time_update()
    }

    /// ?
fn show_row_update(&self) -> bool {
        match self.upd_version() {
            Some(v) => v != self.cfg.upd_seen_row,
            None => false,
        }
    }

    fn mark_upd_seen_rail(&mut self) {
        if let Some(v) = self.upd_version() {
            if self.cfg.upd_seen_rail != v {
                self.cfg.upd_seen_rail = v;
                self.cfg.save();
            }
        }
    }

    fn mark_upd_seen_row(&mut self) {
        if let Some(v) = self.upd_version() {
            if self.cfg.upd_seen_row != v {
                self.cfg.upd_seen_row = v;
                self.cfg.save();
            }
        }
    }

    fn duration_est(&self) -> f64 {

        if self.app_is_soda() {
            //  30s
            if self.soda_trial_active && self.soda_full_dur > self.soda_dur + 20.0 {
                return self.soda_full_dur;
            }
            return if self.soda_dur >= 1.0 { self.soda_dur } else { self.music.duration };
        }
        // /
if self.music.duration >= 20.0 {
            return self.music.duration;
        }
        // 
if self.netease_dur >= 20.0 {
            return self.netease_dur;
        }
        // 
if let Some((_, d)) = platform::netease_posdur_cached() {
            if d >= 20.0 {
                return d;
            }
        }
        // 
        if !self.music.title.is_empty() {
            if let Some(d) = lyrics::known_duration(&self.music.title, &self.music.artist) {
                if d >= 20.0 {
                    return d;
                }
            }
        }
        let mut d = if self.music.duration > 1.0 { self.music.duration } else { 0.0 };
        if let Some(l) = self.lyr_lines.last() {

            if !lyrics::is_instrumental(&self.lyr_lines) {
                d = d.max(l.t + 6.0);
            }
        }
        d
    }

    fn lyric_duration(&self) -> f64 {
        if !self.music.title.is_empty() {
            if let Some(d) = lyrics::known_duration(&self.music.title, &self.music.artist) {
                if d >= 20.0 {
                    return d;
                }
            }
        }
        let d = self.duration_est();
        if d >= 60.0 {
            d
        } else {
            0.0
        }
    }

    fn cur_lyric_idx(&self) -> Option<usize> {
        if self.lyr_lines.is_empty() {
            return None;
        }
        let pos = self.lyric_time() + self.cfg.lyric_shift_ms as f64 / 1000.0;
        let mut idx = 0usize;
        for (i, l) in self.lyr_lines.iter().enumerate() {
            if l.t <= pos + 0.05 {
                idx = i;
            } else {
                break;
            }
        }
        Some(idx)
    }

    fn line_progress(&self, idx: usize) -> f32 {
        if idx >= self.lyr_lines.len() {
            return 0.0;
        }
        let pos = self.lyric_time() + self.cfg.lyric_shift_ms as f64 / 1000.0;
        let cur = self.lyr_lines[idx].t;
        //  yrc
        let segs = &self.lyr_lines[idx].segs;
        if !segs.is_empty() {
            let total: f32 = segs.iter().map(|s| s.n as f32).sum();
            if total <= 0.0 {
                return 0.0;
            }
            let mut sung = 0.0f32;
            for s in segs {
                let st = s.t;
                let ed = s.t + s.dur;
                if pos >= ed {
                    sung += s.n as f32;
                } else if pos > st {
                    let frac = if s.dur > 0.0 { ((pos - st) / s.dur).clamp(0.0, 1.0) } else { 0.0 };
                    sung += s.n as f32 * frac as f32;
                }
            }
            return (sung / total).clamp(0.0, 1.0);
        }
        let mut est = 0.30_f64;
        for ch in self.lyr_lines[idx].text.chars() {
            if ch.is_whitespace() {
                est += 0.16;
            } else {
                est += if (ch as u32) < 0x2E80 { 0.12 } else { 0.28 };
            }
        }
        let next = self.lyr_lines.get(idx + 1).map(|l| l.t).unwrap_or(cur + est);
        let gap = (next - cur).max(0.0);
        //  94% 
        let dur = if gap > 0.55 {
            (gap * 0.94).min(est * 2.2).max(0.55)
        } else {
            est.max(0.55)
        };
        ((pos - cur) / dur).clamp(0.0, 1.0) as f32
    }

    /// e e=1  expanded_size()
    fn side_grow_up(&self) -> bool {
        let mt = self.monitor_rect.min.y;
        let mh = self.monitor.y.max(1.0);
        let bh2 = (H_COLLAPSED + TOP_OVERLAP) * 0.5;
        let cy = (mt + self.cfg.y_frac * mh).clamp(mt + bh2, (mt + mh - bh2).max(mt + bh2));
        let room_up = (cy - bh2) - mt;
        let room_down = (mt + mh) - (cy + bh2);
        //    -
        let need = H_EXPANDED - H_COLLAPSED;
        if room_down >= need {
            false
        } else if room_up >= need {
            true
        } else {
            room_up > room_down
        }
    }

    ///     /// -1=(==(
fn top_h_anchor(&self) -> i32 {
        let mx = self.monitor_rect.min.x;
        let mw = self.monitor.x.max(1.0);
        let cw = self.collapsed_w();
        let bh = cw * 0.5;
        let bx = (mx + self.cfg.x_frac * mw).clamp(mx + bh, (mx + mw - bh).max(mx + bh));
        let half_extra = (W_EXPANDED - cw) * 0.5;
        let room_l = (bx - bh) - mx;
        let room_r = (mx + mw) - (bx + bh);
        if room_l >= half_extra && room_r >= half_extra {
            0
        } else if room_r >= room_l {
            -1
        } else {
            1
        }
    }

    fn pill_rect_e(&self, full: egui::Rect, e: f32) -> egui::Rect {
        let target = self.expanded_size();
        let cw = self.collapsed_w();
        let base_h = H_COLLAPSED;
        let w = cw + (target.x - cw) * e;
        let h = base_h + (target.y - base_h) * e;
        let fh = h + TOP_OVERLAP;
        // origin          // /        //          //  
let (x, y) = match self.cfg.position {
            DockPosition::LeftCenter => {
                let y = if self.side_grow_up() {
                    full.top() + (full.height() - (MARGIN - TOP_OVERLAP)) - fh
                } else {
                    full.top() + MARGIN - TOP_OVERLAP
                };
                (full.left(), y)
            }
            DockPosition::RightCenter => {
                let y = if self.side_grow_up() {
                    full.top() + (full.height() - (MARGIN - TOP_OVERLAP)) - fh
                } else {
                    full.top() + MARGIN - TOP_OVERLAP
                };
                (full.right() - w, y)
            }
            _ => {
                let x = match self.top_h_anchor() {
                    -1 => full.left() + MARGIN,
                    1 => full.right() - MARGIN - w,
                    _ => full.left() + (full.width() - w) * 0.5,
                };
                (x, full.top() + MARGIN - TOP_OVERLAP)
            }
        };
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, fh))
    }

    fn pill_rect(&self, full: egui::Rect) -> egui::Rect {
        self.pill_rect_e(full, self.expanded)
    }

    fn set_hit_region(&self, ctx: &egui::Context) {
        if self.hwnd == 0 {
            return;
        }
        if !self.island_visible {
            platform::set_hit_pill(0, 0, 0, 0);
            return;
        }
        let ppp = ctx.pixels_per_point();
        let (win, pill) = self.layout_screen();
        let mut local = egui::Rect::from_min_max(
            egui::pos2(pill.left() - win.left(), pill.top() - win.top()),
            egui::pos2(pill.right() - win.left(), pill.bottom() - win.top()),
        );
        platform::set_hit_pill(
            (local.left() * ppp) as i32,
            (local.top() * ppp) as i32,
            (local.right() * ppp) as i32,
            (local.bottom() * ppp) as i32,
        );
    }

    fn refresh_monitor(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point().max(0.5);
        let (mx, my) = (self.cfg.mon_x, self.cfg.mon_y);
        if let Some((l, t, r, b)) = platform::monitor_rect_at(mx as i32 + 2, my as i32 + 2) {
            let mw = (r - l) as f32 / ppp;
            let mh = (b - t) as f32 / ppp;
            if mw > 1.0 && mh > 1.0 {
                self.monitor_rect =
                    egui::Rect::from_min_size(egui::pos2(l as f32 / ppp, t as f32 / ppp), egui::vec2(mw, mh));
                self.monitor = egui::vec2(mw, mh);
                return;
            }
        }
        //  egui  0,0
        if let Some(ms) = ctx.input(|i| i.viewport().monitor_size) {
            if ms.x > 1.0 && ms.y > 1.0 {
                self.monitor = ms;
                self.monitor_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), ms);
            }
        }
    }

    /// (W_EXPANDEDH_EXPANDED)?SetWindowRgn ?
    ///  origin ?
    fn layout_screen(&self) -> (egui::Rect, egui::Rect) {
        let w = W_EXPANDED + 2.0 * MARGIN;
        let h = H_EXPANDED + 2.0 * MARGIN;
        let cw = self.collapsed_w();
        //  0,0
        let mx = self.monitor_rect.min.x;
        let my = self.monitor_rect.min.y;
        let mw = self.monitor.x.max(1.0);
        let mh = self.monitor.y.max(1.0);
        let full_size = egui::vec2(w, h);
        // /
let bar_h = H_COLLAPSED + TOP_OVERLAP;
        let bh2 = bar_h * 0.5;
        let origin = match self.cfg.position {
            DockPosition::LeftCenter | DockPosition::RightCenter => {
                // 
let cy = (my + self.cfg.y_frac * mh).clamp(my + bh2, (my + mh - bh2).max(my + bh2));
                //   cy                // local0    origin
                let local0 = if self.side_grow_up() {
                    h - (MARGIN - TOP_OVERLAP) - bar_h
                } else {
                    MARGIN - TOP_OVERLAP
                };
                let oy = (cy - bh2) - local0;
                let ox = if matches!(self.cfg.position, DockPosition::LeftCenter) {
                    mx
                } else {
                    mx + mw - w
                };
                egui::pos2(ox, oy)
            }
            _ => {
                //  origin e 
let bh = cw * 0.5;
                let bx = (mx + self.cfg.x_frac * mw).clamp(mx + bh, (mx + mw - bh).max(mx + bh));
                let ox = match self.top_h_anchor() {
                    -1 => (bx - bh) - MARGIN,
                    1 => (bx + bh) - (w - MARGIN),
                    _ => bx - w * 0.5,
                };
                egui::pos2(ox, my - MARGIN)
            }
        };
        //        // 
let origin = egui::pos2(origin.x, origin.y + self.intro_dy() + self.exit_dy());
        let full = egui::Rect::from_min_size(origin, full_size);
        let pill = self.pill_rect_e(full, self.expanded);
        (full, pill)
    }

    fn hover(&self, ctx: &egui::Context, hwnd: platform::HWND) -> bool {
        let ppp = ctx.pixels_per_point();
        let (cx, cy) = platform::cursor_pos();
        let (wl, wt, _wr, _wb) = platform::window_rect(hwnd);
        let local = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(W_EXPANDED + 2.0 * MARGIN, H_EXPANDED + 2.0 * MARGIN),
        );
        let pill = self.pill_rect(local);
        let scr = egui::Rect::from_min_max(
            egui::pos2(wl as f32 + pill.left() * ppp, wt as f32 + pill.top() * ppp),
            egui::pos2(wl as f32 + pill.right() * ppp, wt as f32 + pill.bottom() * ppp),
        );
        // 
        scr.expand(2.0).contains(egui::pos2(cx as f32, cy as f32))
    }

    fn handle_drag(&mut self, ctx: &egui::Context) {
        if self.hwnd == 0 {
            return;
        }
        let ppp = ctx.pixels_per_point();
        let down = platform::lbutton_down();
        let (cx, cy) = platform::cursor_pos();
        let cur = egui::pos2(cx as f32 / ppp, cy as f32 / ppp);
        let (_win, pill) = self.layout_screen();
        let inside = pill.expand(3.0).contains(cur);
        // 
        // 16px 
let handle_h = if self.expanded < 0.05 {
            pill.height()
        } else {
            16.0_f32.min(pill.height())
        };
        let handle = egui::Rect::from_min_max(
            egui::pos2(pill.left(), pill.top()),
            egui::pos2(pill.right(), pill.top() + handle_h),
        )
        .expand(2.0);
        let on_ctrl = self.ctrl_hit.iter().any(|r| r.contains(cur));

        //  
if down && !self.drag_last_btn {
            if inside && handle.contains(cur) && !on_ctrl && !self.dragging {
                self.drag_pending = Some(cur);
            }
        }
        // 
        if let Some(start) = self.drag_pending {
            if down {
                if !self.dragging && (cur - start).length() > 4.0 {
                    //  layout                     //                    // 
let (wl_px, wt_px, _wr_px, _wb_px) = platform::window_rect(self.hwnd);
                    let win0 = egui::pos2(wl_px as f32 / ppp, wt_px as f32 / ppp);
                    let local_full = egui::Rect::from_min_size(
                        egui::pos2(0.0, 0.0),
                        egui::vec2(W_EXPANDED + 2.0 * MARGIN, H_EXPANDED + 2.0 * MARGIN),
                    );
                    let lp0 = self.pill_rect_e(local_full, self.expanded);
                    // drag_frac                    //  drag_frac 
                    //
                    self.dragging = true;
                    self.drag_off = cur - (win0 + lp0.min.to_vec2());
                    let pw = lp0.width().max(1.0);
                    let ph = lp0.height().max(1.0);
                    self.drag_frac = egui::vec2(
                        (self.drag_off.x / pw).clamp(0.0, 1.0),
                        (self.drag_off.y / ph).clamp(0.0, 1.0),
                    );
                    self.settle = None;
                    self.hover_hold = None;
                    platform::capture_mouse(self.hwnd);
                }
            } else {
                self.drag_pending = None;
            }
        }
        if !down && self.dragging {
            self.dragging = false;
            self.drag_pending = None;
            platform::release_mouse();
            //  snap             // 
            let local_full = egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(W_EXPANDED + 2.0 * MARGIN, H_EXPANDED + 2.0 * MARGIN),
            );
            let lp_old = self.pill_rect_e(local_full, self.expanded);
            let (wl0, wt0, _wr0, _wb0) = platform::window_rect(self.hwnd);
            let bar_before = egui::pos2(wl0 as f32 / ppp, wt0 as f32 / ppp) + lp_old.min.to_vec2();
            self.snap_to_nearest(cur, ppp);
            //  300ms
            self.hover_hold = Some(Instant::now() + Duration::from_millis(300));
            // from bar_before
let lp_new = self.pill_rect_e(local_full, self.expanded);
            let (full, _pill) = self.layout_screen();
            let to = full.min;
            let from = bar_before - lp_new.min.to_vec2();
            self.settle = Some((from, to, Instant::now()));
        }
        self.drag_last_btn = down;
        platform::set_dragging(self.dragging);
        if self.dragging {
            // 
            platform::keep_on_top(self.hwnd);
            // DPI egui   ppp  ppp 
            // DPI 变化时重新确保岛体样式
            platform::ensure_island_style(self.hwnd);
            let win_w = W_EXPANDED + 2.0 * MARGIN;
            let win_h = H_EXPANDED + 2.0 * MARGIN;
            let placeholder = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(win_w, win_h));
            //              //drag_frac            // drive_animation target=0            //             //  drag_frac
            let lp = self.pill_rect_e(placeholder, self.expanded);
            // 
let pill_l = (lp.min.x * ppp).round() as i32;
            let pill_t = (lp.min.y * ppp).round() as i32;
            let pill_w = (lp.width() * ppp).round() as i32;
            let pill_h = (lp.height() * ppp).round() as i32;
            let (cxp, cyp) = platform::cursor_pos();
            //  +   
let grab_x = (lp.min.x + self.drag_frac.x * lp.width()) * ppp;
            let grab_y = (lp.min.y + self.drag_frac.y * lp.height()) * ppp;
            let mut nx = cxp - grab_x.round() as i32;
            let mut ny = cyp - grab_y.round() as i32;
            // 
let (vl, vt, vr, vb) = platform::virtual_screen_rect();
            let min_x = vl - pill_l;
            let max_x = (vr - pill_l - pill_w).max(min_x);
            let min_y = vt - pill_t;
            let max_y = (vb - pill_t - pill_h).max(min_y);
            nx = nx.clamp(min_x, max_x);
            ny = ny.clamp(min_y, max_y);
            platform::set_outer_position_px(self.hwnd, nx, ny);
            self.last_geom.2 = nx as f32 / ppp;
            self.last_geom.3 = ny as f32 / ppp;
        }
    }

    /// 吸附到最近的边
    /// 上/下/左/右
fn snap_to_nearest(&mut self, _cur: egui::Pos2, ppp: f32) {
        let win_w = W_EXPANDED + 2.0 * MARGIN;
        let win_h = H_EXPANDED + 2.0 * MARGIN;
        //  9        // GetWindowRect        //  cur-drag_off
        let local = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(win_w, win_h));
        let lp = self.pill_rect_e(local, self.expanded);
        let (wl_px, wt_px, _wr_px, _wb_px) = platform::window_rect(self.hwnd);
        let wl = wl_px as f32 / ppp;
        let wt = wt_px as f32 / ppp;
        let pill_min = egui::pos2(wl + lp.min.x, wt + lp.min.y);
        let pill_max = egui::pos2(wl + lp.max.x, wt + lp.max.y);
        let cx_log = (pill_min.x + pill_max.x) * 0.5;
        let cy_log = (pill_min.y + pill_max.y) * 0.5;
        let px = (cx_log * ppp) as i32;
        let py = (cy_log * ppp) as i32;
        let mon = platform::monitor_rect_at(px, py).or_else(|| platform::monitor_rect_of_window(self.hwnd));
        let (l, t, r, b) = match mon {
            Some(v) => v,
            None => (0, 0, self.monitor.x as i32, self.monitor.y as i32),
        };
        let ml = l as f32 / ppp;
        let mt = t as f32 / ppp;
        let mw = ((r - l) as f32 / ppp).max(1.0);
        let mh = ((b - t) as f32 / ppp).max(1.0);
        //  layout_screen/apply_geometry 
self.monitor_rect = egui::Rect::from_min_size(egui::pos2(ml, mt), egui::vec2(mw, mh));
        self.monitor = egui::vec2(mw, mh);
        // 
self.cfg.mon_x = l as f32;
        self.cfg.mon_y = t as f32;
        // / /         // 
let gap_top = (pill_min.y - mt).max(0.0);
        let gap_left = (pill_min.x - ml).max(0.0);
        let gap_right = ((ml + mw) - pill_max.x).max(0.0);
        // 
        // 
let side_magnet = 160.0_f32; // "逻辑像素：距左右边缘该范围内即触发侧边吸"
let gap_side = gap_left.min(gap_right);
        let pick_side = self.cfg.side_snap && (gap_side <= side_magnet || gap_side < gap_top);
        let cw = self.collapsed_w();
        if !pick_side {
            // x_frac
            self.cfg.position = DockPosition::TopCenter;
            let margin = (cw * 0.5) / mw;
            self.cfg.x_frac = ((cx_log - ml) / mw).clamp(margin, 1.0 - margin);
        } else {
            //  y_frac
            self.cfg.position = if gap_left <= gap_right {
                DockPosition::LeftCenter
            } else {
                DockPosition::RightCenter
            };
            let margin = ((H_COLLAPSED + TOP_OVERLAP) * 0.5) / mh;
            self.cfg.y_frac = ((cy_log - mt) / mh).clamp(margin, 1.0 - margin);
        }
        self.cfg.save();
    }

    /// ease-out 
fn step_settle(&mut self, ctx: &egui::Context) {
        let dur = Duration::from_millis(260);
        if let Some((from, to, start)) = self.settle {
            let t = (start.elapsed().as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0);
            //  ease-out-back
            let c1 = 0.55_f32;
            let c3 = c1 + 1.0;
            let u = t - 1.0;
            let mut e = 1.0 + c3 * u * u * u + c1 * u * u;
            // fromto  8 
let dist = (to - from).length();
            if e > 1.0 && dist > 0.5 {
                let tol = 8.0_f32;
                e = e.min(1.0 + tol / dist);
            }
            let p = from + (to - from) * e;
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(p));
            self.last_geom.2 = p.x;
            self.last_geom.3 = p.y;
            if t >= 1.0 {
                self.settle = None;
                // apply_geometry SetWindowPos 
ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_millis(8));
            }
        }
    }

    fn apply_geometry(&mut self, ctx: &egui::Context) {
        let (full, pill) = self.layout_screen();
        let (w, h) = (full.width(), full.height());
        let ppp = ctx.pixels_per_point();
        if (w - self.last_geom.0).abs() > 0.5 || (h - self.last_geom.1).abs() > 0.5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
        // Win32 SetWindowPos  winit
        // OuterPosition 4px        // 
let target_x = (full.min.x * ppp).round() as i32;
        let target_y = (full.min.y * ppp).round() as i32;
        if self.hwnd != 0 {
            let (wl, wt, _wr, _wb) = platform::window_rect(self.hwnd);
            if wl != target_x || wt != target_y {
                platform::set_outer_position_px(self.hwnd, target_x, target_y);
            }
        }
        self.last_geom = (w, h, full.min.x, full.min.y);
        if std::env::var_os("WINISLE_GEOM_DEBUG").is_some() && self.hwnd != 0 {
            let local_left = pill.left() - full.left();
            let local_top = pill.top() - full.top();
            let local_bot = pill.bottom() - full.top();
            let (wl, wt, _wr, _wb) = platform::window_rect(self.hwnd);
            let pill_left_phys = wl as f32 + local_left * ppp;
            let pill_top_phys = wt as f32 + local_top * ppp;
            let pill_bot_phys = wt as f32 + local_bot * ppp;
            let target_left_phys = self.monitor_rect.min.x * ppp;
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("D:\\LingDao\\_geom_debug.txt")
            {
                let _ = writeln!(
                    f,
                    "e={:.3} pos={:?} ppp={:.2} win=(L{} T{}) target=({},{}) pillL_phys={:.1} pillT_phys={:.1} pillB_phys={:.1} gapLeft={:.1}",
                    self.expanded,
                    self.cfg.position.as_str(),
                    ppp,
                    wl,
                    wt,
                    target_x,
                    target_y,
                    pill_left_phys,
                    pill_top_phys,
                    pill_bot_phys,
                    pill_left_phys - target_left_phys
                );
            }
        }
    }


    /// ?    /// ??
    /// (l,t,r,b,rad) SetWindowRgn?
    fn apply_region(&mut self, _ctx: &egui::Context) {
        // 1-bit HRGN       // SetWindowRgn 0.75px         // A/B =        // DWM with_transparent + blur-behind        //  GPU         // WM_NCHITTEST set_hit_region 
        //  HTTRANSPARENT
if self.hwnd == 0 {
            return;
        }
        if self.region != (0, 0, 0, 0, 0) {
            platform::clear_window_region(self.hwnd);
            self.region = (0, 0, 0, 0, 0);
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        if !self.island_visible {
            return;
        }
        //  0  intro_dy
        let mut op =
        if let Some(v) = std::env::var_os("WINISLE_OP") {
            v.to_string_lossy().parse::<f32>().map(|f| f.clamp(0.0, 1.0)).unwrap_or(1.0)
        } else if self.intro_done {
            1.0
        } else if let Some(t0) = self.intro_start {
            let ip = (t0.elapsed().as_secs_f32() / 0.26).clamp(0.0, 1.0);
            1.0 - (1.0 - ip).powi(2)
        } else {
            0.0
        };
        if let Some(t0) = self.exit_start {
            let pp = (t0.elapsed().as_secs_f32() / 0.18).clamp(0.0, 1.0);
            op *= (1.0 - pp) * (1.0 - pp);
        }
        ui.set_opacity(op);
        self.ctrl_hit.clear();
        let pill = self.pill_rect(ui.max_rect());
        let e = self.expanded;
        let r = self.cfg.radius.min(pill.height() * 0.5).min(pill.width() * 0.5);
        let cr = self.cfg.position.corner_radius(r);
        let bg = egui::Color32::from_rgba_unmultiplied(self.cfg.bg[0], self.cfg.bg[1], self.cfg.bg[2], self.cfg.alpha);
        let t = ui.input(|i| i.time);
        let st = platform::local_time();
        let painter = ui.painter().clone();
        painter.rect_filled(pill, cr, bg);
        //  ///        // +
        if self.expanded > 0.02 && self.expanded < 0.98 {
            return;
        }
        if e < 0.5 {
            self.draw_collapsed_bar(ui, &painter, pill, e, t);
            // 有未处理提醒时：胶囊边缘光晕呼吸振动
            if let Some(aid) = self.remind_alert {
                let col = self.reminders.iter().find(|r| r.id == aid).map(|r| remind_color(r.icon)).unwrap_or([57,199,255]);
                let phase = (t as f32 * 6.2831853 / 1.1).sin() * 0.5 + 0.5;
                let vis = ((0.5 - e) * 2.0).clamp(0.0,1.0);
                let pulse = 0.12 + 0.88 * phase.powf(1.6);
                // 采样胶囊外侧一点壁纸亮度：浅色背景用更深的描边 + 暗底衬，保证呼吸可见
                let ppp = ui.ctx().pixels_per_point();
                let wr = platform::window_rect(self.hwnd);
                let sx = wr.0 + ((pill.left() - 7.0) * ppp) as i32;
                let sy = wr.1 + ((pill.center().y) * ppp) as i32;
                let lum = platform::screen_luminance_at(sx.max(0), sy.max(0));
                let light_bg = lum > 0.62;
                let cc: [u8; 3] = col;
                let boost = 1.0f32;
                // 紧贴胶囊边界的桥接光：浅色背景用实色，避免泛白
                {
                    let a0 = if light_bg { (240.0 * vis).min(255.0) as u8 } else { (105.0 * pulse * vis).min(255.0) as u8 };
                    if a0 > 0 {
                        let rr0 = egui::CornerRadius {
                            nw: cr.nw,
                            ne: cr.ne,
                            sw: (cr.sw as f32 + 0.3) as u8,
                            se: (cr.se as f32 + 0.3) as u8,
                        };
                        painter.rect_stroke(
                            pill.expand(0.3),
                            rr0,
                            egui::Stroke::new(2.0, egui::Color32::from_rgba_unmultiplied(cc[0], cc[1], cc[2], a0)),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
                if light_bg {
                    // 浅色背景：保持原色实描边 + 极细暗底衬，靠线宽/外扩呼吸，且尽量收窄
                    let ex = 0.15 + phase * 0.75;
                    let rr = egui::CornerRadius {
                        nw: cr.nw,
                        ne: cr.ne,
                        sw: (cr.sw as f32 + ex) as u8,
                        se: (cr.se as f32 + ex) as u8,
                    };
                    painter.rect_stroke(
                        pill.expand(ex + 0.7),
                        rr,
                        egui::Stroke::new(1.9, egui::Color32::from_rgba_unmultiplied(0, 0, 0, (60.0 * vis) as u8)),
                        egui::StrokeKind::Middle,
                    );
                    painter.rect_stroke(
                        pill.expand(ex),
                        rr,
                        egui::Stroke::new(1.0 + 0.5 * pulse, egui::Color32::from_rgba_unmultiplied(cc[0], cc[1], cc[2], (240.0 * vis) as u8)),
                        egui::StrokeKind::Middle,
                    );
                } else {
                    for k in 0..2 {
                        let ex = 0.5 + k as f32 * 1.0 + phase * 0.9;
                        let base = [90.0f32, 30.0][k];
                        let aa = (base * pulse * vis * boost).min(255.0).max(0.0) as u8;
                        if aa == 0 { continue; }
                        let rr = egui::CornerRadius {
                            nw: cr.nw,
                            ne: cr.ne,
                            sw: (cr.sw as f32 + ex) as u8,
                            se: (cr.se as f32 + ex) as u8,
                        };
                        painter.rect_stroke(
                            pill.expand(ex),
                            rr,
                            egui::Stroke::new(1.1 + 0.7 * pulse, egui::Color32::from_rgba_unmultiplied(cc[0], cc[1], cc[2], aa)),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
                // 贴顶时遮掉最上沿那一小条，避免“顶部一条横光”，只留左/右/下三边轮廓闪
                if pill.top() <= 2.0 {
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(pill.left() - 4.0, pill.top() - 6.0),
                            egui::pos2(pill.right() + 4.0, pill.top() + 1.6),
                        ),
                        egui::CornerRadius::same(0),
                        bg,
                    );
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
            }
        } else if false {
            let has = self.music_amt > 0.01;
            let a = ((0.5 - e) * 2.0 * 255.0) as u8;
            let amt = self.music_amt.clamp(0.0, 1.0);
            let am = (a as f32 * amt) as u8; // "音乐内在的淡入淡" 
            let mut time_rect: Option<egui::Rect> = None;
            if has {
                let fg = egui::Color32::from_rgba_unmultiplied(238, 240, 246, am);
                let green = egui::Color32::from_rgba_unmultiplied(64, 214, 160, am);
                let cy = pill.center().y;
                let sx = pill.left() + 15.0;
                // 
                let mut time_s = format!("{:02}:{:02}", st.wHour, st.wMinute);
                if self.cfg.show_seconds {
                    time_s = format!("{}:{:02}", time_s, st.wSecond);
                }
                let time_font = bold_font(14.0);

                let time_w = painter
                    .layout_no_wrap(time_s.clone(), time_font.clone(), fg)
                    .size()
                    .x;
                let time_left = pill.right() - 13.0 - time_w;
                let badge_left = if self.show_time_update() {
                    time_left - UPD_BADGE_GAP - UPD_BADGE_W
                } else {
                    time_left
                };
                // /
                let lyric_font = egui::FontId::proportional(14.0);
                //  3pt/
                let base_y = cy + painter
                    .layout_no_wrap("".to_owned(), lyric_font, fg)
                    .size()
                    .y
                    * 0.5
                    - 3.0;

                // / / 
                {
                    let base_speed = 5.2f32;
                    let fac = {
                        let n = self.lyr_lines.len();
                        if n >= 4 {
                            let dur = (self.lyr_lines[n - 1].t - self.lyr_lines[0].t).max(1.0);
                            let chars: usize = self.lyr_lines.iter().map(|l| l.text.chars().count()).sum();
                            let cps = chars as f64 / dur; // "每的字数：慢"~2"，快"~6+
                            let f_cps = ((cps - 2.0) / 4.0).clamp(0.0, 1.0) as f32;
                            let gap = dur / (n - 1) as f64; //  "平均每句时长（的，
                            let f_gap = ((8.0 - gap) / 6.0).clamp(0.0, 1.0) as f32;
                            let mix = f_cps * 0.6 + f_gap * 0.4;
                            0.78 + mix * 0.62 // 0.78× ~ 1.40×
                        } else {
                            1.0
                        }
                    };
                    let target = base_speed * fac;
                    let dt = ((t - self.spec_last_t) as f32).clamp(0.0, 0.1);
                    self.spec_last_t = t;
                    let ksm = 1.0 - (-dt / 0.4).exp();
                    self.spec_speed += (target - self.spec_speed) * ksm;
                    self.spec_phase += (dt * self.spec_speed) as f64;
                    for k in 0..3 {
                        let ph = (self.spec_phase + (k as f64) * 1.4).sin() as f32 * 0.5 + 0.5;
                        let hgt = 4.0 + ph * 9.0;
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(sx + (k as f32) * 5.0, base_y - hgt),
                                egui::pos2(sx + (k as f32) * 5.0 + 3.0, base_y),
                            ),
                            1.5,
                            green,
                        );
                    }
                }
                //    
                let base = egui::Color32::from_rgba_unmultiplied(200, 205, 218, am);
                let fill_green = if self.cfg.char_green { green } else { base };
                let text_rect = egui::Rect::from_min_size(
                    egui::pos2(sx + 22.0, pill.top()),
                    egui::vec2((badge_left - 10.0) - (sx + 22.0), pill.height()),
                );
                let instrumental = lyrics::is_instrumental(&self.lyr_lines);
                let cur = if instrumental { None } else { self.cur_lyric_idx() };
                if instrumental {

                    self.draw_marquee(&painter, text_rect, lyrics::INSTRUMENTAL_TEXT, egui::FontId::proportional(14.0), green, t, false, self.cfg.edge_fade);
                } else if let Some(i) = cur {
                    let line = self.lyr_lines[i].text.clone();
                    let prog = self.line_progress(i);

                    let lt = (self.lyric_time() - self.lyr_lines[i].t).max(0.0);
                    // ease-out-cubic
                    let roll_dur = self.cfg.roll_ms.clamp(50.0, 2000.0);
                    let mut roll = 1.0f32;
                    let mut old: Option<(String, f64)> = None;
                    if self.cfg.roll_anim {
                        if let Some(at) = self.lyr_anim_at {
                            let el = at.elapsed().as_secs_f32() * 1000.0;
                            if el < roll_dur {
                                let p = (el / roll_dur).clamp(0.0, 1.0);
                                roll = 1.0 - (1.0 - p).powi(3);
                                if let Some(pf) = self.lyr_anim_from {
                                    if pf != i && pf < self.lyr_lines.len() {
                                        let olt = (self.lyric_time() - self.lyr_lines[pf].t).max(0.0);
                                        old = Some((self.lyr_lines[pf].text.clone(), olt));
                                    }
                                }
                            } else {
                                self.lyr_anim_at = None;
                                self.lyr_anim_from = None;
                            }
                        }
                    }
                    if roll < 1.0 {
                        let lh = text_rect.height().max(1.0);
                        if let Some((ot, olt)) = &old {
                            let ody = -lh * roll;
                            let oa = ((am as f32) * (1.0 - roll)).clamp(0.0, 255.0) as u8;
                            let obase = egui::Color32::from_rgba_unmultiplied(200, 205, 218, oa);
                            self.draw_lyric_offset(
                                &painter, text_rect, text_rect, ody, ot,
                                egui::FontId::proportional(14.0), obase, obase, 0.0, *olt, false, self.cfg.edge_fade,
                            );
                        }
                        let ndy = lh * (1.0 - roll);
                        self.draw_lyric_offset(
                            &painter, text_rect, text_rect, ndy, &line,
                            egui::FontId::proportional(14.0), base, fill_green, prog, lt, false, self.cfg.edge_fade,
                        );
                    } else {
                        self.draw_lyric_line(&painter, text_rect, &line, egui::FontId::proportional(14.0), base, fill_green, prog, lt, false, self.cfg.edge_fade);
                    }
                } else {
                    let text = if self.music.title.is_empty() {
                        "正在播放".to_owned()
                    } else if self.music.artist.is_empty() {
                        self.music.title.clone()
                    } else {
                        format!("{} · {}", self.music.title, self.music.artist)
                    };
                    self.draw_marquee(&painter, text_rect, &text, egui::FontId::proportional(14.0), fg, t, false, self.cfg.edge_fade);
                }
                // 
                time_rect = Some(painter.text(
                    egui::pos2(pill.right() - 13.0, cy),
                    egui::Align2::RIGHT_CENTER,
                    &time_s,
                    time_font,
                    fg,
                ));
                if self.show_time_update() {
                    let br = egui::Rect::from_min_size(
                        egui::pos2(badge_left, cy - UPD_BADGE_H * 0.5),
                        egui::vec2(UPD_BADGE_W, UPD_BADGE_H),
                    );
                    self.paint_update_pill(&painter, br, am);
                }
            } else if self.face_mode_active() {
                // + Otto 
                let centered_a = (a as f32 * (1.0 - amt)) as u8;
                let idx = self.cfg.idle_face as usize;
                self.ensure_face_frames(ui.ctx(), idx);
                let metas = Self::face_anim_meta();
                let (aspect, fc) = metas
                    .get(idx)
                    .map(|(w, h, fc, _)| (*w as f32 / (*h).max(1) as f32, *fc as usize))
                    .unwrap_or((1.0, 1));
                let frame = (((t * 1000.0) / 80.0) as usize) % fc.max(1);
                let tex = self
                    .face_frames
                    .get(idx)
                    .and_then(|o| o.as_ref())
                    .and_then(|v| v.get(frame));
                let max_h = (pill.height() * 1.5).max(10.0);
                let fh = max_h.min((pill.width() - 12.0).max(8.0) / aspect.max(0.2));
                draw_robot_face(
                    &painter,
                    egui::pos2(pill.center().x, pill.center().y),
                    fh,
                    centered_a,
                    self.cfg.idle_face,
                    t,
                    tex,
                );
                //  60ms 16.7fps80ms 
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(60));
            } else {
                let mut s = format!("{:02}:{:02}", st.wHour, st.wMinute);
                if self.cfg.show_seconds {
                    s = format!("{}:{:02}", s, st.wSecond);
                }
                let centered_a = (a as f32 * (1.0 - amt)) as u8; // "无音乐时居中时间，随音乐淡入而淡" 
                let col = egui::Color32::from_rgba_unmultiplied(238, 240, 246, centered_a);
                let font = bold_font(self.cfg.font);
                if self.show_time_update() {

                    let time_w = painter
                        .layout_no_wrap(s.clone(), font.clone(), col)
                        .size()
                        .x;
                    let group_w = UPD_BADGE_W + UPD_BADGE_GAP + time_w;
                    let start = pill.center().x - group_w * 0.5;
                    let br = egui::Rect::from_min_size(
                        egui::pos2(start, pill.center().y - UPD_BADGE_H * 0.5),
                        egui::vec2(UPD_BADGE_W, UPD_BADGE_H),
                    );
                    self.paint_update_pill(&painter, br, centered_a);
                    time_rect = Some(painter.text(
                        egui::pos2(start + UPD_BADGE_W + UPD_BADGE_GAP, pill.center().y - self.cfg.font * 0.06),
                        egui::Align2::LEFT_CENTER,
                        &s,
                        font,
                        col,
                    ));
                } else {

                    if let Some(w) = self.w_data.as_ref() {
                        let f = self.cfg.weather_unit == 1;
                        let sky = weather::sky_of(&w.today_type, st.wHour >= 18 || st.wHour < 6);
                        let cy = pill.center().y;
                        let tcol = egui::Color32::from_rgba_unmultiplied(238, 240, 246, centered_a);
                        let dim = egui::Color32::from_rgba_unmultiplied(150, 158, 175, centered_a);
                        let bgm = egui::Color32::from_rgb(self.cfg.bg[0], self.cfg.bg[1], self.cfg.bg[2]);
                        draw_weather_icon(&painter, sky, egui::pos2(pill.left() + 18.0, cy), 20.0, centered_a, t, bgm);
                        let temp = weather::conv_temp(w.temp, f);
                        painter.text(egui::pos2(pill.left() + 34.0, cy - 0.5), egui::Align2::LEFT_CENTER, format!("{:.0}°", temp), bold_font(13.5), tcol);
                        if !w.today_type.is_empty() {
                            painter.text(egui::pos2(pill.left() + 66.0, cy), egui::Align2::LEFT_CENTER, &w.today_type, egui::FontId::proportional(11.0), dim);
                        }
                        time_rect = Some(painter.text(
                            egui::pos2(pill.right() - 13.0, cy - self.cfg.font * 0.06),
                            egui::Align2::RIGHT_CENTER,
                            &s,
                            bold_font(13.5),
                            tcol,
                        ));
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
                    } else {

                        time_rect = Some(painter.text(
                            egui::pos2(pill.center().x, pill.center().y - self.cfg.font * 0.06),
                            egui::Align2::CENTER_CENTER,
                            &s,
                            font,
                            col,
                        ));
                    }
                }
            }
        } else {
            self.draw_expanded_panel(ui, &painter, pill, e, t);
        }
    }

    fn draw_collapsed_bar(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, pill: egui::Rect, e: f32, t: f64) {
        let vis = self.cb_anim.clamp(0.0, 1.0);
        let base_a = (((0.5 - e) * 2.0 * 255.0).clamp(0.0, 255.0) * vis) as u8;
        let a = base_a;
        let am = (a as f32 * self.music_amt.clamp(0.0, 1.0)) as u8;
        let st = platform::local_time();
        let comps = self.active_comps();
        let clip_p = painter.with_clip_rect(pill);
        let painter: &egui::Painter = &clip_p;
        let cy = pill.center().y;
        let fg = egui::Color32::from_rgba_unmultiplied(238, 240, 246, a);
        let green = egui::Color32::from_rgba_unmultiplied(64, 214, 160, am);
        let lyric_font = egui::FontId::proportional(LYRIC_COLLAPSED_PT);
        let base_y = cy + painter.layout_no_wrap("".to_owned(), lyric_font, fg).size().y * 0.5 - 3.0;
        let total: f32 = comps.iter().map(|(_, w)| *w).sum::<f32>()
            + {
                let mut g = 0.0f32;
                let mut prev: Option<&str> = None;
                for (id, _) in comps.iter() {
                    if let Some(p) = prev {
                        g += self.comp_gap(p, id);
                    }
                    prev = Some(id.as_str());
                }
                g
            };
        let mut x = pill.left() + ((pill.width() - total).max(0.0)) * 0.5;
        for (i, (id, cw)) in comps.iter().enumerate() {
            if i > 0 {
                let prev = comps[i - 1].0.as_str();
                x += self.comp_gap(prev, id);
            }
            if x > pill.right() - 2.0 {
                break;
            }
            let x1 = (x + *cw).min(pill.right() - 2.0);
            match id.as_str() {
                "spectrum" => {
                    self.draw_spectrum(painter, x, base_y, green, t);
                }
                "lyrics" => {
                    let tr = egui::Rect::from_min_size(egui::pos2(x, pill.top()), egui::vec2((x1 - x).max(1.0), pill.height()));
                    let base = egui::Color32::from_rgba_unmultiplied(200, 205, 218, am);
                    self.draw_collapsed_lyrics(painter, tr, fg, base, green, am, t);
                }
                "clock" => {
                    self.draw_collapsed_clock(painter, x, x1, cy, a, st);
                }
                "weather" => {
                    self.draw_collapsed_weather(painter, x, x1, cy, a, t, st);
                }
                "face" => {
                    let idx = self.cfg.idle_face as usize;
                    self.ensure_face_frames(ui.ctx(), idx);
                    let metas = Self::face_anim_meta();
                    let fc = metas
                        .get(idx)
                        .map(|(_, _, fc, _)| *fc as usize)
                        .unwrap_or(1);
                    //  UV &mut selftexture
let (u0, v0, u1, v1) = self.face_content_uv(idx);
                    let frame = (((t * 1000.0) / 80.0) as usize) % fc.max(1);
                    let tex = self
                        .face_frames
                        .get(idx)
                        .and_then(|o| o.as_ref())
                        .and_then(|v| v.get(frame));
                    let cx = (x + x1) * 0.5;
                    let slot_w = (x1 - x).max(8.0);
                    // ?
                    //  CB_GAP(10px)
                    let (pw, ph) = metas
                        .get(idx)
                        .map(|(w, h, _, _)| (*w as f32, *h as f32))
                        .unwrap_or((77.0, 80.0));
                    let content_aspect =
                        ((pw * (u1 - u0)) / (ph * (v1 - v0)).max(1.0)).max(0.2);
                    let max_h = (pill.height() * 1.2).max(10.0);
                    let fw = slot_w.min(max_h * content_aspect);
                    let fh = fw / content_aspect;
                    if let Some(tx) = tex {
                        draw_face_img_cropped(
                            painter,
                            egui::pos2(cx, cy),
                            fw,
                            fh,
                            a,
                            tx,
                            (u0, v0, u1, v1),
                            FACE_EYE_GAP_REDUCE,
                        );
                    } else {
                        draw_robot_face(painter, egui::pos2(cx, cy), fh, a, self.cfg.idle_face, t, tex);
                    }
                    //  60ms 
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
                }
                _ => {}
            }
            x = x1;
        }
        //   90
        if self.remind_dot.is_some() && a > 20 {
            let pulse = 0.5 + 0.5 * (t * 3.0).sin() as f32;
            let dc = egui::pos2(pill.right() - 7.0, pill.top() + 7.0);
            painter.circle_filled(dc, 4.6, egui::Color32::from_rgba_unmultiplied(242, 112, 92, (40.0 * pulse) as u8));
            painter.circle_filled(dc, 2.6, egui::Color32::from_rgba_unmultiplied(242, 112, 92, a));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        }
        if self.music_motion {
            //  / /+  60fps            //  music_motion=false16ms60ms 16.7fps
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
        if comps.iter().any(|(id, _)| id.as_str() == "weather") {
            //  60ms 
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    fn draw_spectrum(&mut self, painter: &egui::Painter, sx: f32, base_y: f32, green: egui::Color32, t: f64) {
        // 
let base_speed = 3.2f32;
        let dt = ((t - self.spec_last_t) as f32).clamp(0.0, 0.1);
        self.spec_last_t = t;
        // 
if self.music_motion {
            self.spec_phase += (dt * base_speed) as f64;
        }
        for k in 0..3 {
            let ph = (self.spec_phase + (k as f64) * 1.4).sin() as f32 * 0.5 + 0.5;
            let hgt = 4.0 + ph * 8.0;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(sx + (k as f32) * 5.0, base_y - hgt),
                    egui::pos2(sx + (k as f32) * 5.0 + 3.0, base_y),
                ),
                1.5,
                green,
            );
        }
    }

    fn draw_water_reflection(&mut self, painter: &egui::Painter, rect: egui::Rect, fill_rect: egui::Rect, br: f32, accent: egui::Color32, a: u8, t: f64) {
        if rect.width() < 30.0 || rect.height() < 18.0 || a == 0 {
            self.wave_active = false;
            return;
        }
        // 水波频谱是持续动画：空闲时也有由 tt 驱动的 beat/shape 波形，
        // 不能按静止内容降到 5fps。这里由模块自身声明“需要 60fps 刷新”，
        // 中央调度据此保持 target_ms=16ms。
        self.wave_active = true;
        let zero_r = egui::CornerRadius { nw: 0, ne: 0, sw: 0, se: 0 };
        // 
let over = 4.0f32;
        let clip_rect = egui::Rect::from_min_max(
            egui::pos2(fill_rect.left(), rect.top() - 10.0),
            fill_rect.max,
        );
        let clip = painter.with_clip_rect(clip_rect);
        let uv = egui::epaint::WHITE_UV;
        let grad_rect = |p: &egui::Painter, r: egui::Rect, top: egui::Color32, bot: egui::Color32| {
            if r.width() <= 0.0 || r.height() <= 0.0 {
                return;
            }
            let mut m = egui::Mesh::default();
            m.vertices.push(egui::epaint::Vertex { pos: r.left_top(), uv, color: top });
            m.vertices.push(egui::epaint::Vertex { pos: r.right_top(), uv, color: top });
            m.vertices.push(egui::epaint::Vertex { pos: r.right_bottom(), uv, color: bot });
            m.vertices.push(egui::epaint::Vertex { pos: r.left_bottom(), uv, color: bot });
            m.indices = vec![0, 1, 2, 0, 2, 3];
            p.add(egui::Shape::mesh(m));
        };

        //         // 
let _ = accent;
        let br = br.max(0.0);
        let right = fill_rect.right();
        let bottom_y = fill_rect.bottom();
        //   yxse
        let max_x_at = |y: f32| -> f32 {
            if br <= 0.5 || y <= bottom_y - br {
                right
            } else {
                let dy = (y - (bottom_y - br)).clamp(0.0, br);
                right - br + (br * br - dy * dy).max(0.0).sqrt()
            }
        };
        //  xy
        let bottom_at = |x: f32| -> f32 {
            if br <= 0.5 || x <= right - br {
                bottom_y
            } else {
                let dx = (x - (right - br)).clamp(0.0, br);
                bottom_y - br + (br * br - dx * dx).max(0.0).sqrt()
            }
        };

        let pad_x = 2.0f32;
        let x0 = fill_rect.left() + pad_x;
        let x1 = fill_rect.right();
        // .55.60
let horizon = rect.top() + rect.height() * 0.60;
        let max_h = (horizon - (rect.top() - over) - 2.0).max(2.0);
        let refl_depth = (rect.bottom() - horizon - 1.0).max(2.0);
        let n = (((x1 - x0) / 7.0).floor() as usize).clamp(20, 72);
        let gap = 2.0f32;
        let bw = (((x1 - x0) - gap * (n as f32 - 1.0)) / n as f32).max(1.0);

        let playing = self.is_playing();
        // 每帧只取一次音频数据；不要在逐根柱子的循环里重复调用（每次都会复制 64 个频段）。
        let bands_now = if playing { Some(crate::audio_meter::bands()) } else { None };
        let kick_frame = if playing { crate::audio_meter::kick() } else { 0.0 };
        let tt = t as f32;
        // water-reflection-test.html 
        let dt = painter.ctx().input(|i| i.stable_dt).clamp(0.001, 0.05);
        let ease_idle = 1.0 - (-dt / 0.16).exp();
        // 
        // vv 0.11s  
let ease_mix = 1.0 - (-dt / 0.30).exp();
        self.wave_mix += ((if playing { 1.0 } else { 0.0 }) - self.wave_mix) * ease_mix;
        let wave_mix = self.wave_mix.clamp(0.0, 1.0);
        //  ~0.35s  0         // 
let want_onset = if playing { 1.0 } else { 0.0 };
        let ease_onset = 1.0 - (-dt / 0.35).exp();
        self.wave_onset += (want_onset - self.wave_onset) * ease_onset;
        self.wave_onset = self.wave_onset.clamp(0.0, 1.0);
        let onset = self.wave_onset;
        // 频谱在绘制期间始终是动态模块，刷新需求已在函数入口声明，不再只按过渡期判断。
        // [0,1)/
let hash01 = |mut x: u32| -> f32 {
            x ^= x >> 16;
            x = x.wrapping_mul(0x7feb_352d);
            x ^= x >> 15;
            x = x.wrapping_mul(0x846c_a68b);
            x ^= x >> 16;
            (x & 0xffff) as f32 / 65535.0
        };
        let framef = (tt * 30.0) as u32;
        let smoothstep = |a: f32, b: f32, x: f32| -> f32 {
            let x = ((x - a) / (b - a)).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        // HSLRGB water-reflection-test.html 195  330
let hsl = |h: f32, s: f32, l: f32, alpha: u8| -> egui::Color32 {
            let h = h.rem_euclid(360.0);
            let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
            let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
            let m = l - c * 0.5;
            let (r, g, b) = if h < 60.0 {
                (c, x, 0.0)
            } else if h < 120.0 {
                (x, c, 0.0)
            } else if h < 180.0 {
                (0.0, c, x)
            } else if h < 240.0 {
                (0.0, x, c)
            } else if h < 300.0 {
                (x, 0.0, c)
            } else {
                (c, 0.0, x)
            };
            egui::Color32::from_rgba_unmultiplied(
                ((r + m) * 255.0).clamp(0.0, 255.0) as u8,
                ((g + m) * 255.0).clamp(0.0, 255.0) as u8,
                ((b + m) * 255.0).clamp(0.0, 255.0) as u8,
                alpha,
            )
        };

        grad_rect(
            &clip,
            egui::Rect::from_min_max(egui::pos2(fill_rect.left(), rect.top()), egui::pos2(fill_rect.right(), horizon)),
            egui::Color32::from_rgba_unmultiplied(8, 16, 34, 0),
            egui::Color32::from_rgba_unmultiplied(14, 34, 64, (a as f32 * 0.32) as u8),
        );
        {
            let top_c = egui::Color32::from_rgba_unmultiplied(10, 42, 74, (a as f32 * 0.58) as u8);
            let bot_c = egui::Color32::from_rgba_unmultiplied(3, 10, 24, (a as f32 * 0.88) as u8);
            let cols = ((fill_rect.width() / 6.0).ceil() as usize).max(2);
            let mut m = egui::Mesh::default();
            for k in 0..=cols {
                let x = fill_rect.left() + fill_rect.width() * (k as f32 / cols as f32);
                let yb = bottom_at(x);
                m.vertices.push(egui::epaint::Vertex { pos: egui::pos2(x, horizon), uv, color: top_c });
                m.vertices.push(egui::epaint::Vertex { pos: egui::pos2(x, yb.max(horizon)), uv, color: bot_c });
            }
            for k in 0..cols {
                let i0 = (k * 2) as u32;
                let i1 = i0 + 1;
                let i2 = i0 + 2;
                let i3 = i0 + 3;
                m.indices.extend_from_slice(&[i0, i1, i2, i1, i3, i2]);
            }
            clip.add(egui::Shape::mesh(m));
        }

        // 把所有柱体的渐变和倒影分别合进两个 mesh，避免每帧提交上千个独立矩形（GPU/驱动提交开销很大）。
        let mut bars_mesh = egui::Mesh::default();
        let mut refl_mesh = egui::Mesh::default();
        for i in 0..n {
            let f = i as f32 / (n as f32 - 1.0).max(1.0);
            let d = ((f - 0.5).abs() * 2.0).clamp(0.0, 1.0);
            // 中间地板从 0.18 抬高到 0.26，避免整排柱子中段贴地、鼓点只有两端动。
let bowl = 0.26 + 0.98 * d.powf(1.7);
            let beat = ((tt * std::f32::consts::PI * (128.0 / 60.0) / 2.0).sin() * 0.5 + 0.5).powi(4);
            // +  + 
let shape = 0.12 * (f * std::f32::consts::TAU * 1.0 + tt * 0.9).sin()
                + 0.07 * (f * std::f32::consts::TAU * 2.0 - tt * 0.6).sin()
                + 0.04 * (f * std::f32::consts::TAU * 3.5 + tt * 1.3).sin();
            let rnd = hash01((i as u32).wrapping_mul(2_654_435_761) ^ 0x9e37_79b9) * 2.0 - 1.0;
            let idle_target = (0.50 + shape + 0.05 * rnd + 0.09 * beat).clamp(0.30, 0.80);
            let si = ((f * 127.0).round() as usize).min(127);
            let prev_idle = self.wave_idle[si];
            let idle_s = prev_idle + (idle_target - prev_idle) * ease_idle;
            self.wave_idle[si] = idle_s;
            let idle_v = (idle_s * bowl).clamp(0.0, 1.0);
            let play_v = if playing {
                // 
                // /
                let bands_now = bands_now.as_ref().expect("playing 时已取频段");
                let bpos = f * (crate::audio_meter::NBANDS - 1) as f32;
                let bi = bpos.floor() as usize;
                let bf = bpos - bi as f32;
                let b0 = bands_now[bi.min(crate::audio_meter::NBANDS - 1)];
                let b1 = bands_now[(bi + 1).min(crate::audio_meter::NBANDS - 1)];
                let band = b0 + (b1 - b0) * bf; // 0..1 "真实能量"
                // /gamma flick 
let bandv = band.powf(0.72);
                let flick = 0.97 + 0.06 * hash01((i as u32) ^ framef.wrapping_mul(2_654_435_761));
                // 78%2%
let idle_n = ((idle_target - 0.30) / 0.50).clamp(0.0, 1.0);
                let mix = 0.22 * idle_n + 0.78 * bandv;
                let drive = 0.14 + 1.10 * mix; // "静音".14"，重鼓≈1.24"
                // 
let kick = kick_frame;
                // 鼓点整体抬升：不再只靠两侧碗形把低频放大，而是让整排柱子随底鼓一起弹起；
                // 用 bandv 调制，保留各频段强弱差异，避免整排变成一条平坦的横线。
                let punch = kick.powf(0.60);
                let lift = punch * (0.55 + 0.55 * bandv) * (0.62 + 0.38 * bowl);
                let vr = bowl * drive * flick * (0.60 + 0.55 * kick) + lift;
                let knee = 0.70f32;
                let ceil = 0.99f32;
                let target = if vr <= knee {
                    vr
                } else {
                    knee + (ceil - knee) * (1.0 - (-(vr - knee) / (ceil - knee) * 1.8).exp())
                };
                // 
                //                  //                 // onset
                let target = target.max(idle_v);
                // 鼓点命中时给整排一个共同的地板抬升（用 punch 调制），避免只有低频段在跳、
                // 中高段仍像静止，整体看起来不够“炸”。
                let target = target.max((idle_v + punch * 0.65).min(0.99));
                let target = idle_v + (target - idle_v) * onset;
                self.wave_play[si] = target;
                target
            } else {
                self.wave_play[si]
            };
            // 
let vv = idle_v * (1.0 - wave_mix) + play_v * wave_mix;
            // 
let prev_disp = self.wave_disp[si];
            let tau_disp = if vv > prev_disp { 0.030_f32 } else { 0.17_f32 };
            let ease_disp = 1.0 - (-dt / tau_disp).exp();
            let disp = prev_disp + (vv - prev_disp) * ease_disp;
            self.wave_disp[si] = disp;
            let v = disp.clamp(0.05, 0.99);
            let hgt = (v * max_h).max(1.5) + 5.0;
            let bx = x0 + i as f32 * (bw + gap);

            // HTML 9530
            let hue = 195.0 + f * 135.0;
            let base = hsl(hue, 0.92, 0.62, 255);
            let top_c = hsl(hue, 0.96, 0.78, (a as f32 * 0.98) as u8);
            let bot_c = hsl(hue, 0.90, 0.52, (a as f32 * 0.26) as u8);
            let bi0 = bars_mesh.vertices.len() as u32;
            bars_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(bx, horizon - hgt), uv, color: top_c });
            bars_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(bx + bw, horizon - hgt), uv, color: top_c });
            bars_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(bx + bw, horizon), uv, color: bot_c });
            bars_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(bx, horizon), uv, color: bot_c });
            bars_mesh.indices.extend_from_slice(&[bi0, bi0 + 1, bi0 + 2, bi0, bi0 + 2, bi0 + 3]);
            // .5px cap
            //              // + HTML
            let refl_h = (hgt * 0.85).min(refl_depth);
            let segs = 14usize;
            for s in 0..segs {
                let depth = (s + 1) as f32 / segs as f32;
                let yy = horizon + depth * refl_h;
                let amp = 0.6 + depth * 4.2;
                let off = (yy * 0.09 + tt * 2.6 + i as f32 * 0.4).sin() * amp;
                let ra = ((1.0 - depth) * 0.50 * (0.35 + 0.65 * v) * a as f32) as u8;
                let rc = egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), ra.max(2));
                let seg_h = refl_h / segs as f32 + 0.7;
                let rx0 = bx + off;
                let rx1 = (rx0 + bw).min(max_x_at((yy + seg_h).min(bottom_y)));
                if rx1 > rx0 {
                    let ri = refl_mesh.vertices.len() as u32;
                    let yb = yy + seg_h;
                    refl_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(rx0, yy), uv, color: rc });
                    refl_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(rx1, yy), uv, color: rc });
                    refl_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(rx1, yb), uv, color: rc });
                    refl_mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(rx0, yb), uv, color: rc });
                    refl_mesh.indices.extend_from_slice(&[ri, ri + 1, ri + 2, ri, ri + 2, ri + 3]);
                }
            }
        }

        if !bars_mesh.vertices.is_empty() {
            clip.add(egui::Shape::mesh(bars_mesh));
        }
        if !refl_mesh.vertices.is_empty() {
            clip.add(egui::Shape::mesh(refl_mesh));
        }

        // HTML 
for s in 0..4 {
            let yy = horizon + 5.0 + s as f32 * (refl_depth / 5.0);
            let steps = (((x1 - x0) / 8.0).max(2.0)) as i32;
            let mut pts = Vec::with_capacity((steps + 1) as usize);
            for k in 0..=steps {
                let x = x0 + k as f32 * 8.0;
                let wy = yy + ((x * 0.03 + tt * 2.0 + s as f32).sin()) * 1.2;
                if x <= max_x_at(wy) {
                    pts.push(egui::pos2(x, wy));
                }
            }
            let ca = ((a as f32) * (0.06 - s as f32 * 0.008)).max(0.0) as u8;
            clip.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(140, 225, 255, ca)),
            ));
        }

        clip.rect_filled(
            egui::Rect::from_min_max(egui::pos2(fill_rect.left(), horizon - 0.5), egui::pos2(fill_rect.right(), horizon + 0.5)),
            zero_r,
            egui::Color32::from_rgba_unmultiplied(150, 235, 255, (a as f32 * 0.20) as u8),
        );
    }

    ///  +     ///  250
fn draw_collapsed_lyrics(&mut self, painter: &egui::Painter, text_rect: egui::Rect, fg: egui::Color32, base: egui::Color32, green: egui::Color32, am: u8, t: f64) {
        let fill_green = if self.cfg.char_green { green } else { base };
        let instrumental = lyrics::is_instrumental(&self.lyr_lines);
        let cur = if instrumental { None } else { self.cur_lyric_idx() };
        if instrumental {
            self.draw_marquee(painter, text_rect, lyrics::INSTRUMENTAL_TEXT, egui::FontId::proportional(LYRIC_COLLAPSED_PT), green, t, false, self.cfg.edge_fade);
        } else if let Some(i) = cur {
            let line = self.lyr_lines[i].text.clone();
            let prog = self.line_progress(i);
            let lt = (self.lyric_time() - self.lyr_lines[i].t).max(0.0);
            let roll_dur = self.cfg.roll_ms.clamp(50.0, 2000.0);
            let mut roll = 1.0f32;
            let mut old: Option<(String, f64)> = None;
            if self.cfg.roll_anim {
                if let Some(at) = self.lyr_anim_at {
                    let el = at.elapsed().as_secs_f32() * 1000.0;
                    if el < roll_dur {
                        let p = (el / roll_dur).clamp(0.0, 1.0);
                        roll = 1.0 - (1.0 - p).powi(3);
                        if let Some(pf) = self.lyr_anim_from {
                            if pf != i && pf < self.lyr_lines.len() {
                                let olt = (self.lyric_time() - self.lyr_lines[pf].t).max(0.0);
                                old = Some((self.lyr_lines[pf].text.clone(), olt));
                            }
                        }
                    } else {
                        self.lyr_anim_at = None;
                        self.lyr_anim_from = None;
                    }
                }
            }
            if roll < 1.0 {
                let lh = text_rect.height().max(1.0);
                if let Some((ot, olt)) = &old {
                    let ody = -lh * roll;
                    let oa = ((am as f32) * (1.0 - roll)).clamp(0.0, 255.0) as u8;
                    let obase = egui::Color32::from_rgba_unmultiplied(200, 205, 218, oa);
                    self.draw_lyric_offset(painter, text_rect, text_rect, ody, ot, egui::FontId::proportional(LYRIC_COLLAPSED_PT), obase, obase, 0.0, *olt, false, self.cfg.edge_fade);
                }
                let ndy = lh * (1.0 - roll);
                self.draw_lyric_offset(painter, text_rect, text_rect, ndy, &line, egui::FontId::proportional(LYRIC_COLLAPSED_PT), base, fill_green, prog, lt, false, self.cfg.edge_fade);
            } else {
                self.draw_lyric_line(painter, text_rect, &line, egui::FontId::proportional(LYRIC_COLLAPSED_PT), base, fill_green, prog, lt, false, self.cfg.edge_fade);
            }
        } else {
            let text = if self.music.title.is_empty() {
                "正在播放".to_owned()
            } else if self.music.artist.is_empty() {
                self.music.title.clone()
            } else {
                format!("{} · {}", self.music.title, self.music.artist)
            };
            self.draw_marquee(painter, text_rect, &text, egui::FontId::proportional(LYRIC_COLLAPSED_PT), fg, t, false, self.cfg.edge_fade);
        }
    }

    fn draw_collapsed_clock(&mut self, painter: &egui::Painter, x: f32, x1: f32, cy: f32, a: u8, st: platform::SYSTEMTIME) {
        let mut s = format!("{:02}:{:02}", st.wHour, st.wMinute);
        if self.cfg.show_seconds {
            s = format!("{}:{:02}", s, st.wSecond);
        }
        let tcol = egui::Color32::from_rgba_unmultiplied(238, 240, 246, a);
        let fid = bold_font(13.5);
        // 与天气温度同源：同一 bold 13.5 字体，时间槽宽按实测文字宽度自适应，
        // 不再用固定 34px 槽宽把 36px 的 HH:MM 左右各裁 1px。
        self.w_clock_w = painter.layout_no_wrap(s.clone(), fid.clone(), tcol).size().x;
        let right = if self.show_time_update() { x1 - 2.0 } else { x1 };
        if self.show_time_update() {
            let br = egui::Rect::from_min_size(egui::pos2(x, cy - UPD_BADGE_H * 0.5), egui::vec2(UPD_BADGE_W, UPD_BADGE_H));
            self.paint_update_pill(painter, br, a);
            painter.text(egui::pos2(right, cy - 0.5), egui::Align2::RIGHT_CENTER, &s, fid, tcol);
        } else {
            painter.text(egui::pos2((x + right) * 0.5, cy - 0.5), egui::Align2::CENTER_CENTER, &s, fid, tcol);
        }
    }

    /// + 
fn draw_collapsed_weather(&mut self, painter: &egui::Painter, x: f32, _x1: f32, cy: f32, a: u8, t: f64, st: platform::SYSTEMTIME) {
        let (sky, temp) = match self.w_data.as_ref() {
            Some(w) => (
                weather::sky_of(&w.today_type, st.wHour >= 18 || st.wHour < 6),
                weather::conv_temp(w.temp, self.cfg.weather_unit == 1),
            ),
            None => return,
        };
        let tcol = egui::Color32::from_rgba_unmultiplied(238, 240, 246, a);
        let bgm = egui::Color32::from_rgb(self.cfg.bg[0], self.cfg.bg[1], self.cfg.bg[2]);
        let text = format!("{:.0}°", temp);
        let fid = bold_font(13.5);
        // 
let tw = painter.layout_no_wrap(text.clone(), fid.clone(), tcol).size().x;
        self.w_temp_w = tw;
        draw_weather_icon(painter, sky, egui::pos2(x + 10.0, cy), 20.0, a, t, bgm);
        painter.text(egui::pos2(x + 26.0, cy - 0.5), egui::Align2::LEFT_CENTER, text, fid, tcol);
    }

    fn draw_expanded_idle(&self, painter: &egui::Painter, rect: egui::Rect, e: f32) {
        let a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as u8;
        painter.text(
            egui::pos2(rect.center().x, rect.center().y - 10.0),
            egui::Align2::CENTER_CENTER,
            "未播放音乐",
            egui::FontId::proportional(15.0),
            egui::Color32::from_rgba_unmultiplied(150, 158, 175, a),
        );
        painter.text(
            egui::pos2(rect.center().x, rect.center().y + 12.0),
            egui::Align2::CENTER_CENTER,
            "打开音乐客户端播放后自动显示歌词",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgba_unmultiplied(118, 126, 142, a),
        );
    }

    fn draw_marquee(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        text: &str,
        font: egui::FontId,
        color: egui::Color32,
        t: f64,
        left_align: bool,
        left_fade: bool,
    ) {
        let galley = painter.layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER);
        let tw = galley.size().x;
        let y = rect.center().y - galley.size().y * 0.5;
        if tw <= rect.width() {
            let lx = if left_align { rect.left() } else { rect.center().x - tw * 0.5 };
            painter.galley(egui::pos2(lx, y), galley, color);
        } else {
            // 
let max_off = (tw - rect.width()).max(0.0);
            let off = ((t * 28.0) as f32).min(max_off);
            //  alpha             // 
            let fade_left = left_fade;
            let fade_right = left_fade && (tw - (off + rect.width()) > 0.5);
            let fw = (rect.width() * 0.18).clamp(18.0, 42.0);
            self.galley_left_fade(painter, rect, egui::pos2(rect.left() - off, y), &galley, color, fw, fade_left, fade_right);
        }
    }

    /// clip galleyalpha     /// - fade_left
    fn galley_left_fade(
        &self,
        painter: &egui::Painter,
        clip: egui::Rect,
        pos: egui::Pos2,
        galley: &std::sync::Arc<egui::Galley>,
        color: egui::Color32,
        fade_w: f32,
        fade_left: bool,
        fade_right: bool,
    ) {
        if (!fade_left && !fade_right) || color.a() == 0 {
            painter.with_clip_rect(clip).galley(pos, galley.clone(), color);
            return;
        }
        let fw = fade_w.min(clip.width() * 0.5).max(1.0);
        let lf = if fade_left { fw } else { 0.0 };
        let rf = if fade_right { fw } else { 0.0 };
        let crisp = egui::Rect::from_min_max(
            egui::pos2(clip.left() + lf, clip.top()),
            egui::pos2(clip.right() - rf, clip.bottom()),
        );
        if crisp.width() > 0.5 {
            painter.with_clip_rect(crisp).galley(pos, galley.clone(), color);
        }
        let bands = 20usize;
        // alpha 0 
if fade_left {
            let bw = fw / bands as f32;
            for i in 0..bands {
                let tt = (i as f32 + 0.5) / bands as f32;
                // smoothstep
                let ee = tt * tt * (3.0 - 2.0 * tt);
                let a = (color.a() as f32 * ee).round().clamp(0.0, 255.0) as u8;
                if a == 0 {
                    continue;
                }
                let c = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);
                let band = egui::Rect::from_min_max(
                    egui::pos2(clip.left() + i as f32 * bw, clip.top()),
                    egui::pos2(clip.left() + (i as f32 + 1.0) * bw, clip.bottom()),
                )
                .intersect(clip);
                if band.width() <= 0.0 {
                    continue;
                }
                painter.with_clip_rect(band).galley(pos, galley.clone(), c);
            }
        }
        // alpha  0
if fade_right {
            let bw = fw / bands as f32;
            for i in 0..bands {
                let t_in = (i as f32 + 0.5) / bands as f32; //  0="靠近一?", 1="朢右辀
                let ee = 1.0 - t_in * t_in * (3.0 - 2.0 * t_in);
                let a = (color.a() as f32 * ee).round().clamp(0.0, 255.0) as u8;
                if a == 0 {
                    continue;
                }
                let c = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);
                let x0 = clip.right() - fw + i as f32 * bw;
                let band = egui::Rect::from_min_max(
                    egui::pos2(x0, clip.top()),
                    egui::pos2(x0 + bw, clip.bottom()),
                )
                .intersect(clip);
                if band.width() <= 0.0 {
                    continue;
                }
                painter.with_clip_rect(band).galley(pos, galley.clone(), c);
            }
        }
    }

    /// base prog fill    /// 
    fn lyric_fill_dx(&self, galley: &egui::Galley, prog: f32) -> f32 {
        let target = (prog.clamp(0.0, 1.0) as f64)
            * galley
                .rows
                .iter()
                .flat_map(|r| r.glyphs.iter())
                .filter(|g| !g.chr.is_whitespace())
                .count() as f64;
        let mut n = 0usize;
        for g in galley.rows.iter().flat_map(|r| r.glyphs.iter()) {
            if g.chr.is_whitespace() {
                continue;
            }
            let lo = n as f64;
            let hi = lo + 1.0;
            if target <= lo {
                return g.pos.x;
            }
            if target < hi {
                return g.pos.x + (target - lo) as f32 * g.advance_width;
            }
            n += 1;
        }
        galley.size().x
    }

    fn paint_update_pill(&mut self, painter: &egui::Painter, rect: egui::Rect, alpha: u8) {
        if self.upd_badge_tex.is_none() {
            let bytes: &[u8] = include_bytes!("../assets/update_badge.png");
            if let Ok(img) = image::load_from_memory(bytes) {
                let img = img.to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                self.upd_badge_tex = Some(painter.ctx().load_texture(
                    "update_badge",
                    ci,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }
        if let Some(tex) = self.upd_badge_tex.as_ref() {
            painter.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::from_white_alpha(alpha),
            );
        }
    }

    fn paint_update_mark(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        size: f32,
        animate: bool,
    ) {
        //  Segoe MDL2 AssetsE74A = Up
        let t = painter.ctx().input(|i| i.time) as f32;
        let wave = 0.5 + 0.5 * (t * 2.6).sin();
        let (bg_a, bob) = if animate {
            ((205.0 + 50.0 * wave) as u8, -1.2 * wave)
        } else {
            (245, 0.0)
        };
        let c = egui::pos2(center.x, center.y + bob);
        painter.circle_filled(
            c,
            size * 0.5,
            egui::Color32::from_rgba_unmultiplied(74, 222, 128, bg_a),
        );
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            "\u{E74A}",
            icon_font(size * 0.56),
            egui::Color32::from_rgba_unmultiplied(6, 46, 35, 255),
        );
    }

    fn fade_lyric_color(
        &self,
        base: egui::Color32,
        row_top: f32,
        row_bottom: f32,
        rect: egui::Rect,
    ) -> egui::Color32 {
        let fade = 42.0f32;
        let d = (row_top - rect.top())
            .min(rect.bottom() - row_bottom)
            .max(0.0);
        let t = (d / fade).clamp(0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t);
        egui::Color32::from_rgba_unmultiplied(
            base.r(),
            base.g(),
            base.b(),
            (base.a() as f32 * s) as u8,
        )
    }

    fn draw_lyric_line(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        text: &str,
        font: egui::FontId,
        base: egui::Color32,
        fill: egui::Color32,
        prog: f32,
        t: f64,
        left_align: bool,
        left_fade: bool,
    ) {
        let galley = painter.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER);
        let tw = galley.size().x;
        let y = rect.center().y - galley.size().y * 0.5;
        let scroll = tw > rect.width();

        let off = if scroll {
            let max_off = (tw - rect.width()).max(0.0);
            ((t * 30.0) as f32).min(max_off)
        } else {
            0.0
        };
        // fill galley
        let galf = if prog > 0.0 {
            Some(painter.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER))
        } else {
            None
        };
        //  
let fade_on = left_fade && scroll;
        let more_right = scroll && (tw - (off + rect.width()) > 0.5);
        let fade_right = left_fade && more_right;
        let fw = (rect.width() * 0.18).clamp(18.0, 42.0);
        for k in 0..1 {
            let left = if scroll {
                rect.left() - off + (k as f32) * tw
            } else if left_align {
                rect.left()
            } else {
                rect.center().x - tw * 0.5
            };
            self.galley_left_fade(painter, rect, egui::pos2(left, y), &galley, base, fw, fade_on, fade_right);
            if let Some(gf) = &galf {
                let right = left + self.lyric_fill_dx(&galley, prog);
                let gcl = egui::Rect::from_min_max(
                    egui::pos2(left, rect.top()),
                    egui::pos2(right, rect.bottom()),
                )
                .intersect(rect);
                let fill_right_fade = fade_right && (right >= rect.right() - 0.5);
                if gcl.width() > 0.5 {
                    self.galley_left_fade(painter, gcl, egui::pos2(left, y), gf, fill, fw, fade_on, fill_right_fade);
                }
            }
        }
    }

    /// +     /// 
fn draw_instrumental_art(&self, painter: &egui::Painter, rect: egui::Rect, cy: f32, a: u8) {
        if a == 0 {
            return;
        }
        let gray = egui::Color32::from_rgba_unmultiplied(150, 150, 150, a);
        painter.text(
            egui::pos2(rect.center().x, cy),
            egui::Align2::CENTER_CENTER,
            lyrics::INSTRUMENTAL_TEXT,
            egui::FontId::proportional(15.0),
            gray,
        );
    }

    ///  dy  clip

    #[allow(clippy::too_many_arguments)]
    fn draw_lyric_offset(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        clip: egui::Rect,
        dy: f32,
        text: &str,
        font: egui::FontId,
        base: egui::Color32,
        fill: egui::Color32,
        prog: f32,
        t: f64,
        left_align: bool,
        left_fade: bool,
    ) {
        let galley = painter.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER);
        let tw = galley.size().x;
        let gh = galley.size().y;
        let y = rect.center().y - gh * 0.5 + dy;
        let scroll = tw > rect.width();
        let off = if scroll {
            let max_off = (tw - rect.width()).max(0.0);
            ((t * 30.0) as f32).min(max_off)
        } else {
            0.0
        };
        let left = if scroll {
            rect.left() - off
        } else if left_align {
            rect.left()
        } else {
            rect.center().x - tw * 0.5
        };
        //  
let fade_on = left_fade && scroll;
        let more_right = scroll && (tw - (off + rect.width()) > 0.5);
        let fade_right = left_fade && more_right;
        let fw = (rect.width() * 0.18).clamp(18.0, 42.0);
        self.galley_left_fade(painter, clip, egui::pos2(left, y), &galley, base, fw, fade_on, fade_right);
        if prog > 0.0 {
            let galf = painter.layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER);
            let right = left + self.lyric_fill_dx(&galf, prog);
            let gcl = egui::Rect::from_min_max(
                egui::pos2(left, y),
                egui::pos2(right, y + gh),
            )
            .intersect(clip);
            let fill_right_fade = fade_right && (right >= clip.right() - 0.5);
            if gcl.width() > 0.5 && gcl.height() > 0.5 {
                self.galley_left_fade(painter, gcl, egui::pos2(left, y), &galf, fill, fw, fade_on, fill_right_fade);
            }
        }
    }

    ///  +  ////    /// + // + 
fn draw_music_expanded(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, e: f32, t: f64) {
        let a = (((e - 0.5) * 2.0) * 255.0 * self.music_amt.clamp(0.0, 1.0)).clamp(0.0, 255.0) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let dim = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let faint = egui::Color32::from_rgba_unmultiplied(110, 117, 133, a);
        let green = egui::Color32::from_rgba_unmultiplied(56, 214, 150, a);
        let green_dim = egui::Color32::from_rgba_unmultiplied(26, 84, 64, a);

        // ---------- ---------
        // 614
        let cx0 = rect.left() + CONTENT_PAD_X;
        let cx1 = rect.right() - CONTENT_PAD_X;

        // + /PLAY=-15PAUSE=-41
let dt = ui.ctx().input(|i| i.stable_dt).clamp(0.0, 0.1);
        let playing_now = self.is_playing();
        if playing_now {
            self.disc_angle = (self.disc_angle + dt * 0.45) % std::f32::consts::TAU; // "14s/圈（放慢" 
        }
        let arm_target = if playing_now { -22.0 } else { -40.0 };
        let k = (1.0 - (-dt * 9.0).exp()).clamp(0.0, 1.0);
        self.arm_angle += (arm_target - self.arm_angle) * k;

        let tt_size = 150.0;
        let outer_r = 68.0 / 180.0 * tt_size; // "朢在层灰玐?待        // 唱片150x150 的方框里有明留白（在玐��< 半个方漫）败让看得败的圆盘边缺       // 与内容区上边距完全相等，就把方漫整体持��这圈留白"
let disc_pad = tt_size * 0.5 - outer_r;
        // 
        let disc_top_nudge = 11.0f32;
        //  4 
        let disc_dx = 4.0f32;
        let tt_rect = egui::Rect::from_min_size(
            egui::pos2(
                cx0 - disc_pad + disc_dx,
                rect.top() + CONTENT_PAD_T - disc_pad + disc_top_nudge,
            ),
            egui::vec2(tt_size, tt_size),
        );
        let dc = tt_rect.center();
        let vinyl_r = 64.2 / 180.0 * tt_size; // "黑胶半径（灰玐��细约2px" 
        let cover_r = 46.6 / 180.0 * tt_size; // "一��封面半径（再放大 ~3px 物理" 
        //  1)
        painter.circle_filled(dc, outer_r, egui::Color32::from_rgba_unmultiplied(54, 58, 66, a));
        
        // 2) 
        painter.circle_filled(dc, vinyl_r, egui::Color32::from_rgba_unmultiplied(8, 9, 13, a));
        for k in 0..9 {
            let rr = cover_r + (vinyl_r - cover_r) * (k as f32 + 0.5) / 9.0;
            painter.circle_stroke(dc, rr, egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 9)));
        }
        if let Some(ct) = self.cover_tex.as_ref() {
            // 3) 
            draw_rotating_circle_tex(painter, ct.id(), dc, cover_r, self.disc_angle, a);
        } else {
            //  + 
            let mut h: u64 = 1469598103934665603;
            for b in self.music.title.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(1099511628211);
            }
            let hue = (h % 360) as f32;
            // 
let need_rebuild = match &self.fallback_cover_tex {
                Some((hk, _)) => *hk != h,
                None => true,
            };
            if need_rebuild {
                let ppp = ui.ctx().pixels_per_point().max(1.0);
                let s = (((cover_r * 2.0 * ppp).ceil() as usize) + 2).clamp(24, 512);
                let top = hsv(hue, 0.55, 0.62, 255);
                let bot = hsv((hue + 40.0) % 360.0, 0.62, 0.34, 255);
                let (tr, tg, tb) = (top.r() as f32, top.g() as f32, top.b() as f32);
                let (br, bg, bb) = (bot.r() as f32, bot.g() as f32, bot.b() as f32);
                let mut rgba = vec![0u8; s * s * 4];
                let c = (s as f32 - 1.0) * 0.5;
                let rad = s as f32 * 0.5 - 1.0;
                for y in 0..s {
                    let t = y as f32 / (s as f32 - 1.0).max(1.0);
                    let cr = (tr + (br - tr) * t).round() as u8;
                    let cg = (tg + (bg - tg) * t).round() as u8;
                    let cb = (tb + (bb - tb) * t).round() as u8;
                    for x in 0..s {
                        let dx = x as f32 - c;
                        let dy = y as f32 - c;
                        let dist = (dx * dx + dy * dy).sqrt();
                        let cov = (rad + 0.5 - dist).clamp(0.0, 1.0);
                        let idx = (y * s + x) * 4;
                        rgba[idx] = cr;
                        rgba[idx + 1] = cg;
                        rgba[idx + 2] = cb;
                        rgba[idx + 3] = (cov * 255.0).round() as u8;
                    }
                }
                let ci = egui::ColorImage::from_rgba_unmultiplied([s, s], &rgba);
                let tex = ui.ctx().load_texture("fallback_cover", ci, egui::TextureOptions::LINEAR);
                self.fallback_cover_tex = Some((h, tex));
            }
            if let Some((_, tex)) = self.fallback_cover_tex.as_ref() {
                // mesh disc_angle 
                draw_rotating_circle_tex(painter, tex.id(), dc, cover_r, self.disc_angle, a);
            }
            //  TextShape 
            let note_color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 210);
            let ng = painter.layout_no_wrap("".to_owned(), egui::FontId::proportional(cover_r * 0.9), note_color);
            //  TextShape  pospos = dc - R(angle)*            //
            let c0 = ng.mesh_bounds.center().to_vec2();
            let ang = self.disc_angle;
            let (ns, nc) = (ang.sin(), ang.cos());
            let rot_c0 = egui::vec2(nc * c0.x - ns * c0.y, ns * c0.x + nc * c0.y);
            let note_pos = egui::pos2(dc.x - rot_c0.x, dc.y - rot_c0.y);
            painter.add(egui::epaint::TextShape::new(note_pos, ng, note_color).with_angle(ang));
        }
        // + (146,20) translate(-12,0)
        if let Some(base) = self.tt_base.as_ref() {
            painter.image(base.id(), tt_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::from_rgba_unmultiplied(255, 255, 255, a));
        }
        if let Some(tex) = self.tt_arm0.as_ref() {
            let pivot = egui::pos2(tt_rect.left() + 146.0 / 180.0 * tt_size, tt_rect.top() + 20.0 / 180.0 * tt_size);
            let delta = self.arm_angle.to_radians();
            draw_tex_rotated(painter, tex.id(), tt_rect, pivot, delta, a);
        }

        // / /  10px ?3px 7px
        //   /  / -10px       //
        let head_dx = -7.0f32;
        let tt_right_ref = tt_rect.right() - disc_dx;
        let tx = tt_right_ref + 18.0 + head_dx;
        let title = if self.music.title.is_empty() { "未播放曲目".to_string() } else { self.music.title.clone() };
        let artist = self.music.artist.clone();
        let avail = (cx1 - tx).max(40.0);
        //  LayoutJob 1 + ?
        let one_line = |text: &str, font: egui::FontId, color: egui::Color32, maxw: f32| -> std::sync::Arc<egui::Galley> {
            let mut job = egui::text::LayoutJob::single_section(
                text.to_string(),
                egui::TextFormat { font_id: font, color, ..Default::default() },
            );
            job.wrap.max_width = maxw;
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            job.wrap.overflow_character = Some('…');
            ui.fonts(|f| f.layout_job(job))
        };
        let af = egui::FontId::proportional(12.0);
        let gap = if artist.is_empty() { 0.0 } else { 16.0 };
        let aw = if artist.is_empty() {
            0.0
        } else {
            ui.fonts(|f| f.layout_no_wrap(artist.clone(), af.clone(), dim).size().x)
        };
        // - ?>=12.5
let mut fit_tsz: Option<f32> = None;
        let mut s = 15.5f32;
        while s >= 12.5 - 1e-3 {
            let w = ui.fonts(|f| f.layout_no_wrap(title.clone(), bold_font(s), ink).size().x);
            if w + gap + aw <= avail {
                fit_tsz = Some(s);
                break;
            }
            s -= 0.5;
        }
        let title_cy_single = rect.top() + CONTENT_PAD_T + 31.0;
        if let Some(ts) = fit_tsz {
            let tf = bold_font(ts);
            let tw = ui.fonts(|f| f.layout_no_wrap(title.clone(), tf.clone(), ink).size().x);
            painter.text(egui::pos2(tx, title_cy_single), egui::Align2::LEFT_CENTER, &title, tf, ink);
            if !artist.is_empty() {
                let mid_x = tx + tw + gap * 0.5;
                let ax = tx + tw + gap;
                painter.text(egui::pos2(mid_x, title_cy_single), egui::Align2::CENTER_CENTER, "-", af.clone(), dim);
                painter.text(egui::pos2(ax, title_cy_single), egui::Align2::LEFT_CENTER, &artist, af, dim);
            }
        } else {
            let mut tsz = 15.5f32;
            loop {
                let w = ui.fonts(|f| f.layout_no_wrap(title.clone(), bold_font(tsz), ink).size().x);
                if w <= avail || tsz <= 12.5 { break; }
                tsz -= 0.5;
            }
            let tf = bold_font(tsz);
            let af2 = egui::FontId::proportional(11.5);
            let (title_cy, artist_cy) = if artist.is_empty() {
                (rect.top() + CONTENT_PAD_T + 31.0, 0.0)
            } else {
                (rect.top() + CONTENT_PAD_T + 23.0, rect.top() + CONTENT_PAD_T + 39.0)
            };
            let tg = one_line(&title, tf, ink, avail);
            painter.galley(egui::pos2(tx, title_cy - tg.size().y * 0.5), tg, ink);
            if !artist.is_empty() {
                let prefix = "?";
                let pw = ui.fonts(|f| f.layout_no_wrap(prefix.to_string(), af2.clone(), dim).size().x);
                let ag = one_line(&artist, af2.clone(), dim, (avail - pw).max(10.0));
                let ay = artist_cy - ag.size().y * 0.5;
                painter.text(egui::pos2(tx, artist_cy), egui::Align2::LEFT_CENTER, prefix, af2.clone(), dim);
                painter.galley(egui::pos2(tx + pw, ay), ag, dim);
            }
        }

        // + 
let dur = self.duration_est();
        let pos = self.display_pos();
        let prog = if dur > 1.0 { (pos / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
        let bar_y = rect.top() + CONTENT_PAD_T + 69.0 - 10.0;
        let bar = egui::Rect::from_min_max(egui::pos2(tx, bar_y), egui::pos2(cx1, bar_y + 3.0));
        painter.rect_filled(bar, 1.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30));
        painter.rect_filled(egui::Rect::from_min_max(bar.min, egui::pos2(tx + bar.width() * prog, bar_y + 3.0)), 1.5, green);
        // 
        let fmt = |s: f64| {
            let s = s.max(0.0) as u32;
            format!("{:02}:{:02}", s / 60, s % 60)
        };
        painter.text(egui::pos2(tx, bar_y + 6.0), egui::Align2::LEFT_TOP, fmt(pos), egui::FontId::proportional(10.0), faint);
        painter.text(egui::pos2(cx1, bar_y + 6.0), egui::Align2::RIGHT_TOP, fmt(dur), egui::FontId::proportional(10.0), faint);

        //  () / 
let btn_y = rect.top() + CONTENT_PAD_T + 108.0 - 10.0;
        let mut clicked: Option<&str> = None;
        let prev_c = egui::pos2(tx + 12.0, btn_y);
        let rprev = egui::Rect::from_center_size(prev_c, egui::vec2(22.0, 22.0));
        self.ctrl_hit.push(rprev);
        let resp_prev = ui.interact(rprev, ui.id().with("mctrl_prev"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        painter.circle_filled(prev_c, 11.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 18));
        let c_dn = egui::Color32::from_rgba_unmultiplied(200, 210, 220, 200);
        painter.text(prev_c, egui::Align2::CENTER_CENTER, "\u{E892}", icon_font(12.0), c_dn); // Previous
        if resp_prev.clicked() {
            clicked = Some("prev");
        }
        let play_c = egui::pos2(tx + 58.0, btn_y);
        let rplay = egui::Rect::from_center_size(play_c, egui::vec2(36.0, 36.0));
        self.ctrl_hit.push(rplay);
        let resp_play = ui.interact(rplay, ui.id().with("mctrl_play"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        painter.circle_filled(play_c, 17.0, green);
        let ic = egui::Color32::from_rgba_unmultiplied(10, 20, 16, a);
        let play_glyph = if self.is_playing() { "\u{E769}" } else { "\u{E768}" }; // Pause / Play
        painter.text(play_c, egui::Align2::CENTER_CENTER, play_glyph, icon_font(14.0), ic);
        if resp_play.clicked() {
            clicked = Some("play");
        }
        let next_c = egui::pos2(tx + 104.0, btn_y);
        let rnext = egui::Rect::from_center_size(next_c, egui::vec2(22.0, 22.0));
        self.ctrl_hit.push(rnext);
        let resp_next = ui.interact(rnext, ui.id().with("mctrl_next"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        painter.circle_filled(next_c, 11.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 18));
        painter.text(next_c, egui::Align2::CENTER_CENTER, "\u{E893}", icon_font(12.0), c_dn); // Next
        if resp_next.clicked() {
            clicked = Some("next");
        }
        if !self.music.app_id.is_empty() {
            //              // cx1
let state = self.music_link_state();
            let (cr, cg, cb) = match state {
                0 => (46u8, 204u8, 113u8),  // "绿：已连上漫"
                1 => (245u8, 190u8, 40u8),  // "黄：已连上但当前无歌词（如歌单模式）"
                _ => (235u8, 64u8, 52u8),   // "纯��未��上播放器"
            };
            let fade = (a as f32 / 255.0).clamp(0.0, 1.0);
            let pulse = 0.5 + 0.5 * (t as f32 * 2.0).sin();
            let dot_r = 4.6f32;
            let dot = egui::pos2(cx1 - dot_r, btn_y - 6.0);
            painter.text(
                egui::pos2(dot.x - dot_r - 5.0, btn_y - 6.0),
                egui::Align2::RIGHT_CENTER,
                src_label(&self.music.app_id),
                egui::FontId::proportional(10.5),
                faint,
            );
            painter.circle_filled(dot, dot_r, egui::Color32::from_rgba_unmultiplied(cr, cg, cb, (40.0 * (0.35 + 0.65 * pulse) * fade) as u8));
            painter.circle_filled(dot, 2.5, egui::Color32::from_rgba_unmultiplied(cr, cg, cb, ((0.45 + 0.55 * pulse) * fade * 255.0) as u8));
        }
        if let Some(id) = clicked {
            {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::paths::file("ctrl_debug.log")) {
                    let _ = writeln!(f, "click={}", id);
                }
            }
            let was_playing = self.is_playing();
            let cmd = match id {
                "prev" => smtc::Cmd::Prev,
                "play" => {
                    if was_playing {
                        smtc::Cmd::Pause
                    } else {
                        smtc::Cmd::Play
                    }
                }
                _ => smtc::Cmd::Next,
            };
            // / SMTC 
match id {
                "play" => {
                    let want_play = !was_playing;
                    self.ui_playing = want_play;
                    let cp = self.current_pos();
                    self.clock_base = cp;
                    self.clock_start = Instant::now();
                    self.clock_playing = want_play;
                    self.override_play = Some((want_play, Instant::now() + Duration::from_millis(1200)));
                    //  spos 
self.spos = cp;
                    self.spos_at = Instant::now();
                    self.spos_valid = true;
                    // ??
                    self.pause_pending_at = if want_play { None } else { Some(Instant::now()) };
                }
                _ => {
                    self.ui_playing = true;
                    self.override_play = Some((true, Instant::now() + Duration::from_millis(1200)));
                    self.pause_pending_at = None;
                }
            }
            //   CDP SMTC
            let app_l = self.music.app_id.to_ascii_lowercase();
            let is_soda = app_l.contains("汽水") || app_l.contains("soda") || app_l.contains("qishui");
            let mut sent = false;
            if is_soda && crate::soda_patch::cdp_up() {
                let c = id.to_string();
                std::thread::spawn(move || {
                    let _ = crate::cdp_lyric::control(&c);
                });
                sent = true;
            }
            if !sent {
                if is_soda || !self.from_smtc {
                    let vk = match id {
                        "prev" => 0xB1u8, // VK_MEDIA_PREV_TRACK
                        "play" => 0xB3u8, // VK_MEDIA_PLAY_PAUSE
                        _ => 0xB0u8,      // VK_MEDIA_NEXT_TRACK
                    };
                    platform::media_key(vk);
                } else {
                    self.tracker.send(cmd);
                }
            }
        }

        // ----------  ----------
        let instrumental = lyrics::is_instrumental(&self.lyr_lines);
        let cur = if instrumental { None } else { self.cur_lyric_idx() };
        let lfont = self.cfg.lyric_font.clamp(10.0, 20.0);
        let line_font = egui::FontId::proportional(lfont);
        let row_h = 22.0f32;
        let lx0 = cx0 - disc_pad;
        // 
let wf_h = 46.0f32;
        let wf_rect = egui::Rect::from_min_max(
            egui::pos2(lx0, rect.bottom() - wf_h),
            egui::pos2(cx1, rect.bottom()),
        );
        // ?
        let vtop = rect.top() + CONTENT_PAD_T + 146.0 - 10.0 - 12.5;
        let vbot = rect.bottom() - CONTENT_PAD_B - 10.0 - 12.5;
        let lyric_rect = egui::Rect::from_min_max(egui::pos2(lx0, vtop), egui::pos2(cx1, vbot));
        let view_h = (vbot - vtop).max(row_h);
        let rows = ((view_h / row_h).floor() as i32).max(1);
        let grid_top = vtop + (view_h - rows as f32 * row_h) * 0.5;
        let anchor = (rows - 1) as f32 * 0.5;
        let center_y = grid_top + anchor * row_h;
        if self.cfg.water_spectrum {
            let wf_fill = egui::Rect::from_min_max(
                egui::pos2(wf_rect.left(), wf_rect.top()),
                egui::pos2(rect.right(), rect.bottom()),
            );
            let wf_r = self.cfg.radius.min(rect.height() * 0.5).min(rect.width() * 0.5);
            let wf_br = self.cfg.position.corner_radius(wf_r).se as f32;
            self.draw_water_reflection(painter, wf_rect, wf_fill, wf_br, green, a, t);
        }
        if let Some(ci) = cur {
            // 
            let resp = ui
                .interact(lyric_rect, ui.id().with("lyric_scroll"), egui::Sense::hover())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let n = self.lyr_lines.len();
            let min_off = -((n.saturating_sub(1) - ci) as f32) * row_h;
            let max_off = ci as f32 * row_h;
            let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
            if resp.hovered() {
                let d = ui.input(|i| i.raw_scroll_delta.y);
                if d != 0.0 {
                    self.lyr_scroll_target += d;
                    self.lyr_release_at = Some(Instant::now());
                    self.lyr_return_start = None;
                }
            }
            self.lyr_scroll_target = self.lyr_scroll_target.clamp(min_off, max_off);

            //  2s  0.25s 
if self.lyr_return_start.is_none() {
                if let Some(rel) = self.lyr_release_at {
                    if rel.elapsed().as_secs_f32() >= 2.0
                        && (self.lyr_drag_off.abs() > 0.5 || self.lyr_scroll_target.abs() > 0.5)
                    {
                        self.lyr_return_start = Some(Instant::now());
                        self.lyr_return_from = self.lyr_drag_off;
                        self.lyr_scroll_target = 0.0;
                    }
                }
            }
            if let Some(st) = self.lyr_return_start {
                let p = (st.elapsed().as_secs_f32() / 0.25).clamp(0.0, 1.0);
                let e = 1.0 - (1.0 - p).powi(3);
                self.lyr_drag_off = self.lyr_return_from * (1.0 - e);
                if p >= 1.0 {
                    self.lyr_drag_off = 0.0;
                    self.lyr_scroll_target = 0.0;
                    self.lyr_return_start = None;
                    self.lyr_release_at = None;
                }
            } else {
                // 
                let ks = 1.0 - (-dt / 0.05).exp();
                self.lyr_drag_off += (self.lyr_scroll_target - self.lyr_drag_off) * ks;
                if (self.lyr_scroll_target - self.lyr_drag_off).abs() < 0.05 {
                    self.lyr_drag_off = self.lyr_scroll_target;
                }
            }
            self.lyr_drag_off = self.lyr_drag_off.clamp(min_off, max_off);

            if self.lyr_ci_anim < 0.0 {
                self.lyr_ci_anim = ci as f32;
            }
            let kc = 1.0 - (-dt / 0.075).exp();
            self.lyr_ci_anim += (ci as f32 - self.lyr_ci_anim) * kc;
            if (ci as f32 - self.lyr_ci_anim).abs() < 0.002 {
                self.lyr_ci_anim = ci as f32;
            }

            // ci_anim + drag_off
let clip = painter.with_clip_rect(lyric_rect);
            for i in 0..n {
                let y = grid_top + (i as f32 - self.lyr_ci_anim + self.lyr_drag_off / row_h + anchor) * row_h;
                if y >= vbot - 0.5 {
                    break;
                }
                if y + row_h <= vtop + 0.5 {
                    continue;
                }
                let lrect = egui::Rect::from_min_size(egui::pos2(lx0, y), egui::vec2(cx1 - lx0, row_h));
                // /
                let at_current = i == ci;
                if at_current {
                    let pr = self.line_progress(i);
                    let lt = (self.lyric_time() - self.lyr_lines[i].t).max(0.0);
                    let gdim = egui::Color32::from_rgba_unmultiplied(120, 170, 150, a);
                    let fill = if self.cfg.char_green { green } else { gdim };
                    let gdim = self.fade_lyric_color(gdim, y, y + row_h, lyric_rect);
                    let fill = self.fade_lyric_color(fill, y, y + row_h, lyric_rect);
                    self.draw_lyric_line(&clip, lrect, &self.lyr_lines[i].text, line_font.clone(), gdim, fill, pr, lt, false, false);
                } else {
                    let fc = self.fade_lyric_color(faint, y, y + row_h, lyric_rect);
                    // ?
                    // 
let ilt = (self.lyric_time() - self.lyr_lines[i].t).max(0.0);
                    self.draw_lyric_line(&clip, lrect, &self.lyr_lines[i].text, line_font.clone(), fc, fc, 0.0, ilt, false, false);
                }
            }
        } else if instrumental {
            //
            self.draw_instrumental_art(painter, lyric_rect, center_y, a);
        } else {
            let hint = if self.lyr_done { "暂无歌词" } else { "歌词加载中…" };
            painter.text(egui::pos2((lx0 + cx1) * 0.5, center_y), egui::Align2::CENTER_CENTER, hint, egui::FontId::proportional(13.0), egui::Color32::from_rgba_unmultiplied(120, 196, 160, a));
        }
    }

    // ----------------  + ----------------
    ///  +  / / 
fn draw_expanded_panel(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, pill: egui::Rect, e: f32, t: f64) {
        let a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as u8;
        let rail_w = 66.0f32;
        let muted = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);

        let modal = self.remind_alert.is_some();
        if modal {
            ui.set_enabled(false);
            platform::set_activatable(self.hwnd, true);
            ui.ctx().input_mut(|i| {
                i.raw_scroll_delta = egui::Vec2::ZERO;
                i.smooth_scroll_delta = egui::Vec2::ZERO;
            });
        }

        // ---- ----
        let rail = egui::Rect::from_min_max(pill.left_top(), egui::pos2(pill.left() + rail_w, pill.bottom()));
        let rail_bg = egui::Color32::from_rgba_unmultiplied(22, 23, 28, a);
        //          // TopCenter =/=LeftCenter =RightCenter         //  nw:0
        let rr_pill = self.cfg.radius.min(pill.height() * 0.5).min(pill.width() * 0.5);
        let cr = self.cfg.position.corner_radius(rr_pill);
        painter.rect_filled(
            rail,
            egui::CornerRadius { nw: cr.nw, ne: 0, sw: cr.sw, se: 0 },
            rail_bg,
        );

        let active = self.active.rail_idx() as usize;
        let items: [(&str, u8); 4] = [("音乐", 0), ("待办", 3), ("提醒", 4), ("天气", 2)];
        let item_h = 48.0f32;
        let top_y = pill.top() + 34.0; // "让出顶部时间"

        // 
        {
            let stt = platform::local_time();
            let mut ts = format!("{:02}:{:02}", stt.wHour, stt.wMinute);
            let fsz = if self.cfg.show_seconds { ts = format!("{}:{:02}", ts, stt.wSecond); 13.0 } else { 17.0 };
            painter.text(egui::pos2(pill.left() + rail_w * 0.5, pill.top() + 17.5), egui::Align2::CENTER_CENTER, &ts, bold_font(fsz), egui::Color32::from_rgba_unmultiplied(233, 236, 242, a));
        }

        // 
        if self.active != self.last_active {
            self.rail_from = self.last_active.rail_idx();
            self.rail_anim_at = Some(std::time::Instant::now());
            self.last_active = self.active;
        }
        let mut rail_anim_done = false;
        let shown_idx = if let Some(at) = self.rail_anim_at {
            let p = (at.elapsed().as_secs_f32() / 0.26).clamp(0.0, 1.0);
            let ease = 1.0 - (1.0 - p).powi(3);
            if p >= 1.0 { rail_anim_done = true; }
            self.rail_from + (active as f32 - self.rail_from) * ease
        } else { active as f32 };

        let settings_active = self.show_settings && !(self.w_city_search && self.w_city_search_from_weather);
        let eff_shown = if settings_active { -2.0 } else { shown_idx };
        // 
        if !settings_active {
            let hl_y = top_y + shown_idx * item_h;
            let hl_rect = egui::Rect::from_min_size(egui::pos2(pill.left() + 9.0, hl_y), egui::vec2(rail_w - 18.0, item_h - 8.0));
            painter.rect_filled(hl_rect, 10.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * 0.14) as u8));
        }

        let mut iy = top_y;
        for (idx, (label, icon_kind)) in items.iter().enumerate() {
            let item = egui::Rect::from_min_size(egui::pos2(pill.left() + 9.0, iy), egui::vec2(rail_w - 18.0, item_h - 8.0));
            let w = (1.0 - (idx as f32 - eff_shown).abs()).clamp(0.0, 1.0);
            let icol = lerp_color(muted, accent, w);
            let cx = item.center().x;
            draw_nav_icon(painter, *icon_kind, egui::pos2(cx, iy + 13.0), 18.0, icol);
            painter.text(egui::pos2(cx, iy + 31.5), egui::Align2::CENTER_CENTER, *label, bold_font(10.5), icol);
            if idx == 2 && self.remind_dot.is_some() {
                let dc = egui::pos2(item.right() - 3.0, iy + 6.0);
                painter.circle_filled(dc, 4.2, egui::Color32::from_rgba_unmultiplied(242, 112, 92, (a as f32 * 0.22) as u8));
                painter.circle_filled(dc, 2.6, egui::Color32::from_rgba_unmultiplied(242, 112, 92, a));
            }
            let resp = ui.interact(item, ui.id().with(("rail", idx)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                self.show_settings = false;
                self.w_city_search = false;
                self.w_city_search_from_weather = false;
                self.w_search_need_focus = false;
                self.w_search_focus_tries = 0;
                let ni = match idx { 0 => Module::Music, 1 => Module::Todo, 2 => Module::Reminder, _ => Module::Weather };
                if ni == Module::Reminder {
                    // ?
self.remind_dot = None;
                    self.remind_dot_at = None;
                }
                if ni != self.active {
                    self.rail_from = self.active.rail_idx();
                    self.rail_anim_at = Some(std::time::Instant::now());
                    self.active = ni;
                    self.last_active = ni;
                }
            }
            iy += item_h;
        }
        if rail_anim_done { self.rail_anim_at = None; }


        {
            let set_y = pill.bottom() - item_h;
            let item = egui::Rect::from_min_size(egui::pos2(pill.left() + 9.0, set_y), egui::vec2(rail_w - 18.0, item_h - 8.0));
            if settings_active {
                painter.rect_filled(item, 10.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * 0.14) as u8));
            }
            let icol = if settings_active { accent } else { muted };
            let cx = item.center().x;
            painter.text(egui::pos2(cx, set_y + 13.0), egui::Align2::CENTER_CENTER, "\u{E713}", icon_font(17.0), icol);
            painter.text(egui::pos2(cx, set_y + 31.5), egui::Align2::CENTER_CENTER, "设置", egui::FontId::proportional(10.5), icol);
            let resp = ui.interact(item, ui.id().with("rail_settings"), egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                if self.w_city_search {
                    self.w_city_search = false;
                    self.w_city_search_from_weather = false;
                    self.w_search_need_focus = false;
                    self.w_search_focus_tries = 0;
                }
                if !self.show_settings {
                    self.settings_scroll = 0.0;
                    self.settings_scroll_target = 0.0;
                    self.settings_scroll_act = std::time::Instant::now() - std::time::Duration::from_secs(10);
                    self.show_settings = true;
                    self.mark_upd_seen_rail();
                    if self.upd_ver.is_none() && !self.upd_checking {
                        self.start_update_check();
                    }
                }
            }
            // 
if self.show_rail_update() {
                let bc = egui::pos2(item.right() - 8.0, item.top() + 8.0);
                painter.circle_filled(bc, 4.8, egui::Color32::from_rgba_unmultiplied(18, 20, 25, a));
                painter.circle_filled(bc, 3.0, accent);
            }
        }

        // 
        let (switch_p, switch_dir) = if let Some(at) = self.rail_anim_at {
            let p = (at.elapsed().as_secs_f32() / 0.26).clamp(0.0, 1.0);
            let new_idx = self.active.rail_idx();
            (p, (new_idx - self.rail_from).signum())
        } else {
            (1.0f32, 0.0f32)
        };


        let content = egui::Rect::from_min_max(
            egui::pos2(pill.left() + rail_w, pill.top()),
            egui::pos2(pill.right(), pill.bottom()),
        );
        let svis = ui.ctx().animate_bool_with_time(egui::Id::new("settings_vis"), self.show_settings, 0.22);
        let switch_ease = 1.0 - (1.0 - switch_p).powi(3);
        let dx = switch_dir * (1.0 - switch_ease) * 26.0;
        let cslide = egui::Rect::from_min_max(
            egui::pos2(content.left() + dx, content.top()),
            egui::pos2(content.right() + dx, content.bottom()),
        );
        let cclip = painter.with_clip_rect(content);
        if svis < 0.999 {
            match self.active {
                Module::Music => {
                    if self.music_amt > 0.5 {
                        self.draw_music_expanded(ui, &cclip, cslide, e, t);
                    } else {
                        let st = platform::local_time();
                        if self.cfg.show_date {
                            let wd = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"][(st.wDayOfWeek as usize).min(6)];
                            let date = format!("{:04}-{:02}-{:02} {}", st.wYear, st.wMonth, st.wDay, wd);
                            cclip.text(egui::pos2(cslide.right() - 16.0, cslide.top() + 16.0), egui::Align2::RIGHT_TOP, &date, egui::FontId::proportional(13.0), muted);
                        }
                        self.draw_expanded_idle(&cclip, cslide, e);
                    }
                }
                Module::Schedule => self.draw_placeholder_page(&cclip, cslide, e, "日程", "日程模块即将上线"),
                Module::Todo => self.draw_todo_page(ui, &cclip, cslide, e),
                Module::Reminder => self.draw_reminder_page(ui, &cclip, cslide, e),
                Module::Weather => self.draw_weather_page(ui, &cclip, cslide, e),
            }
        }
        if self.show_settings || svis > 0.001 {
            self.draw_settings_page(ui, painter, content, e, svis);
        }        // "模块切换过渡：用岛体底色做淡入遮罩，让新页面“浮玐 "
        if switch_p < 0.999 {
            let ma = ((1.0 - switch_p).powf(1.5) * self.cfg.alpha as f32) as u8;
            let mask = egui::Color32::from_rgba_unmultiplied(self.cfg.bg[0], self.cfg.bg[1], self.cfg.bg[2], ma);
            let rr = self.cfg.radius.min(content.height() * 0.5);
            painter.with_clip_rect(content).rect_filled(content, rr, mask);
        }
        // 
        if self.remind_alert.is_some() {
            if modal {
                ui.set_enabled(true);
            }
            self.draw_reminder_alert(ui, painter, content, e, t);
        }
    }

    ///  1 2 3 4Stroke/Shape9
    fn draw_remind_icon(painter: &egui::Painter, icon: u8, c: egui::Pos2, s: f32, color: egui::Color32, sw: f32) {
        let stroke = egui::Stroke::new(sw, color);
        let f = s / 24.0;
        match icon {
            5 => {
                //  ?+
                painter.circle_stroke(c, 8.0 * f, stroke);
                painter.line_segment([egui::pos2(c.x - 4.0 * f, c.y), egui::pos2(c.x + 4.0 * f, c.y)], stroke);
                painter.line_segment([egui::pos2(c.x, c.y - 4.0 * f), egui::pos2(c.x, c.y + 4.0 * f)], stroke);
            }
            0 => {
                // +  +  + 
                let x = c.x;
                let y = c.y;
                // 
                painter.line_segment([egui::pos2(x - 4.0 * f, y - 8.0 * f), egui::pos2(x + 4.0 * f, y - 8.0 * f)], stroke);
                painter.line_segment([egui::pos2(x - 4.0 * f, y - 8.0 * f), egui::pos2(x - 4.0 * f, y + 1.0 * f)], stroke);
                painter.line_segment([egui::pos2(x + 4.0 * f, y - 8.0 * f), egui::pos2(x + 4.0 * f, y + 1.0 * f)], stroke);
                // 
                painter.line_segment([egui::pos2(x - 7.0 * f, y - 3.0 * f), egui::pos2(x - 7.0 * f, y + 1.0 * f)], stroke);
                painter.line_segment([egui::pos2(x + 7.0 * f, y - 3.0 * f), egui::pos2(x + 7.0 * f, y + 1.0 * f)], stroke);
                painter.line_segment([egui::pos2(x - 7.0 * f, y - 3.0 * f), egui::pos2(x - 4.0 * f, y - 3.0 * f)], stroke);
                painter.line_segment([egui::pos2(x + 7.0 * f, y - 3.0 * f), egui::pos2(x + 4.0 * f, y - 3.0 * f)], stroke);
                // 
                painter.line_segment([egui::pos2(x - 7.0 * f, y + 1.0 * f), egui::pos2(x + 7.0 * f, y + 1.0 * f)], stroke);
                // 
                painter.line_segment([egui::pos2(x - 5.5 * f, y + 1.0 * f), egui::pos2(x - 5.5 * f, y + 8.0 * f)], stroke);
                painter.line_segment([egui::pos2(x + 5.5 * f, y + 1.0 * f), egui::pos2(x + 5.5 * f, y + 8.0 * f)], stroke);
            }
            1 => {
                // 4  + 
                let x = c.x;
                let y = c.y;
                let top = egui::pos2(x, y - 9.5 * f);
                let right = egui::pos2(x + 5.6 * f, y + 1.0 * f);
                let bottom = egui::pos2(x, y + 8.0 * f);
                let left = egui::pos2(x - 5.6 * f, y + 1.0 * f);
                let fill = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 60);
                let seg = |pts: [egui::Pos2; 4]| {
                    egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
                        pts, false, egui::Color32::TRANSPARENT, stroke,
                    ))
                };
                painter.add(seg([top, egui::pos2(x + 1.6 * f, y - 5.5 * f), egui::pos2(x + 5.6 * f, y - 3.0 * f), right]));
                painter.add(seg([right, egui::pos2(x + 5.6 * f, y + 5.0 * f), egui::pos2(x + 2.8 * f, y + 8.0 * f), bottom]));
                painter.add(seg([bottom, egui::pos2(x - 2.8 * f, y + 8.0 * f), egui::pos2(x - 5.6 * f, y + 5.0 * f), left]));
                painter.add(seg([left, egui::pos2(x - 5.6 * f, y - 3.0 * f), egui::pos2(x - 1.6 * f, y - 5.5 * f), top]));
                painter.circle_filled(egui::pos2(x, y + 1.5 * f), 3.6 * f, fill);
            }
            2 => {
                //  + 
                let x = c.x;
                let y = c.y;
                let l = egui::pos2(x - 10.0 * f, y);
                let r = egui::pos2(x + 10.0 * f, y);
                painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
                    [l, egui::pos2(x - 6.0 * f, y - 6.0 * f), egui::pos2(x + 6.0 * f, y - 6.0 * f), r],
                    false,
                    egui::Color32::TRANSPARENT,
                    stroke,
                )));
                painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
                    [r, egui::pos2(x + 6.0 * f, y + 6.0 * f), egui::pos2(x - 6.0 * f, y + 6.0 * f), l],
                    false,
                    egui::Color32::TRANSPARENT,
                    stroke,
                )));
                painter.circle_filled(egui::pos2(x, y), 2.7 * f, color);
            }
            3 => {
                //  + 
                let dir = egui::vec2(0.7071, 0.7071);
                let ang = 0.7853982f32;
                let r = 4.0 * f;
                let l = 7.6 * f;
                let a = c - dir * l;
                let b = c + dir * l;
                let n = 14;
                let mut pts: Vec<egui::Pos2> = Vec::new();
                //  a ang+/2 ang+3/2
                for i in 0..=n {
                    let t = ang + std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (i as f32 / n as f32);
                    pts.push(egui::pos2(a.x + r * t.cos(), a.y + r * t.sin()));
                }
                //  b ang-/2 ang+/2
                for i in 0..=n {
                    let t = ang - std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (i as f32 / n as f32);
                    pts.push(egui::pos2(b.x + r * t.cos(), b.y + r * t.sin()));
                }
                painter.add(egui::Shape::closed_line(pts, stroke));
            }
            _ => {
                // 
                let x = c.x;
                let y = c.y;
                painter.circle_stroke(egui::pos2(x, y - 8.0 * f), 2.7 * f, stroke);
                painter.line_segment([egui::pos2(x, y - 5.0 * f), egui::pos2(x, y + 2.2 * f)], stroke);
                painter.line_segment([egui::pos2(x, y - 4.0 * f), egui::pos2(x - 6.6 * f, y - 8.6 * f)], stroke);
                painter.line_segment([egui::pos2(x, y - 4.0 * f), egui::pos2(x + 6.6 * f, y - 8.6 * f)], stroke);
                painter.line_segment([egui::pos2(x, y + 2.2 * f), egui::pos2(x - 5.0 * f, y + 9.0 * f)], stroke);
                painter.line_segment([egui::pos2(x, y + 2.2 * f), egui::pos2(x + 5.0 * f, y + 9.0 * f)], stroke);
            }
        }
    }

    fn remind_cd(&self, r: &Reminder, now: i64) -> String {
        let secs = (r.next_due - now).max(0);
        let mins = (secs + 30) / 60;
        if r.next_due <= now {
            "即将".to_string()
        } else if mins < 60 {
            format!("{} 分钟后", mins)
        } else if mins % 60 == 0 {
            format!("{} 小时后", mins / 60)
        } else {
            format!("{} 小时 {} 分后", mins / 60, mins % 60)
        }
    }

    fn remind_next_summary(&self) -> Option<(String, String)> {
        let now = now_secs();
        let mut best: Option<&Reminder> = None;
        for r in self.reminders.iter() {
            if !r.enabled {
                continue;
            }
            best = match best {
                None => Some(r),
                Some(b) => {
                    if (r.next_due - now) < (b.next_due - now) { Some(r) } else { Some(b) }
                }
            };
        }
        best.map(|r| (r.name.clone(), self.remind_cd(r, now)))
    }

    fn remind_glow_sprite(&mut self, ctx: &egui::Context, icon: u8) -> egui::TextureHandle {
        // 0=blue() 1=amber(/) 2=mint()
        let idx: usize = match icon { 0 | 3 => 1, 2 => 2, _ => 0 };
        let cached = match idx {
            1 => self.remind_glow_amber.clone(),
            2 => self.remind_glow_mint.clone(),
            _ => self.remind_glow_blue.clone(),
        };
        if let Some(t) = cached {
            return t;
        }
        let bytes: &[u8] = match idx {
            1 => include_bytes!("../assets/glow_amber.png"),
            2 => include_bytes!("../assets/glow_mint.png"),
            _ => include_bytes!("../assets/glow_blue.png"),
        };
        let img = image::load_from_memory(bytes).expect("glow sprite").to_rgba8();
        let (w, h) = img.dimensions();
        let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        let tex = ctx.load_texture(format!("remind_glow_sprite_{}", idx), ci, egui::TextureOptions::LINEAR);
        match idx {
            1 => self.remind_glow_amber = Some(tex.clone()),
            2 => self.remind_glow_mint = Some(tex.clone()),
            _ => self.remind_glow_blue = Some(tex.clone()),
        }
        tex
    }

    fn remind_glow_phase_sprite(&mut self, ctx: &egui::Context, icon: u8, idx: usize) -> egui::TextureHandle {
        let ci: usize = match icon { 5 => 3, 0 | 3 => 1, 2 => 2, _ => 0 };
        let slot = ci * 11 + idx;
        if let Some(t) = &self.remind_glow_phases[slot] {
            return t.clone();
        }
        let bytes: &[u8] = match (idx, ci) {
            (0, 0) => include_bytes!("../assets/g0_blue.png") as &[u8],
            (1, 0) => include_bytes!("../assets/g1_blue.png") as &[u8],
            (2, 0) => include_bytes!("../assets/g2_blue.png") as &[u8],
            (3, 0) => include_bytes!("../assets/g3_blue.png") as &[u8],
            (4, 0) => include_bytes!("../assets/g4_blue.png") as &[u8],
            (5, 0) => include_bytes!("../assets/g5_blue.png") as &[u8],
            (6, 0) => include_bytes!("../assets/g6_blue.png") as &[u8],
            (7, 0) => include_bytes!("../assets/g7_blue.png") as &[u8],
            (8, 0) => include_bytes!("../assets/g8_blue.png") as &[u8],
            (9, 0) => include_bytes!("../assets/g9_blue.png") as &[u8],
            (10, 0) => include_bytes!("../assets/g10_blue.png") as &[u8],
            (0, 1) => include_bytes!("../assets/g0_amber.png") as &[u8],
            (1, 1) => include_bytes!("../assets/g1_amber.png") as &[u8],
            (2, 1) => include_bytes!("../assets/g2_amber.png") as &[u8],
            (3, 1) => include_bytes!("../assets/g3_amber.png") as &[u8],
            (4, 1) => include_bytes!("../assets/g4_amber.png") as &[u8],
            (5, 1) => include_bytes!("../assets/g5_amber.png") as &[u8],
            (6, 1) => include_bytes!("../assets/g6_amber.png") as &[u8],
            (7, 1) => include_bytes!("../assets/g7_amber.png") as &[u8],
            (8, 1) => include_bytes!("../assets/g8_amber.png") as &[u8],
            (9, 1) => include_bytes!("../assets/g9_amber.png") as &[u8],
            (10, 1) => include_bytes!("../assets/g10_amber.png") as &[u8],
            (0, 2) => include_bytes!("../assets/g0_mint.png") as &[u8],
            (1, 2) => include_bytes!("../assets/g1_mint.png") as &[u8],
            (2, 2) => include_bytes!("../assets/g2_mint.png") as &[u8],
            (3, 2) => include_bytes!("../assets/g3_mint.png") as &[u8],
            (4, 2) => include_bytes!("../assets/g4_mint.png") as &[u8],
            (5, 2) => include_bytes!("../assets/g5_mint.png") as &[u8],
            (6, 2) => include_bytes!("../assets/g6_mint.png") as &[u8],
            (7, 2) => include_bytes!("../assets/g7_mint.png") as &[u8],
            (8, 2) => include_bytes!("../assets/g8_mint.png") as &[u8],
            (9, 2) => include_bytes!("../assets/g9_mint.png") as &[u8],
            (10, 2) => include_bytes!("../assets/g10_mint.png") as &[u8],
            (0, 3) => include_bytes!("../assets/g0_violet.png") as &[u8],
            (1, 3) => include_bytes!("../assets/g1_violet.png") as &[u8],
            (2, 3) => include_bytes!("../assets/g2_violet.png") as &[u8],
            (3, 3) => include_bytes!("../assets/g3_violet.png") as &[u8],
            (4, 3) => include_bytes!("../assets/g4_violet.png") as &[u8],
            (5, 3) => include_bytes!("../assets/g5_violet.png") as &[u8],
            (6, 3) => include_bytes!("../assets/g6_violet.png") as &[u8],
            (7, 3) => include_bytes!("../assets/g7_violet.png") as &[u8],
            (8, 3) => include_bytes!("../assets/g8_violet.png") as &[u8],
            (9, 3) => include_bytes!("../assets/g9_violet.png") as &[u8],
            (10, 3) => include_bytes!("../assets/g10_violet.png") as &[u8],
            _ => include_bytes!("../assets/g0_blue.png") as &[u8],
        };
        let img = image::load_from_memory(bytes).expect("glow phase").to_rgba8();
        let (w, h) = img.dimensions();
        // 裁掉四周透明留白，让光晕更贴边（卡片挖空区在源图内位置固定，故裁边后只需按比例换算绘制宽度）
        const CROP: u32 = 26;
        let cw = w.saturating_sub(2 * CROP);
        let ch = h.saturating_sub(2 * CROP);
        let sub = image::imageops::crop_imm(&img, CROP, CROP, cw, ch).to_image();
        let cimg = egui::ColorImage::from_rgba_unmultiplied([cw as usize, ch as usize], sub.as_raw());
        let tex = ctx.load_texture(format!("remind_glow_phase_{}_{}", ci, idx), cimg, egui::TextureOptions::LINEAR);
        self.remind_glow_phases[slot] = Some(tex.clone());
        tex
    }

    fn remind_glow18_sprite(&mut self, ctx: &egui::Context, icon: u8) -> egui::TextureHandle {
        let idx: usize = match icon { 0 | 3 => 1, 2 => 2, _ => 0 };
        let cached = match idx {
            1 => self.remind_glow18_amber.clone(),
            2 => self.remind_glow18_mint.clone(),
            _ => self.remind_glow18_blue.clone(),
        };
        if let Some(t) = cached {
            return t;
        }
        let bytes: &[u8] = match idx {
            1 => include_bytes!("../assets/glow18_amber.png"),
            2 => include_bytes!("../assets/glow18_mint.png"),
            _ => include_bytes!("../assets/glow18_blue.png"),
        };
        let img = image::load_from_memory(bytes).expect("glow18 sprite").to_rgba8();
        let (w, h) = img.dimensions();
        let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        let tex = ctx.load_texture(format!("remind_glow18_sprite_{}", idx), ci, egui::TextureOptions::LINEAR);
        match idx {
            1 => self.remind_glow18_amber = Some(tex.clone()),
            2 => self.remind_glow18_mint = Some(tex.clone()),
            _ => self.remind_glow18_blue = Some(tex.clone()),
        }
        tex
    }

    fn remind_glow_texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        if let Some(t) = &self.remind_glow_tex {
            return t.clone();
        }
        let n = 256usize;
        let halo = 20.0f32;
        let hc = (n as f32) * 0.5 - halo;
        let rad = 26.0f32;
        let mut pixels = vec![egui::Color32::TRANSPARENT; n * n];
        for y in 0..n {
            for x in 0..n {
                let px = x as f32 + 0.5 - n as f32 * 0.5;
                let py = y as f32 + 0.5 - n as f32 * 0.5;
                let qx = px.abs() - hc;
                let qy = py.abs() - hc;
                let ox = qx.max(0.0);
                let oy = qy.max(0.0);
                let d = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - rad;
                let a = if d <= 0.0 {
                    1.0
                } else {
                    let t = d / (halo * 0.5);
                    (-(t * t)).exp()
                };
                pixels[y * n + x] = egui::Color32::from_rgba_unmultiplied(255, 255, 255, (a * 255.0) as u8);
            }
        }
        let img = egui::ColorImage { size: [n, n], pixels };
        let tex = ctx.load_texture("remind_glow", img, egui::TextureOptions::LINEAR);
        self.remind_glow_tex = Some(tex.clone());
        tex
    }

    fn step_reminders(&mut self, ctx: &egui::Context) {
        let now = now_secs();
        let st = platform::local_time();
        let _ = &st;
        if self.remind_closing && !self.remind_queue.is_empty() {
            self.remind_closing = false;
            self.remind_alert = self.remind_queue.first().copied();
            self.remind_alert_at = Some(Instant::now());
        }
        // .16s  + 
        let aa_target = if self.remind_alert.is_some() && !self.remind_closing { 1.0 } else { 0.0 };
        if let Some(v) = std::env::var("WINISLE_REMIND_ALERTA").ok().and_then(|s| s.parse::<f32>().ok()) {
            self.remind_alert_anim = v.clamp(0.0, 1.0);
        } else if (self.remind_alert_anim - aa_target).abs() > 0.001 {
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            let dur = if self.remind_closing { 0.09 } else { 0.16 };
            let k = 1.0 - (-dt / dur).exp();
            self.remind_alert_anim += (aa_target - self.remind_alert_anim) * k;
            if (self.remind_alert_anim - aa_target).abs() < 0.002 {
                self.remind_alert_anim = aa_target;
                if self.remind_closing && self.remind_alert_anim <= 0.01 {
                    self.remind_alert = None;
                    self.remind_closing = false;
                    self.remind_mlist_mode = false;
                }
            }
            ctx.request_repaint();
        }
        let mut fired: Vec<u64> = Vec::new();
        for r in self.reminders.iter_mut() {
            if !r.enabled {
                continue;
            }
            if r.next_due == 0 {
                r.next_due = now + r.interval_min.max(1) as i64 * 60;
                continue;
            }
            if now >= r.next_due {
                fired.push(r.id);
                r.next_due = i64::MAX; // "已弹出，等待完成/稍后写回"
            }
        }
        for id in fired {
            if !self.remind_queue.iter().any(|i| *i == id) {
                self.remind_queue.push(id);
            }
        }
        if self.remind_alert.is_none() {
            if let Some(id) = self.remind_queue.first().copied() {
                self.remind_alert = Some(id);
                self.remind_alert_at = Some(Instant::now());
                self.remind_dot = None;
                self.remind_dot_at = None;
            }
        }
        if self.remind_alert.is_some() {
            //
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        } else if self.active == Module::Reminder && self.expanded > 0.99 {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if let Some(da) = self.remind_dot_at {
            if da.elapsed().as_secs_f32() > 90.0 {
                self.remind_dot = None;
                self.remind_dot_at = None;
            }
        }
    }

    fn sync_bud_primary(&mut self) {
        if let Some(first) = self.remind_queue.first().copied() {
            self.remind_closing = false;
            self.remind_alert = Some(first);
            self.remind_alert_at = Some(Instant::now());
        } else if self.remind_alert.is_some() {
            // 队列已清空：保留当前 id 让叠层做淡出过渡，动画结束后再真正清除
            self.remind_closing = true;
            self.remind_alert_at = None;
        } else {
            self.remind_alert_at = None;
        }
    }

    fn snooze_reminder(&mut self, id: u64, secs: i64) {
        if let Some(r) = self.reminders.iter_mut().find(|r| r.id == id) {
            r.next_due = now_secs() + secs;
        }
        save_reminders(&self.reminders);
        self.remind_queue.retain(|i| *i != id);
        self.sync_bud_primary();
        self.remind_dot = Some(id);
        self.remind_dot_at = Some(Instant::now());
    }

    fn complete_reminder(&mut self, id: u64) {
        if let Some(r) = self.reminders.iter_mut().find(|r| r.id == id) {
            if r.repeat {
                r.next_due = now_secs() + r.interval_min.max(1) as i64 * 60;
            } else {
                r.next_due = 0;
                r.enabled = false;
            }
        }
        save_reminders(&self.reminders);
        self.remind_queue.retain(|i| *i != id);
        self.sync_bud_primary();
        self.remind_dot = Some(id);
        self.remind_dot_at = Some(Instant::now());
    }

    fn snooze_alert(&mut self, secs: i64) {
        if let Some(id) = self.remind_alert {
            self.snooze_reminder(id, secs);
        }
        self.remind_alert_at = None;
    }

    fn complete_alert(&mut self) {
        if let Some(id) = self.remind_alert {
            self.complete_reminder(id);
        }
        self.remind_alert_at = None;
    }

    fn draw_reminder_page(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, e: f32) {
        let modal = self.remind_alert.is_some();
        let base_a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as f32;
        self.remind_list_anim = (self.remind_list_anim + ui.input(|i| i.stable_dt).clamp(0.001, 0.05) / 0.18).min(1.0);
        if let Some(v) = self.remind_lista_force { self.remind_list_anim = v; }
        if self.remind_list_anim < 1.0 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
        let a = (base_a * self.remind_list_anim) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);

        self.remind_soft_hover = false;
        if self.remind_form.is_some() && self.remind_form_open {
            self.draw_remind_form(ui, painter, rect, e);
            return;
        }

        let x0 = rect.left() + CONTENT_PAD_X;
        let x1 = rect.right() - CONTENT_PAD_X;
        let y0 = rect.top() + CONTENT_PAD_T;

        painter.text(egui::pos2(x0, y0), egui::Align2::LEFT_TOP, "提醒", bold_font(17.0), ink);
        let on_count = self.reminders.iter().filter(|r| r.enabled).count();
        painter.text(egui::pos2(x1, y0 + 4.0), egui::Align2::RIGHT_TOP, &format!("{} 项开启", on_count), bold_font(10.5), muted2);
        painter.text(
            egui::pos2(x0, y0 + 22.0),
            egui::Align2::LEFT_TOP,
            "开启后从现在开始计时 · 重新打开 WinIsle 会从启动时重新计时",
            egui::FontId::proportional(9.0),
            muted2,
        );

        let list_top = y0 + 40.0;
        let add_h = 28.0;
        let add_gap = 8.0;
        let add_bottom = rect.bottom() - CONTENT_PAD_B;
        let add_top = add_bottom - add_h;
        let list_bottom = add_top - add_gap;
        let list_rect = egui::Rect::from_min_max(egui::pos2(x0, list_top), egui::pos2(x1, list_bottom));
        //  /        // WS_EX_NOACTIVATEMA_NOACTIVATE        // ?
        let addrow_rect = egui::Rect::from_min_max(egui::pos2(x0, add_top), egui::pos2(x1, add_bottom));
        let force_hover = std::env::var_os("WINISLE_REMIND_HOVERTEST").is_some();
        self.remind_soft_hover = force_hover
            || (!modal && ui.rect_contains_pointer(list_rect))
            || (!modal && ui.rect_contains_pointer(addrow_rect));
        if self.remind_soft_hover {
            platform::set_activatable_soft(self.hwnd, true);
        }

        struct RRow {
            id: u64,
            name: String,
            icon: u8,
            sub: String,
            cd: String,
            enabled: bool,
            repeat: bool,
            custom: bool,
            interval_min: u32,
        }
        let now = now_secs();
        let mk = |r: &Reminder| RRow {
            id: r.id,
            name: r.name.clone(),
            icon: r.icon,
            sub: format!("每 {} 分钟", r.interval_min),
            cd: self.remind_cd(r, now),
            enabled: r.enabled,
            repeat: r.repeat,
            custom: r.custom,
            interval_min: r.interval_min,
        };
        let health: Vec<RRow> = self.reminders.iter().filter(|r| !r.custom).map(|r| mk(r)).collect();
        let customv: Vec<RRow> = self.reminders.iter().filter(|r| r.custom).map(|r| mk(r)).collect();

        let row_h = 30.0f32;
        let label_h = 16.0f32;
        let content_h = (if health.is_empty() { 0.0 } else { label_h })
            + health.len() as f32 * row_h
            + (if health.is_empty() || customv.is_empty() { 0.0 } else { 8.0 })
            + (if customv.is_empty() { 0.0 } else { label_h })
            + customv.len() as f32 * row_h;
        let view_h = list_rect.height();
        let max_scroll = (content_h - view_h).max(0.0);
        let hovering = !modal && ui.rect_contains_pointer(list_rect);
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        if hovering {
            let dy = ui.input(|i| i.raw_scroll_delta.y);
            if dy.abs() > 0.0 {
                self.remind_scroll_target = (self.remind_scroll_target - dy).clamp(0.0, max_scroll);
                self.remind_scroll_act = Instant::now();
            }
        }
        let k = 1.0 - (-dt / 0.07).exp();
        self.remind_scroll += (self.remind_scroll_target - self.remind_scroll) * k;
        if (self.remind_scroll_target - self.remind_scroll).abs() < 0.2 {
            self.remind_scroll = self.remind_scroll_target;
        }
        let active_sb = hovering && self.remind_scroll_act.elapsed().as_secs_f32() < 1.2 && max_scroll > 0.5;
        let vgoal = if active_sb { 1.0 } else { 0.0 };
        self.remind_scroll_vis += (vgoal - self.remind_scroll_vis) * (1.0 - (-dt / 0.12).exp());
        // debug-only: force scrollbar visible for offscreen screenshots
        if std::env::var_os("WINISLE_REMIND_SB").is_some() {
            self.remind_scroll_vis = 1.0;
        }

        let cp = painter.with_clip_rect(list_rect);
        let mut y = list_rect.top() - self.remind_scroll;
        let mut toggle_apply: Vec<(u64, bool)> = Vec::new();

        let dbg_hover = self.remind_dbg_hover;
        let draw_group = |cp: &egui::Painter, y: &mut f32, title: &str, rows: &[RRow], toggle_apply: &mut Vec<(u64, bool)>, open_edit: &mut Option<u64>, delete_quick: &mut Vec<u64>, ui: &mut egui::Ui| {
            cp.text(egui::pos2(x0, *y), egui::Align2::LEFT_TOP, title, bold_font(10.5), muted2);
            *y += label_h;
            for r in rows.iter() {
                let rr = egui::Rect::from_min_size(egui::pos2(x0, *y), egui::vec2(x1 - x0, row_h));
                if (!modal && ui.rect_contains_pointer(rr)) || dbg_hover == Some(r.id) {
                    cp.rect_filled(rr, 8.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * 0.10) as u8));
                }
                let col = remind_color(r.icon);
                let cc = egui::Color32::from_rgb(col[0], col[1], col[2]);
                let irect = egui::Rect::from_center_size(egui::pos2(x0 + 15.0, *y + row_h * 0.5), egui::vec2(24.0, 24.0));
                cp.rect_filled(irect, 8.0, egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], (a as f32 * 0.16) as u8));
                Self::draw_remind_icon(cp, r.icon, irect.center(), 16.0, cc, 1.4);
                let name_dy = -7.0;
                cp.text(egui::pos2(x0 + 34.0, *y + row_h * 0.5 + name_dy), egui::Align2::LEFT_CENTER, &r.name, bold_font(12.5), ink);
                // 标签：循环/一次，循环型额外显示「每 N 分钟」
                {
                    let name_w = ui.fonts(|f| f.layout_no_wrap(r.name.clone(), bold_font(12.5), ink).size().x);
                    let tag_font = bold_font(8.5);
                    let mut tx = x0 + 34.0 + name_w + 8.0;
                    let tag_ty = *y + row_h * 0.5 + name_dy - 7.5;
                    let tag_txt = if r.repeat { "循环" } else { "一次" };
                    let (cr_, cg_, cb_) = if r.repeat { (52.0, 211.0, 153.0) } else { (230.0, 178.0, 60.0) };
                    let w1 = ui.fonts(|f| f.layout_no_wrap(tag_txt.to_string(), tag_font.clone(), ink).size().x) + 16.0;
                    let r1 = egui::Rect::from_min_size(egui::pos2(tx, tag_ty), egui::vec2(w1, 15.0));
                    cp.rect_stroke(r1, 7.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(cr_ as u8, cg_ as u8, cb_ as u8, (a as f32 * 0.38) as u8)), egui::StrokeKind::Inside);
                    cp.text(r1.center(), egui::Align2::CENTER_CENTER, tag_txt, tag_font.clone(), egui::Color32::from_rgba_unmultiplied(cr_ as u8, cg_ as u8, cb_ as u8, a));
                    tx += w1 + 6.0;
                    if r.repeat {
                        let t2 = format!("每 {} 分钟", r.interval_min);
                        let w2 = ui.fonts(|f| f.layout_no_wrap(t2.clone(), tag_font.clone(), ink).size().x) + 16.0;
                        let r2 = egui::Rect::from_min_size(egui::pos2(tx, tag_ty), egui::vec2(w2, 15.0));
                        cp.rect_stroke(r2, 7.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(150, 158, 175, (a as f32 * 0.30) as u8)), egui::StrokeKind::Inside);
                        cp.text(r2.center(), egui::Align2::CENTER_CENTER, &t2, tag_font, muted2);
                    }
                }
                let tw = 34.0;
                let th = 18.0;
                // 6px
                let tr = egui::Rect::from_min_size(egui::pos2(x1 - 12.0 - tw, *y + row_h * 0.5 - th * 0.5), egui::vec2(tw, th));
                // 
                let mut trash_rect = egui::Rect::from_min_size(egui::pos2(tr.left() - 30.0, *y + row_h * 0.5 - 10.0), egui::vec2(20.0, 20.0));
                if r.custom {
                    let tcol = egui::Color32::from_rgba_unmultiplied(150, 158, 175, (a as f32 * 0.9) as u8);
                    let tc = trash_rect.center();
                    let sw = egui::Stroke::new(1.3, tcol);
                    let y0 = tc.y - 4.8;
                    cp.line_segment([egui::pos2(tc.x - 5.2, y0), egui::pos2(tc.x + 5.2, y0)], sw);
                    cp.line_segment([egui::pos2(tc.x - 1.9, y0 - 2.0), egui::pos2(tc.x + 1.9, y0 - 2.0)], sw);
                    cp.line_segment([egui::pos2(tc.x - 3.7, y0), egui::pos2(tc.x - 3.1, tc.y + 5.6)], sw);
                    cp.line_segment([egui::pos2(tc.x + 3.7, y0), egui::pos2(tc.x + 3.1, tc.y + 5.6)], sw);
                    cp.line_segment([egui::pos2(tc.x - 3.1, tc.y + 5.6), egui::pos2(tc.x + 3.1, tc.y + 5.6)], sw);
                    cp.line_segment([egui::pos2(tc.x - 1.2, y0 + 1.6), egui::pos2(tc.x - 1.0, tc.y + 3.6)], egui::Stroke::new(1.0, tcol));
                    cp.line_segment([egui::pos2(tc.x + 1.2, y0 + 1.6), egui::pos2(tc.x + 1.0, tc.y + 3.6)], egui::Stroke::new(1.0, tcol));
                }
                let cd_right = if r.custom { trash_rect.left() - 8.0 } else { tr.left() - 10.0 };
                if r.enabled {
                    cp.text(egui::pos2(cd_right, *y + row_h * 0.5), egui::Align2::RIGHT_CENTER, &r.cd, egui::FontId::proportional(10.0), muted);
                }
                if let Some(v) = Self::draw_toggle(ui, cp, tr, r.enabled, ui.id().with(("rem_tog", r.id)), accent, a, true) {
                    toggle_apply.push((r.id, v));
                }
                if r.custom {
                    let tresp = ui.interact(trash_rect, ui.id().with(("rem_del_row", r.id)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if tresp.clicked() {
                        delete_quick.push(r.id);
                    }
                    if tresp.hovered() {
                        cp.rect_filled(trash_rect, 5.0, egui::Color32::from_rgba_unmultiplied(242, 112, 92, (a as f32 * 0.14) as u8));
                    }
                }
                // /
                let row_right = if r.custom { trash_rect.left() - 4.0 } else { tr.left() - 10.0 };
                let row_hit = egui::Rect::from_min_max(rr.min, egui::pos2(row_right, rr.max.y));
                let row_resp = ui
                    .interact(row_hit, ui.id().with(("rem_row", r.id)), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if row_resp.clicked() {
                    *open_edit = Some(r.id);
                }
                *y += row_h;
            }
        };

        let mut open_edit: Option<u64> = None;
        let mut delete_quick: Vec<u64> = Vec::new();
        if !health.is_empty() {
            draw_group(&cp, &mut y, "健康习惯", &health, &mut toggle_apply, &mut open_edit, &mut delete_quick, ui);
        }
        if !health.is_empty() && !customv.is_empty() {
            y += 8.0;
        }
        if !customv.is_empty() {
            draw_group(&cp, &mut y, "自定义", &customv, &mut toggle_apply, &mut open_edit, &mut delete_quick, ui);
        }

        if !delete_quick.is_empty() {
            for did in delete_quick.iter() {
                self.reminders.retain(|r| r.id != *did);
                if self.remind_alert == Some(*did) { self.remind_alert = None; self.remind_alert_at = None; }
            }
            save_reminders(&self.reminders);
        }
        for (id, v) in toggle_apply {
            if let Some(r) = self.reminders.iter_mut().find(|r| r.id == id) {
                r.enabled = v;
                r.next_due = if v { now + r.interval_min.max(1) as i64 * 60 } else { 0 };
            }
            save_reminders(&self.reminders);
        }
        if let Some(id) = open_edit {
            if let Some(r) = self.reminders.iter().find(|r| r.id == id) {
                self.remind_form = Some(RemindForm {
                    id: r.id,
                    name: r.name.clone(),
                    icon: r.icon,
                    interval_min: r.interval_min,
                    interval_text: r.interval_min.to_string(),
                    window_start: r.window_start,
                    window_end: r.window_end,
                    days: r.days,
                    repeat: r.repeat,
                    sound: true,
                });
                self.remind_form_focus = true;
                self.remind_focus_tries = 0;
                platform::set_activatable(self.hwnd, true);
                self.remind_tp_open = None;
                self.remind_form_open = true;
                self.remind_anim = 0.0;
            }
        }

        // ---- rect.right()-4 ----
        if max_scroll > 0.5 && self.remind_scroll_vis > 0.01 {
            let sv = self.remind_scroll_vis;
            let tx = rect.right() - 4.0;
            let track = egui::Rect::from_min_max(egui::pos2(tx, list_rect.top() + 2.0), egui::pos2(tx + 2.0, list_rect.bottom() - 2.0));
            painter.rect_filled(track, 1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (12.0 * sv) as u8));
            let th = ((view_h / content_h.max(1.0)) * track.height()).max(14.0);
            let range = (track.height() - th).max(1.0);
            let tp = (self.remind_scroll / max_scroll.max(1.0)) * range;
            let thumb = egui::Rect::from_min_size(egui::pos2(tx, track.top() + tp), egui::vec2(2.0, th));
            painter.rect_filled(thumb, 1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (60.0 * sv) as u8));
        }

        let addrow = egui::Rect::from_min_size(egui::pos2(x0, add_top), egui::vec2(x1 - x0, add_h));
        if !modal && ui.rect_contains_pointer(addrow) {
            painter.rect_filled(addrow, 9.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * 0.08) as u8));
        }
        painter.rect_stroke(addrow, 9.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * 0.30) as u8)), egui::StrokeKind::Inside);
        painter.text(addrow.center(), egui::Align2::CENTER_CENTER, "＋  添加提醒", bold_font(12.0), accent);
        let resp = ui.interact(addrow, ui.id().with("rem_add"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            self.remind_form = Some(RemindForm {
                id: 0,
                name: String::new(),
                icon: 5,
                interval_min: 30,
                interval_text: "30".to_string(),
                window_start: 540,
                window_end: 1200,
                days: 0b1111111,
                repeat: true,
                sound: true,
            });
            self.remind_form_focus = true;
            self.remind_focus_tries = 0;
            platform::set_activatable(self.hwnd, true);
            self.remind_form_open = true;
            self.remind_anim = 0.0;
        }
        if !self.remind_form_open && self.remind_anim > 0.001 && self.remind_form.is_some() {
            self.draw_remind_form(ui, painter, rect, e);
        }
    }

    fn draw_remind_form(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, e: f32) {
        let mut form = match self.remind_form.take() {
            Some(f) => f,
            None => return,
        };
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        let open_target = if self.remind_form_open { 1.0 } else { 0.0 };
        self.remind_anim += (open_target - self.remind_anim) * (1.0 - (-dt / 0.09).exp());
        if (open_target - self.remind_anim).abs() < 0.01 { self.remind_anim = open_target; }
        if !self.remind_form_open && self.remind_anim < 0.01 { self.remind_form = None; return; }
        let fa = self.remind_anim.clamp(0.0, 1.0);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        let rect = rect.translate(egui::vec2(0.0, (1.0 - fa) * 10.0));
        let a = ((((e - 0.5) * 2.0) * 255.0) * fa).clamp(0.0, 255.0) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);
        let interactive = self.remind_form_open;
        let force_hover = std::env::var_os("WINISLE_REMIND_HOVERTEST").is_some();
        //         //  NOACTIVATE 
        if ui.rect_contains_pointer(rect) {
            platform::set_activatable(self.hwnd, true);
        }
        // 
        {
            let v = &mut ui.style_mut().visuals;
            v.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(52, 211, 153, 56);
            v.selection.stroke = egui::Stroke::new(1.0, ink);
            v.extreme_bg_color = egui::Color32::TRANSPARENT;
            // caret
            v.text_cursor = egui::style::TextCursorStyle {
                stroke: egui::Stroke::new(1.6, egui::Color32::from_rgba_unmultiplied(233, 236, 242, a.max(200))),
                ..Default::default()
            };
        }
        let x0 = rect.left() + CONTENT_PAD_X;
        let x1 = rect.right() - CONTENT_PAD_X;
        let label_x = x0;
        let ctrl_x = x0 + 52.0;
        let ctrl_w = x1 - ctrl_x;
        let y0 = rect.top() + CONTENT_PAD_T;
        let gap = 10.0f32;

        let back_r = egui::Rect::from_min_size(egui::pos2(x0, y0 - 2.0), egui::vec2(22.0, 22.0));
        let back_hover = ui.rect_contains_pointer(back_r);
        let bc = if back_hover { ink } else { muted };
        painter.line_segment([egui::pos2(back_r.center().x + 3.0, back_r.center().y - 5.0), egui::pos2(back_r.center().x - 2.5, back_r.center().y)], egui::Stroke::new(1.6, bc));
        painter.line_segment([egui::pos2(back_r.center().x - 2.5, back_r.center().y), egui::pos2(back_r.center().x + 3.0, back_r.center().y + 5.0)], egui::Stroke::new(1.6, bc));
        let back_resp = ui.interact(back_r, ui.id().with("rem_back"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if back_resp.clicked() && interactive {
            self.remind_tp_open = None;
            self.remind_form_open = false;
            self.remind_anim = 0.0;
            self.remind_list_anim = 0.0;
            self.remind_form = None;
            return;
        }
        let mut y = y0 + 28.0;

        painter.text(egui::pos2(label_x, y + 14.0), egui::Align2::LEFT_CENTER, "名称", bold_font(10.5), muted2);
        let name_rect = egui::Rect::from_min_size(egui::pos2(ctrl_x, y), egui::vec2(ctrl_w, 28.0));
        if form.name.is_empty() {
            painter.text(
                egui::pos2(name_rect.left() + 9.0, name_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "例在：站起来拉伸",
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgba_unmultiplied(107, 113, 128, a),
            );
        }
        let name_id = egui::Id::new("remind_name_input");
        //
        self.remind_form_focus = false;
        let dbg_press: Option<u8> = std::env::var("WINISLE_REMIND_FOCUS_FIELD").ok().and_then(|v| v.parse::<u8>().ok());
        let name_hover = ui.rect_contains_pointer(name_rect);
        if (name_hover && ui.input(|i| i.pointer.primary_pressed() || i.pointer.primary_down())) || dbg_press == Some(0) {
            // TextEdit 
            // 强制前台，确保单击即可聚焦输入框
            platform::force_foreground(self.hwnd);
            if name_hover && ui.input(|i| i.pointer.primary_pressed()) {
                self.remind_active_field = 1;
                ui.memory_mut(|m| m.surrender_focus(egui::Id::new("remind_interval_input")));
            }
            if dbg_press == Some(0) {
                ui.memory_mut(|m| m.request_focus(name_id));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                self.remind_active_field = 1;
            }
        }
        let name_mem_foc = ui.memory(|m| m.has_focus(name_id));
        // egui  viewport focused         // winit viewport  focusedegui 
        //  viewport focused  egui +?
        let vp_before = ui.ctx().input(|i| i.focused);
        let force_caret = !vp_before && name_mem_foc;
        if force_caret {
            ui.ctx().input_mut(|i| i.focused = true);
        }
        if std::env::var_os("WINISLE_REMIND_SELALL_NAME").is_some() && !form.name.is_empty() {
            let n = form.name.chars().count();
            if let Some(mut st) = egui::text_edit::TextEditState::load(ui.ctx(), name_id) {
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(n),
                )));
                st.store(ui.ctx(), name_id);
            }
        }
        let te = ui
            .scope(|ui| {
                let v = ui.visuals_mut();
                v.text_cursor.stroke = egui::Stroke::new(1.6, egui::Color32::from_rgb(233, 236, 242));
                v.extreme_bg_color = egui::Color32::TRANSPARENT;
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(name_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                child.add_sized(
                    [name_rect.width(), name_rect.height()],
                    egui::TextEdit::singleline(&mut form.name)
                        .frame(false)
                        .id(name_id)
                        .margin(egui::Margin::symmetric(9, 3))
                        .font(egui::FontId::proportional(12.5))
                        .text_color(ink),
                )
            })
            .inner;
        if force_caret {
            ui.ctx().input_mut(|i| i.focused = vp_before);
        }
        {
            let hot = name_mem_foc || ui.rect_contains_pointer(name_rect);
            if force_hover || ui.rect_contains_pointer(name_rect) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                platform::set_activatable_soft(self.hwnd, true);
                self.remind_soft_hover = true;
            }
            painter.rect_stroke(
                name_rect,
                8.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * if hot { 0.42 } else { 0.13 }) as u8)),
                egui::StrokeKind::Inside,
            );
        }
        if std::env::var_os("WINISLE_FOCUS_LOG").is_some() {
            if self.remind_focus_tries == 30 || self.remind_focus_tries == 22 || self.remind_focus_tries == 14 || self.remind_focus_tries == 6 {
                let vp = ui.ctx().input(|i| i.focused);
                eprintln!("[focusdbg] tries={} mem_focus={} vp_focused={} id={:?}",
                    self.remind_focus_tries, name_mem_foc, vp, name_id);
            }
        }
        y += 28.0 + gap;


        painter.text(egui::pos2(label_x, y + 11.0), egui::Align2::LEFT_CENTER, "每隔", bold_font(10.5), muted2);
        //   TextEdit
        let ibox = egui::Rect::from_min_size(egui::pos2(ctrl_x, y), egui::vec2(66.0, 24.0));
        let interval_id = egui::Id::new("remind_interval_input");
        {
            if dbg_press == Some(1) {
                ui.memory_mut(|m| m.request_focus(interval_id));
            }
            let ivp_before = ui.ctx().input(|i| i.focused);
            let imem0 = ui.ctx().memory(|m| m.has_focus(interval_id));
            let iforce = !ivp_before && imem0;
            if iforce { ui.ctx().input_mut(|i| i.focused = true); }
            let mut itxt = std::mem::take(&mut form.interval_text);
            if std::env::var_os("WINISLE_REMIND_SELALL_IV").is_some() && !itxt.is_empty() {
                let n = itxt.chars().count();
                if let Some(mut st) = egui::text_edit::TextEditState::load(ui.ctx(), interval_id) {
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(n),
                    )));
                    st.store(ui.ctx(), interval_id);
                }
            }
            let ite = egui::TextEdit::singleline(&mut itxt)
                .frame(false)
                .id(interval_id)
                .char_limit(4)
                .horizontal_align(egui::Align::Center)
                .font(egui::FontId::proportional(12.5))
                .text_color(ink);
            ui.scope(|ui| {
                let v = ui.visuals_mut();
                v.extreme_bg_color = egui::Color32::TRANSPARENT;
                v.text_cursor.stroke = egui::Stroke::new(1.6, egui::Color32::from_rgb(233, 236, 242));
                ui.put(ibox, ite);
            });
            if iforce { let r = ivp_before; ui.ctx().input_mut(|i| i.focused = r); }
            itxt.retain(|c| c.is_ascii_digit());
            if itxt.chars().count() > 4 { itxt = itxt.chars().take(4).collect(); }
            form.interval_text = itxt;
        }
        {
            let hot = ui.memory(|m| m.has_focus(interval_id)) || ui.rect_contains_pointer(ibox);
            if force_hover || ui.rect_contains_pointer(ibox) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                platform::set_activatable_soft(self.hwnd, true);
                self.remind_soft_hover = true;
            }
            painter.rect_stroke(
                ibox,
                7.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * if hot { 0.42 } else { 0.13 }) as u8)),
                egui::StrokeKind::Inside,
            );
        }
        painter.text(egui::pos2(ibox.right() + 8.0, y + 12.0), egui::Align2::LEFT_CENTER, "分钟", egui::FontId::proportional(10.5), muted2);
        if let Ok(v) = form.interval_text.trim().parse::<u32>() {
            form.interval_min = v.clamp(1, 1440);
        }
        y += 26.0 + gap;

        painter.text(egui::pos2(label_x, y + 13.0), egui::Align2::LEFT_CENTER, "方式", bold_font(10.5), muted2);
        {
            let mh = 24.0f32;
            let w1 = 58.0f32;
            let w2 = 88.0f32;
            let r1 = egui::Rect::from_min_size(egui::pos2(ctrl_x, y), egui::vec2(w1, mh));
            let r2 = egui::Rect::from_min_size(egui::pos2(ctrl_x + w1 + 8.0, y), egui::vec2(w2, mh));
            for (rc, on, txt) in [(r1, form.repeat, "循环"), (r2, !form.repeat, "只提示一次")] {
                painter.rect_stroke(
                    rc, 7.0,
                    egui::Stroke::new(if on { 1.3 } else { 1.0 }, if on { accent } else { egui::Color32::from_rgba_unmultiplied(255, 255, 255, (a as f32 * 0.12) as u8) }),
                    egui::StrokeKind::Inside,
                );
                painter.text(rc.center(), egui::Align2::CENTER_CENTER, txt, bold_font(11.0), if on { accent } else { muted2 });
            }
            if ui.interact(r1, ui.id().with("rem_mode_loop"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                form.repeat = true;
            }
            if ui.interact(r2, ui.id().with("rem_mode_once"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                form.repeat = false;
            }
        }
        y += 24.0 + gap;

        let bh = 30.0f32;
        let save_w = 84.0f32;
        let cancel_w = 74.0f32;
        let save_rect = egui::Rect::from_min_size(egui::pos2(x1 - save_w, y), egui::vec2(save_w, bh));
        let cancel_rect = egui::Rect::from_min_size(egui::pos2(save_rect.left() - 10.0 - cancel_w, y), egui::vec2(cancel_w, bh));
        let can_delete = form.id != 0 && self.reminders.iter().any(|r| r.id == form.id && r.custom);
        let del_rect = egui::Rect::from_min_size(egui::pos2(cancel_rect.left() - 10.0 - 64.0, y), egui::vec2(64.0, bh));
        let mut del_clicked = false;
        if can_delete {
            let danger = egui::Color32::from_rgba_unmultiplied(242, 112, 92, a);
            painter.rect_stroke(del_rect, 9.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(242, 112, 92, (a as f32 * 0.35) as u8)), egui::StrokeKind::Inside);
            painter.text(del_rect.center(), egui::Align2::CENTER_CENTER, "删除", bold_font(12.0), danger);
            del_clicked = ui.interact(del_rect, ui.id().with("rem_delete"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        }
        painter.rect_stroke(cancel_rect, 9.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (a as f32 * 0.13) as u8)), egui::StrokeKind::Inside);
        painter.text(cancel_rect.center(), egui::Align2::CENTER_CENTER, "取消", bold_font(12.0), muted);
        painter.rect_filled(save_rect, 9.0, accent);
        painter.text(save_rect.center(), egui::Align2::CENTER_CENTER, "保存", bold_font(12.0), egui::Color32::from_rgb(16, 20, 24));
        let cancel = ui.interact(cancel_rect, ui.id().with("rem_cancel"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        let save = ui.interact(save_rect, ui.id().with("rem_save"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();

        if del_clicked && interactive {
            let did = form.id;
            self.reminders.retain(|r| r.id != did);
            if self.remind_alert == Some(did) { self.remind_alert = None; self.remind_alert_at = None; }
            save_reminders(&self.reminders);
            self.remind_form_open = false;
            self.remind_anim = 0.0;
            self.remind_list_anim = 0.0;
            self.remind_form = None;
            return;
        }
        if cancel && interactive {
            self.remind_form_open = false;
            self.remind_anim = 0.0;
            self.remind_list_anim = 0.0;
            self.remind_form = None;
            return;
        }
        if save && interactive {
            let now = now_secs();
            let nm = if form.name.trim().is_empty() { "新提醒".to_string() } else { form.name.trim().to_string() };            let days = if form.days == 0 { 0b1111111u8 } else { form.days };
            if form.id == 0 {
                self.remind_next_id += 1;
                let id = self.remind_next_id;
                form.id = id;
                self.reminders.push(Reminder {
                    id,
                    name: nm,
                    icon: form.icon,
                    interval_min: form.interval_min.max(1),
                    window_start: form.window_start,
                    window_end: form.window_end,
                    days,
                    enabled: true,
                    custom: true,
                    repeat: form.repeat,
                    next_due: 0,
                });
            } else if let Some(r) = self.reminders.iter_mut().find(|r| r.id == form.id) {
                r.name = nm;
                r.icon = form.icon;
                r.interval_min = form.interval_min.max(1);
                r.window_start = form.window_start;
                r.window_end = form.window_end;
                r.days = days;
                r.repeat = form.repeat;
                r.next_due = 0;
                r.enabled = true;
            }
            if let Some(r) = self.reminders.iter_mut().find(|x| x.id == form.id) {
                r.next_due = now + r.interval_min.max(1) as i64 * 60;
            }
            save_reminders(&self.reminders);
            self.remind_form_open = false;
            self.remind_anim = 0.0;
            self.remind_list_anim = 0.0;
            self.remind_form = None;
            return;
        }
        if self.remind_tp_open.is_some() && interactive {
            self.draw_remind_time_picker(ui, painter, rect, e, &mut form);
        }
        self.remind_form = Some(form);
    }

    ///  /  + ?
fn draw_remind_time_picker(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, e: f32, form: &mut RemindForm) {
        let which = match self.remind_tp_open {
            Some(w) => w,
            None => return,
        };
        let a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);
        //  + 
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(8, 10, 14, (a as f32 * 0.55) as u8));
        let cw = 214.0f32;
        let ch = 214.0f32;
        let card = egui::Rect::from_center_size(rect.center(), egui::vec2(cw, ch));
        painter.rect_filled(card, 16.0, egui::Color32::from_rgb(30, 33, 40));
        painter.rect_stroke(card, 16.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, 60)), egui::StrokeKind::Inside);
        painter.text(egui::pos2(card.center().x, card.top() + 18.0), egui::Align2::CENTER_CENTER, if which == 0 { "开始时间" } else { "结束时间" }, bold_font(12.5), ink);

        let row_h = 26.0f32;
        let view_rows = 5.0f32;
        let view_h = row_h * view_rows;
        let list_top = card.top() + 34.0;
        let col_w = 62.0f32;
        let gap = 18.0f32;
        let hx = card.center().x - col_w - gap * 0.5;
        let mx = card.center().x + gap * 0.5;
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        //  5
        let hour_count = 24i32;
        let min_count = 12i32;
        let m_idx = (self.remind_tp_min / 5) as i32;
        // 
        let center_off = |idx: i32| idx as f32 * row_h - (view_rows * 0.5 - 0.5) * row_h;
        let max_h_off = ((hour_count - 1) as f32 * row_h).max(0.0);
        let max_m_off = ((min_count - 1) as f32 * row_h).max(0.0);
        let hr_rect = egui::Rect::from_min_size(egui::pos2(hx, list_top), egui::vec2(col_w, view_h));
        let mr_rect = egui::Rect::from_min_size(egui::pos2(mx, list_top), egui::vec2(col_w, view_h));
        // 
        let sel_y = list_top + 2.0 * row_h;
        let line_col = egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * 0.55) as u8);
        painter.line_segment([egui::pos2(hx - 10.0, sel_y), egui::pos2(mx + col_w + 10.0, sel_y)], egui::Stroke::new(1.2, line_col));
        painter.line_segment([egui::pos2(hx - 10.0, sel_y + row_h), egui::pos2(mx + col_w + 10.0, sel_y + row_h)], egui::Stroke::new(1.2, line_col));
        //  ?+  +
        if ui.rect_contains_pointer(hr_rect) {
            let dy = ui.input(|i| i.raw_scroll_delta.y);
            if dy.abs() > 0.0 {
                self.remind_tp_hs_t = (self.remind_tp_hs_t - dy * 0.55).clamp(0.0, max_h_off);
            }
        }
        if ui.rect_contains_pointer(mr_rect) {
            let dy = ui.input(|i| i.raw_scroll_delta.y);
            if dy.abs() > 0.0 {
                self.remind_tp_ms_t = (self.remind_tp_ms_t - dy * 0.55).clamp(0.0, max_m_off);
            }
        }
        let k = 1.0 - (-dt / 0.075).exp();
        self.remind_tp_hs += (self.remind_tp_hs_t - self.remind_tp_hs) * k;
        self.remind_tp_ms += (self.remind_tp_ms_t - self.remind_tp_ms) * k;
        if (self.remind_tp_hs_t - self.remind_tp_hs).abs() < 0.6 {
            self.remind_tp_hs = self.remind_tp_hs_t;
            let idx = (self.remind_tp_hs / row_h).round().clamp(0.0, (hour_count - 1) as f32);
            self.remind_tp_hour = idx as u32;
            self.remind_tp_hs_t = idx * row_h;
        }
        if (self.remind_tp_ms_t - self.remind_tp_ms).abs() < 0.6 {
            self.remind_tp_ms = self.remind_tp_ms_t;
            let idx = (self.remind_tp_ms / row_h).round().clamp(0.0, (min_count - 1) as f32);
            self.remind_tp_min = (idx as u32) * 5;
            self.remind_tp_ms_t = idx * row_h;
        }
        if (self.remind_tp_hs_t - self.remind_tp_hs).abs() > 0.3 || (self.remind_tp_ms_t - self.remind_tp_ms).abs() > 0.3 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
        let hp = painter.with_clip_rect(egui::Rect::from_min_size(egui::pos2(hx, list_top), egui::vec2(col_w, view_h)));
        let mp = painter.with_clip_rect(egui::Rect::from_min_size(egui::pos2(mx, list_top), egui::vec2(col_w, view_h)));
        // 
for i in 0..hour_count {
            let ry = list_top + 2.0 * row_h + i as f32 * row_h - self.remind_tp_hs;
            if ry + row_h < list_top - 1.0 || ry > list_top + view_h + 1.0 {
                continue;
            }
            let sel = i as u32 == self.remind_tp_hour;
            let rr = egui::Rect::from_min_size(egui::pos2(hx, ry), egui::vec2(col_w, row_h));
            hp.text(rr.center(), egui::Align2::CENTER_CENTER, format!("{:02}", i), if sel { bold_font(14.0) } else { egui::FontId::proportional(12.5) }, if sel { ink } else { muted2 });
            if ui.interact(rr, ui.id().with(("tp_h", i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.remind_tp_hour = i as u32;
                self.remind_tp_hs_t = self.remind_tp_hour as f32 * row_h;
            }
        }
        // 
for i in 0..min_count {
            let ry = list_top + 2.0 * row_h + i as f32 * row_h - self.remind_tp_ms;
            if ry + row_h < list_top - 1.0 || ry > list_top + view_h + 1.0 {
                continue;
            }
            let sel = i == m_idx;
            let rr = egui::Rect::from_min_size(egui::pos2(mx, ry), egui::vec2(col_w, row_h));
            mp.text(rr.center(), egui::Align2::CENTER_CENTER, format!("{:02}", i * 5), if sel { bold_font(14.0) } else { egui::FontId::proportional(12.5) }, if sel { ink } else { muted2 });
            if ui.interact(rr, ui.id().with(("tp_m", i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.remind_tp_min = (i * 5) as u32;
                self.remind_tp_ms_t = i as f32 * row_h;
            }
        }
        let _ = dt;
        // 
        let bh = 28.0f32;
        let bw = 76.0f32;
        let by = card.bottom() - 16.0 - bh;
        let cancel_rect = egui::Rect::from_min_size(egui::pos2(card.center().x - bw - 6.0, by), egui::vec2(bw, bh));
        let ok_rect = egui::Rect::from_min_size(egui::pos2(card.center().x + 6.0, by), egui::vec2(bw, bh));
        painter.rect_stroke(cancel_rect, 9.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (a as f32 * 0.13) as u8)), egui::StrokeKind::Inside);
        painter.text(cancel_rect.center(), egui::Align2::CENTER_CENTER, "取消", bold_font(12.0), egui::Color32::from_rgba_unmultiplied(150, 158, 175, a));
        painter.rect_filled(ok_rect, 9.0, accent);
        painter.text(ok_rect.center(), egui::Align2::CENTER_CENTER, "确定", bold_font(12.0), egui::Color32::from_rgb(16, 20, 24));
        let cancel = ui.interact(cancel_rect, ui.id().with("tp_cancel"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        let ok = ui.interact(ok_rect, ui.id().with("tp_ok"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        if cancel {
            self.remind_tp_open = None;
        }
        if ok {
            let val = self.remind_tp_hour * 60 + self.remind_tp_min;
            if which == 0 {
                form.window_start = val;
                if form.window_end <= form.window_start {
                    form.window_end = (form.window_start + 60).min(24 * 60 - 1);
                }
            } else {
                form.window_end = val;
                if form.window_end <= form.window_start {
                    form.window_start = form.window_end.saturating_sub(60);
                }
            }
            self.remind_tp_open = None;
        }
    }

    fn draw_reminder_alert(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, content: egui::Rect, e: f32, t: f64) {
        let id = match self.remind_alert {
            Some(i) => i,
            None => return,
        };
        let icon_hint = match self.reminders.iter().find(|r| r.id == id) {
            Some(r) => r.icon,
            None => return,
        };
        let rem = match self.reminders.iter().find(|r| r.id == id) {
            Some(r) => r.clone(),
            None => return,
        };
        let _ = e;
        let aa = self.remind_alert_anim.clamp(0.0, 1.0);
        if aa <= 0.002 {
            return;
        }
        // 单卡 <-> 列表 切换过渡（1→N 时大卡淡出、列表淡入）
        let cur_n = self.remind_queue.len().max(if self.remind_alert.is_some() { 1 } else { 0 });
        let was_mode = self.remind_mlist_mode;
        if cur_n > 1 {
            self.remind_mlist_mode = true;
        }
        let want_list = self.remind_mlist_mode;
        let dtm = ui.input(|i| i.stable_dt).clamp(0.001, 0.1);
        if want_list && !was_mode && cur_n > 1 {
            self.remind_morph = 0.0;
        } else if !want_list {
            self.remind_morph = 1.0;
        } else {
            self.remind_morph = (self.remind_morph + dtm / 0.20).min(1.0);
        }
        if let Some(v) = std::env::var("WINISLE_MORPH_FREEZE").ok().and_then(|s| s.parse::<f32>().ok()) {
            self.remind_morph = v.clamp(0.0, 1.0);
        }
        let ease = aa * aa * (3.0 - 2.0 * aa);
        let fade = |c: egui::Color32| egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), ((c.a() as f32) * aa) as u8);
        // ---------- 多条提醒：列表逐条处理（同屏 3 条，处理后剩余上移渐显） ----------
        {
            let mut ids: Vec<u64> = self.remind_queue.clone();
            if ids.is_empty() {
                if let Some(a) = self.remind_alert { ids.push(a); }
            }
            ids.retain(|i| self.reminders.iter().any(|r| r.id == *i));
            if want_list && !ids.is_empty() {
                let n_total = ids.len();
                let vis: Vec<u64> = ids.iter().take(3).copied().collect();
                let sig = (n_total as u64).wrapping_mul(1000003) ^ vis.first().copied().unwrap_or(0);
                let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.1);
                if sig != self.remind_mlist_sig {
                    self.remind_mlist_sig = sig;
                    self.remind_mlist_anim = 0.0;
                } else {
                    self.remind_mlist_anim = (self.remind_mlist_anim + dt / 0.14).min(1.0);
                }
                let le = self.remind_mlist_anim;
                let le_ease = le * le * (3.0 - 2.0 * le);
                let ra = aa * (0.35 + 0.65 * le_ease) * self.remind_morph.clamp(0.0, 1.0);
                let rfade = |c: egui::Color32| egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), ((c.a() as f32) * ra) as u8);
                // 遮罩（跟随内容区右侧圆角）
                let rr = self.cfg.radius.min(content.height() * 0.5).min(content.width() * 0.5);
                let cr = self.cfg.position.corner_radius(rr);
                painter.rect_filled(
                    content,
                    egui::CornerRadius { nw: 0, ne: cr.ne, sw: 0, se: cr.se },
                    egui::Color32::from_rgba_unmultiplied(12, 14, 18, (176.0 * aa) as u8),
                );
                let list_w = (content.width() - 36.0).min(392.0);
                let row_h = 62.0;
                let gap = 8.0;
                let n = vis.len() as f32;
                let total_h = n * row_h + (n - 1.0) * gap;
                let hdr_y = content.center().y - total_h * 0.5 - 24.0;
                painter.text(
                    egui::pos2(content.center().x, hdr_y),
                    egui::Align2::CENTER_CENTER,
                    format!("{} 条提醒待处理 · 逐条处理", n_total),
                    bold_font(12.0),
                    fade(egui::Color32::from_rgb(233, 236, 242)),
                );
                let left = content.center().x - list_w * 0.5;
                let top0 = content.center().y - total_h * 0.5 + 6.0 + (1.0 - le_ease) * 10.0;
                let ptr = ui.ctx().pointer_interact_pos();
                let clicked = ui.input(|i| i.pointer.primary_clicked());
                let mut action: Option<(u64, bool)> = None; // (id, is_done)
                for (k, rid) in vis.iter().copied().enumerate() {
                    let rem = match self.reminders.iter().find(|r| r.id == rid) { Some(r) => r.clone(), None => continue };
                    let icon = rem.icon;
                    let col = remind_color(icon);
                    let row = egui::Rect::from_min_size(
                        egui::pos2(left, top0 + k as f32 * (row_h + gap)),
                        egui::vec2(list_w, row_h),
                    );
                    painter.rect_filled(row, 14.0, rfade(egui::Color32::from_rgb(30, 33, 40)));
                    painter.rect_stroke(row, 14.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], (60.0 * ra) as u8)), egui::StrokeKind::Inside);
                    let ic = egui::pos2(row.left() + 28.0, row.center().y);
                    painter.circle_filled(ic, 17.0, egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], (52.0 * ra) as u8));
                    Self::draw_remind_icon(painter, icon, ic, 19.0, rfade(egui::Color32::from_rgb(col[0], col[1], col[2])), 1.8);
                    let title = if icon == 5 { rem.name.clone() } else { remind_tip(icon).to_string() };
                    let sub = if icon == 5 { format!("自定义 · 每{}分钟", rem.interval_min) } else { remind_desc(&rem) };
                    let tx = row.left() + 52.0;
                    painter.text(egui::pos2(tx, row.center().y - 9.0), egui::Align2::LEFT_CENTER, &title, bold_font(13.0), rfade(egui::Color32::from_rgb(233, 236, 242)));
                    painter.text(egui::pos2(tx, row.center().y + 10.0), egui::Align2::LEFT_CENTER, &sub, egui::FontId::proportional(10.0), rfade(egui::Color32::from_rgb(150, 158, 175)));
                    let done = egui::Rect::from_min_size(egui::pos2(row.right() - 16.0 - 46.0, row.center().y - 12.0), egui::vec2(46.0, 24.0));
                    let snooze = egui::Rect::from_min_size(egui::pos2(done.left() - 8.0 - 54.0, row.center().y - 12.0), egui::vec2(54.0, 24.0));
                    painter.rect_stroke(snooze, 8.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (40.0 * ra) as u8)), egui::StrokeKind::Inside);
                    painter.text(snooze.center(), egui::Align2::CENTER_CENTER, "稍后", bold_font(10.5), rfade(egui::Color32::from_rgb(233, 236, 242)));
                    painter.rect_filled(done, 8.0, egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], (255.0 * ra) as u8));
                    painter.text(done.center(), egui::Align2::CENTER_CENTER, "完成", bold_font(10.5), rfade(egui::Color32::from_rgb(16, 20, 24)));
                    if let Some(p) = ptr {
                        if snooze.contains(p) || done.contains(p) {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if clicked && !self.remind_closing {
                            if snooze.contains(p) { action = Some((rid, false)); }
                            else if done.contains(p) { action = Some((rid, true)); }
                        }
                    }
                }
                if let Some((rid, is_done)) = action {
                    if is_done { self.complete_reminder(rid); } else { self.snooze_reminder(rid, 600); }
                    self.remind_mlist_sig = 0; // 下一帧触发上移渐显
                    self.remind_mlist_anim = 0.0;
                    ui.ctx().request_repaint();
                    return;
                }
                let wait = if le < 1.0 || self.remind_morph < 1.0 { 16 } else { 33 };
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(wait));
                if self.remind_morph >= 0.995 {
                    return;
                }
            }
        }
        let morph = self.remind_morph.clamp(0.0, 1.0);
        if self.remind_mlist_mode && morph >= 0.995 {
            return;
        }
        let aa = if self.remind_mlist_mode { aa * (1.0 - morph) } else { aa };
        if aa <= 0.002 {
            return;
        }
        let ease = aa * aa * (3.0 - 2.0 * aa);
        let fade = |c: egui::Color32| egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), ((c.a() as f32) * aa) as u8);
        let icon = rem.icon;
        let tip = if icon == 5 { rem.name.clone() } else { remind_tip(icon).to_string() };
        let desc = if icon == 5 { format!("自定义提醒 · 每{}分钟", rem.interval_min) } else { remind_desc(&rem) };
        let col = remind_color(icon);
        let cc = egui::Color32::from_rgb(col[0], col[1], col[2]);
        let ink = fade(egui::Color32::from_rgb(233, 236, 242));
        let muted = fade(egui::Color32::from_rgb(150, 158, 175));
        {
            let rr = self.cfg.radius.min(content.height() * 0.5).min(content.width() * 0.5);
            let cr = self.cfg.position.corner_radius(rr);
            painter.rect_filled(
                content,
                egui::CornerRadius { nw: 0, ne: cr.ne, sw: 0, se: cr.se },
                egui::Color32::from_rgba_unmultiplied(12, 14, 18, (168.0 * aa) as u8),
            );
        }
        let cw = (content.width() - 40.0).min(360.0);
        let ch = 172.0;
        let sc = 0.94 + 0.06 * ease;
        let card_ctr = content.center() + egui::vec2(0.0, (1.0 - ease) * 10.0);
        let card = egui::Rect::from_center_size(card_ctr, egui::vec2(cw * sc, ch * sc));
        let phase = if let Some(v) = std::env::var("WINISLE_GLOW_PHASE2").ok().and_then(|s| s.parse::<f32>().ok()) { v.clamp(0.0,1.0) } else { 0.5 + 0.5 * (t * (6.283185307 / 1.1)).sin() as f32 };
        if std::env::var_os("WINISLE_GLOW_DBG").is_some() { eprintln!("[glow] phase={:.3}", phase); }
        let amp = 0.14 + 0.32 * phase;
        // epaint RectShape.blur_width         //  + 
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        // CSS box-shadow ?
        let gwd = card.width() * (940.0 - 2.0 * 26.0) / 718.0;
        let gr = egui::Rect::from_center_size(card.center(), egui::vec2(gwd, gwd * 282.0 / 470.0));
        let pidx = ((phase * 10.0).round() as i32).clamp(0, 10) as usize;
        let gspr = self.remind_glow_phase_sprite(ui.ctx(), icon_hint, pidx);
        painter.add(egui::Shape::image(gspr.id(), gr, uv, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (168.0 * aa).clamp(0.0,255.0) as u8)));
        painter.rect_filled(card, 16.0, fade(egui::Color32::from_rgb(30, 33, 40)));
        let ic = egui::Rect::from_center_size(egui::pos2(card.center().x, card.top() + 42.0), egui::vec2(46.0, 46.0));
        painter.circle_filled(ic.center(), 23.0, egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], (56.0 * aa) as u8));
        Self::draw_remind_icon(painter, icon, ic.center(), 24.0, fade(cc), 2.0);
        painter.text(egui::pos2(card.center().x, card.top() + 84.0), egui::Align2::CENTER_CENTER, &tip, bold_font(19.0), ink);
        painter.text(egui::pos2(card.center().x, card.top() + 108.0), egui::Align2::CENTER_CENTER, &desc, egui::FontId::proportional(11.5), muted);
        let bw = (cw - 40.0 - 12.0) / 2.0;
        let bh = 34.0;
        let by = card.bottom() - 20.0 - bh;
        let snooze = egui::Rect::from_min_size(egui::pos2(card.left() + 20.0, by), egui::vec2(bw, bh));
        let done = egui::Rect::from_min_size(egui::pos2(snooze.right() + 12.0, by), egui::vec2(bw, bh));
        painter.rect_stroke(snooze, 10.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (40.0 * aa) as u8)), egui::StrokeKind::Inside);
        painter.text(snooze.center(), egui::Align2::CENTER_CENTER, "稍后 10 分钟", bold_font(12.0), ink);
        painter.rect_filled(done, 10.0, egui::Color32::from_rgba_unmultiplied(col[0], col[1], col[2], (255.0 * aa) as u8));
        painter.text(done.center(), egui::Align2::CENTER_CENTER, "完成", bold_font(12.0), fade(egui::Color32::from_rgb(16, 20, 24)));
        painter.text(
            egui::pos2(card.center().x, card.bottom() - 10.0),
            egui::Align2::CENTER_CENTER,
            "持续提醒中 · 完成后收起",
            egui::FontId::proportional(9.5),
            muted,
        );
        // + Ui 
        let ptr = ui.ctx().pointer_interact_pos();
        let clicked = ui.input(|i| i.pointer.primary_clicked());
        let over_snooze = ptr.map_or(false, |p| snooze.contains(p));
        let over_done = ptr.map_or(false, |p| done.contains(p));
        if over_snooze || over_done {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if clicked && !self.remind_closing && over_snooze {
            self.snooze_alert(600);
            return;
        }
        if clicked && !self.remind_closing && over_done {
            self.complete_alert();
            return;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn draw_placeholder_page(&self, painter: &egui::Painter, content: egui::Rect, e: f32, title: &str, sub: &str) {
        let a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        painter.text(egui::pos2(content.left() + CONTENT_PAD_X, content.top() + CONTENT_PAD_T), egui::Align2::LEFT_TOP, title, bold_font(17.0), ink);
        painter.text(egui::pos2(content.center().x, content.center().y - 6.0), egui::Align2::CENTER_CENTER, sub, egui::FontId::proportional(13.0), muted);
        painter.text(egui::pos2(content.center().x, content.center().y + 18.0), egui::Align2::CENTER_CENTER, "数?期待", egui::FontId::proportional(11.0), muted2);
    }



    fn draw_weather_page(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, e: f32) {
        let a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);
        let bg_col = egui::Color32::from_rgba_unmultiplied(self.cfg.bg[0], self.cfg.bg[1], self.cfg.bg[2], 255);

        let x0 = rect.left() + CONTENT_PAD_X;
        let x1 = rect.right() - CONTENT_PAD_X;
        let y0 = rect.top() + CONTENT_PAD_T;

        // 
let city = match &self.w_data {
            Some(w) if !w.city.is_empty() => w.city.clone(),
            _ => {
                if !self.cfg.weather_city_name.is_empty() {
                    self.cfg.weather_city_name.clone()
                } else if self.cfg.weather_city.is_empty() {
                    "定位中…".to_string()
                } else {
                    self.cfg.weather_city.clone()
                }
            }
        };
        painter.text(egui::pos2(x0, y0 + 8.0), egui::Align2::LEFT_CENTER, &city, bold_font(16.0), ink);

        //  + 
        let refresh_hit = egui::Rect::from_min_max(egui::pos2(x1 - 80.0, y0 - 3.0), egui::pos2(x1, y0 + 21.0));
        let refresh_resp = ui
            .interact(refresh_hit, ui.id().with("w_refresh"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if let Some(w) = &self.w_data {
            let local = if self.w_fetch_label.is_empty() { ""  } else { self.w_fetch_label.as_str() };
            let src = if w.updated.is_empty() { ""  } else { w.updated.as_str() };
            refresh_resp
                .clone()
                .on_hover_text(format!("上次拉取 {}｜数据更新 {}", local, src));
        }
        {
            let upd = self.w_data.as_ref().map(|w| w.updated.clone()).unwrap_or_default();
            let notice_on = self.w_notice_until.map(|t| t > Instant::now()).unwrap_or(false);
            if self.w_inflight {
                painter.text(
                    egui::pos2(x1 - 24.0, y0 + 8.0),
                    egui::Align2::RIGHT_CENTER,
                    "更新一",
                    egui::FontId::proportional(10.5),
                    accent,
                );
            } else if notice_on {
                let col = if self.w_notice_err {
                    egui::Color32::from_rgba_unmultiplied(242, 110, 92, a)
                } else {
                    accent
                };
                painter.text(
                    egui::pos2(x1 - 24.0, y0 + 8.0),
                    egui::Align2::RIGHT_CENTER,
                    &self.w_notice,
                    egui::FontId::proportional(10.5),
                    col,
                );
                if self.w_notice_err {
                    if let Some(err) = self.w_err.clone() {
                        refresh_resp.clone().on_hover_text(err);
                    }
                }
            } else {
                // ?
                // ?
                let label = if !self.w_fetch_label.is_empty() {
                    self.w_fetch_label.clone()
                } else {
                    upd.clone()
                };
                if !label.is_empty() {
                    let text = if self.w_fetch_label == "缓存" {
                        "缓存数据".to_string()
                    } else {
                        format!("{} 更新", label)
                    };
                    painter.text(
                        egui::pos2(x1 - 24.0, y0 + 8.0),
                        egui::Align2::RIGHT_CENTER,
                        text,
                        egui::FontId::proportional(10.5),
                        muted2,
                    );
                }
            }
            if self.w_inflight {
                // ?
                let c = egui::pos2(x1 - 6.0, y0 + 8.0);
                let r = 5.5;
                let spin_t = ui.input(|i| i.time) as f32;
                let start = spin_t * 5.0;
                let steps = 16;
                let pts: Vec<egui::Pos2> = (0..=steps)
                    .map(|i| {
                        let ang = start + (i as f32 / steps as f32) * std::f32::consts::TAU * 0.72;
                        egui::pos2(c.x + ang.cos() * r, c.y + ang.sin() * r)
                    })
                    .collect();
                painter.add(egui::Shape::line(pts, egui::Stroke::new(1.6, accent)));
            } else {
                painter.text(
                    egui::pos2(x1 - 4.0, y0 + 8.0),
                    egui::Align2::RIGHT_CENTER,
                    "\u{21bb}",
                    egui::FontId::proportional(14.0),
                    muted,
                );
            }
        }


        let Some(w) = self.w_data.clone() else {
            let title = if self.w_err.is_some() { "天气加载失败" } else { "天气加载中…" };
            painter.text(
                egui::pos2(rect.center().x, rect.center().y - 8.0),
                egui::Align2::CENTER_CENTER,
                title,
                egui::FontId::proportional(13.0),
                muted,
            );
            if let Some(err) = &self.w_err {
                let g = ui.fonts(|f| {
                    f.layout(err.clone(), egui::FontId::proportional(10.5), muted2, (x1 - x0).max(120.0))
                });
                let eh = g.size().y;
                painter.galley(
                    egui::pos2(rect.center().x - g.size().x * 0.5, rect.center().y + 10.0),
                    g,
                    muted2,
                );
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y + 14.0 + eh),
                    egui::Align2::CENTER_CENTER,
                    "点击右上「重试。?",
                    egui::FontId::proportional(10.0),
                    muted2,
                );
            }
            if refresh_resp.clicked() {
                self.start_weather_fetch();
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            return;
        };

        let f = self.cfg.weather_unit == 1;
        let unit = weather::temp_unit(f);
        let temp = weather::conv_temp(w.temp, f);
        let (hi, lo) = w.today_high_low();
        let st = platform::local_time();
        let is_night = st.wHour >= 18 || st.wHour < 6;
        let sky = weather::sky_of(&w.today_type, is_night);
        let now_t = ui.input(|i| i.time);


        let hero_top = y0 + 26.0;
        draw_weather_icon(painter, sky, egui::pos2(x0 + 26.0, hero_top + 22.0), 46.0, a, now_t, bg_col);
        painter.text(egui::pos2(x0 + 58.0, hero_top + 12.0), egui::Align2::LEFT_CENTER, format!("{:.0}{}", temp, unit), bold_font(30.0), ink);
        painter.text(egui::pos2(x0 + 60.0, hero_top + 34.0), egui::Align2::LEFT_CENTER, &w.today_type, egui::FontId::proportional(12.5), muted);
        painter.text(
            egui::pos2(x0 + 60.0, hero_top + 51.0),
            egui::Align2::LEFT_CENTER,
            format!("昨{:.0}°  今{:.0}°", weather::conv_temp(hi as f32, f), weather::conv_temp(lo as f32, f)),
            egui::FontId::proportional(11.0),
            muted2,
        );

        {
            let q = if w.quality.is_empty() { "".to_string() } else { w.quality.clone() };
            painter.text(egui::pos2(x1, hero_top - 2.0), egui::Align2::RIGHT_TOP, "空气质量", egui::FontId::proportional(9.5), muted2);
            painter.text(egui::pos2(x1, hero_top + 11.0), egui::Align2::RIGHT_TOP, &q, bold_font(14.0), aqi_color(&w.quality, a));
            let pm = if w.pm25 > 0.0 { format!("PM2.5 {:.0}", w.pm25) } else { "PM2.5 ".to_string() };
            painter.text(egui::pos2(x1, hero_top + 30.0), egui::Align2::RIGHT_TOP, pm, egui::FontId::proportional(10.0), muted2);
            let hum = if w.humidity.is_empty() { "".to_string() } else { w.humidity.clone() };
            painter.text(egui::pos2(x1, hero_top + 44.0), egui::Align2::RIGHT_TOP, format!("湿度 {}", hum), egui::FontId::proportional(10.0), muted2);
        }


        let div_y = hero_top + 63.0;
        painter.line_segment([egui::pos2(x0, div_y), egui::pos2(x1, div_y)], egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (a as f32 * 0.06) as u8)));


        let list_top = div_y + 8.0;
        let list_bottom = rect.bottom() - CONTENT_PAD_B;
        let view_h = (list_bottom - list_top).max(1.0);
        let days = &w.days;
        let nshow = days.len().min(7);
        let row_h = 24.0f32;
        let title_h = 15.0f32;
        let days_h = title_h + nshow as f32 * row_h;
        let hourly_h = if w.hours.is_empty() { 0.0 } else { 62.0 };
        // 
        let ganmao_galley = if w.ganmao.trim().is_empty() {
            None
        } else {
            Some(ui.fonts(|fnt| {
                fnt.layout(w.ganmao.clone(), egui::FontId::proportional(11.0), muted, (x1 - x0).max(1.0))
            }))
        };
        let ganmao_h = ganmao_galley.as_ref().map(|g| g.size().y).unwrap_or(0.0);
        let detail_h = 80.0 + ganmao_h + 4.0;
        let total_h = hourly_h + days_h + 8.0 + detail_h;
        let max_scroll = (total_h - view_h).max(0.0);
        let mut w_scroll_activity = false;
        // WINISLE_W_SCROLL=<px> 
        if let Ok(v) = std::env::var("WINISLE_W_SCROLL") {
            if let Ok(px) = v.parse::<f32>() {
                self.w_scroll_target = px;
                self.w_scroll = px;
                self.w_scroll_act = std::time::Instant::now();
                w_scroll_activity = true;
            }
        }
        // WINISLE_W_SCROLL_ONCE 
        if std::env::var("WINISLE_W_SCROLL_ONCE").is_ok() {
            static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !ONCE.swap(true, std::sync::atomic::Ordering::Relaxed) {
                self.w_scroll_target = 90.0;
                self.w_scroll = 90.0;
                self.w_scroll_act = std::time::Instant::now();
                w_scroll_activity = true;
            }
        }
        if let Some(ph) = ui.ctx().pointer_hover_pos() {
            if rect.contains(ph) && max_scroll > 0.0 {
                let d = ui.input(|i| i.raw_scroll_delta.y);
                if d.abs() > 0.01 {
                    self.w_scroll_target -= d;
                    w_scroll_activity = true;
                }
            }
        }
        self.w_scroll_target = self.w_scroll_target.clamp(0.0, max_scroll);
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        let k = 1.0 - (-dt / 0.07).exp();
        self.w_scroll += (self.w_scroll_target - self.w_scroll) * k;
        self.w_scroll = self.w_scroll.clamp(0.0, max_scroll);
        let w_settling = (self.w_scroll_target - self.w_scroll).abs() > 0.5;
        if (self.w_scroll_target - self.w_scroll).abs() > 0.25 {
            ui.ctx().request_repaint();
        }
        //  /  / 1.2 
if w_scroll_activity {
            self.w_scroll_act = std::time::Instant::now();
        }
        let w_since = self.w_scroll_act.elapsed().as_secs_f32();
        let w_want_vis = if max_scroll > 0.0 && (w_scroll_activity || w_settling || w_since < 1.2) {
            1.0
        } else {
            0.0
        };
        let wkv = 1.0 - (-dt / 0.12).exp();
        self.w_scroll_vis += (w_want_vis - self.w_scroll_vis) * wkv;
        if self.w_scroll_vis < 0.005 {
            self.w_scroll_vis = 0.0;
        } else if self.w_scroll_vis > 0.995 {
            self.w_scroll_vis = 1.0;
        }
        if (self.w_scroll_vis - w_want_vis).abs() > 0.005 {
            ui.ctx().request_repaint();
        }
        if std::env::var("WINISLE_WDBG").is_ok() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("D:\\LingDao\\_wdebug.txt") {
                let _ = writeln!(f, "max={:.1} scroll={:.1} clamp={} want={:.0} vis={:.3} rect=({:.0},{:.0},{:.0},{:.0})", max_scroll, self.w_scroll, self.w_scroll_target, w_want_vis, self.w_scroll_vis, rect.left(), rect.top(), rect.right(), rect.bottom());
            }
        }
        let list_rect = egui::Rect::from_min_max(egui::pos2(rect.left(), list_top), egui::pos2(rect.right(), list_bottom));
        let lp = painter.with_clip_rect(list_rect);
        let p: &egui::Painter = &lp;
        let top = list_top - self.w_scroll;


        if !w.hours.is_empty() {
            p.text(egui::pos2(x0, top + 6.0), egui::Align2::LEFT_CENTER, "过去 24 小时", egui::FontId::proportional(11.0), muted2);
            let hn = w.hours.len().min(8);
            let cw = (x1 - x0) / hn as f32;
            for (i, h) in w.hours.iter().take(hn).enumerate() {
                let cx = x0 + cw * (i as f32 + 0.5);
                p.text(egui::pos2(cx, top + 24.0), egui::Align2::CENTER_CENTER, &h.label, egui::FontId::proportional(9.5), muted2);
                draw_weather_icon(p, h.sky, egui::pos2(cx, top + 38.0), 14.0, a, now_t, bg_col);
                p.text(egui::pos2(cx, top + 51.0), egui::Align2::CENTER_CENTER, format!("{:.0}°", weather::conv_temp(h.temp as f32, f)), egui::FontId::proportional(10.5), ink);
            }
        }
        let base = top + hourly_h;

        p.text(egui::pos2(x0, base + 6.0), egui::Align2::LEFT_CENTER, format!("未来 {} 天", w.days.len()), egui::FontId::proportional(11.0), muted2);


        let mut mn = i32::MAX;
        let mut mx = i32::MIN;
        for d in days.iter().take(nshow) {
            mn = mn.min(d.low);
            mx = mx.max(d.high);
        }
        if nshow == 0 {
            mn = 0;
            mx = 1;
        }
        let span = (mx - mn).max(1) as f32;
        let bar_x0 = x0 + 150.0;
        let bar_x1 = x1 - 96.0;
        for (i, d) in days.iter().take(nshow).enumerate() {
            let ry = base + title_h + i as f32 * row_h + row_h * 0.5;
            if ry < list_top - row_h || ry > list_bottom + row_h {
                continue;
            }
            let label = if i == 0 { "今天".to_string() } else { d.week.replace("星期", "") };
            p.text(egui::pos2(x0, ry), egui::Align2::LEFT_CENTER, &label, egui::FontId::proportional(12.0), if i == 0 { ink } else { muted });
            let mmdd = if d.date.len() >= 10 { d.date[5..].replace('-', "/") } else { d.date.clone() };
            p.text(egui::pos2(x0 + 34.0, ry), egui::Align2::LEFT_CENTER, &mmdd, egui::FontId::proportional(9.5), muted2);
            draw_weather_icon(p, weather::sky_of(&d.day_type, false), egui::pos2(x0 + 94.0, ry), 18.0, a, now_t, bg_col);
            p.text(egui::pos2(x0 + 112.0, ry), egui::Align2::LEFT_CENTER, &d.day_type, egui::FontId::proportional(11.0), muted);

            p.line_segment([egui::pos2(bar_x0, ry), egui::pos2(bar_x1, ry)], egui::Stroke::new(3.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (a as f32 * 0.08) as u8)));
            let t_low = (d.low - mn) as f32 / span;
            let t_high = (d.high - mn) as f32 / span;
            let sx = bar_x0 + (bar_x1 - bar_x0) * t_low;
            let ex = bar_x0 + (bar_x1 - bar_x0) * t_high;
            p.line_segment([egui::pos2(sx, ry), egui::pos2(ex, ry)], egui::Stroke::new(3.0, accent));
            p.text(
                egui::pos2(x1, ry),
                egui::Align2::RIGHT_CENTER,
                format!("{:.0}° / {:.0}°", weather::conv_temp(d.high as f32, f), weather::conv_temp(d.low as f32, f)),
                egui::FontId::proportional(11.5),
                ink,
            );
        }

        // ----  ----
        let dy = base + days_h + 8.0;
        let wind = w.today().map(|d| format!("{} {}", d.wind_dir, d.wind_level)).unwrap_or_default();
        let sun = w.today().map(|d| format!("{} / {}", d.sunrise, d.sunset)).unwrap_or_default();
        let items = [
            ("风向风力", wind.as_str()),
            ("日出 / 日落", sun.as_str()),
            ("空气质量", if w.quality.is_empty() { ""  } else { w.quality.as_str() }),
            ("PM2.5", ""),
        ];
        //  ~14pt

        let col_w = (x1 - x0) / 2.0;
        for (idx, (k2, v2)) in items.iter().enumerate() {
            let cx = x0 + (idx % 2) as f32 * col_w;
            let cy = dy + (idx / 2) as f32 * 34.0;
            p.text(egui::pos2(cx, cy + 4.0), egui::Align2::LEFT_CENTER, *k2, egui::FontId::proportional(10.0), muted2);
            let disp = if *k2 == "PM2.5" {
                if w.pm25 > 0.0 { format!("{:.0}", w.pm25) } else { "".to_string() }
            } else {
                v2.to_string()
            };
            p.text(egui::pos2(cx, cy + 18.0), egui::Align2::LEFT_CENTER, disp, egui::FontId::proportional(12.0), ink);
        }

        let gy = dy + 72.0;
        p.text(egui::pos2(x0, gy), egui::Align2::LEFT_CENTER, "感冒提示", egui::FontId::proportional(10.0), muted2);
        if let Some(g) = &ganmao_galley {
            p.galley(egui::pos2(x0, gy + 8.0), g.clone(), muted);
        }

        //  / 
        if w.from_cache {
            p.text(egui::pos2(x1, list_bottom - 2.0), egui::Align2::RIGHT_BOTTOM, "离线数据", egui::FontId::proportional(9.0), muted2);
        }

        //  1.2  2px 
if max_scroll > 4.0 && self.w_scroll_vis > 0.01 {
            let sv = self.w_scroll_vis;
            let tx = rect.right() - 4.0;
            let track = egui::Rect::from_min_max(egui::pos2(tx, list_top + 2.0), egui::pos2(tx + 2.0, list_bottom - 2.0));
            painter.rect_filled(track, 1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (12.0 * sv) as u8));
            let th = ((view_h / total_h.max(1.0)) * track.height()).max(14.0);
            let range = (track.height() - th).max(1.0);
            let tp = (self.w_scroll / max_scroll.max(1.0)) * range;
            let thumb = egui::Rect::from_min_size(egui::pos2(tx, track.top() + tp), egui::vec2(2.0, th));
            painter.rect_filled(thumb, 1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (60.0 * sv) as u8));
        }

        // 
        if refresh_resp.clicked() {
            self.start_weather_fetch();
        }
        //
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }

    /// header+ +  tab/  + 
    ///  1:1  todo-prototype.html?
    fn draw_todo_page(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, e: f32) {
        // ---- ?----
        fn text_w(ui: &egui::Ui, s: &str, font: egui::FontId) -> f32 {
            ui.fonts(|f| f.layout_no_wrap(s.to_string(), font, egui::Color32::WHITE).size().x)
        }
        fn fit_text(ui: &egui::Ui, s: &str, font: egui::FontId, max: f32) -> String {
            if text_w(ui, s, font.clone()) <= max {
                return s.to_string();
            }
            let mut out = String::new();
            for ch in s.chars() {
                let mut t = out.clone();
                t.push(ch);
                if text_w(ui, &t, font.clone()) > (max - 8.0).max(1.0) {
                    break;
                }
                out.push(ch);
            }
            out.push('…');
            out
        }
        fn draw_check(p: &egui::Painter, c: egui::Pos2, size: f32, col: egui::Color32) {
            let f = size / 16.0;
            let pts = vec![
                egui::pos2(c.x - 2.8 * f, c.y + 0.3 * f),
                egui::pos2(c.x - 0.8 * f, c.y + 2.2 * f),
                egui::pos2(c.x + 2.9 * f, c.y - 2.2 * f),
            ];
            p.add(egui::Shape::line(pts, egui::Stroke::new(1.8 * f, col)));
        }
        fn draw_box(p: &egui::Painter, c: egui::Pos2, size: f32, col: egui::Color32) {
            let f = size / 24.0;
            let st = egui::Stroke::new(1.4 * f, col);
            let l = c.x - 6.0 * f;
            let r = c.x + 6.0 * f;
            let t = c.y - 3.0 * f;
            let b = c.y + 7.0 * f;
            // 
            p.add(egui::Shape::line(
                vec![egui::pos2(l, t), egui::pos2(l, b), egui::pos2(r, b), egui::pos2(r, t)],
                st,
            ));
            // 
            p.add(egui::Shape::line(
                vec![egui::pos2(l - 1.0 * f, t), egui::pos2(r + 1.0 * f, t)],
                st,
            ));
            // 
            p.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - 2.0 * f, t),
                    egui::pos2(c.x - 2.0 * f, t - 2.0 * f),
                    egui::pos2(c.x + 2.0 * f, t - 2.0 * f),
                    egui::pos2(c.x + 2.0 * f, t),
                ],
                st,
            ));
        }
        fn draw_trash(p: &egui::Painter, c: egui::Pos2, size: f32, col: egui::Color32) {
            let f = size / 24.0;
            let st = egui::Stroke::new(1.4 * f, col);
            let l = c.x - 5.0 * f;
            let r = c.x + 5.0 * f;
            let t = c.y - 2.0 * f;
            let b = c.y + 7.5 * f;
            // 
            p.add(egui::Shape::line(
                vec![egui::pos2(l, t), egui::pos2(l + 1.0 * f, b), egui::pos2(r - 1.0 * f, b), egui::pos2(r, t)],
                st,
            ));
            // 
            p.add(egui::Shape::line(
                vec![egui::pos2(l - 1.8 * f, t), egui::pos2(r + 1.8 * f, t)],
                st,
            ));
            // 
            p.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - 2.4 * f, t),
                    egui::pos2(c.x - 2.4 * f, t - 2.2 * f),
                    egui::pos2(c.x + 2.4 * f, t - 2.2 * f),
                    egui::pos2(c.x + 2.4 * f, t),
                ],
                st,
            ));
            // 
            p.add(egui::Shape::line(
                vec![egui::pos2(c.x - 1.6 * f, t + 2.2 * f), egui::pos2(c.x - 1.6 * f, b - 2.2 * f)],
                st,
            ));
            p.add(egui::Shape::line(
                vec![egui::pos2(c.x + 1.6 * f, t + 2.2 * f), egui::pos2(c.x + 1.6 * f, b - 2.2 * f)],
                st,
            ));
        }
        fn draw_chev(p: &egui::Painter, c: egui::Pos2, size: f32, col: egui::Color32) {
            let h = size * 0.28;
            p.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - h, c.y - h * 0.5),
                    egui::pos2(c.x, c.y + h * 0.6),
                    egui::pos2(c.x + h, c.y - h * 0.5),
                ],
                egui::Stroke::new(1.5, col),
            ));
        }
        fn draw_plus(p: &egui::Painter, c: egui::Pos2, size: f32, col: egui::Color32) {
            let h = size * 0.5;
            p.add(egui::Shape::line(
                vec![egui::pos2(c.x - h, c.y), egui::pos2(c.x + h, c.y)],
                egui::Stroke::new(1.5, col),
            ));
            p.add(egui::Shape::line(
                vec![egui::pos2(c.x, c.y - h), egui::pos2(c.x, c.y + h)],
                egui::Stroke::new(1.5, col),
            ));
        }

        let a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as u8;
        let wa = |al: f32| -> u8 { (a as f32 * al).clamp(0.0, 255.0) as u8 };
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);
        let danger = egui::Color32::from_rgba_unmultiplied(244, 114, 114, a);
        let white = |al: f32| egui::Color32::from_rgba_unmultiplied(255, 255, 255, wa(al));
        let mint = |al: f32| egui::Color32::from_rgba_unmultiplied(52, 211, 153, wa(al));
        let menu_bg = egui::Color32::from_rgba_unmultiplied(31, 33, 40, a);

        // ----  ----
        let todo_count = self.todos.iter().filter(|t| !t.archived && !t.done).count();
        let done_count = self.todos.iter().filter(|t| !t.archived && t.done).count();
        let total = todo_count + done_count;
        let arch_total = self.todos.iter().filter(|t| t.archived).count();
        let done_frac = if total > 0 { done_count as f32 / total as f32 } else { 0.0 };
        let done_anim = ui
            .ctx()
            .animate_value_with_time(ui.id().with("todo_prog"), done_frac, 0.35);
        if (done_anim - done_frac).abs() > 0.001 {
            ui.ctx().request_repaint();
        }

        // ab / 
        let prev_tab = self.todo_tab;
        let prev_filter = self.todo_filter;

        let x0 = rect.left() + CONTENT_PAD_X;
        let x1 = rect.right() - CONTENT_PAD_X;
        let y0 = rect.top() + CONTENT_PAD_T;

        // ---- header ----
        let header_h = 26.0;
        let title_lbl = "待办";
        painter.text(
            egui::pos2(x0, y0 + header_h * 0.5),
            egui::Align2::LEFT_CENTER,
            title_lbl,
            bold_font(17.0),
            ink,
        );
        {
            // 
            let cy = y0 + header_h * 0.5;
            let title_w = text_w(ui, title_lbl, bold_font(17.0));
            let frac_txt = format!("{} / {}", done_count, total);
            let frac_font = bold_font(10.5);
            let fw = text_w(ui, &frac_txt, frac_font.clone());
            painter.text(
                egui::pos2(x1, cy),
                egui::Align2::RIGHT_CENTER,
                &frac_txt,
                frac_font,
                muted,
            );
            let bx0 = x0 + title_w + 16.0;
            let bx1 = x1 - fw - 12.0;
            let bh = 5.0;
            if bx1 > bx0 + 8.0 {
                let track = egui::Rect::from_min_max(
                    egui::pos2(bx0, cy - bh * 0.5),
                    egui::pos2(bx1, cy + bh * 0.5),
                );
                painter.rect_filled(track, bh * 0.5, white(0.10));
                if done_anim > 0.001 {
                    let w = ((bx1 - bx0) * done_anim).clamp(bh, bx1 - bx0);
                    let fill = egui::Rect::from_min_max(
                        egui::pos2(bx0, cy - bh * 0.5),
                        egui::pos2(bx0 + w, cy + bh * 0.5),
                    );
                    painter.rect_filled(fill, bh * 0.5, accent);
                }
            }
        }

        // ---- tabs / / ----
        let tab_font = bold_font(11.5);
        let badge_font = bold_font(9.5);
        let pad_h = 11.0f32;
        let tab_h = 22.0f32;
        let tabs_y = y0 + header_h + 9.0;

        let lerp_c = |c1: egui::Color32, c2: egui::Color32, t: f32| -> egui::Color32 {
            let t = t.clamp(0.0, 1.0);
            let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
            egui::Color32::from_rgba_unmultiplied(
                m(c1.r(), c2.r()),
                m(c1.g(), c2.g()),
                m(c1.b(), c2.b()),
                m(c1.a(), c2.a()),
            )
        };

        let tab_w = |label: &str, count: usize| -> f32 {
            pad_h + text_w(ui, label, tab_font.clone()) + 6.0
                + (text_w(ui, &count.to_string(), badge_font.clone()) + 10.0).max(15.0)
                + pad_h
        };
        let w_all = tab_w("全部", total);
        let w_inc = tab_w("未完成", todo_count);
        let w_done = tab_w("已完成", done_count);
        let w_arch = tab_w("归档", arch_total);
        let r_all = egui::Rect::from_min_size(egui::pos2(x0, tabs_y), egui::vec2(w_all, tab_h));
        let r_inc = egui::Rect::from_min_size(
            egui::pos2(r_all.right() + 8.0, tabs_y),
            egui::vec2(w_inc, tab_h),
        );
        let r_done = egui::Rect::from_min_size(
            egui::pos2(r_inc.right() + 8.0, tabs_y),
            egui::vec2(w_done, tab_h),
        );
        let r_arch = egui::Rect::from_min_size(
            egui::pos2(r_done.right() + 8.0, tabs_y),
            egui::vec2(w_arch, tab_h),
        );

        let all_act = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("todo_tab_all"), self.todo_tab == 0 && self.todo_filter == 0, 0.18);
        let inc_act = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("todo_tab_inc"), self.todo_tab == 0 && self.todo_filter == 1, 0.18);
        let done_act = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("todo_tab_done"), self.todo_tab == 0 && self.todo_filter == 2, 0.18);
        let arch_act = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("todo_tab_arch"), self.todo_tab == 1, 0.18);
        if all_act < 0.999 || inc_act < 0.999 || done_act < 0.999 || arch_act < 0.999 {
            ui.ctx().request_repaint();
        }

        let draw_tab = |p: &egui::Painter,
                        r: egui::Rect,
                        act: f32,
                        label: &str,
                        count: usize| {
            let act = act.clamp(0.0, 1.0);
            if act > 0.001 {
                // 
                p.rect_stroke(r, tab_h * 0.5, egui::Stroke::new(1.0, mint(0.26 * act)), egui::StrokeKind::Inside);
            }
            let tcol = lerp_c(muted, accent, act);
            let cy = r.center().y;
            let mut cx = r.left() + pad_h;
            p.text(egui::pos2(cx, cy), egui::Align2::LEFT_CENTER, label, tab_font.clone(), tcol);
            cx += text_w(ui, label, tab_font.clone()) + 6.0;
            let bw = (text_w(ui, &count.to_string(), badge_font.clone()) + 10.0).max(15.0);
            let br = egui::Rect::from_min_size(egui::pos2(cx, cy - 7.0), egui::vec2(bw, 14.0));
            p.text(
                br.center(),
                egui::Align2::CENTER_CENTER,
                &count.to_string(),
                badge_font.clone(),
                lerp_c(muted2, accent, act),
            );
        };

        draw_tab(painter, r_all, all_act, "全部", total);
        draw_tab(painter, r_inc, inc_act, "未完成", todo_count);
        draw_tab(painter, r_done, done_act, "已完成", done_count);
        draw_tab(painter, r_arch, arch_act, "归档", arch_total);

        let all_resp = ui
            .interact(r_all, ui.id().with("todo_tab_all_btn"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if all_resp.clicked() {
            if self.todo_tab != 0 || self.todo_filter != 0 {
                self.todo_tab = 0;
                self.todo_filter = 0;
                self.todo_scroll = 0.0;
                self.todo_scroll_target = 0.0;
            }
            self.todo_menu_open = false;
        }
        let inc_resp = ui
            .interact(r_inc, ui.id().with("todo_tab_inc_btn"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if inc_resp.clicked() {
            if self.todo_tab != 0 || self.todo_filter != 1 {
                self.todo_tab = 0;
                self.todo_filter = 1;
                self.todo_scroll = 0.0;
                self.todo_scroll_target = 0.0;
            }
            self.todo_menu_open = false;
        }
        let done_resp = ui
            .interact(r_done, ui.id().with("todo_tab_done_btn"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if done_resp.clicked() {
            if self.todo_tab != 0 || self.todo_filter != 2 {
                self.todo_tab = 0;
                self.todo_filter = 2;
                self.todo_scroll = 0.0;
                self.todo_scroll_target = 0.0;
            }
            self.todo_menu_open = false;
        }
        let arch_resp = ui
            .interact(r_arch, ui.id().with("todo_tab_arch_btn"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if arch_resp.clicked() {
            if self.todo_tab != 1 {
                self.todo_tab = 1;
                self.todo_scroll = 0.0;
                self.todo_scroll_target = 0.0;
            }
            self.todo_menu_open = false;
        }

        let tabs_bottom = tabs_y + tab_h;

        // tab / + 
        if self.todo_tab != prev_tab || self.todo_filter != prev_filter {
            self.todo_anim = 0.0;
        }
        let adt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        if self.todo_anim < 1.0 {
            self.todo_anim += (1.0 - self.todo_anim) * (1.0 - (-adt / 0.09).exp());
            if self.todo_anim > 0.998 {
                self.todo_anim = 1.0;
            }
            ui.ctx().request_repaint();
        }
        // 
        if let Ok(v) = std::env::var("WINISLE_TODO_ANIM_HOLD") {
            if let Ok(f) = v.parse::<f32>() {
                self.todo_anim = f.clamp(0.0, 1.0);
            }
        }
        let content_ease = 1.0 - (1.0 - self.todo_anim.clamp(0.0, 1.0)).powi(3);
        let content_dy = (1.0 - content_ease) * 7.0;
        let content_op = content_ease;

        // ---- ?----
        enum Tr {
            G(String, usize),
            T(Todo),
            A(Todo),
        }
        let mut rows: Vec<Tr> = Vec::new();
        if self.todo_tab == 0 {
            if self.todo_filter == 0 || self.todo_filter == 1 {
                let mut inc: Vec<Todo> =
                    self.todos.iter().filter(|t| !t.archived && !t.done).cloned().collect();
                inc.sort_by_key(|t| t.id);
                rows.push(Tr::G("未完成".to_string(), inc.len()));
                for t in inc {
                    rows.push(Tr::T(t));
                }
            }
            if self.todo_filter == 0 || self.todo_filter == 2 {
                let mut cmp: Vec<Todo> =
                    self.todos.iter().filter(|t| !t.archived && t.done).cloned().collect();
                cmp.sort_by(|a, b| b.done_seq.cmp(&a.done_seq));
                rows.push(Tr::G("已完成".to_string(), cmp.len()));
                for t in cmp {
                    rows.push(Tr::T(t));
                }
            }
        } else {
            let mut arch: Vec<Todo> = self.todos.iter().filter(|t| t.archived).cloned().collect();
            arch.sort_by(|a, b| b.created.cmp(&a.created).then(a.id.cmp(&b.id)));
            let mut i = 0;
            while i < arch.len() {
                let d = arch[i].created.clone();
                let mut j = i;
                while j < arch.len() && arch[j].created == d {
                    j += 1;
                }
                rows.push(Tr::G(format!("创建{}", fmt_cn_date(&d)), j - i));
                for k in i..j {
                    rows.push(Tr::A(arch[k].clone()));
                }
                i = j;
            }
        }
        // 
        let item_rows = rows.iter().filter(|r| !matches!(r, Tr::G(_, _))).count();
        let empty = item_rows == 0;
        if empty {
            rows.clear();
        }

        // ---- ?----
        let show_add = self.todo_tab == 0 && self.todo_filter != 2;
        // 
        let reserve_bottom = show_add;
        let add_h = 28.0f32;
        let add_gap = 7.0f32;
        let addrow_bottom = rect.bottom() - CONTENT_PAD_B;
        let addrow_top = addrow_bottom - add_h;
        let list_top = tabs_bottom + 9.0;
        let list_bottom = if reserve_bottom { addrow_top - add_gap } else { rect.bottom() - CONTENT_PAD_B };
        let list_bottom = list_bottom.max(list_top + 1.0);
        let view_h = (list_bottom - list_top).max(1.0);

        // ?
        let mut total_h = 0.0f32;
        let mut gi = 0;
        for row in &rows {
            match row {
                Tr::G(_, _) => {
                    if gi > 0 {
                        total_h += 12.0;
                    }
                    total_h += 16.0;
                    gi += 1;
                }
                _ => total_h += 26.0,
            }
        }
        let max_scroll = (total_h - view_h).max(0.0);

        //  + 
        let mut scroll_activity = false;
        if let Some(pt) = ui.ctx().pointer_hover_pos() {
            if rect.contains(pt) && max_scroll > 0.0 {
                let d = ui.input(|i| i.raw_scroll_delta.y);
                if d.abs() > 0.01 {
                    self.todo_scroll_target -= d;
                    scroll_activity = true;
                }
            }
        }
        self.todo_scroll_target = self.todo_scroll_target.clamp(0.0, max_scroll);
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        let k = 1.0 - (-dt / 0.07).exp();
        self.todo_scroll += (self.todo_scroll_target - self.todo_scroll) * k;
        self.todo_scroll = self.todo_scroll.clamp(0.0, max_scroll);
        let settling = (self.todo_scroll_target - self.todo_scroll).abs() > 0.5;
        if (self.todo_scroll_target - self.todo_scroll).abs() > 0.25 {
            ui.ctx().request_repaint();
        }
        if scroll_activity {
            self.todo_scroll_act = std::time::Instant::now();
        }
        let since = self.todo_scroll_act.elapsed().as_secs_f32();
        let want_vis = if max_scroll > 0.0 && (scroll_activity || settling || since < 1.2) { 1.0 } else { 0.0 };
        let kv = 1.0 - (-dt / 0.12).exp();
        self.todo_scroll_vis += (want_vis - self.todo_scroll_vis) * kv;
        if self.todo_scroll_vis < 0.005 {
            self.todo_scroll_vis = 0.0;
        } else if self.todo_scroll_vis > 0.995 {
            self.todo_scroll_vis = 1.0;
        }
        if (self.todo_scroll_vis - want_vis).abs() > 0.005 {
            ui.ctx().request_repaint();
        }

        let list_rect = egui::Rect::from_min_max(egui::pos2(rect.left(), list_top), egui::pos2(rect.right(), list_bottom));
        let mut lp = painter.with_clip_rect(list_rect);
        lp.multiply_opacity(content_op.clamp(0.0, 1.0));
        let p: &egui::Painter = &lp;
        let mut y = list_top - self.todo_scroll + content_dy;

        let task_font = bold_font(12.5);
        let glabel_font = bold_font(10.5);
        let act_font = bold_font(9.5);
        let arch_title_font = bold_font(12.0);

        // 
        let todos = &self.todos;
        let mut toggle_id: Option<u64> = None;
        let mut archive_id: Option<u64> = None;
        let mut restore_id: Option<u64> = None;
        let mut delete_id: Option<u64> = None;
        let interactive = true;
        // ??
        let del_w = 20.0;

        let mut first = true;
        for row in &rows {
            match row {
                Tr::G(label, n) => {
                    if !first {
                        y += 12.0;
                    }
                    first = false;
                    if y >= list_top - 20.0 && y <= list_bottom + 20.0 {
                        p.text(egui::pos2(x0, y + 8.0), egui::Align2::LEFT_CENTER, label, glabel_font.clone(), muted2);
                        let lw = text_w(ui, label, glabel_font.clone());
                        p.text(
                            egui::pos2(x0 + lw + 6.0, y + 8.0),
                            egui::Align2::LEFT_CENTER,
                            format!("· {}", n),
                            bold_font(9.5),
                            egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.7)),
                        );
                    }
                    y += 16.0;
                }
                Tr::T(t) => {
                    let row_rect = egui::Rect::from_min_size(egui::pos2(x0, y), egui::vec2(x1 - x0, 26.0));
                    let ry = y + 13.0;
                    let visible = row_rect.center().y >= list_top - 13.0 && row_rect.center().y <= list_bottom + 13.0;
                    if visible {
                        // checkbox
                        let cb_c = egui::pos2(x0 + 8.0, ry);
                        if t.done {
                            p.circle_filled(cb_c, 7.0, accent);
                            draw_check(p, cb_c, 16.0, egui::Color32::from_rgba_unmultiplied(12, 42, 34, a));
                        } else {
                            p.circle_stroke(cb_c, 6.6, egui::Stroke::new(1.5, muted2));
                        }
                        // 
                        let title_x = x0 + 25.0;
                        let btn_area = if t.done {
                            let bw = 6.0 + 12.0 + 4.0 + text_w(ui, "归档", act_font.clone()) + 6.0;
                            bw
                        } else {
                            0.0
                        };
                        let avail = (x1 - del_w - btn_area - 8.0) - title_x;
                        let shown = fit_text(ui, &t.title, task_font.clone(), avail);
                        let tcol = if t.done { muted2 } else { ink };
                        p.text(egui::pos2(title_x, ry), egui::Align2::LEFT_CENTER, &shown, task_font.clone(), tcol);
                        if t.done {
                            // ?
                            let tw = text_w(ui, &shown, task_font.clone());
                            p.line_segment(
                                [egui::pos2(title_x, ry), egui::pos2(title_x + tw.min(avail), ry)],
                                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.8))),
                            );
                            //  muted2 + 
                            // ?
                            let bw = btn_area;
                            let btn = egui::Rect::from_min_size(
                                egui::pos2(x1 - del_w - bw, ry - 10.0),
                                egui::vec2(bw, 20.0),
                            );
                            let hov = interactive
                                && visible
                                && (ui.rect_contains_pointer(btn)
                                    || std::env::var_os("WINISLE_TODO_ACT_HOVER").is_some());
                            let acol = if hov { accent } else { muted2 };
                            draw_box(p, egui::pos2(btn.left() + 6.0 + 6.0, ry), 12.0, acol);
                            p.text(
                                egui::pos2(btn.left() + 6.0 + 12.0 + 4.0, ry),
                                egui::Align2::LEFT_CENTER,
                                "归档",
                                act_font.clone(),
                                acol,
                            );
                            if interactive && visible {
                                let br = ui
                                    .interact(btn, ui.id().with(("todo_arch", t.id)), egui::Sense::click())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if br.clicked() {
                                    archive_id = Some(t.id);
                                }
                            }
                        }
                        // ?
                        let del_btn = egui::Rect::from_min_size(
                            egui::pos2(x1 - del_w, ry - 10.0),
                            egui::vec2(del_w, 20.0),
                        );
                        let row_hover = interactive && visible && ui.rect_contains_pointer(row_rect);
                        let dhov = interactive
                            && visible
                            && (ui.rect_contains_pointer(del_btn)
                                || std::env::var_os("WINISLE_TODO_DEL_HOVER").is_some());
                        let dcol = if dhov {
                            danger
                        } else if row_hover {
                            muted
                        } else {
                            egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.65))
                        };
                        draw_trash(p, del_btn.center(), 13.0, dcol);
                        if interactive && visible {
                            let dr = ui
                                .interact(del_btn, ui.id().with(("todo_del", t.id)), egui::Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if dr.clicked() {
                                delete_id = Some(t.id);
                            }
                        }
                        // checkbox ?16px ?
                        if interactive && visible {
                            let cb_hit = egui::Rect::from_min_size(egui::pos2(x0, ry - 10.0), egui::vec2(22.0, 20.0));
                            let cr = ui
                                .interact(cb_hit, ui.id().with(("todo_cb", t.id)), egui::Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if cr.clicked() {
                                toggle_id = Some(t.id);
                            }
                        }
                    }
                    y += 26.0;
                }
                Tr::A(t) => {
                    let ry = y + 13.0;
                    let visible = ry >= list_top - 13.0 && ry <= list_bottom + 13.0;
                    if visible {
                        draw_box(p, egui::pos2(x0 + 7.0, ry), 14.0, muted2);
                        let rw = text_w(ui, "恢在", act_font.clone());
                        let bw = 10.0 + rw + 10.0;
                        let btn = egui::Rect::from_min_size(egui::pos2(x1 - del_w - bw, ry - 10.0), egui::vec2(bw, 20.0));
                        let avail = (btn.left() - 8.0) - (x0 + 24.0);
                        let shown = fit_text(ui, &t.title, arch_title_font.clone(), avail);
                        p.text(egui::pos2(x0 + 24.0, ry), egui::Align2::LEFT_CENTER, &shown, arch_title_font.clone(), muted);
                        // 
                        let rhov = interactive
                            && visible
                            && (ui.rect_contains_pointer(btn)
                                || std::env::var_os("WINISLE_TODO_ACT_HOVER").is_some());
                        let rcol = if rhov { accent } else { muted2 };
                        p.text(btn.center(), egui::Align2::CENTER_CENTER, "恢在", act_font.clone(), rcol);
                        if interactive && visible {
                            let br = ui
                                .interact(btn, ui.id().with(("todo_restore", t.id)), egui::Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if br.clicked() {
                                restore_id = Some(t.id);
                            }
                        }
                        // 
                        let arow_rect = egui::Rect::from_min_size(egui::pos2(x0, y), egui::vec2(x1 - x0, 26.0));
                        let del_btn = egui::Rect::from_min_size(
                            egui::pos2(x1 - del_w, ry - 10.0),
                            egui::vec2(del_w, 20.0),
                        );
                        let row_hover = interactive && visible && ui.rect_contains_pointer(arow_rect);
                        let dhov = interactive
                            && visible
                            && (ui.rect_contains_pointer(del_btn)
                                || std::env::var_os("WINISLE_TODO_DEL_HOVER").is_some());
                        let dcol = if dhov {
                            danger
                        } else if row_hover {
                            muted
                        } else {
                            egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.65))
                        };
                        draw_trash(p, del_btn.center(), 13.0, dcol);
                        if interactive && visible {
                            let dr = ui
                                .interact(del_btn, ui.id().with(("todo_del", t.id)), egui::Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if dr.clicked() {
                                delete_id = Some(t.id);
                            }
                        }
                    }
                    y += 26.0;
                }
            }
        }

        // ---- ?----
        if show_add {
            let add_rect = egui::Rect::from_min_max(
                egui::pos2(x0, addrow_top),
                egui::pos2(x1, addrow_bottom),
            );
            // ?
            painter.rect_stroke(add_rect, 9.0, egui::Stroke::new(1.2, mint(0.30)), egui::StrokeKind::Inside);
            // 
            //  egui 
            // 
            if let Some(p) = ui.ctx().pointer_hover_pos() {
                self.todo_addrow_hover = add_rect.contains(p);
            }
            if self.todo_addrow_hover {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            // 
            //  WM_MOUSEACTIVATE ?MA_NOACTIVATE ?
            if !self.todo_editing && !self.todo_debug_edit && !self.w_city_search {
                // WINISLE_TODO_HOVERTEST ??
                //   WS_EX_NOACTIVATE
                let hover_add = std::env::var_os("WINISLE_TODO_HOVERTEST").is_some()
                    || ui
                        .ctx()
                        .pointer_hover_pos()
                        .map(|p| add_rect.contains(p))
                        .unwrap_or(false);
                platform::set_activatable_soft(self.hwnd, hover_add);
            }
            if self.todo_editing || self.todo_debug_edit {
                // TextEdit ***addrow_top + add_h/2
                // LEFT_CENTER
                // TextEdit  =  - margin(4,2) row_h ?
                //  =  - row_h/2 - margin_y = row_h + 2*margin_y?
                let row_h = ui.fonts(|f| f.row_height(&egui::FontId::proportional(12.0)));
                let m_y = 2.0f32;
                let mid_y = addrow_top + add_h * 0.5;
                let text_rect = egui::Rect::from_min_max(
                    egui::pos2(x0 + 24.0, mid_y - row_h * 0.5 - m_y),
                    egui::pos2(x1 - 66.0, mid_y + row_h * 0.5 + m_y),
                );
                let mut txt = std::mem::take(&mut self.todo_input);
                // */
                //  TextEdit hint_text  margin//DPI 
                // TextEdit 
                if txt.is_empty() {
                    painter.text(
                        egui::pos2(x0 + 28.0, addrow_top + add_h * 0.5),
                        egui::Align2::LEFT_CENTER,
                        "添加待办…",
                        egui::FontId::proportional(11.5),
                        muted2,
                    );
                }
                let inp_id = egui::Id::new("winisle_todo_input");
                // 
                if std::env::var_os("WINISLE_TODO_SELALL").is_some() && !txt.is_empty() {
                    let n = txt.chars().count();
                    if let Some(mut st) = egui::text_edit::TextEditState::load(ui.ctx(), inp_id) {
                        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(n),
                        )));
                        st.store(ui.ctx(), inp_id);
                    }
                }
                let te = egui::TextEdit::singleline(&mut txt)
                    .frame(false)
                    .id(inp_id)
                    .font(egui::FontId::proportional(12.0))
                    .text_color(ink);
                // egui viewport focused SetFocus/?
                //  winit viewport  focused
                //  egui ?viewport focused  egui ?
                //  + ?
                let vp_focused_before = ui.ctx().input(|i| i.focused);
                let mem_foc0 = ui.ctx().memory(|m| m.has_focus(inp_id));
                let force_caret = !vp_focused_before && mem_foc0;
                if force_caret {
                    ui.ctx().input_mut(|i| i.focused = true);
                }
                let _tresp = ui
                    .scope(|ui| {
                        let v = ui.visuals_mut();
                        v.extreme_bg_color = egui::Color32::TRANSPARENT;
                        v.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                        v.widgets.hovered.bg_fill = egui::Color32::TRANSPARENT;
                        v.widgets.active.bg_fill = egui::Color32::TRANSPARENT;
                        // hintmuted2
                        // egui hint weak_text_color()=gray_out(text_color())                        // override_text_color ?ink muted2?
                        v.override_text_color = None;
                        v.widgets.noninteractive.fg_stroke.color = muted2;
                        v.widgets.noninteractive.weak_bg_fill = muted2;
                        //                         v.text_cursor.stroke.color = accent;
                        v.text_cursor.stroke.width = 1.5;
                        ui.put(text_rect, te)
                    })
                    .inner;
                if force_caret {
                    let restore = vp_focused_before;
                    ui.ctx().input_mut(|i| i.focused = restore);
                }
                self.todo_input = txt;
                // egui mem.has_focus
                // Response::has_focus() OS false
                // / egui 
                let mem_focused = ui.ctx().memory(|m| m.has_focus(inp_id));
                //  egui 
                if std::env::var_os("WINISLE_TODO_MEMFOCUS").is_some() && !mem_focused {
                    ui.ctx().memory_mut(|m| m.request_focus(inp_id));
                }
                if self.todo_need_focus {
                    if !mem_focused {
                        ui.ctx().memory_mut(|m| m.request_focus(inp_id));
                        // ?OS ?egui
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.todo_focus_tries = self.todo_focus_tries.saturating_sub(1);
                        if self.todo_focus_tries == 0 {
                            self.todo_need_focus = false;
                        }
                    } else {
                        self.todo_need_focus = false;
                    }
                }
                if mem_focused {
                    self.todo_was_focused = true;
                }
                draw_plus(painter, egui::pos2(x0 + 14.0, addrow_top + add_h * 0.5), 10.5, muted2);
                // 
                painter.text(
                    egui::pos2(x1 - 12.0, addrow_top + add_h * 0.5),
                    egui::Align2::RIGHT_CENTER,
                    "回车保存",
                    egui::FontId::proportional(9.5),
                    egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.75)),
                );
                //  egui 
                // 
                let mem_focused_now = ui.ctx().memory(|m| m.has_focus(inp_id));
                if self.todo_was_focused && !mem_focused_now {
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter))
                        || std::env::var_os("WINISLE_TODO_FAKE_ENTER").is_some();
                    if enter {
                        let txt = self.todo_input.trim().to_string();
                        if !txt.is_empty() {
                            self.todo_next_id += 1;
                            self.todos.push(Todo {
                                id: self.todo_next_id,
                                title: txt,
                                done: false,
                                archived: false,
                                created: today_str(),
                                done_seq: 0,
                            });
                            save_todos(&self.todos);
                            self.todo_scroll_target = 1.0e7;
                        }
                        // *
                        // 
                        self.todo_input.clear();
                        self.todo_editing = true;
                        self.todo_need_focus = true;
                        self.todo_focus_tries = 30;
                        self.todo_was_focused = false;
                        //  gui
                        ui.ctx().memory_mut(|m| m.request_focus(inp_id));
                    } else {
                        // ?/ Esc?
                        self.todo_input.clear();
                        self.todo_editing = false;
                        self.todo_need_focus = false;
                        self.todo_was_focused = false;
                    }
                }
            } else {
                draw_plus(painter, egui::pos2(x0 + 14.0, addrow_top + add_h * 0.5), 10.5, muted2);
                painter.text(
                    egui::pos2(x0 + 28.0, addrow_top + add_h * 0.5),
                    egui::Align2::LEFT_CENTER,
                    "添加待办…",
                    egui::FontId::proportional(11.5),
                    muted2,
                );
                painter.text(
                    egui::pos2(x1 - 12.0, addrow_top + add_h * 0.5),
                    egui::Align2::RIGHT_CENTER,
                    "回车保存",
                    egui::FontId::proportional(9.5),
                    egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.75)),
                );
                let ar = ui
                    .interact(add_rect, ui.id().with("todo_addrow"), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::Text);
                if ar.clicked() {
                    self.todo_editing = true;
                    self.todo_input.clear();
                    self.todo_need_focus = true;
                    self.todo_focus_tries = 30;
                    self.todo_was_focused = false;
                    // egui
                    //  want_activatable 
                    // soft/
                    if !platform::is_activatable() {
                        platform::set_activatable(self.hwnd, true);
                    }
                    // update() want_activatable                     // set_activatable/ViewportCommand::Focus
                    self.w_activatable_last = true;
                }
            }
        }

        // ---- ----
        if empty {
            let cy = (list_top + list_bottom) * 0.5 - 10.0;
            let cx = rect.center().x;
            // ?+ 
            let ic = egui::pos2(cx, cy - 20.0);
            p.circle_stroke(ic, 15.0, egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.65))));
            draw_check(p, ic, 26.0, egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.6)));
            p.text(
                egui::pos2(cx, cy + 8.0),
                egui::Align2::CENTER_CENTER,
                if self.todo_tab == 1 { "归档为空" } else { "暂无待办" },
                bold_font(12.5),
                muted,
            );
            p.text(
                egui::pos2(cx, cy + 26.0),
                egui::Align2::CENTER_CENTER,
                if self.todo_tab == 1 {
                    "已完成的任务收起在这里归档"
                } else if self.todo_filter == 2 {
                    "完成的任务会显示在这" 
                } else {
                    "在最下方输入框添加待" 
                },
                egui::FontId::proportional(10.5),
                egui::Color32::from_rgba_unmultiplied(107, 113, 128, wa(0.7)),
            );
        }

        // ---- ?2px 1.2s ?---
        if max_scroll > 4.0 && self.todo_scroll_vis > 0.01 {
            let sv = self.todo_scroll_vis;
            let tx = rect.right() - 4.0;
            let track = egui::Rect::from_min_max(egui::pos2(tx, list_top + 2.0), egui::pos2(tx + 2.0, list_bottom - 2.0));
            painter.rect_filled(track, 1.0, white(12.0 / 255.0 * sv));
            let th = ((view_h / total_h.max(1.0)) * track.height()).max(14.0);
            let range = (track.height() - th).max(1.0);
            let tp = (self.todo_scroll / max_scroll.max(1.0)) * range;
            let thumb = egui::Rect::from_min_size(egui::pos2(tx, track.top() + tp), egui::vec2(2.0, th));
            painter.rect_filled(thumb, 1.0, white(60.0 / 255.0 * sv));
        }

        // ----  ----
        let mut dirty = false;
        if let Some(id) = toggle_id {
            let was_done = self.todos.iter().find(|t| t.id == id).map(|t| t.done).unwrap_or(true);
            let mut newseq = 0u64;
            if !was_done {
                self.todo_done_seq += 1;
                newseq = self.todo_done_seq;
            }
            if let Some(t) = self.todos.iter_mut().find(|t| t.id == id) {
                if was_done {
                    t.done = false;
                    t.done_seq = 0;
                } else {
                    t.done = true;
                    t.done_seq = newseq;
                }
                dirty = true;
            }
        }
        if let Some(id) = archive_id {
            if let Some(t) = self.todos.iter_mut().find(|t| t.id == id) {
                t.archived = true;
                dirty = true;
            }
        }
        if let Some(id) = restore_id {
            if let Some(t) = self.todos.iter_mut().find(|t| t.id == id) {
                t.archived = false;
                t.done = false;
                t.done_seq = 0;
                dirty = true;
            }
        }
        // 
        if std::env::var_os("WINISLE_TODO_FAKE_DELETE").is_some()
            && delete_id.is_none()
            && !self.todo_fake_deleted
        {
            self.todo_fake_deleted = true;
            if let Some(first) = self.todos.iter().find(|t| !t.archived) {
                delete_id = Some(first.id);
            }
        }
        if let Some(id) = delete_id {
            if let Some(pos) = self.todos.iter().position(|t| t.id == id) {
                self.todos.remove(pos);
                dirty = true;
            }
        }
        if dirty {
            save_todos(&self.todos);
        }

        // 
        if self.todo_editing {
            ui.ctx().request_repaint();
        }
    }

    #[allow(dead_code)]
    fn draw_gear(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, content: egui::Rect, e: f32, vis: f32) {
        let vis = vis.clamp(0.0, 1.0);
        let a = ((((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) * vis) as u8;
        let c = egui::pos2(content.right() - CONTENT_PAD_X - 12.5, content.top() + CONTENT_PAD_T + 12.5);
        let hit = egui::Rect::from_center_size(c, egui::vec2(30.0, 30.0));
        let resp = ui.interact(hit, ui.id().with("gear"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        // 
let col = egui::Color32::from_rgba_unmultiplied(170, 178, 195, a);
        painter.text(c, egui::Align2::CENTER_CENTER, "\u{E713}", icon_font(15.0), col); // Settings "齿轮"
        if vis > 0.5 && resp.clicked() { self.settings_scroll = 0.0; self.settings_scroll_target = 0.0; self.settings_scroll_act = std::time::Instant::now() - std::time::Duration::from_secs(10); self.show_settings = true; self.mark_upd_seen_rail(); }
    }

    fn start_update_check(&mut self) {
        if self.upd_checking || self.upd_busy {
            return;
        }
        // 
        if self.cfg.upd_checked_day == update::today_local() && self.upd_ver.is_none() {
            return;
        }
        self.upd_checking = true;
        self.upd_msg = "检查中…".to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
        });
        self.upd_rx = Some(rx);
    }

    fn start_update_install(&mut self) {
        use update::Manifest;
        let m: Manifest = match &self.upd_ver {
            Some(m) => m.clone(),
            None => return,
        };
        if self.upd_busy {
            return;
        }
        self.upd_busy = true;
        self.upd_dl_pct = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        self.upd_msg = format!("正在下载 v{}", m.version);
        let pct = self.upd_dl_pct.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        std::thread::spawn(move || {
            let r = update::download_setup_with_progress(&m, pct.clone()).and_then(|p| {
                // ?
                std::thread::sleep(Duration::from_millis(700));
                // ?
                let mut cfg = Config::load();
                cfg.upd_checked_day = update::today_local();
                cfg.save();
                update::run_setup(&p)
            });
            let _ = tx.send(r);
        });
        self.upd_inst_rx = Some(rx);
    }

    /// ?
fn poll_update(&mut self) {
        let got = self
            .upd_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(r) = got {
            self.upd_rx = None;
            self.upd_checking = false;
            self.upd_last_check = Instant::now();
            match r {
                Ok(Some(m)) => {
                    self.upd_msg = format!("发现新版未v{}", m.version);
                    self.upd_ver = Some(m);

                    if self.show_settings {
                        self.mark_upd_seen_rail();
                    }
                }
                Ok(None) => {
                    self.upd_ver = None;
                    self.upd_msg = "已是最新版本".to_string();
                    self.cfg.upd_checked_day = update::today_local();
                    self.cfg.save();
                }
                Err(e) => {
                    self.upd_msg = format!("棢查失败：{}", e);
                }
            }
        }
        if let Some(rx) = self.upd_inst_rx.as_ref() {
            if let Ok(r) = rx.try_recv() {
                self.upd_inst_rx = None;
                self.upd_busy = false;
                match r {
                    Ok(()) => self.upd_msg = "安装程序已启动".to_string(),
                    Err(e) => self.upd_msg = format!("更新失败：{}", e),
                }
            }
        }
        //  5s  4 ?
        if self.upd_rx.is_none()
            && !self.upd_checking
            && self.upd_last_check.elapsed() > Duration::from_secs(4 * 3600)
            && self.cfg.upd_checked_day != update::today_local()
        {
            self.start_update_check();
        }
    }

    fn start_weather_fetch(&mut self) {
        if self.w_inflight {
            return;
        }
        self.w_inflight = true;
        self.w_err = None;
        self.w_notice_until = None;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // 
            let real = weather::locate()
                .map(|(_, k)| k)
                .unwrap_or_else(|| "101010100".to_string());
            let _ = tx.send(weather::fetch(&real));
        });
        self.w_rx = Some(rx);
    }


    fn poll_weather(&mut self) {
        if let Some(rx) = self.w_rx.as_ref() {
            if let Ok(r) = rx.try_recv() {
                self.w_rx = None;
                self.w_inflight = false;
                self.w_last_fetch = Instant::now();
                match r {
                    Ok(w) => {
                        // 
                        {
                            let mut w = w;
                            if w.city.is_empty() {
                                w.city = if !self.cfg.weather_city_name.is_empty() {
                                    self.cfg.weather_city_name.clone()
                                } else {
                                    "当前位置".to_string()
                                };
                            }
                            self.w_data = Some(w);
                            self.w_err = None;
                            let lt = platform::local_time();
                            self.w_fetch_label = format!("{:02}:{:02}", lt.wHour, lt.wMinute);
                            self.w_notice = "已更新".to_string();
                            self.w_notice_err = false;
                            self.w_notice_until = Some(Instant::now() + Duration::from_secs(4));
                        }
                    }
                    Err(e) => {
                        self.w_err = Some(e);
                        self.w_notice = "更新失败".to_string();
                        self.w_notice_err = true;
                        self.w_notice_until = Some(Instant::now() + Duration::from_secs(6));
                        if self.w_data.is_none() {
                            self.w_data = weather::load_cache(&self.cfg.weather_city);
                            if self.w_data.is_some() {
                                self.w_fetch_label = "缓存".to_string();
                            }
                        }
                    }
                }
            }
        }
        let due = if self.w_err.is_some() {
            Duration::from_secs(60)
        } else {
            Duration::from_secs(1800)
        };
        if self.w_rx.is_none() && !self.w_inflight && self.w_last_fetch.elapsed() > due {
            self.start_weather_fetch();
        }
    }


    fn set_weather_city(&mut self, key: &str, name: &str) {
        if self.cfg.weather_city == key && self.cfg.weather_city_name == name {
            return;
        }
        self.cfg.weather_city = key.to_string();
        self.cfg.weather_city_name = name.to_string();
        self.cfg.save();
        self.w_data = weather::load_cache(key);
        self.w_fetch_label = if self.w_data.is_some() { "缓存".to_string() } else { String::new() };
        self.w_err = None;
        self.w_scroll = 0.0;
        self.w_scroll_target = 0.0;
        self.w_last_fetch = Instant::now() - Duration::from_secs(3600);
        self.start_weather_fetch();
    }


    fn start_city_search(&mut self) {
        let q = self.w_search_query.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.w_search_inflight = true;
        self.w_search_last_query = q.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(weather::search_cities(&q));
        });
        self.w_search_rx = Some(rx);
    }


    fn poll_city_search(&mut self) {
        if let Some(rx) = self.w_search_rx.as_ref() {
            if let Ok(list) = rx.try_recv() {
                self.w_search_rx = None;
                self.w_search_inflight = false;
                self.w_search_results = list;
                self.w_search_results_at = Some(Instant::now());
                if let Ok(pick) = std::env::var("WINISLE_SEARCH_PICK") {
                    if let Some(hit) = self
                        .w_search_results
                        .iter()
                        .find(|h| h.key == pick || h.name == pick)
                        .cloned()
                    {
                        self.set_weather_city(&hit.key, &hit.name);
                        self.close_city_search();
                        self.w_search_results.clear();
                    }
                }
            }
        }
        if !self.w_city_search {
            return;
        }
        let q = self.w_search_query.trim().to_string();
        if q.is_empty() {
            return;
        }
        if !self.w_search_inflight
            && q != self.w_search_last_query
            && self.w_search_last_edit.elapsed() >= Duration::from_millis(320)
        {
            self.start_city_search();
        }
    }

    fn open_city_search(&mut self, from_weather: bool) {
        self.show_settings = true;
        self.w_city_search = true;
        self.w_city_search_from_weather = from_weather;
        self.w_search_need_focus = true;
        self.w_search_focus_tries = 5;
        self.w_search_query.clear();
        self.w_search_results.clear();
        self.w_search_results_at = None;
        self.w_search_last_query.clear();
        self.settings_scroll = 0.0;
        self.settings_scroll_target = 0.0;
    }

    fn close_city_search(&mut self) {
        self.w_city_search = false;
        if self.w_city_search_from_weather {
            self.show_settings = false;
        }
        self.w_city_search_from_weather = false;
        self.w_search_need_focus = false;
        self.w_search_focus_tries = 0;
    }


    fn draw_city_search_page(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, content: egui::Rect, e: f32, vis: f32) {
        let vis = vis.clamp(0.0, 1.0);
        let clip = painter.with_clip_rect(content);
        let painter: &egui::Painter = &clip;
        let mask = egui::Color32::from_rgba_unmultiplied(
            self.cfg.bg[0], self.cfg.bg[1], self.cfg.bg[2], (self.cfg.alpha as f32 * vis) as u8,
        );
        let rr = self.cfg.radius.min(content.height() * 0.5);
        painter.rect_filled(content, rr, mask);
        let dy = (1.0 - vis) * 18.0;
        let content = egui::Rect::from_min_max(
            egui::pos2(content.left(), content.top() + dy),
            egui::pos2(content.right(), content.bottom() + dy),
        );
        let a = ((((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) * vis) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);

        let x0 = content.left() + CONTENT_PAD_X;
        let x1 = content.right() - CONTENT_PAD_X;
        let y0 = content.top() + CONTENT_PAD_T;

        //  + 
        let back = egui::Rect::from_center_size(egui::pos2(x0 + 9.0, y0 + 10.0), egui::vec2(26.0, 26.0));
        let br = ui
            .interact(back, ui.id().with("w_city_back"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let bt = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("w_city_back_hover"), br.hovered(), 0.12);
        painter.text(
            back.center(),
            egui::Align2::CENTER_CENTER,
            "\u{2039}",
            egui::FontId::proportional(20.0),
            lerp_color(muted2, accent, bt),
        );
        if br.clicked() {
            self.close_city_search();
        }
        painter.text(
            egui::pos2(x0 + 26.0, y0 + 10.0),
            egui::Align2::LEFT_CENTER,
            "搜索城市",
            bold_font(15.0),
            ink,
        );



        let mint = |al: f32| {
            egui::Color32::from_rgba_unmultiplied(52, 211, 153, (a as f32 * al).clamp(0.0, 255.0) as u8)
        };
        let iy = y0 + 30.0;
        let auto_w = 64.0f32;
        let input_r = egui::Rect::from_min_max(
            egui::pos2(x0, iy),
            egui::pos2(x1 - auto_w - 6.0, iy + 26.0),
        );
        painter.rect_filled(input_r, 7.0, egui::Color32::from_rgba_unmultiplied(22, 23, 28, a));
        painter.rect_stroke(
            input_r,
            7.0,
            egui::Stroke::new(
                0.8,
                egui::Color32::from_rgba_unmultiplied(150, 158, 175, (a as f32 * 0.50) as u8),
            ),
            egui::StrokeKind::Inside,
        );
        let mut q = std::mem::take(&mut self.w_search_query);
        let te = egui::TextEdit::singleline(&mut q)
            .frame(false)
            .id(ui.id().with("w_city_input"))
            .hint_text("城市 / 区县，例如 浦东新区")
            .font(egui::FontId::proportional(12.0))
            .text_color(ink);


        let tresp = ui
            .scope(|ui| {
                let v = ui.visuals_mut();
                v.extreme_bg_color = egui::Color32::TRANSPARENT;
                v.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                v.widgets.hovered.bg_fill = egui::Color32::TRANSPARENT;
                v.widgets.active.bg_fill = egui::Color32::TRANSPARENT;
                ui.put(input_r.shrink2(egui::vec2(8.0, 3.0)), te)
            })
            .inner;
        self.w_search_query = q;
        if self.w_search_need_focus && self.w_search_focus_tries > 0 {
            if !tresp.has_focus() {
                tresp.request_focus();
            }
            if tresp.has_focus() {
                self.w_search_need_focus = false;
            } else {
                self.w_search_focus_tries = self.w_search_focus_tries.saturating_sub(1);
            }
        }
        let ft = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("w_city_input_focus"), tresp.has_focus(), 0.16);
        painter.rect_stroke(
            input_r,
            7.0,
            egui::Stroke::new(
                0.8 + 0.2 * ft,
                egui::Color32::from_rgba_unmultiplied(
                    150,
                    158,
                    175,
                    (a as f32 * (0.50 + 0.35 * ft)) as u8,
                ),
            ),
            egui::StrokeKind::Inside,
        );
        if ft > 0.01 {
            painter.rect_stroke(
                input_r.expand(1.0),
                8.0,
                egui::Stroke::new(
                    0.6,
                    egui::Color32::from_rgba_unmultiplied(
                        150,
                        158,
                        175,
                        (a as f32 * 0.12 * ft) as u8,
                    ),
                ),
                egui::StrokeKind::Inside,
            );
        }
        if tresp.changed() {
            self.w_search_last_edit = Instant::now();
            if self.w_search_query.trim().is_empty() {
                self.w_search_results.clear();
                self.w_search_results_at = None;
                self.w_search_last_query.clear();
            }
        }
        let auto_r = egui::Rect::from_min_max(
            egui::pos2(x1 - auto_w, iy),
            egui::pos2(x1, iy + 26.0),
        );
        let ar = ui
            .interact(auto_r, ui.id().with("w_city_auto"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let at = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("w_city_auto_hover"), ar.hovered(), 0.13);
        painter.rect_filled(
            auto_r,
            7.0,
            lerp_color(
                accent,
                egui::Color32::from_rgba_unmultiplied(108, 232, 186, a),
                at,
            ),
        );
        painter.text(
            auto_r.center(),
            egui::Align2::CENTER_CENTER,
            "自动定位",
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgba_unmultiplied(6, 42, 32, a),
        );
        if ar.clicked() {
            self.set_weather_city("", "");
            self.close_city_search();
        }

        // 
        let mut yy = iy + 34.0;
        let q = self.w_search_query.trim().to_string();
        if q.is_empty() {
            painter.text(
                egui::pos2(x0 + 2.0, yy + 4.0),
                egui::Align2::LEFT_TOP,
                "输入城市或区县名，从全国范围搜索",
                egui::FontId::proportional(11.0),
                muted2,
            );
        } else if self.w_search_inflight && self.w_search_results.is_empty() {
            painter.text(
                egui::pos2(x0 + 2.0, yy + 4.0),
                egui::Align2::LEFT_TOP,
                "搜索一",
                egui::FontId::proportional(11.0),
                muted2,
            );
        } else if self.w_search_results.is_empty() {
            painter.text(
                egui::pos2(x0 + 2.0, yy + 4.0),
                egui::Align2::LEFT_TOP,
                format!("未找到「{}」，换个名称试试", q),
                egui::FontId::proportional(11.0),
                muted2,
            );
        } else {
            let list = self.w_search_results.clone();
            let list_age = self
                .w_search_results_at
                .map(|t| t.elapsed().as_secs_f32())
                .unwrap_or(1.0);
            for (i, hit) in list.iter().take(6).enumerate() {

                let raw = ((list_age - i as f32 * 0.028) / 0.16).clamp(0.0, 1.0);
                let appear = raw * raw * (3.0 - 2.0 * raw);
                if appear <= 0.001 {
                    continue;
                }
                let row_top = yy + (1.0 - appear) * 5.0;
                let row = egui::Rect::from_min_size(egui::pos2(x0, row_top), egui::vec2(x1 - x0, 28.0));
                let hr = ui
                    .interact(row, ui.id().with(("w_city_row", i)), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let ht = ui
                    .ctx()
                    .animate_bool_with_time(ui.id().with(("w_city_row_hover", i)), hr.hovered(), 0.11);
                if ht > 0.001 {
                    painter.rect_filled(row, 6.0, mint(0.10 * appear * ht));
                }
                let row_ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, (a as f32 * appear) as u8);
                let row_dim = egui::Color32::from_rgba_unmultiplied(
                    107,
                    113,
                    128,
                    (a as f32 * 0.60 * appear) as u8,
                );
                painter.text(
                    egui::pos2(x0 + 8.0, row_top + 14.0),
                    egui::Align2::LEFT_CENTER,
                    &hit.name,
                    egui::FontId::proportional(12.0),
                    row_ink,
                );
                painter.text(
                    egui::pos2(x1 - 8.0, row_top + 14.0),
                    egui::Align2::RIGHT_CENTER,
                    &hit.province,
                    egui::FontId::proportional(9.5),
                    row_dim,
                );
                if hr.clicked() && appear > 0.5 {
                    let key = hit.key.clone();
                    let name = hit.name.clone();
                    self.set_weather_city(&key, &name);
                    self.close_city_search();
                }
                yy += 28.0;
            }
            if list_age < 0.5 {
                ui.ctx().request_repaint();
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
    }

    fn draw_settings_page(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, content: egui::Rect, e: f32, vis: f32) {
        let vis = vis.clamp(0.0, 1.0);
        let full = content;
        // 
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        let target = if self.w_city_search { 1.0 } else { 0.0 };
        let k = 1.0 - (-dt / 0.085).exp();
        self.w_city_search_anim += (target - self.w_city_search_anim) * k;
        if (target - self.w_city_search_anim).abs() > 0.002 {
            ui.ctx().request_repaint();
        }
        let search_anim = self.w_city_search_anim.clamp(0.0, 1.0);
        let base_vis = vis * (1.0 - search_anim);
        // ?
        if !(self.w_city_search && search_anim >= 0.999) {
            self.draw_settings_base(ui, painter, content, e, base_vis);
        }
        if search_anim > 0.001 {

            let _ = ui.interact(full, ui.id().with("w_city_modal"), egui::Sense::click());
            self.draw_city_search_page(ui, painter, full, e, vis * search_anim);
        }
    }

    fn draw_settings_base(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, content: egui::Rect, e: f32, vis: f32) {
        let vis = vis.clamp(0.0, 1.0);
        let full = content;
        // 
        if self.face_mode_active() {
            for i in 0..FACES.len() {
                self.ensure_face_frames(ui.ctx(), i);
            }
        }
        // vis 
let clip_p = painter.with_clip_rect(full);
        let painter: &egui::Painter = &clip_p;
        let mask = egui::Color32::from_rgba_unmultiplied(
            self.cfg.bg[0], self.cfg.bg[1], self.cfg.bg[2], (self.cfg.alpha as f32 * vis) as u8,
        );
        let rr = self.cfg.radius.min(full.height() * 0.5);
        painter.rect_filled(full, rr, mask);
        // 
        let dy = (1.0 - vis) * 18.0;
        let content = egui::Rect::from_min_max(
            egui::pos2(full.left(), full.top() + dy),
            egui::pos2(full.right(), full.bottom() + dy),
        );
        let a = ((((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) * vis) as u8;
        let ink = egui::Color32::from_rgba_unmultiplied(233, 236, 242, a);
        let muted2 = egui::Color32::from_rgba_unmultiplied(107, 113, 128, a);
        let accent = egui::Color32::from_rgba_unmultiplied(52, 211, 153, a);

        let header_h = 48.0f32;
        let back_r = 11.0f32;
        // 
        let bc = egui::pos2(content.left() + CONTENT_PAD_X + back_r, content.top() + header_h * 0.5);
        let bh = egui::Rect::from_center_size(bc, egui::vec2(30.0, 30.0));
        let bresp = ui.interact(bh, ui.id().with("set_back"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        painter.text(bc, egui::Align2::CENTER_CENTER, "\u{E72B}", icon_font(12.5), ink); // Back
        if bresp.clicked() { self.show_settings = false; self.w_city_search = false; self.w_city_search_from_weather = false; }

        let left = content.left() + CONTENT_PAD_X;
        let right = content.right() - CONTENT_PAD_X;
        let row_h = 30.0f32;
        let label_h = 18.0f32;
        let group_gap = 12.0f32;
        // 7  2 
let tile_h = 60.0f32;
        let tile_gap = 5.0f32;
        let grid_h = tile_h * 2.0 + tile_gap;
        // 
        let list_top = content.top() + header_h;
        let list_bottom = content.bottom() - CONTENT_PAD_B;
        let view_h = (list_bottom - list_top).max(1.0);
        let face_on = self.face_mode_active();
        let mut total_h = label_h * 6.0 + group_gap * 5.0 + row_h * 11.0 + label_h;
        if face_on {
            total_h += row_h + grid_h;
        }
        let max_scroll = (total_h - view_h).max(0.0);
        // 
        let mut scroll_activity = false;
        if let Some(ph) = ui.ctx().pointer_hover_pos() {
            if full.contains(ph) && max_scroll > 0.0 {
                let d = ui.input(|i| i.raw_scroll_delta.y);
                if d.abs() > 0.01 {
                    self.settings_scroll_target -= d;
                    scroll_activity = true;
                }
            }
        }
        self.settings_scroll_target = self.settings_scroll_target.clamp(0.0, max_scroll);
        // WINISLE_SET_SCROLL=<px> 
if let Ok(v) = std::env::var("WINISLE_SET_SCROLL") {
            if let Ok(px) = v.trim().parse::<f32>() {
                self.settings_scroll_target = px.clamp(0.0, max_scroll);
                self.settings_scroll = self.settings_scroll_target;
            }
        }

        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        let k = 1.0 - (-dt / 0.07).exp();
        self.settings_scroll += (self.settings_scroll_target - self.settings_scroll) * k;
        self.settings_scroll = self.settings_scroll.clamp(0.0, max_scroll);
        // WINISLE_SETTINGS_SCROLL=<px> 
        if let Ok(sv) = std::env::var("WINISLE_SETTINGS_SCROLL") {
            if let Ok(v) = sv.parse::<f32>() {
                self.settings_scroll_target = v.clamp(0.0, max_scroll);
                self.settings_scroll = self.settings_scroll_target;
            }
        }
        let settling = (self.settings_scroll_target - self.settings_scroll).abs() > 0.5;
        if (self.settings_scroll_target - self.settings_scroll).abs() > 0.25 {
            ui.ctx().request_repaint();
        }

        if scroll_activity {
            self.settings_scroll_act = std::time::Instant::now();
        }
        let since_act = self.settings_scroll_act.elapsed().as_secs_f32();
        let want_vis = if max_scroll > 0.0 && (scroll_activity || settling || since_act < 1.2) {
            1.0
        } else {
            0.0
        };
        let kv = 1.0 - (-dt / 0.12).exp();
        self.settings_scroll_vis += (want_vis - self.settings_scroll_vis) * kv;
        if self.settings_scroll_vis < 0.005 {
            self.settings_scroll_vis = 0.0;
        } else if self.settings_scroll_vis > 0.995 {
            self.settings_scroll_vis = 1.0;
        }
        if (self.settings_scroll_vis - want_vis).abs() > 0.005 {
            ui.ctx().request_repaint();
        }
        let list_rect = egui::Rect::from_min_max(
            egui::pos2(full.left(), list_top),
            egui::pos2(full.right(), list_bottom),
        );
        let lp = painter.with_clip_rect(list_rect);
        let painter: &egui::Painter = &lp;
        let mut y = list_top - self.settings_scroll;
        let mut changed = false;

        let row_label = |painter: &egui::Painter, y: f32, s: &str, c: egui::Color32| {
            painter.text(egui::pos2(left, y + row_h * 0.5), egui::Align2::LEFT_CENTER, s, egui::FontId::proportional(13.5), c);
        };
        let tg = |y: f32| egui::Rect::from_min_size(egui::pos2(right - 32.0, y + row_h * 0.5 - 8.0), egui::vec2(32.0, 16.0));

        //   +
        painter.text(egui::pos2(left, y), egui::Align2::LEFT_TOP, "", egui::FontId::proportional(13.0), muted2);
        y += label_h;
        {
            let mut order = self.comp_order();
            let comps_top = y;
            let row_w = right - left;
            let toggle_w = 44.0;
            let drag_id = ui.id().with("cset_drag");
            let dragging: Option<String> = ui.data(|d| d.get_temp::<String>(drag_id));
            let n = order.len();
            let slots: Vec<f32> = order
                .iter()
                .enumerate()
                .map(|(i, (id, _))| {
                    ui.ctx().animate_value_with_time(ui.id().with(("cset_pos", id)), i as f32, 0.09)
                })
                .collect();
            let ptr = ui.ctx().pointer_hover_pos();
            let down = ui.ctx().input(|i| i.pointer.any_down());
            let force_hl: Option<String> = std::env::var("WINISLE_SET_HL").ok();
            let drag_preview: Option<String> = std::env::var("WINISLE_DRAG_PREVIEW").ok();
            // 
            let drag_fy: Option<(String, f32)> = if let Some(did) = dragging.clone() {
                if down {
                    ptr.map(|p| (did, (((p.y - comps_top) / row_h) - 0.5).clamp(0.0, (n.max(1) - 1) as f32)))
                } else {
                    None
                }
            } else {
                None
            };
            let mut draw_order: Vec<usize> = (0..n).collect();
            if let Some((did, _)) = &drag_fy {
                if let Some(ci) = order.iter().position(|(x, _)| x == did) {
                    draw_order.retain(|&k| k != ci);
                    draw_order.push(ci);
                }
            }
            for &i in &draw_order {
                let (id, on) = order[i].clone();
                let is_drag = (drag_fy.as_ref().map(|(d, _)| d == &id).unwrap_or(false) && down)
                    || drag_preview.as_deref() == Some(id.as_str());
                let slot = if is_drag {
                    drag_fy.as_ref().map(|(_, f)| *f).unwrap_or(i as f32)
                } else if drag_preview.as_deref() == Some(id.as_str()) {
                    i as f32
                } else {
                    slots[i]
                };
                let ry = comps_top + slot * row_h;
                let rrect = egui::Rect::from_min_size(egui::pos2(left, ry), egui::vec2(row_w, row_h));
                let hovered = ptr.map(|p| rrect.contains(p)).unwrap_or(false);
                // /
                if is_drag {
                    // left=right
                    let card = egui::Rect::from_min_max(
                        egui::pos2(left, ry + 1.0),
                        egui::pos2(right, ry + row_h - 1.0),
                    );
                    painter.rect_filled(card.translate(egui::vec2(0.0, 3.0)), 8.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 70));
                    let c = egui::Color32::from_rgb(
                        self.cfg.bg[0].saturating_add(16),
                        self.cfg.bg[1].saturating_add(16),
                        self.cfg.bg[2].saturating_add(18),
                    );
                    painter.rect_filled(card, 8.0, c);
                    painter.rect_stroke(card, 8.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, 110)), egui::StrokeKind::Inside);
                }
                // left..right
                if hovered || is_drag || force_hl.as_deref() == Some(id.as_str()) {
                    let ha = ((if is_drag { 30.0 } else { 14.0 }) * (a as f32 / 255.0)) as u8;
                    let hl = egui::Rect::from_min_max(
                        egui::pos2(left, ry + 2.0),
                        egui::pos2(right, ry + row_h - 2.0),
                    );
                    painter.rect_filled(hl, 7.0, egui::Color32::from_rgba_unmultiplied(52, 211, 153, ha));
                }
                let hrect = egui::Rect::from_min_max(rrect.min, egui::pos2(rrect.right() - toggle_w, rrect.bottom()));
                let hresp = ui.interact(hrect, ui.id().with(("cset_h", &id)), egui::Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Grab);
                if hresp.drag_started() {
                    ui.data_mut(|d| d.insert_temp(drag_id, id.clone()));
                }
                //  23
                let hcx = left + 9.0;
                let hcy = ry + row_h * 0.5;
                for col in 0..2 {
                    for rr in 0..3 {
                        painter.circle_filled(
                            egui::pos2(hcx + col as f32 * 4.0, hcy - 5.0 + rr as f32 * 5.0),
                            1.35,
                            muted2,
                        );
                    }
                }
                //   520
                let can_toggle = if on {
                    true
                } else {
                    let mut probe = order.clone();
                    probe[i].1 = true;
                    self.order_base_width(&probe) <= CW_MAX + 0.01
                };
                let label_col = if can_toggle { ink } else { muted2 };
                painter.text(egui::pos2(left + 24.0, ry + row_h * 0.5), egui::Align2::LEFT_CENTER, Self::comp_label(&id), egui::FontId::proportional(13.5), label_col);
                let trect = egui::Rect::from_min_size(egui::pos2(right - 37.0, ry + row_h * 0.5 - 8.0), egui::vec2(32.0, 16.0));
                if let Some(v) = Self::draw_toggle(ui, painter, trect, on, ui.id().with(("cset_tg", &id)), accent, a, can_toggle) {
                    order[i].1 = v;
                    self.set_comp_order(&order);
                    changed = true;
                }
            }
            y = comps_top + n as f32 * row_h;
            if let Some((did, _)) = drag_fy.clone() {
                if let Some(p) = ptr {
                    if let Some(cur) = order.iter().position(|(x, _)| x == &did) {
                        //  0.30 
                        // 
let fy = (p.y - comps_top) / row_h;
                        let mut target = cur;
                        // ?                        //  0.30 
                        //  
if fy - cur as f32 > 0.5 {
                            target = (cur + 1).min(n - 1);
                        } else if cur as f32 - fy > 0.5 {
                            target = cur.saturating_sub(1);
                        }
                        if target != cur {
                            let item = order.remove(cur);
                            order.insert(target, item);
                            self.set_comp_order(&order);
                            changed = true;
                        }
                    }
                }
                ui.ctx().request_repaint();
            } else if dragging.is_some() {
                ui.data_mut(|d| d.remove_temp::<String>(drag_id));
            }
            // 
if slots.iter().enumerate().any(|(i, v)| (v - i as f32).abs() > 0.001) {
                ui.ctx().request_repaint();
            }
            // / 
            let used = self.order_base_width(&order);
            let comp_n = order.iter().filter(|(_, on)| *on).count();
            let hint = format!("占用 {:.0} / {:.0}（{} 项启用）", used, CW_MAX, comp_n);
            painter.text(egui::pos2(left, y + label_h * 0.5), egui::Align2::LEFT_CENTER, hint, egui::FontId::proportional(11.0), muted2);
            y += label_h;
        }
        if face_on {
            // 
            row_label(painter, y, "待机表情", ink);
            y += row_h;
            let area = egui::Rect::from_min_size(egui::pos2(left, y), egui::vec2(right - left, grid_h));
            let tsec = ui.input(|i| i.time);
            let mut face_uvs: Vec<Option<(f32, f32, f32, f32)>> = Vec::with_capacity(FACES.len());
            for fi in 0..FACES.len() {
                let has = self.face_frames.get(fi).map_or(false, |o| o.is_some());
                face_uvs.push(if has { Some(self.face_content_uv(fi)) } else { None });
            }
            if let Some(v) = Self::draw_face_grid(ui, painter, area, self.cfg.idle_face, ui.id().with("face_grid"), a, accent, &self.face_frames, &face_uvs, tsec) {
                self.cfg.idle_face = v;
                changed = true;
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
            y += grid_h;
        }


        // 
        y += group_gap;
        painter.text(egui::pos2(left, y), egui::Align2::LEFT_TOP, "音乐", egui::FontId::proportional(13.0), muted2);
        y += label_h;
        // 
        row_label(painter, y, "波纹频谱", ink);
        { let trect = tg(y); if let Some(v) = Self::draw_toggle(ui, painter, trect, self.cfg.water_spectrum, ui.id().with("tg_water_spec"), accent, a, true) { self.cfg.water_spectrum = v; changed = true; } }
        y += row_h;

        y += group_gap;
        painter.text(egui::pos2(left, y), egui::Align2::LEFT_TOP, "歌词", egui::FontId::proportional(13.0), muted2);
        y += label_h;
        // 
        row_label(painter, y, "换败滚动动画", ink);
        { let trect = tg(y); if let Some(v) = Self::draw_toggle(ui, painter, trect, self.cfg.roll_anim, ui.id().with("tg_roll"), accent, a, true) { self.cfg.roll_anim = v; changed = true; } }
        y += row_h;
        //  / 
        row_label(painter, y, "边缘渐隐", ink);
        { let trect = tg(y); if let Some(v) = Self::draw_toggle(ui, painter, trect, self.cfg.edge_fade, ui.id().with("tg_edge_fade"), accent, a, true) { self.cfg.edge_fade = v; changed = true; } }
        y += row_h;

        // 
        y += group_gap;
        painter.text(egui::pos2(left, y), egui::Align2::LEFT_TOP, "窗口", egui::FontId::proportional(13.0), muted2);
        y += label_h;
        row_label(painter, y, "左右吸附", ink);
        { let trect = tg(y); if let Some(v) = Self::draw_toggle(ui, painter, trect, self.cfg.side_snap, ui.id().with("tg_side_snap"), accent, a, true) { self.cfg.side_snap = v; changed = true; } }
        y += row_h;

        // 
        y += group_gap;
        painter.text(egui::pos2(left, y), egui::Align2::LEFT_TOP, "天气", egui::FontId::proportional(13.0), muted2);
        y += label_h;
        row_label(painter, y, "温度单位", ink);
        {
            let seg = egui::Rect::from_min_max(egui::pos2(right - 104.0, y + 5.0), egui::pos2(right, y + row_h - 5.0));
            if let Some(v) = Self::draw_segment(ui, painter, seg, &["摄氏", "华氏"], self.cfg.weather_unit as usize, ui.id().with("seg_unit"), accent, a) {
                self.cfg.weather_unit = v as u8;
                changed = true;
            }
        }
        y += row_h;

        //         y += group_gap;
        painter.text(egui::pos2(left, y), egui::Align2::LEFT_TOP, "通用", egui::FontId::proportional(13.0), muted2);
        y += label_h;
        row_label(painter, y, "检查更新", ink);
        {
            let up_row = egui::Rect::from_min_max(
                egui::pos2(right - 220.0, y + 3.0),
                egui::pos2(right, y + row_h - 3.0),
            );
            let today = update::today_local();
            let checked_today = self.cfg.upd_checked_day == today;
            let ver = self.upd_ver.as_ref().map(|m| m.version.clone()).unwrap_or_default();
            let pct = self.upd_dl_pct.load(std::sync::atomic::Ordering::Relaxed);
            let (txt, col, green, show_dot, progress, enabled) = if self.upd_busy {
                if pct >= 100 {
                    (format!("v{} · 正在安装 v{}", update::VERSION, ver), accent, true, false, Some(100u8), false)
                } else {
                    (format!("v{} · 正在下载 v{}{}%", update::VERSION, ver, pct), accent, true, false, Some(pct), false)
                }
            } else if let Some(m) = &self.upd_ver {
                (
                    format!("v{} · 更新 (v{})", update::VERSION, m.version),
                    accent,
                    true,
                    self.show_row_update(),
                    None,
                    true,
                )
            } else if self.upd_checking {
                (format!("v{} · 检查中", update::VERSION), muted2, false, false, None, false)
            } else if self.upd_msg.contains("失败") {
                (self.upd_msg.clone(), egui::Color32::from_rgba_unmultiplied(230, 120, 120, a), false, false, None, true)
            } else if checked_today {
                (format!("v{} · 已是最新版本", update::VERSION), muted2, false, false, None, false)
            } else {
                (format!("v{} · 检查更新", update::VERSION), muted2, false, false, None, true)
            };
            let font = egui::FontId::proportional(11.5);
            // v0.2.1  
            painter.text(egui::pos2(right, y + row_h * 0.5), egui::Align2::RIGHT_CENTER, txt, font, col);
            if green {
                // 
                if show_dot {
                    painter.circle_filled(egui::pos2(right - 3.0, y + row_h * 0.5 - 8.0), 3.0, accent);
                }
            }
            if let Some(p) = progress {
                let bar = egui::Rect::from_min_max(
                    egui::pos2(up_row.left() + 4.0, y + row_h - 5.0),
                    egui::pos2(up_row.right(), y + row_h - 2.0),
                );
                painter.rect_filled(bar, 1.5, egui::Color32::from_rgba_unmultiplied(42, 58, 52, a));
                let fill = egui::Rect::from_min_max(
                    bar.left_top(),
                    egui::pos2(bar.left() + bar.width() * p as f32 / 100.0, bar.bottom()),
                );
                painter.rect_filled(fill, 1.5, accent);
            }
            let resp = ui.interact(up_row, ui.id().with("upd_row"), egui::Sense::click());
            let resp = if enabled {
                resp.on_hover_cursor(egui::CursorIcon::PointingHand)
            } else {
                resp
            };
            if enabled && resp.clicked() {
                self.mark_upd_seen_row();
                if self.upd_ver.is_some() {
                    self.start_update_install();
                } else {
                    self.start_update_check();
                }
            }
        }
        y += row_h;


        if max_scroll > 0.0 && self.settings_scroll_vis > 0.01 {
            let sv = self.settings_scroll_vis;
            let tx = full.right() - 4.0;
            let track = egui::Rect::from_min_max(
                egui::pos2(tx, list_top + 2.0),
                egui::pos2(tx + 2.0, list_bottom - 2.0),
            );
            painter.rect_filled(track, 1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (12.0 * sv) as u8));
            let th = ((view_h / total_h.max(1.0)) * track.height()).max(14.0);
            let range = (track.height() - th).max(1.0);
            let tp = (self.settings_scroll / max_scroll.max(1.0)) * range;
            let thumb = egui::Rect::from_min_size(egui::pos2(tx, track.top() + tp), egui::vec2(2.0, th));
            painter.rect_filled(thumb, 1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, (60.0 * sv) as u8));
        }

        if changed { self.cfg.save(); }
    }

    /// + Some(9
    fn draw_toggle(ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, on: bool, id: egui::Id, accent: egui::Color32, a: u8, enabled: bool) -> Option<bool> {
        let t = ui.ctx().animate_bool_with_time(id.with("anim"), on, 0.16);
        // 
let da = if enabled { a } else { ((a as f32) * 0.42) as u8 };
        let lerp_u = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        let bg = egui::Color32::from_rgba_unmultiplied(
            lerp_u(78, accent.r()), lerp_u(82, accent.g()), lerp_u(96, accent.b()), da,
        );
        painter.rect_filled(rect, rect.height() * 0.5, bg);
        let r = rect.height() * 0.5 - 2.0;
        let kx0 = rect.left() + r + 2.0;
        let kx1 = rect.right() - r - 2.0;
        let kx = kx0 + (kx1 - kx0) * t;
        painter.circle_filled(egui::pos2(kx, rect.center().y), r, egui::Color32::from_rgba_unmultiplied(246, 248, 251, da));
        if !enabled { return None; }
        let resp = ui.interact(rect.expand(5.0), id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() { Some(!on) } else { None }
    }


    #[allow(clippy::too_many_arguments)]
    fn draw_slider(ui: &mut egui::Ui, painter: &egui::Painter, zone: egui::Rect, val: f32, min: f32, max: f32, step: f32, suffix: &str, enabled: bool, id: egui::Id, accent: egui::Color32, a: u8) -> Option<f32> {
        let track = egui::Rect::from_min_max(egui::pos2(zone.left(), zone.center().y - 2.0), egui::pos2(zone.right() - 56.0, zone.center().y + 2.0));
        let frac = if max > min { ((val - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
        let active_col = if enabled { accent } else { egui::Color32::from_rgba_unmultiplied(120, 124, 138, a) };
        painter.rect_filled(track, 2.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 26));
        let fx = track.left() + track.width() * frac;
        painter.rect_filled(egui::Rect::from_min_max(track.min, egui::pos2(fx, track.bottom())), 2.0, active_col);
        painter.circle_filled(egui::pos2(fx, track.center().y), 6.0, active_col);
        let shown = if (val - val.round()).abs() < 0.01 { format!("{:.0}{}", val.round(), suffix) } else { format!("{:.1}{}", val, suffix) };
        let tcol = if enabled { egui::Color32::from_rgba_unmultiplied(200, 206, 220, a) } else { egui::Color32::from_rgba_unmultiplied(120, 124, 138, a) };
        painter.text(egui::pos2(zone.right(), zone.center().y), egui::Align2::RIGHT_CENTER, shown, egui::FontId::proportional(12.0), tcol);
        if !enabled { return None; }
        let hit = track.expand2(egui::vec2(6.0, 10.0));
        let resp = ui.interact(hit, id, egui::Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.dragged() || resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let f = ((pos.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
                let raw = min + f * (max - min);
                let stepped = if step > 0.0 { (raw / step).round() * step } else { raw };
                return Some(stepped.clamp(min, max));
            }
        }
        None
    }

    /// |     ///  Some( 9
    fn draw_segment(ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, opts: &[&str], sel: usize, id: egui::Id, accent: egui::Color32, a: u8) -> Option<usize> {
        let resp = ui.interact(rect, id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let n = opts.len().max(1);
        let seg_w = rect.width() / n as f32;
        // 
let target = sel as f32;
        let pos = ui.ctx().animate_value_with_time(id.with("seg_pos"), target, 0.20);
        // 22,23,28)
        painter.rect_filled(rect, rect.height() * 0.5, egui::Color32::from_rgba_unmultiplied(22, 23, 28, a));
        // 
        let hx = rect.left() + seg_w * pos;
        let hr = egui::Rect::from_min_max(
            egui::pos2(hx + 1.0, rect.top() + 1.0),
            egui::pos2(hx + seg_w - 1.0, rect.bottom() - 1.0),
        );
        painter.rect_filled(hr, hr.height() * 0.5, egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), a));
        let lerp_u = |x: u8, y: u8, t: f32| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        let mut out = None;
        for (i, label) in opts.iter().enumerate() {
            // /
let w = (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0);
            let tc = egui::Color32::from_rgba_unmultiplied(
                lerp_u(178, 16, w),
                lerp_u(186, 26, w),
                lerp_u(202, 22, w),
                a,
            );
            let c = egui::pos2(rect.left() + seg_w * (i as f32 + 0.5), rect.center().y);
            painter.text(c, egui::Align2::CENTER_CENTER, *label, egui::FontId::proportional(12.0), tc);
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let sr = egui::Rect::from_min_max(
                        egui::pos2(rect.left() + seg_w * i as f32, rect.top()),
                        egui::pos2(rect.left() + seg_w * (i + 1) as f32, rect.bottom()),
                    );
                    if sr.contains(p) { out = Some(i); }
                }
            }
        }
        if (pos - target).abs() > 0.001 { ui.ctx().request_repaint(); }
        out
    }

    ///  +  + Some(
fn draw_face_stepper(ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect, cur: u8, id: egui::Id, a: u8, face_frames: &[Option<Vec<egui::TextureHandle>>]) -> Option<u8> {
        let resp = ui.interact(rect, id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let kept = face_kept_indices();
        let n = kept.len().max(1);
        let cur_pos = kept
            .iter()
            .position(|&x| x == cur as usize)
            .unwrap_or(0);
        let cur_i = kept.get(cur_pos).copied().unwrap_or(0);
        let tex = face_frames.get(cur_i).and_then(|o| o.as_ref()).and_then(|v| v.get(0));
        let fh = (rect.height() - 6.0).min(16.0).max(8.0);
        let fw = match tex {
            Some(t) => {
                let s = t.size_vec2();
                fh * (s.x / s.y.max(1.0))
            }
            None => fh,
        };
        let pv = egui::pos2(rect.left() + fw * 0.5 + 1.0, rect.center().y);
        draw_robot_face(painter, pv, fh, a, cur_i as u8, 0.0, tex);
        painter.text(
            egui::pos2(rect.left() + fw + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            FACES[cur_i],
            egui::FontId::proportional(12.5),
            egui::Color32::from_rgba_unmultiplied(230, 234, 242, a),
        );
        let arrow_c = egui::Color32::from_rgba_unmultiplied(150, 158, 175, a);
        let lz = egui::Rect::from_center_size(egui::pos2(rect.right() - 36.0, rect.center().y), egui::vec2(22.0, rect.height()));
        let rz = egui::Rect::from_center_size(egui::pos2(rect.right() - 12.0, rect.center().y), egui::vec2(22.0, rect.height()));
        painter.text(lz.center(), egui::Align2::CENTER_CENTER, "\u{2039}", egui::FontId::proportional(17.0), arrow_c);
        painter.text(rz.center(), egui::Align2::CENTER_CENTER, "\u{203A}", egui::FontId::proportional(17.0), arrow_c);
        let mut out = None;
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if lz.contains(p) {
                    out = Some(kept[(cur_pos + n - 1) % n] as u8);
                } else if rz.contains(p) {
                    out = Some(kept[(cur_pos + 1) % n] as u8);
                }
            }
        }
        out
    }

    /// 7  2     /// Some(
fn draw_face_grid(
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        area: egui::Rect,
        cur: u8,
        id: egui::Id,
        a: u8,
        accent: egui::Color32,
        face_frames: &[Option<Vec<egui::TextureHandle>>],
        uvs: &[Option<(f32, f32, f32, f32)>],
        t: f64,
    ) -> Option<u8> {
        let kept = face_kept_indices();
        let n = kept.len();
        let cols = 7usize;
        let rows = (n + cols - 1) / cols;
        let gap = 5.0f32;
        let cell_w = area.width() / cols as f32;
        let cell_h = (area.height() - (rows as f32 - 1.0) * gap) / rows as f32;
        let metas = Self::face_anim_meta();
        let mut out = None;
        for (pos, &i) in kept.iter().enumerate() {
            let r = pos / cols;
            let c = pos % cols;
            let trect = egui::Rect::from_min_size(
                egui::pos2(area.left() + c as f32 * cell_w + gap * 0.5, area.top() + r as f32 * (cell_h + gap)),
                egui::vec2(cell_w - gap, cell_h),
            );
            let resp = ui
                .interact(trect, id.with(i), egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let sel = i == cur as usize;
            // 2,23,28)
            painter.rect_filled(trect, 8.0, egui::Color32::from_rgba_unmultiplied(22, 23, 28, a));
            if sel {
                painter.rect_stroke(
                    trect,
                    egui::CornerRadius::same(8),
                    egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), a)),
                    egui::StrokeKind::Inside,
                );
            }
            // 
            let fc = metas.get(i).map(|m| m.2 as usize).unwrap_or(1).max(1);
            let frame = (((t * 1000.0) / 80.0) as usize) % fc;
            let tex = face_frames
                .get(i)
                .and_then(|o| o.as_ref())
                .and_then(|v| v.get(frame));
            let fh_max = (cell_h - 8.0).clamp(16.0, 46.0);
            let (pw, ph) = metas
                .get(i)
                .map(|m| (m.0 as f32, m.1 as f32))
                .unwrap_or((77.0, 80.0));
            let uv = uvs.get(i).and_then(|o| *o);
            // ? 4px14px
let av_top = trect.top() + 4.0;
            let av_bot = trect.bottom() - 14.0;
            let av_h = (av_bot - av_top).max(8.0);
            let cy = (av_top + av_bot) * 0.5;
            if let (Some(tx), Some((u0, v0, u1, v1))) = (tex, uv) {
                // + 
                let content_aspect =
                    ((pw * (u1 - u0)) / (ph * (v1 - v0)).max(1.0)).max(0.2);
                let mut draw_h = fh_max.min(av_h);
                let mut fw = draw_h * content_aspect;
                let maxw = (trect.width() - 6.0).max(8.0);
                if fw > maxw {
                    fw = maxw;
                    draw_h = fw / content_aspect;
                }
                draw_face_img_cropped(
                    painter,
                    egui::pos2(trect.center().x, cy),
                    fw,
                    draw_h,
                    a,
                    tx,
                    (u0, v0, u1, v1),
                    FACE_EYE_GAP_REDUCE,
                );
            } else {
                let draw_h = fh_max.min(av_h);
                draw_robot_face(painter, egui::pos2(trect.center().x, cy), draw_h, a, i as u8, 0.0, tex);
            }
            // 
            let lcol = if sel {
                egui::Color32::from_rgba_unmultiplied(235, 238, 245, a)
            } else {
                egui::Color32::from_rgba_unmultiplied(150, 158, 175, a)
            };
            painter.text(
                egui::pos2(trect.center().x, trect.bottom() - 9.0),
                egui::Align2::CENTER_CENTER,
                FACES[i],
                egui::FontId::proportional(9.0),
                lcol,
            );
            if resp.clicked() {
                out = Some(i as u8);
            }
        }
        out
    }

    // ----------------  ----------------
    fn settings_ui(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            self.confirm_open = false;
            self.settings_hwnd = 0;
            self.settings_shown = false;
            return;
        }
        apply_style(ctx, true);
        let face_idx0 = self.cfg.idle_face as usize;
        self.ensure_face_frames(ctx, face_idx0);
        let c = palette(true);
        let focus = self.focus_settings;
        let mut do_hide = false;
        let mut do_exit = false;
        let mut confirm_open = self.confirm_open;

        //  Win11 DWM
        let first = !self.settings_shown;
        let mut builder = egui::ViewportBuilder::default()
            .with_title("WinIsle 设置")
            .with_decorations(false)
            .with_resizable(false)
            .with_inner_size([440.0, 320.0])
            .with_min_inner_size([440.0, 320.0])
            .with_max_inner_size([440.0, 320.0]);

        if first {
            builder = builder.with_visible(false);
        }

        ctx.set_theme(egui::ThemePreference::Light);
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("winisle_settings"),
            builder,
            |ctx, _class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    confirm_open = true;
                }
                if focus {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }

                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let rect = ui.max_rect();
                        let p = ui.painter().clone();
                        p.rect_filled(rect, egui::CornerRadius::same(0), c.bg);

                        //  + 
                        p.circle_filled(egui::pos2(rect.min.x + 22.0, rect.min.y + 26.0), 4.5, c.accent);
                        p.text(
                            egui::pos2(rect.min.x + 32.0, rect.min.y + 26.0),
                            egui::Align2::LEFT_CENTER,
                            "WinIsle",
                            egui::FontId::proportional(15.0),
                            c.text,
                        );

                        let header = egui::Rect::from_min_max(
                            egui::pos2(rect.min.x, rect.min.y),
                            egui::pos2(rect.max.x - 48.0, rect.min.y + 48.0),
                        );
                        let drag = ui.interact(header, egui::Id::new("winisle_title_drag"), egui::Sense::drag());
                        if drag.drag_started() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }

                        let btn = egui::Rect::from_min_size(
                            egui::pos2(rect.max.x - 42.0, rect.min.y + 12.0),
                            egui::vec2(30.0, 30.0),
                        );
                        let cresp = ui.interact(btn, egui::Id::new("winisle_close"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        if cresp.hovered() {
                            p.rect_filled(btn, egui::CornerRadius::same(8), egui::Color32::from_rgb(238, 238, 240));
                        }
                        let xcol = if cresp.hovered() {
                            egui::Color32::from_rgb(60, 60, 66)
                        } else {
                            c.muted
                        };
                        p.text(btn.center(), egui::Align2::CENTER_CENTER, "", egui::FontId::proportional(16.0), xcol);
                        if cresp.clicked() {
                            confirm_open = true;
                        }

                        let cx = rect.center().x;
                        p.text(
                            egui::pos2(cx, rect.min.y + 148.0),
                            egui::Align2::CENTER_CENTER,
                            "WinIsle",
                            egui::FontId::proportional(34.0),
                            c.text,
                        );
                        p.text(
                            egui::pos2(cx, rect.min.y + 196.0),
                            egui::Align2::CENTER_CENTER,
                            "正在系统托盘运行",
                            egui::FontId::proportional(14.0),
                            c.muted,
                        );
                        p.text(
                            egui::pos2(cx, rect.min.y + 262.0),
                            egui::Align2::CENTER_CENTER,
                            "点右上角关闭按钮可隐藏",
                            egui::FontId::proportional(11.5),
                            c.muted2,
                        );
                    });

                //   / Esc 
                if confirm_open {
                    let resp = egui::Modal::new(egui::Id::new("winisle_confirm_modal"))
                        .frame(
                            egui::Frame::NONE
                                .fill(c.bg)
                                .inner_margin(egui::Margin::same(18))
                                .corner_radius(egui::CornerRadius::same(14))
                                .stroke(egui::Stroke::new(1.0_f32, c.line)),
                        )
                        .show(ctx, |ui| {
                            ui.set_width(240.0);
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new("关闭 WinIsle").size(16.0).strong().color(c.text));
                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("隐藏到托盘，还是彻底退出？").size(12.0).color(c.muted));
                                ui.add_space(14.0);
                                ui.horizontal(|ui| {
                                    let total = 96.0 * 2.0 + 10.0;
                                    let pad = ((ui.available_width() - total) / 2.0).max(0.0);
                                    ui.add_space(pad);
                                    if ui
                                        .add_sized(
                                            [96.0, 36.0],
                                            egui::Button::new(egui::RichText::new("隐藏到托").size(12.5).color(c.text))
                                                .fill(c.card2)
                                                .corner_radius(egui::CornerRadius::same(10)),
                                        )
                                        .clicked()
                                    {
                                        do_hide = true;
                                        confirm_open = false;
                                    }
                                    ui.add_space(10.0);
                                    if ui
                                        .add_sized(
                                            [96.0, 36.0],
                                            egui::Button::new(egui::RichText::new("彻底退出").size(12.5).color(c.danger))
                                                .fill(c.danger_dim)
                                                .corner_radius(egui::CornerRadius::same(10)),
                                        )
                                        .clicked()
                                    {
                                        do_exit = true;
                                        confirm_open = false;
                                    }
                                });
                            });
                        });
                    if resp.should_close() {
                        confirm_open = false;
                    }
                }
            },
        );
        //
        ctx.set_theme(egui::ThemePreference::Dark);
        if let Ok(v) = std::env::var("WINISLE_SIM_CLICK") {
            if let Some((xs, ys)) = v.split_once(',') {
                if let (Ok(x), Ok(y)) = (xs.trim().parse::<f32>(), ys.trim().parse::<f32>()) {
                    let pos = egui::pos2(x, y);
                    let mods = egui::Modifiers::default();
                    match self.sim_click_stage {
                        0 => { ctx.input_mut(|i| i.events.push(egui::Event::PointerMoved(pos))); self.sim_click_stage = 1; ctx.request_repaint(); }
                        1 => { ctx.input_mut(|i| i.events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: mods })); self.sim_click_stage = 2; ctx.request_repaint(); }
                        2 => { ctx.input_mut(|i| i.events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: mods })); self.sim_click_stage = 3; ctx.request_repaint(); }
                        _ => {}
                    }
                }
            }
        }

        // ?Win11 DWM 
        if !self.settings_shown {
            let h = platform::find_process_window("WinIsle 设置");
            if h != 0 {
                platform::set_round_corners(h);
                platform::reveal_centered(h);
                self.settings_hwnd = h;
                self.settings_shown = true;
            }
        }

        self.focus_settings = false;
        self.confirm_open = confirm_open;
        if do_exit {
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
            return;
        }
        if do_hide {
            self.settings_open = false;
            self.confirm_open = false;
            self.settings_hwnd = 0;
            self.settings_shown = false;
        }
    }
}

impl eframe::App for IslandApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        //  Windows egui ?Visuals::light()?
        // 
        ctx.set_theme(egui::ThemePreference::Dark);
        // WINISLE_PPP=<f> 
        if let Ok(v) = std::env::var("WINISLE_PPP") {
            if let Ok(ppp) = v.trim().parse::<f32>() {
                ctx.set_pixels_per_point(ppp);
            }
        }
        // WINISLE_IDLE_FACE=<n> 
if let Ok(v) = std::env::var("WINISLE_IDLE_FACE") {
            if let Ok(n) = v.trim().parse::<u8>() {
                self.cfg.idle_face = n;
            }
        }
        let _pf_t0 = std::time::Instant::now();
        // [FIX] egui request_repaint_after  delay input.predicted_dt
        //  egui context.rs:187-190eframe  predicted_dt       // 1/60s6.67msegui input.rs:94request_repaint_after(16ms)         // 16-16.67 0  GUI 220fps 1        // predicted_dt delay
        ctx.input_mut(|i| i.predicted_dt = 0.0);
        self.perf_region_ms = 0.0;
        self.perf_netease_ms = 0.0;
        self.poll_update();
        self.poll_weather();
        self.poll_city_search();
        self.ensure_window(ctx, frame);
        if (self.active != Module::Todo || self.show_settings) && !self.todo_debug_edit {
            self.todo_editing = false;
            self.todo_need_focus = false;
            self.todo_was_focused = false;
            // 
            if !self.w_city_search && !self.remind_soft_hover && self.remind_form.is_none() {
                platform::set_activatable_soft(self.hwnd, false);
            }
        }
        let want_activatable = self.w_city_search || self.todo_editing || self.remind_form.is_some();
        if want_activatable != self.w_activatable_last {
            self.w_activatable_last = want_activatable;
            platform::set_activatable(self.hwnd, want_activatable);
            if want_activatable {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        platform::ensure_erase_guard(self.hwnd);
        let _pf_t_win = std::time::Instant::now();
        //          // "double-buffer
        let _gap = self.last_tick.elapsed();
        self.last_tick = std::time::Instant::now();
        // 
        if self.tt_play.is_none() {
            self.tt_play = Some(make_tex(ctx, "tt_play", include_bytes!("../assets/tt_play.rgba"), 540, 540));
            self.tt_pause = Some(make_tex(ctx, "tt_pause", include_bytes!("../assets/tt_pause.rgba"), 540, 540));
        }
        if self.tt_base.is_none() {
            self.tt_base = Some(make_tex(ctx, "tt_base", include_bytes!("../assets/tt_base.rgba"), 540, 540));
            self.tt_arm0 = Some(make_tex(ctx, "tt_arm0", include_bytes!("../assets/tt_arm.rgba"), 540, 540));
        }

        self.refresh_monitor(ctx);
        let _pf_t_a0 = std::time::Instant::now();

        //  SMTC
        let _pf_t_snap0 = std::time::Instant::now();
        self.music = self.tracker.snapshot();
        let _pf_snap_ms = _pf_t_snap0.elapsed().as_secs_f32() * 1000.0;
        // ? SMTC
self.from_smtc = self.music.has_session && !self.music.title.is_empty();
        //  0CDP CDP SMTC        //  SMTC CDP //
let app_l = self.music.app_id.to_ascii_lowercase();
        let cur_is_soda = app_l.contains("汽水") || app_l.contains("soda") || app_l.contains("qishui");
        //  + CDP 205ms
        // 
if self.soda_live_rx.is_none() && self.soda_live_at.elapsed() >= Duration::from_millis(1500) {
            self.soda_live_at = Instant::now();
            let (tx, rx) = std::sync::mpsc::channel();
            self.soda_live_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(crate::soda_patch::soda_running() && crate::soda_patch::cdp_up());
            });
        }
        if let Some(rx) = &self.soda_live_rx {
            match rx.try_recv() {
                Ok(v) => {
                    self.soda_live_cached = v;
                    self.soda_live_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.soda_live_rx = None;
                }
            }
        }
        let soda_live = self.soda_live_cached;
        let soda_ctx = cur_is_soda || (self.music.title.is_empty() && soda_live);
        crate::soda_now::request(soda_ctx);
        if cur_is_soda || self.music.title.is_empty() {
            if let Some((nm, ar, dur, pos, playing, cov)) = crate::soda_now::snapshot() {
                // CDP SMTC 
                //  CDP 
let smtc_has_title = self.from_smtc && !self.music.title.is_empty();
                let stale_name = smtc_has_title && nm.trim() != self.music.title.trim();
                if !nm.trim().is_empty() && !stale_name {
                    self.music.has_session = true;
                    self.music.playing = playing;
                    self.music.title = nm;
                    self.music.artist = ar;
                    if self.music.app_id.is_empty() || cur_is_soda {
                        self.music.app_id = "汽水音乐".to_string();
                    }
                    if dur > 1.0 {
                        self.music.duration = dur;
                    }
                    self.music.position = pos;
                    self.music.has_position = pos > 0.0;
                    self.from_smtc = true;
                    // /
                    if let Some(cb) = cov {
                        use std::hash::{Hash, Hasher};
                        let mut hh = std::collections::hash_map::DefaultHasher::new();
                        cb.as_slice().hash(&mut hh);
                        let h = hh.finish();
                        if h != self.soda_cover_hash || self.cover_tex.is_none() {
                            if let Ok(img) = image::load_from_memory(cb.as_slice()) {
                                let rgba = img.to_rgba8();
                                let (w, hgt) = rgba.dimensions();
                                let ci = egui::ColorImage::from_rgba_unmultiplied(
                                    [w as usize, hgt as usize],
                                    rgba.as_raw(),
                                );
                                self.cover_tex = Some(
                                    ctx.load_texture("cover", ci, egui::TextureOptions::LINEAR),
                                );
                                self.soda_cover_hash = h;
                            }
                        }
                    }
                }
            }
        }
        let _pf_t_a1 = std::time::Instant::now();
        {
            let a = self.music.app_id.to_ascii_lowercase();
            let is_netease = self.music.has_session
                && (a.contains("cloudmusic") || a.contains("netease") || a.contains("网易"));
            platform::set_netease_reader_enabled(is_netease);
        }
        //   CDP ? app.asar / 15s
        if !self.soda_boot_checked && std::env::var_os("WINISLE_NOSODA").is_none() {
            self.soda_boot_checked = true;
            // 5s ?            crate::soda_patch::boot_check();
        }
        if self.last_soda_heal.elapsed() > Duration::from_secs(15)
            && std::env::var_os("WINISLE_NOSODA").is_none()
            && !self
                .soda_tick_busy
                .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            self.last_soda_heal = Instant::now();
            let aid = self.music.app_id.to_lowercase();
            let is_soda = self.music.has_session
                && (aid.contains("汽水") || aid.contains("soda") || aid.contains("qishui"));
            if is_soda {
                let playing = self.music.playing;
                let flag = self.soda_tick_busy.clone();
                std::thread::spawn(move || {
                    crate::soda_patch::tick(playing);
                    flag.store(false, std::sync::atomic::Ordering::Relaxed);
                });
            } else {
                self.soda_tick_busy
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
        }
        // WINISLE_DEMO="|" ?/
        if let Ok(spec) = std::env::var("WINISLE_DEMO") {
            let mut it = spec.splitn(2, '|');
            let t = it.next().unwrap_or("Demo").to_string();
            let ar = it.next().unwrap_or("Demo Artist").to_string();
            self.music.has_session = true;
            self.music.title = t;
            self.music.artist = ar;
            self.music.app_id = std::env::var("WINISLE_DEMO_APP").unwrap_or_else(|_| "cloudmusic.exe".into());
            let demo_paused = std::env::var_os("WINISLE_DEMO_PAUSED").is_some();
            self.music.playing = !demo_paused;
            self.music.position = 42.0;
            self.music.duration = 240.0;
            self.music.has_position = true;
            if let Ok(p) = std::env::var("WINISLE_POS") {
                if let Ok(v) = p.parse::<f64>() {
                    self.music.position = v;
                }
            }
            self.from_smtc = false;
            self.ui_playing = !demo_paused;
        }
        // SMTC  Position /       // positionhaspos=true         // /       // 
if self.music.has_position && self.music.position > 0.0 {
            let rep = self.music.position;
            let prev = self.smtc_rep_pos;
            let rep_changed = (rep - prev).abs() > 0.05;
            if !self.spos_valid {
                self.spos = rep;
                self.spos_at = Instant::now();
                self.spos_valid = true;
                self.smtc_rep_pos = rep;
            } else if rep_changed {
                let cur = self.current_pos();
                let d = rep - cur;
                // =0.5s/                // ?               // 
                let rewound = rep + 0.5 < prev;
                if d > 0.5 || (d < -1.5 && rewound) {
                    self.spos = rep;
                    self.spos_at = Instant::now();
                }
                self.smtc_rep_pos = rep;
            }
            // current_pos() 
        }
        // 
if self.spos_valid && !self.is_playing() {
            self.spos_at = Instant::now();
        }
        //  WINISLE_PDBG=1/
        if std::env::var_os("WINISLE_PDBG").is_some() {
            use std::sync::atomic::{AtomicU64, Ordering};
            static LAST: AtomicU64 = AtomicU64::new(0);
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if now_ms.saturating_sub(LAST.load(Ordering::Relaxed)) >= 1000 {
                LAST.store(now_ms, Ordering::Relaxed);
                use std::io::Write;
                let ci = self.cur_lyric_idx();
                let lt = ci
                    .and_then(|i| self.lyr_lines.get(i))
                    .map(|l| l.t)
                    .unwrap_or(-1.0);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(crate::paths::file("pdbg.log"))
                {
                    let _ = writeln!(
                        f,
                        "pos={:.2} spos={:.2} spos_valid={} since_spos={:.2} playing={} from_smtc={} m.playing={} m.pos={:.2} haspos={} ci={:?} ci_anim={:.2} lyr_t={:.2} nlyr={} title={:?} soda={} app={:?} sprx={} trial={} hint={} clipstart={:.2} clipdur={:.2} full={:.2} trstart={:.2}",
                        self.current_pos(),
                        self.spos,
                        self.spos_valid,
                        self.spos_at.elapsed().as_secs_f64(),
                        self.is_playing(),
                        self.from_smtc,
                        self.music.playing,
                        self.music.position,
                        self.music.has_position,
                        ci,
                        self.lyr_ci_anim,
                        lt,
                        self.lyr_lines.len(),
                        self.music.title,
                        self.app_is_soda(),
                        self.music.app_id,
                        self.soda_prog_rx.is_some(),
                        self.soda_trial_active,
                        self.soda_trial_hint,
                        self.soda_clip_start,
                        self.soda_clip_dur,
                        self.soda_full_dur,
                        self.trial_start,
                    );
                }
            }
        }
        //   SMTC  SMTC -        // EnumWindows + ms
        if self.media_title_rx.is_none()
            && self.music.title.is_empty()
            && !soda_live
            && self.media_title_at.elapsed() >= Duration::from_millis(800)
        {
            self.media_title_at = Instant::now();
            let (tx, rx) = std::sync::mpsc::channel();
            self.media_title_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(platform::media_title_fallback());
            });
        }
        if let Some(rx) = &self.media_title_rx {
            match rx.try_recv() {
                Ok(v) => {
                    self.media_title_cache = v;
                    self.media_title_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.media_title_rx = None;
                }
            }
        }
        if !self.music.title.is_empty() {
            self.media_title_cache = None;
        }
        if self.music.title.is_empty() {
            if let Some((exe, t, a)) = self.media_title_cache.clone() {
                self.music.has_session = true;
                self.music.playing = true;
                self.music.title = t;
                self.music.artist = a;
                self.music.app_id = exe;
                self.spos_valid = false;
            }
        }
        let _pf_t_a2 = std::time::Instant::now();

        if !self.music.title.is_empty() {
            self.last_title = Some((
                self.music.app_id.clone(),
                self.music.title.clone(),
                self.music.artist.clone(),
            ));
            self.last_title_at = Instant::now();
        } else if let Some((exe, t, a)) = self.last_title.clone() {
            if self.music.title.is_empty() && self.player_is_running() {
                self.music.has_session = true;
                self.music.playing = true;
                self.music.title = t;
                self.music.artist = a;
                self.music.app_id = exe;
            }
        }

        if !self.music.title.is_empty() {
            self.music.has_session = true;
        }
        // ~1s        // 2        // 400ms 
let player_running_now = if let Some(ms) = std::env::var("WINISLE_PLAYER_GONE_MS")
            .ok()
            .and_then(|s| s.parse::<u128>().ok())
        {
            // 
static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let t0 = *T0.get_or_init(Instant::now);
            t0.elapsed().as_millis() < ms
        } else {
            self.player_is_running() || std::env::var_os("WINISLE_DEMO").is_some()
        };
        let want_music = !self.music.title.is_empty() && player_running_now;
        let mut music_present = want_music;
        if !want_music {
            //   400ms /           //  music_was_present false             // ?
            if self.music_was_present {
                let since = *self.music_lost_since.get_or_insert_with(Instant::now);
                if since.elapsed() < Duration::from_millis(400) {
                    music_present = true;
                }
            } else {
                self.music_lost_since = None;
            }
        } else {
            self.music_lost_since = None;
        }
        self.music_was_present = music_present;
        {
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            let target_amt = if music_present { 1.0f32 } else { 0.0 };
            let k = 1.0 - (-7.0 * dt).exp();
            self.music_amt += (target_amt - self.music_amt) * k;
            if (self.music_amt - target_amt).abs() < 0.004 {
                self.music_amt = target_amt;
            } else {
                // [FIX] 16ms  request_repaint
                // /~200fps                 // dt  16ms .8s 60fps
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            //  music_amt  0fps            // amt false16.7fps CPU
            self.music_motion = (self.music_amt - target_amt).abs() >= 0.004
                || (self.music_amt > 0.01 && self.is_playing());
            // 
if std::env::var("WINISLE_FACE").is_ok() {
                self.music_amt = 0.0;
            }
            // /
if !music_present && self.music_amt < 0.02 {
                self.lyr_lines.clear();
                self.lyr_key.clear();
                self.lyr_dur = 0.0;
                self.lyr_done = false;
                self.lyr_settled = false;
                self.wave_onset = 0.0;
                self.trial_start = 0.0;
                self.trial_probe = None;
                self.soda_full_dur = 0.0;
                self.soda_clip_start = 0.0;
                self.soda_clip_dur = 0.0;
                self.soda_trial_active = false;
                self.soda_trial_hint = false;
                self.soda_last_p = -1.0;
                self.cover_tex = None;
                self.music = smtc::Track::default();
                self.media_title_cache = None;
                self.last_title = None;
                self.override_play = None;
                self.pause_pending_at = None;
                self.spos_valid = false;
                self.clock_playing = false;
                self.ui_playing = true;
            }
        }
        if std::env::var_os("WINISLE_WDBG").is_some() {
            use std::io::Write;
            static LAST_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if now_ms.saturating_sub(LAST_MS.load(std::sync::atomic::Ordering::Relaxed)) >= 16 {
                LAST_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(crate::paths::file("wdbg.log"))
                {
                    let _ = writeln!(f, "ms={} amt={:.4} w={:.2}", now_ms, self.music_amt, self.collapsed_w());
                }
            }
        }
        //  SMTC        //  SMTC  SMTC
        let smtc_timeline_ok = self.from_smtc
            && self.music.has_position
            && self.music.duration > 20.0
            && self.music.position > 0.0;
        let is_netease_ctx = {
            let a = self.music.app_id.to_ascii_lowercase();
            a.contains("cloudmusic") || a.contains("netease") || a.contains("网易")
        };
        if is_netease_ctx && !smtc_timeline_ok && self.player_running_cached && self.netease_at.elapsed() >= Duration::from_millis(450) {
            self.netease_at = Instant::now();
            let _ne_t = std::time::Instant::now();
            let _ne_res = platform::netease_pos_dur().or_else(platform::netease_posdur_cached);
            self.perf_netease_ms = _ne_t.elapsed().as_secs_f32() * 1000.0;
            // (seek)
            // seek /            // 
let now = std::time::Instant::now();
            let prev_valid = self.spos_valid;
            let prev_raw = self.netease_prev_raw;
            if let Some((p, d)) = _ne_res {
                self.netease_dur = d;
                //  position  0.5~1s                 //  / seek
                let raw_moved = prev_raw < 0.0 || (p - prev_raw).abs() >= 0.02;
                let raw_rewound = prev_raw >= 0.0 && p + 0.5 < prev_raw;
                let raw_jumped_fwd = prev_raw >= 0.0 && p - prev_raw > 2.0;
                if raw_moved {
                    self.netease_prev_raw = p;
                    self.netease_raw_move_at = now;
                }
                //   1.6s                // 450ms                 // 2.30 / 2.31
                if raw_jumped_fwd {
                    // seek
                    } else if raw_moved {
                    self.ui_playing = true;
                    self.music.playing = true;
                    self.clock_playing = true;
                } else if self.netease_raw_move_at.elapsed() > Duration::from_millis(1600) {
                    self.ui_playing = false;
                    self.music.playing = false;
                    self.clock_playing = false;
                }
                // 
                //  -2.0..0.25 /0.01
                if !prev_valid {
                    self.spos = p;
                    self.spos_at = now;
                    self.spos_valid = true;
                } else {
                    let cur = self.current_pos();
                    let diff = p - cur;
                    if diff > 2.0 || raw_rewound {
                        //
                        self.spos = p;
                        self.spos_at = now;
                    } else if diff > 0.25 {
                        // 
self.spos = cur + diff * 0.5;
                        self.spos_at = now;
                    } else if diff < -2.0 {
                        // 
self.spos = p;
                        self.spos_at = now;
                    }
                    self.spos_valid = true;
                }
                if self.music.app_id.is_empty() {
                    self.music.app_id = "cloudmusic.exe".to_string();
                }
            } else if self.music.app_id.to_ascii_lowercase().contains("cloudmusic") {
                if let Some(f) = platform::netease_frac_cached() {
                    let d = self.duration_est();
                    if d > 1.0 {
                        self.spos = f * d;
                        self.spos_at = Instant::now();
                        self.spos_valid = true;
                    }
                }
            }
        }
        // SMTC position / playing position playing 
        // `.slider-fill`
        // SMTC from_smtc
if self.app_is_soda() {
            if self.soda_pos_at.elapsed() >= Duration::from_millis(400)
                && self.soda_prog_rx.is_none()
                && std::env::var_os("WINISLE_NOSODAPROG").is_none()
            {
                self.soda_pos_at = Instant::now();
                let dur = if self.music.duration > 20.0 {
                    self.music.duration
                } else {
                    self.duration_est()
                };
                let (tx, rx) = std::sync::mpsc::channel();
                self.soda_prog_rx = Some(rx);
                std::thread::spawn(move || {
                    let pr = crate::cdp_lyric::fetch_progress(dur);
                    let pl = crate::cdp_lyric::fetch_play_state().map(|(p, _, _)| p);
                    let tm = crate::cdp_lyric::fetch_trial_meta();
                    let _ = tx.send((pr, pl, tm));
                });
            }
            let recv = self.soda_prog_rx.as_ref().map(|rx| rx.try_recv());
            match recv {
                Some(Ok((pr, pl, tm))) => {
                    self.soda_prog_rx = None;
                    if let Some(m) = tm {
                        //  track/
let a = m.track_name.trim();
                        let b = self.music.title.trim();
                        let same_track = a.is_empty()
                            || b.is_empty()
                            || a == b
                            || a.contains(b)
                            || b.contains(a);
                        if same_track {
                            self.soda_full_dur = m.full_duration;
                            self.soda_clip_start = m.clip_start;
                            self.soda_clip_dur = m.clip_duration;
                            self.soda_trial_hint = m.limited_free;
                        }
                    }
                    let prev_pl = self.soda_playing;
                    let mut force_sync = false;
                    // 3 ?
                    // ?
let pause_pending = self
                        .pause_pending_at
                        .map(|t| t.elapsed() < Duration::from_secs(3))
                        .unwrap_or(false);
                    if let Some(p) = pl {
                        if prev_pl != Some(p) {
                            force_sync = true; // "播��/暂停切换时精硬?齐一"
                            }
                        self.soda_playing = Some(p);
                        //  music.playing UI /
self.music.playing = p;
                    }
                    if let Some((p, tot, _age)) = pr {
                        // DOM age 
                        //  p + age1.0 
let p_now = p.min(tot.max(1.0));
                        self.soda_dur = tot;
                        //  30s  track.duration                        //  trial_start
let full = if self.soda_full_dur > 1.0 {
                            self.soda_full_dur
                        } else if self.music.duration > 1.0 {
                            self.music.duration
                        } else {
                            0.0
                        };
                        let is_trial = self.soda_trial_hint
                            && full > 30.0
                            && self.soda_clip_dur >= 10.0
                            && self.soda_clip_dur < full * 0.95;
                        self.soda_trial_active = is_trial;
                        if is_trial {
                            self.trial_start = self.soda_clip_start.max(0.0) as f32;
                        } else {
                            self.trial_start = 0.0;
                        }
                        //  DOM 1 fetch_progress 400ms ?                        // ?2~3 
                        //  1~2                         //
                        let changed = (p - self.soda_last_p).abs() > 0.002;
                        self.soda_last_p = p;
                        let cur = self.current_pos();
                        let diff = p_now - cur;
                        if self.soda_playing == Some(false) {
                            // 
                            // 2:31 2:30:30/2:31 
self.spos = p;
                            self.spos_at = Instant::now();
                            self.spos_valid = true;
                            self.pause_pending_at = None;
                        } else if pause_pending {
                            // ??
                            } else if force_sync || diff.abs() > 2.0 {
                            // / ?
self.spos = p_now;
                            self.spos_at = Instant::now();
                            self.spos_valid = true;
                        } else if changed {
                            // 1 p+age                            //  400ms                             // 
self.spos = p_now.max(cur);
                            self.spos_at = Instant::now();
                            self.spos_valid = true;
                        }
                    }
                }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    self.soda_prog_rx = None;
                }
                Some(Err(std::sync::mpsc::TryRecvError::Empty)) | None => {}
            }
        }        // VIP "试听片漫起点探测：播放时"<60s"（试后��段）时，托��网易云内存日志里"

        if self.app_is_soda() {
            // soda 
self.trial_probe = None;
        } else if let Some((at, n)) = self.trial_probe {
            if self.music.title.is_empty() || self.netease_dur >= 60.0 {
                self.trial_probe = None;
                self.trial_start = 0.0;
            } else if at.elapsed() >= Duration::from_millis(700) {
                if let Some(s) = platform::netease_trial_start_since(&self.trial_since) {
                    self.trial_start = s;
                    self.trial_probe = None;
                } else if n >= 6 {
                    self.trial_probe = None;
                } else {
                    self.trial_probe = Some((Instant::now(), n + 1));
                }
            }
        }
        //         // / CDP 
        let title_now = self.music.title.clone();
        let key = title_now.clone();
        if !key.is_empty() && key != self.lyr_key {
            self.lyr_key = key.clone();
            self.lyr_idx_last = -1;
            // freeTrialInfo            // ?            // since 
let first_song = self.trial_since.is_empty();
            self.trial_start = 0.0;
            self.soda_full_dur = 0.0;
            self.soda_clip_start = 0.0;
            self.soda_clip_dur = 0.0;
            self.soda_trial_active = false;
            self.soda_trial_hint = false;
            self.soda_last_p = -1.0;
            self.trial_since = if first_song {
                String::new()
            } else {
                platform::local_stamp()
            };
            self.trial_probe = Some((Instant::now(), 0));
            self.lyr_lines.clear();
            self.lyr_done = false;
            self.lyr_settled = false;
            self.lyr_retry_n = 0;
            self.lyr_retry_at = Instant::now() - Duration::from_secs(10);
            self.wave_onset = 0.0;
            self.lyr_inflight = false;
            self.cover_tex = None;
            self.pending_title = None;
            self.netease_dur = 0.0;

            if self.app_is_soda() {
                self.spos_valid = self.music.has_position && self.music.position > 0.0;
                self.spos = self.music.position;
                self.spos_at = Instant::now();
            } else if let Some((p, d)) = platform::netease_pos_dur() {
                self.netease_dur = d;
                self.spos = p;
                self.spos_at = Instant::now();
                self.spos_valid = true;
            } else {
                self.spos_valid = self.music.has_position && self.music.position > 0.0;
                self.spos = self.music.position;
                self.spos_at = Instant::now();
            }
            self.clock_base = if self.music.position > 0.5 { self.music.position } else { 0.0 };
            self.smtc_rep_pos = if self.music.has_position { self.music.position } else { -1.0 };
            self.clock_start = Instant::now();
            //  position/playing
self.clock_playing = if !self.music.title.is_empty() { true } else { self.is_playing() };
            self.prev_playing = self.clock_playing;
            self.ui_playing = true;
            if !self.music.title.is_empty() {
                let dur_now = self.lyric_duration();
                self.lyr_dur = dur_now;
                let is_soda = {
                    let a = self.music.app_id.to_ascii_lowercase();
                    a.contains("汽水") || a.contains("soda") || a.contains("qishui")
                };
                self.lyr_inflight = true;
                let _ = self.lyr_tx.send((key.clone(), self.music.title.clone(), self.music.artist.clone(), dur_now, is_soda, false));
            }
        } else {



            if !self.music.title.is_empty()
                && self.netease_dur >= 60.0
                && !self.lyr_lines.is_empty()
                && (self.lyr_dur < 20.0 || (self.netease_dur - self.lyr_dur).abs() > 4.0)
            {
                self.lyr_dur = self.netease_dur;
                let is_soda = {
                    let a = self.music.app_id.to_ascii_lowercase();
                    a.contains("汽水") || a.contains("soda") || a.contains("qishui")
                };
                let _ = self.lyr_tx.send((
                    key.clone(),
                    self.music.title.clone(),
                    self.music.artist.clone(),
                    self.netease_dur,
                    is_soda,
                    false,
                ));
            }
            // position ?            //  SMTC position/playing
let playing_eff = if let Some((v, tt)) = self.override_play {
                if Instant::now() < tt {
                    v
                } else if self.spos_valid {
                    self.is_playing()
                } else {
                    !self.music.title.is_empty()
                }
            } else if self.spos_valid {
                self.is_playing()
            } else {
                !self.music.title.is_empty()
            };
            if playing_eff != self.prev_playing {
                let cur = self.current_pos();
                self.clock_base = cur;
                self.clock_start = Instant::now();
                self.clock_playing = playing_eff;
                self.prev_playing = playing_eff;
            }
            // SMTC  0
            if self.music.position > 0.5 && (self.current_pos() - self.music.position).abs() > 2.5 {
                self.clock_base = self.music.position;
                self.clock_start = Instant::now();
            }
        }
        // 
        if !self.lyr_settled
            && !self.music.title.is_empty()
            && self.lyr_lines.is_empty()
            && !self.lyr_inflight
            && self.lyr_retry_n < 7
        {
            let a = self.music.app_id.to_ascii_lowercase();
            if a.contains("汽水") || a.contains("soda") || a.contains("qishui") {
                let delay_ms: u64 = match self.lyr_retry_n {
                    0 => 2000,
                    1 => 3000,
                    2 => 5000,
                    3 => 8000,
                    4 => 13000,
                    5 => 21000,
                    _ => 34000,
                };
                if self.lyr_retry_at.elapsed() > Duration::from_millis(delay_ms) {
                    self.lyr_retry_at = Instant::now();
                    self.lyr_retry_n += 1;
                    self.lyr_done = false;
                    let dur_now = self.lyric_duration();
                    self.lyr_inflight = true;
                    //  CDP /
                    let _ = self.lyr_tx.send((
                        self.lyr_key.clone(),
                        self.music.title.clone(),
                        self.music.artist.clone(),
                        dur_now,
                        true,
                        true,
                    ));
                }
            }
        }
        // ?
if !self.lyr_settled
            && !self.lyr_inflight
            && self.lyr_retry_n >= 7
            && self.lyr_lines.is_empty()
            && !self.music.title.is_empty()
            && !self.lyr_done
        {
            self.lyr_done = true;
            self.lyr_settled = true;
        }        // "收集歌词后台线程结果"
        while let Ok((k, opt, cov, settled)) = self.lyr_rx.try_recv() {
            self.lyr_inflight = false;
            let same = k == self.lyr_key;
            if same {
                let got = opt.as_ref().map(|v| !v.is_empty()).unwrap_or(false);
                if got || settled {
                    self.lyr_done = true;
                }
                if settled {
                    self.lyr_settled = true;
                }
            }
            if same || self.lyr_lines.is_empty() {
                self.lyr_lines = opt.unwrap_or_default();
                self.lyr_drag_off = 0.0;
                self.lyr_scroll_target = 0.0;
                self.lyr_ci_anim = -1.0;
                self.lyr_release_at = None;
                self.lyr_return_from = 0.0;
                self.lyr_return_start = None;
                if !same {
                    self.lyr_key = k.clone();
                }
            }
            if let Some(bytes) = cov {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
                    self.cover_tex = Some(ctx.load_texture("cover", ci, egui::TextureOptions::LINEAR));
                }
            }
        }

        //  WINISLE_FAKE_LYRICS=1WINISLE_DEMO        // ?
        if std::env::var_os("WINISLE_FAKE_LYRICS").is_some() && self.lyr_lines.len() < 2 {
            let demo = [
                "第一，风继续吹不忍远离",
                "第二，心里极渴望希望留下伴着你",
                "第三，风继续吹不忍远离",
                "第四，心里亦有泪不愿流泪望着你",
                "第五，过去多少快乐记忆",
                "第六，何妨与你一起去追",
                "第七，要将忧郁苦痛洗去",
                "第八，柔情蜜意我愿记取",
            ];
            let mut v = Vec::new();
            for (i, s) in demo.iter().enumerate() {
                v.push(lyrics::LrcLine {
                    t: 30.0 + i as f64 * 2.5,
                    text: (*s).to_string(),
                    segs: Vec::new(),
                });
            }
            self.lyr_lines = v;
        }

        // WINISLE_LONG=" "
        if let Ok(long) = std::env::var("WINISLE_LONG") {
            let changed = self.lyr_lines.len() != 1 || self.lyr_lines.first().map(|l| l.text != long).unwrap_or(true);
            if changed {
                self.lyr_lines = vec![lyrics::LrcLine { t: 0.0, text: long, segs: Vec::new() }];
                self.lyr_idx_last = -1;
            }
            self.lyr_key = self.music.title.clone();
        }



        let _pf_t_m2 = std::time::Instant::now();
        let _pf_t_m3 = std::time::Instant::now();
        {
            let cur_idx = self.cur_lyric_idx().map(|i| i as i32).unwrap_or(-1);
            if cur_idx != self.lyr_idx_last {
                let prev = self.lyr_idx_last;
                let valid_prev =
                    prev >= 0 && (prev as usize) < self.lyr_lines.len() && cur_idx >= 0;
                if valid_prev {
                    self.lyr_anim_at = Some(Instant::now());
                    self.lyr_anim_from = Some(prev as usize);
                } else {
                    self.lyr_anim_at = None;
                    self.lyr_anim_from = None;
                }
                self.lyr_idx_last = cur_idx;
            }

            if let Some(at) = self.lyr_anim_at {
                if at.elapsed() > Duration::from_millis(700) {
                    self.lyr_anim_at = None;
                    self.lyr_anim_from = None;
                }
            }
        }

        // 
if let Some(t) = &self.tray {
            match t.poll() {
                Some(TrayAction::Exit) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                None => {}
            }
        }

        // ?
        if let Some(t) = trigger_mtime() {
            if self.last_trigger.map_or(true, |lt| t > lt) {
                self.last_trigger = Some(t);
            }
        }

        let _pf_t_state = std::time::Instant::now();
        self.handle_drag(ctx);
        self.drive_animation(ctx);
        self.step_cb_anim(ctx);
        self.step_reminders(ctx);
        // + 
if self.exit_start.is_none()
            && crate::tray::QUIT_REQUESTED.load(std::sync::atomic::Ordering::SeqCst)
        {
            self.exit_start = Some(Instant::now());
            ctx.request_repaint();
        }
        if let Some(t0) = self.exit_start {
            let p = (t0.elapsed().as_secs_f32() / 0.18).clamp(0.0, 1.0);
            if p >= 1.0 {
                std::process::exit(0);
            }
            ctx.request_repaint();
        }
        // WINISLE_PAGE
if let Some(p) = self.dbg_page {
            if self.expanded > 0.9 {
                match p {
                    DbgView::Settings => self.show_settings = true,
                    DbgView::Schedule => self.active = Module::Schedule,
                    DbgView::Todo => self.active = Module::Todo,
                    DbgView::Reminder => self.active = Module::Reminder,
                    DbgView::Weather => self.active = Module::Weather,
                }
            }
        }
        //  
if self.dbg_page.is_none() && self.target < 0.5 && self.expanded < 0.02 {
            let ai = self.active.rail_idx();
            self.last_active = self.active;
            self.rail_from = ai;
            self.rail_anim_at = None;
            self.lyr_drag_off = 0.0;
            self.lyr_scroll_target = 0.0;
            self.lyr_ci_anim = -1.0;
            self.lyr_release_at = None;
            self.lyr_return_from = 0.0;
            self.lyr_return_start = None;
            // 
            self.todo_editing = false;
            self.todo_need_focus = false;
            self.todo_was_focused = false;
            if !self.w_city_search {
                platform::set_activatable_soft(self.hwnd, false);
            }
        }
        // //
self.full = self.island_visible
            && (self.expanded > 0.001 || self.target > 0.5 || self.settle.is_some() || self.dragging);
        if self.settle.is_some() {
            self.step_settle(ctx);
        } else if !self.dragging {
            self.apply_geometry(ctx);
        }
        // "+ " SetWindowRgn 
        // PAD)?egui 
        // ?        //  -> surface -> 
        self.apply_region(ctx);
        // /
self.set_hit_region(ctx);

        //  cloak  + apply_geometry loak        // cloak  cloak         // 
if self.cloaked {
            self.cloak_frames = self.cloak_frames.saturating_add(1);
            if self.cloak_frames >= 8 {
                platform::set_cloak(self.hwnd, false);
                self.cloaked = false;
                self.intro_start = Some(Instant::now());
                self.intro_done = false;
            }
        }

        let _pf_t_pre = std::time::Instant::now();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| self.draw(ui));
        let _pf_t_draw = std::time::Instant::now();
        self.settings_ui(ctx);
        let _pf_t_post = std::time::Instant::now();

        // intro_dy 0.34s
if !self.intro_done {
            if let Some(t0) = self.intro_start {
                if t0.elapsed().as_secs_f32() >= 0.34 {
                    self.intro_done = true;
                }
            }
        }

        //  120fps 10fps        //
        let pointer_active = ctx.input(|i| i.pointer.has_pointer() || i.pointer.any_down());
        //         // expanded>0.01 60fps        // allow idle  CPU 600ms <0.6s
        let busy = !self.intro_done
            || self.cloaked
            || self.exit_start.is_some()
            || self.intro_dy() != 0.0
            || (self.cb_anim > 0.002 && self.cb_anim < 0.998)
            || (self.expanded - self.target).abs() > 0.002
            || self.music_motion
            || (self.expanded > 0.99 && self.cfg.water_spectrum && self.wave_active)
            || self.dragging
            || self.settle.is_some()
            || self.upd_ver.is_some()
            || self.upd_checking
            || self.upd_busy
            || pointer_active;
        if busy {
            self.last_busy = Instant::now();
        }
        let animating = (busy || self.last_busy.elapsed() < Duration::from_millis(600))
            && std::env::var_os("WINISLE_IDLE").is_none();
        // 16ms 60fps0fps00ms 5fps       //  8ms 125fps+  1 CPU  60fps 
let target_ms: u64 = if animating { 16 } else { 200 };
        if std::env::var_os("WINISLE_PERF").is_some() {
            let el = _pf_t0.elapsed().as_secs_f32() * 1000.0;
            let (dt, udt) = ctx.input(|i| (i.stable_dt, i.unstable_dt));
            if el > 12.0 || dt > 0.020 {
                use std::io::Write as _;
                let line = format!(
                    "{} update={:.1}ms (win={:.1} state={:.1} a0={:.1} a1={:.1} a2={:.1} m1={:.1} m2={:.1} m3={:.1} post={:.1} draw={:.1}) snap={:.1} player={:.1} ne={:.1} region={:.1}ms stable_dt={:.1}ms exp={:.3} tgt={:.3} amt={:.3} playing={}\n",
                    chrono_like_now(),
                    el,
                    (_pf_t_win - _pf_t0).as_secs_f32() * 1000.0,
                    (_pf_t_state - _pf_t_win).as_secs_f32() * 1000.0,
                    (_pf_t_a0 - _pf_t_win).as_secs_f32() * 1000.0,
                    (_pf_t_a1 - _pf_t_a0).as_secs_f32() * 1000.0,
                    (_pf_t_a2 - _pf_t_a1).as_secs_f32() * 1000.0,
                    0.0,
                    0.0,
                    (_pf_t_m3 - _pf_t_m2).as_secs_f32() * 1000.0,
                    (_pf_t_pre - _pf_t_state).as_secs_f32() * 1000.0,
                    (_pf_t_draw - _pf_t_pre).as_secs_f32() * 1000.0,
                    _pf_snap_ms,
                    0.0,
                    self.perf_netease_ms,
                    self.perf_region_ms,
                    dt * 1000.0,
                    self.expanded,
                    self.target,
                    self.music_amt,
                    self.music.playing,
                );
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(crate::paths::file("perf.log"))
                {
                    let _ = f.write_all(line.as_bytes());
                }
            }
        }
        // [DIAG] WINISLE_PERF  perf.log FPS
        //  vsync 60? request_repaint
        let _diag_interval_ms = self.frame_last.elapsed().as_secs_f32() * 1000.0;
        let _diag_work_ms = _pf_t0.elapsed().as_secs_f32() * 1000.0;
        if std::env::var_os("WINISLE_PERF").is_some() {
            use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
            static FRAMES: AtomicU32 = AtomicU32::new(0);
            static LAST_MS: AtomicU64 = AtomicU64::new(0);
            FRAMES.fetch_add(1, Ordering::Relaxed);
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let last = LAST_MS.load(Ordering::Relaxed);
            if last == 0 {
                LAST_MS.store(now_ms, Ordering::Relaxed);
            } else if now_ms.saturating_sub(last) >= 1000 {
                let fr = FRAMES.swap(0, Ordering::Relaxed);
                LAST_MS.store(now_ms, Ordering::Relaxed);
                use std::io::Write as _;
                if let Ok(mut fh) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(crate::paths::file("perf.log"))
                {
                    let causes: Vec<String> = ctx
                        .repaint_causes()
                        .iter()
                        .map(|c| format!("{}:{}", c.file, c.line))
                        .collect();
                    let _ = writeln!(
                        fh,
                        "FPS={} interval={:.1}ms work={:.1}ms animating={} exp={:.3} amt={:.3} playing={} causes={:?}",
                        fr, _diag_interval_ms, _diag_work_ms, animating, self.expanded, self.music_amt, self.music.playing, causes
                    );
                }
            }
        }
        // (_pf_t0 )        // 
        // +  1 frame_last         // interval) (prev_req_ms)        //  egui  + GPU swap        // target_ms 16ms0fpspredicted_dt request_repaint_after         // egui 16.67ms       // predicted_dt request_repaint_after         // (target_ms work)         // target_ms >60
        let req_ms = ((target_ms as f32) - _diag_work_ms).max(1.0);
        self.prev_req_ms = req_ms;
        self.frame_last = Instant::now();
        ctx.request_repaint_after(Duration::from_secs_f32(req_ms / 1000.0));
    }
}

fn chrono_like_now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let s = d.as_secs() % 86400;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600, (s % 3600) / 60, s % 60, d.subsec_millis())
}

/// UV /// `gap_reduce`gap_reduce/2 
/// gap_reduce 
fn draw_face_img_cropped(
    painter: &egui::Painter,
    c: egui::Pos2,
    w: f32,
    h: f32,
    a: u8,
    tx: &egui::TextureHandle,
    uv: (f32, f32, f32, f32),
    gap_reduce: f32,
) {
    if a == 0 || w <= 1.0 || h <= 1.0 {
        return;
    }
    let full = egui::Rect::from_center_size(c, egui::vec2(w, h));
    let col = egui::Color32::from_rgba_unmultiplied(255, 255, 255, a);
    if gap_reduce.abs() < 0.01 {
        painter.image(
            tx.id(),
            full,
            egui::Rect::from_min_max(egui::pos2(uv.0, uv.1), egui::pos2(uv.2, uv.3)),
            col,
        );
        return;
    }
    let um = (uv.0 + uv.2) * 0.5;
    let half = gap_reduce * 0.5;
    let lw = w * (um - uv.0) / (uv.2 - uv.0).max(1e-4);
    let rw = w * (uv.2 - um) / (uv.2 - uv.0).max(1e-4);
    let left_rect = egui::Rect::from_min_size(
        egui::pos2(full.left() + half, full.top()),
        egui::vec2(lw, h),
    );
    let right_rect = egui::Rect::from_min_size(
        egui::pos2(full.right() - half - rw, full.top()),
        egui::vec2(rw, h),
    );
    painter.image(
        tx.id(),
        left_rect,
        egui::Rect::from_min_max(egui::pos2(uv.0, uv.1), egui::pos2(um, uv.3)),
        col,
    );
    painter.image(
        tx.id(),
        right_rect,
        egui::Rect::from_min_max(egui::pos2(um, uv.1), egui::pos2(uv.2, uv.3)),
        col,
    );
}

fn draw_robot_face(painter: &egui::Painter, c: egui::Pos2, h: f32, a: u8, expr: u8, t: f64, tex: Option<&egui::TextureHandle>) {
    if a == 0 || h <= 1.0 {
        return;
    }
    let bob = ((t * 1.7).sin() as f32) * h * 0.028;
    //  1:1  Otto + 
if let Some(tx) = tex {
        let s = tx.size_vec2();
        let fw = h * (s.x / s.y.max(1.0));
        let center = egui::pos2(c.x, c.y);
        let rect = egui::Rect::from_center_size(center, egui::vec2(fw, h));
        painter.image(
            tx.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, a),
        );
        return;
    }
    let pi = std::f32::consts::PI;
    let col = egui::Color32::from_rgba_unmultiplied(255, 255, 255, a);
    let cy = c.y + bob;
    let eye_dx = h * 0.26;
    let eye_cy = cy - h * 0.10;
    let eye_w = h * 0.185;
    let eye_h = h * 0.26;
    let lw = (h * 0.085).max(1.4);
    let bp = ((t * 0.30) % 1.0) as f32;
    let blink = bp > 0.94;
    let my = cy + h * 0.24;

    // w*0.5 
let round_open = |dx: f32, w: f32, hh: f32| {
        if blink {
            let r = egui::Rect::from_center_size(
                egui::pos2(c.x + dx, eye_cy),
                egui::vec2(w * 1.9, (h * 0.10).max(1.5)),
            );
            painter.rect_filled(r, (h * 0.05).max(1.0), col);
        } else {
            let r = egui::Rect::from_center_size(egui::pos2(c.x + dx, eye_cy), egui::vec2(w, hh));
            painter.rect_filled(r, w * 0.5, col);
        }
    };
    // round_top=true (false ()
    let half_eye = |dx: f32, w: f32, hh: f32, round_top: bool| {
        let n = 18;
        let mut pts: Vec<egui::Pos2> = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let ang = pi * (i as f32) / (n as f32);
            let x = c.x + dx + w * 0.5 * ang.cos();
            let sy = if round_top { 1.0 } else { -1.0 };
            let y = eye_cy - hh * ang.sin() * sy;
            pts.push(egui::pos2(x, y));
        }
        painter.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
    };
    // 
    let ellipse = |cx: f32, cyy: f32, w: f32, hh: f32, ang: f32| {
        let n = 22;
        let mut pts: Vec<egui::Pos2> = Vec::with_capacity(n);
        for i in 0..n {
            let a2 = 2.0 * pi * (i as f32) / (n as f32);
            let x0 = w * 0.5 * a2.cos();
            let y0 = hh * 0.5 * a2.sin();
            let xr = x0 * ang.cos() - y0 * ang.sin();
            let yr = x0 * ang.sin() + y0 * ang.cos();
            pts.push(egui::pos2(cx + xr, cyy + yr));
        }
        painter.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
    };
    // + 
    let heart = |cx: f32, cyy: f32, s: f32| {
        let r = s * 0.30;
        painter.circle_filled(egui::pos2(cx - s * 0.24, cyy - s * 0.16), r, col);
        painter.circle_filled(egui::pos2(cx + s * 0.24, cyy - s * 0.16), r, col);
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(cx - s * 0.52, cyy - s * 0.02),
                egui::pos2(cx + s * 0.52, cyy - s * 0.02),
                egui::pos2(cx, cyy + s * 0.58),
            ],
            col,
            egui::Stroke::NONE,
        ));
    };
    // depth>0 (U)depth<0 (ycen 
let arc = |w: f32, depth: f32, ycen: f32| {
        let n = 14;
        let mut pts = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let fx = i as f32 / n as f32;
            let x = -w * 0.5 + w * fx;
            let u = (x / (w * 0.5)).clamp(-1.0, 1.0);
            let y = depth * (1.0 - u * u);
            pts.push(egui::pos2(c.x + x, ycen + y));
        }
        painter.add(egui::Shape::line(pts, egui::Stroke::new(lw, col)));
    };
    let flat_mouth = |w: f32| {
        let r = egui::Rect::from_center_size(
            egui::pos2(c.x, my),
            egui::vec2(w, (h * 0.09).max(1.4)),
        );
        painter.rect_filled(r, h * 0.045, col);
    };
    // up=true  
let closed_eye_line = |dx: f32, w: f32, up: bool| {
        let n = 12;
        let mut pts = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let fx = i as f32 / n as f32;
            let x = -w * 0.5 + w * fx;
            let u = (x / (w * 0.5)).clamp(-1.0, 1.0);
            let y = (1.0 - u * u) * h * 0.10 * if up { -1.0 } else { 1.0 };
            pts.push(egui::pos2(c.x + dx + x, eye_cy + y));
        }
        painter.add(egui::Shape::line(pts, egui::Stroke::new(lw, col)));
    };

    match expr {
        // 0 + 
        0 => {
            round_open(-eye_dx, eye_w, eye_h);
            round_open(eye_dx, eye_w, eye_h);
            flat_mouth(h * 0.30);
        }
        // 1 
        1 => {
            half_eye(-eye_dx, eye_w * 1.5, eye_h * 0.95, true);
            half_eye(eye_dx, eye_w * 1.5, eye_h * 0.95, true);
            arc(h * 0.54, h * 0.25, my);
        }
        // 2  
        2 => {
            half_eye(-eye_dx, eye_w * 1.4, eye_h * 0.82, false);
            half_eye(eye_dx, eye_w * 1.4, eye_h * 0.82, false);
            arc(h * 0.40, -h * 0.12, my + h * 0.03);
        }
        // 3 +  + 
        3 => {
            ellipse(c.x - eye_dx, eye_cy, eye_w * 1.5, eye_h * 0.78, 0.5);
            ellipse(c.x + eye_dx, eye_cy, eye_w * 1.5, eye_h * 0.78, -0.5);
            let gx = c.x + eye_dx + eye_w * 1.5;
            let gy = eye_cy - eye_h * 1.15;
            let s = h * 0.05;
            painter.add(egui::Shape::line(
                vec![egui::pos2(gx - s, gy - s), egui::pos2(gx + s, gy + s)],
                egui::Stroke::new(lw * 0.7, col),
            ));
            painter.add(egui::Shape::line(
                vec![egui::pos2(gx + s, gy - s), egui::pos2(gx - s, gy + s)],
                egui::Stroke::new(lw * 0.7, col),
            ));
            flat_mouth(h * 0.26);
        }
        // 4 +  + 
        4 => {
            half_eye(-eye_dx, eye_w * 1.3, eye_h * 0.78, false);
            half_eye(eye_dx, eye_w * 1.3, eye_h * 0.78, false);
            for side in [-1.0f32, 1.0] {
                let tx = c.x + side * eye_dx;
                let mut yv = eye_cy + eye_h * 0.85;
                for _ in 0..3 {
                    painter.add(egui::Shape::line(
                        vec![egui::pos2(tx, yv), egui::pos2(tx, yv + h * 0.06)],
                        egui::Stroke::new(lw * 0.7, col),
                    ));
                    yv += h * 0.11;
                }
                painter.circle_filled(egui::pos2(tx, yv + h * 0.01), h * 0.035, col);
            }
            arc(h * 0.38, -h * 0.11, my + h * 0.04);
        }
        // 5 + 
        5 => {
            heart(c.x - eye_dx, eye_cy, eye_w * 2.5);
            heart(c.x + eye_dx, eye_cy, eye_w * 2.5);
            arc(h * 0.44, h * 0.14, my);
        }
        // 6 +  +  + 
        6 => {
            round_open(-eye_dx, eye_w, eye_h);
            round_open(eye_dx, eye_w, eye_h);
            for side in [-1.0f32, 1.0] {
                let bx = c.x + side * (eye_dx + eye_w * 0.95);
                for k in 0..3 {
                    let o = (k as f32 - 1.0) * h * 0.05;
                    painter.add(egui::Shape::line(
                        vec![
                            egui::pos2(bx - h * 0.04, eye_cy + eye_h * 0.55 + o + h * 0.04),
                            egui::pos2(bx + h * 0.02, eye_cy + eye_h * 0.55 + o - h * 0.02),
                        ],
                        egui::Stroke::new(lw * 0.6, col),
                    ));
                }
            }
            let sx = c.x + eye_dx + eye_w * 0.8;
            let sy = eye_cy - eye_h * 1.1;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(sx, sy - h * 0.07),
                    egui::pos2(sx - h * 0.04, sy + h * 0.02),
                    egui::pos2(sx + h * 0.04, sy + h * 0.02),
                ],
                col,
                egui::Stroke::NONE,
            ));
            painter.circle_filled(egui::pos2(sx, sy + h * 0.03), h * 0.04, col);
            arc(h * 0.40, h * 0.16, my);
        }
        // 7  + 
        7 => {
            round_open(-eye_dx, eye_w * 1.2, eye_h);
            painter.circle_filled(
                egui::pos2(c.x + eye_dx + h * 0.03, eye_cy - h * 0.04),
                eye_w * 0.7,
                col,
            );
            flat_mouth(h * 0.24);
        }
        // 8  + + 
        8 => {
            closed_eye_line(-eye_dx, eye_w * 1.7, true);
            round_open(eye_dx, eye_w, eye_h);
            arc(h * 0.42, h * 0.15, my);
        }
        // 9  + 
        9 => {
            let lens_w = eye_w * 1.7;
            let lens_h = eye_h * 0.95;
            for side in [-1.0f32, 1.0] {
                let r = egui::Rect::from_center_size(
                    egui::pos2(c.x + side * eye_dx, eye_cy),
                    egui::vec2(lens_w, lens_h),
                );
                painter.rect_filled(r, h * 0.055, col);
            }
            let br = egui::Rect::from_center_size(
                egui::pos2(c.x, eye_cy - lens_h * 0.18),
                egui::vec2((eye_dx * 2.0 - lens_w).max(1.0), h * 0.05),
            );
            painter.rect_filled(br, h * 0.02, col);
            arc(h * 0.30, h * 0.07, my);
        }
        // 10 +  + 
        10 => {
            closed_eye_line(-eye_dx, eye_w * 1.7, true);
            closed_eye_line(eye_dx, eye_w * 1.7, true);
            painter.circle_filled(egui::pos2(c.x, my), h * 0.055, col);
            heart(c.x + h * 0.30, my - h * 0.02, h * 0.18);
        }
        // 11  + 
        11 => {
            half_eye(-eye_dx, eye_w * 1.5, eye_h * 0.55, false);
            half_eye(eye_dx, eye_w * 1.5, eye_h * 0.55, false);
            arc(h * 0.36, h * 0.12, my);
        }
        // 12 + Zz
        12 => {
            closed_eye_line(-eye_dx, eye_w * 1.8, false);
            closed_eye_line(eye_dx, eye_w * 1.8, false);
            painter.text(
                egui::pos2(c.x + eye_dx + eye_w * 0.6, eye_cy - h * 0.28),
                egui::Align2::LEFT_CENTER,
                "z",
                egui::FontId::proportional((h * 0.20).max(7.0)),
                col,
            );
            painter.text(
                egui::pos2(c.x + eye_dx + eye_w * 1.5, eye_cy - h * 0.44),
                egui::Align2::LEFT_CENTER,
                "z",
                egui::FontId::proportional((h * 0.15).max(6.0)),
                col,
            );
        }

        _ => {
            round_open(-eye_dx, eye_w * 1.2, eye_h);
            painter.circle_filled(
                egui::pos2(c.x + eye_dx + h * 0.03, eye_cy + h * 0.02),
                eye_w * 0.7,
                col,
            );
            painter.text(
                egui::pos2(c.x + eye_dx + eye_w * 0.4, eye_cy - eye_h * 1.25),
                egui::Align2::LEFT_CENTER,
                "?",
                egui::FontId::proportional((h * 0.24).max(7.0)),
                col,
            );
            flat_mouth(h * 0.24);
        }
    }
}


fn aqi_color(q: &str, a: u8) -> egui::Color32 {
    let (r, g, b) = if q.contains("") {
        (52, 211, 153)
    } else if q.contains("") {
        (176, 208, 82)
    } else if q.contains("轻度") {
        (238, 198, 74)
    } else if q.contains("一下") {
        (238, 150, 60)
    } else if q.contains("重度") || q.contains("严重") {
        (230, 92, 82)
    } else {
        (150, 158, 175)
    };
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}


fn draw_weather_icon(p: &egui::Painter, sky: crate::weather::Sky, c: egui::Pos2, s: f32, a: u8, t: f64, bg: egui::Color32) {
    use crate::weather::Sky;
    let mk = |r: u8, g: u8, b: u8| egui::Color32::from_rgba_unmultiplied(r, g, b, a);
    let sun = mk(255, 206, 92);
    let sun2 = mk(246, 170, 60);
    let cloud = mk(206, 216, 230);
    let rain = mk(120, 182, 238);
    let snow = mk(233, 244, 255);
    let bolt = mk(247, 206, 84);
    let fogc = mk(174, 185, 200);
    let moon = mk(226, 233, 245);

    let sun_at = |pp: &egui::Painter, cc: egui::Pos2, rad: f32, rays: bool| {
        pp.circle_filled(cc, rad, sun);
        pp.circle_filled(cc, rad * 0.70, sun2);
        if rays {
            let n = 8;
            for i in 0..n {
                let ang = t as f32 * 0.7 + i as f32 * (std::f32::consts::TAU / n as f32);
                let d1 = rad * 1.30;
                let d2 = rad * 1.62;
                pp.line_segment(
                    [
                        egui::pos2(cc.x + ang.cos() * d1, cc.y + ang.sin() * d1),
                        egui::pos2(cc.x + ang.cos() * d2, cc.y + ang.sin() * d2),
                    ],
                    egui::Stroke::new((s * 0.045).max(1.4), sun),
                );
            }
        }
    };
    let cloud_at = |pp: &egui::Painter, base: egui::Pos2, cw: f32| {
        let rr = cw * 0.26;
        pp.circle_filled(egui::pos2(base.x - cw * 0.22, base.y), rr, cloud);
        pp.circle_filled(egui::pos2(base.x + cw * 0.02, base.y - rr * 0.55), rr * 1.18, cloud);
        pp.circle_filled(egui::pos2(base.x + cw * 0.24, base.y), rr * 0.92, cloud);
        pp.rect_filled(
            egui::Rect::from_min_max(egui::pos2(base.x - cw * 0.24, base.y), egui::pos2(base.x + cw * 0.26, base.y + rr)),
            rr,
            cloud,
        );
    };

    match sky {
        Sky::Sun => sun_at(p, c, s * 0.30, true),
        Sky::Partly => {
            sun_at(p, egui::pos2(c.x - s * 0.14, c.y - s * 0.16), s * 0.20, true);
            cloud_at(p, egui::pos2(c.x + s * 0.06, c.y + s * 0.14), s * 0.92);
        }
        Sky::Cloud => cloud_at(p, egui::pos2(c.x, c.y + s * 0.10), s),
        Sky::Rain => {
            cloud_at(p, egui::pos2(c.x, c.y - s * 0.10), s * 0.92);
            for i in 0..3 {
                let bx = c.x - s * 0.26 + i as f32 * s * 0.26;
                let ph = (t as f32 * 1.7 + i as f32 * 0.55) % 1.0;
                let by = c.y + s * 0.20 + ph * s * 0.42;
                p.line_segment(
                    [egui::pos2(bx, by), egui::pos2(bx - s * 0.05, by + s * 0.16)],
                    egui::Stroke::new((s * 0.045).max(1.3), rain),
                );
            }
        }
        Sky::Thunder => {
            cloud_at(p, egui::pos2(c.x, c.y - s * 0.12), s * 0.92);
            let bx = c.x;
            let by = c.y + s * 0.06;
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(bx - s * 0.07, by),
                    egui::pos2(bx + s * 0.09, by),
                    egui::pos2(bx - s * 0.01, by + s * 0.20),
                ],
                bolt,
                egui::Stroke::NONE,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(bx - s * 0.01, by + s * 0.18),
                    egui::pos2(bx + s * 0.13, by + s * 0.13),
                    egui::pos2(bx - s * 0.03, by + s * 0.40),
                ],
                bolt,
                egui::Stroke::NONE,
            ));
        }
        Sky::Snow => {
            cloud_at(p, egui::pos2(c.x, c.y - s * 0.12), s * 0.92);
            for i in 0..3 {
                let bx = c.x - s * 0.26 + i as f32 * s * 0.26;
                let ph = (t as f32 * 0.7 + i as f32 * 0.33) % 1.0;
                let by = c.y + s * 0.20 + ph * s * 0.38;
                p.circle_filled(egui::pos2(bx, by), (s * 0.035).max(1.4), snow);
            }
        }
        Sky::Fog => {
            cloud_at(p, egui::pos2(c.x, c.y - s * 0.22), s * 0.90);
            for i in 0..3 {
                let off = (t as f32 * 0.6 + i as f32 * 0.33).sin() * s * 0.06;
                let yy = c.y + s * 0.14 + i as f32 * s * 0.13;
                p.line_segment(
                    [egui::pos2(c.x - s * 0.34 + off, yy), egui::pos2(c.x + s * 0.30 + off, yy)],
                    egui::Stroke::new((s * 0.05).max(1.5), fogc),
                );
            }
        }
        Sky::Night => {
            let mar = s * 0.30;
            p.circle_filled(egui::pos2(c.x + s * 0.03, c.y), mar, moon);
            p.circle_filled(egui::pos2(c.x + s * 0.17, c.y - s * 0.09), mar * 0.92, bg);
            for i in 0..3 {
                let tw = ((t as f32 * 2.0 + i as f32 * 1.3).sin() * 0.5 + 0.5) as f32;
                let sx = c.x - s * 0.26 + i as f32 * s * 0.24;
                let sy = c.y - s * 0.24 + (i % 2) as f32 * s * 0.14;
                let col = egui::Color32::from_rgba_unmultiplied(226, 233, 245, (tw * 200.0 * (a as f32 / 255.0)) as u8);
                p.circle_filled(egui::pos2(sx, sy), (s * 0.02).max(0.9), col);
            }
        }
    }
}

fn draw_nav_icon(painter: &egui::Painter, kind: u8, c: egui::Pos2, size: f32, color: egui::Color32) {
    match kind {
        // 
        0 => { painter.text(c, egui::Align2::CENTER_CENTER, "\u{266B}", egui::FontId::proportional(size * 1.15), color); }
        1 => { painter.text(c, egui::Align2::CENTER_CENTER, "\u{E787}", icon_font(size * 1.15), color); } // Calendar
        3 => {
            // 圆形 + 对勾（待办）
            let f = size / 24.0;
            let r = 9.0 * f;
            painter.circle_stroke(c, r, egui::Stroke::new(1.7 * f, color));
            let pts = vec![
                egui::pos2(c.x - 4.0 * f, c.y + 0.4 * f),
                egui::pos2(c.x - 1.3 * f, c.y + 3.1 * f),
                egui::pos2(c.x + 4.2 * f, c.y - 3.0 * f),
            ];
            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.7 * f, color)));
        }
        4 => {
            //  +  + 
            let f = size / 24.0;
            let cx = c.x;
            let top = c.y - 8.5 * f;
            let rim = c.y + 4.0 * f;
            let half = 6.2 * f;
            let pts = vec![
                egui::pos2(cx - half, rim),
                egui::pos2(cx - half * 0.82, rim - 4.0 * f),
                egui::pos2(cx - half * 0.5, top + 2.2 * f),
                egui::pos2(cx, top),
                egui::pos2(cx + half * 0.5, top + 2.2 * f),
                egui::pos2(cx + half * 0.82, rim - 4.0 * f),
                egui::pos2(cx + half, rim),
            ];
            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.6 * f, color)));
            painter.line_segment(
                [egui::pos2(cx - half - 1.4 * f, rim), egui::pos2(cx + half + 1.4 * f, rim)],
                egui::Stroke::new(1.6 * f, color),
            );
            painter.circle_filled(egui::pos2(cx, rim + 2.6 * f), 1.5 * f, color);
        }
        _ => { painter.text(c, egui::Align2::CENTER_CENTER, "\u{E753}", icon_font(size * 1.15), color); } // Cloud
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let load = |fonts: &mut egui::FontDefinitions, key: &str, path: &str| -> bool {
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts
                    .font_data
                    .insert(key.to_owned(), egui::FontData::from_owned(bytes).into());
                true
            }
            Err(_) => false,
        }
    };
    let has_segoe = load(&mut fonts, "segoe", r"C:\Windows\Fonts\segoeui.ttf");
    load(&mut fonts, "segoeb", r"C:\Windows\Fonts\segoeuib.ttf");
    load(&mut fonts, "deng", r"C:\Windows\Fonts\Deng.ttf");
    load(&mut fonts, "sym", r"C:\Windows\Fonts\seguisym.ttf");
    let has_mdl2 = load(&mut fonts, "mdl2", r"C:\Windows\Fonts\segmdl2.ttf");
    // Hangul(Deng)/Segoe UI 
    //  Windows  malgun.ttfdeng deng
    let has_malgun = load(&mut fonts, "malgun", r"C:\Windows\Fonts\malgun.ttf");
    //  ANUFLY /Segoe UI/Malgun 
    // Windows  himalaya.ttfMicrosoft Himalaya
let has_himalaya = load(&mut fonts, "himalaya", r"C:\Windows\Fonts\himalaya.ttf");
    // Microsoft YaHei?.ttc index 0**?*?
    // App  Segoe UI
    let has_msyh = load(&mut fonts, "msyh", r"C:\Windows\Fonts\msyh.ttc");
    let has_msyhbd = load(&mut fonts, "msyhbd", r"C:\Windows\Fonts\msyhbd.ttc");

    //  Noto Sans SC**
    // 
    fonts.font_data.insert(
        "noto".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/NotoSansSC-Regular.otf")).into(),
    );
    fonts.font_data.insert(
        "notob".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/NotoSansSC-Bold.otf")).into(),
    );

    // Microsoft YaHei?Windows  UI ?
    //  noto/notob
    let cjk = if has_msyh { "msyh" } else { "noto" };
    let cjk_bold = if has_msyhbd { "msyhbd" } else { "notob" };

    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        if has_segoe {
            list.insert(0, "segoe".to_owned());
        }
        // 在 Segoe UI 之后插入中文字体
        list.insert(
            if has_segoe { 1 } else { 0 }, cjk.to_owned());
        list.push("deng".to_owned());
        list.push("sym".to_owned());
        if has_malgun {
            list.push("malgun".to_owned());
        }
        if has_himalaya {
            list.push("himalaya".to_owned());
        }
        list.push("noto".to_owned());
    }
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        list.push(cjk.to_owned());
        list.push("deng".to_owned());
        list.push("sym".to_owned());
        if has_malgun {
            list.push("malgun".to_owned());
        }
        if has_himalaya {
            list.push("himalaya".to_owned());
        }
        list.push("noto".to_owned());
    }
    //  Segoe UI Bold Boldnoto / 
let mut bold_list = vec!["segoeb".to_owned(), cjk_bold.to_owned(), "notob".to_owned(), "noto".to_owned(), "deng".to_owned()];
    if has_malgun {
        bold_list.push("malgun".to_owned());
    }
    if has_himalaya {
        bold_list.push("himalaya".to_owned());
    }
    fonts.families.insert(egui::FontFamily::Name(Arc::from("bold")), bold_list);
    if has_mdl2 {
        fonts.families.insert(
            egui::FontFamily::Name(Arc::from("icons")),
            vec!["mdl2".to_owned()],
        );
    }
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result<()> {
    if platform::ensure_single_instance() {
        let _ = std::fs::write(trigger_path(), b"open");
        return Ok(());
    }

    // WINISLE_LYRIC_TEST="|" 
    if let Ok(spec) = std::env::var("WINISLE_LYRIC_TEST") {
        let (t, a) = spec.split_once('|').unwrap_or((spec.as_str(), ""));
        match lyrics::fetch(t, a) {
            Some(v) => println!(
                "LYRIC OK lines={} first3={:?}",
                v.len(),
                v.iter().take(3).map(|l| (l.t, l.text.clone())).collect::<Vec<_>>()
            ),
            None => println!("LYRIC NONE for {}|{}", t, a),
        }
        return Ok(());
    }

    // ""
    // 
let open_panel = false;

    // 
    crate::audio_meter::spawn();
    // CDP SMTC 
    crate::soda_now::spawn();

    // 监控退出请求：一旦 QUIT_REQUESTED 置位，延迟 1500ms 再退出
    std::thread::spawn(|| loop {
        use std::sync::atomic::Ordering::SeqCst;
        if crate::tray::QUIT_REQUESTED.load(SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            std::process::exit(0);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    });

    //  WINISLE_AUTOQUIT_MS=N  N ?
    if let Ok(ms) = std::env::var("WINISLE_AUTOQUIT_MS") {
        if let Ok(ms) = ms.parse::<u64>() {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                crate::tray::QUIT_REQUESTED
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            });
        }
    }

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("WinIsle")
            .with_inner_size([W_EXPANDED + 2.0 * MARGIN, H_EXPANDED + 2.0 * MARGIN])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(false)
            .with_visible(false),
        // 关闭 MSAA，减少 2D 界面 GPU 开销
        multisampling: 0,
        vsync: false,
        ..Default::default()
    };
    eframe::run_native(
        "WinIsle",
        opts,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            apply_style(&cc.egui_ctx, false);
            let tray = Tray::new();
            Ok(Box::new(IslandApp::new(tray, open_panel)))
        }),
    )
}


fn make_tex(ctx: &egui::Context, name: &str, bytes: &'static [u8], w: usize, h: usize) -> egui::TextureHandle {
    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], bytes);
    ctx.load_texture(name, img, egui::TextureOptions::LINEAR)
}
