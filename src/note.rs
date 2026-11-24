//! 便签模块：随手记的碎片卡片。
//!
//! 设计定位：便签 ≠ 待办。待办是会被勾掉的任务；便签是随手记下的碎片，
//! 核心是「快速记 + 零决断」——不设标题、回车即存、颜色只作事后粗筛。
//!
//! 功能：便签墙（2 列网格）/ 5 色筛选 / 置顶 / 快捷记 / 编辑器 /
//! 贴图（Ctrl+V 粘贴、拖入文件）/ 岛内灯箱 / 独立置顶图片查看器。

#![allow(dead_code)]

use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, TextureHandle, Vec2};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// 读取剪贴板「复制的文件」(CF_HDROP) 所需的少量 Win32 接口。
#[link(name = "user32")]
extern "system" {
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn OpenClipboard(hwnd: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn GetClipboardData(format: u32) -> isize;
}
#[link(name = "shell32")]
extern "system" {
    fn DragQueryFileW(hdrop: isize, ifile: u32, buf: *mut u16, cch: u32) -> u32;
}
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_HDROP: u32 = 15;

/// 5 种便签色（mint / amber / coral / sky / lilac）。
const COLORS: [(u8, u8, u8); 5] = [
    (52, 211, 153),
    (230, 178, 60),
    (242, 112, 92),
    (91, 182, 230),
    (181, 140, 232),
];

const CARD_H: f32 = 76.0;
const GRID_GAP: f32 = 10.0;

/// 便签卡片坐落的深色底（岛内容区背景），用于把 9–13% 的同色淡底预混成不透明色，
/// 避免逐次 alpha 叠色导致颜色发闷、发艳，保证与原型一致的「淡底 + 细描边」观感。
const WALL_BASE: (u8, u8, u8) = (28, 28, 34);

fn pre_blend(col: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let m = |b: u8, c: u8| (b as f32 + (c as f32 - b as f32) * t).round().clamp(0.0, 255.0) as u8;
    (m(WALL_BASE.0, col.0), m(WALL_BASE.1, col.1), m(WALL_BASE.2, col.2))
}

/// 预混色 + 整体淡入 alpha（a=255 时即为不透明的目标实色，避免 alpha 叠色过重）。
fn lay(col: (u8, u8, u8), t: f32, a: u8) -> Color32 {
    let p = pre_blend(col, t);
    Color32::from_rgba_unmultiplied(p.0, p.1, p.2, a)
}

/// 按可用高度把文本裁剪成「整行数」，超过则末行加省略号（避免半行被硬切）。
/// 用于便签卡片正文：保证不会出现“半截字/截断”的难看效果。
fn clamped_galley(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    wrap_w: f32,
    max_h: f32,
) -> std::sync::Arc<egui::Galley> {
    let line_h = (font.size * 1.22).max(1.0);
    let max_lines = ((max_h / line_h).floor() as usize).max(1);
    let mut fmt = egui::TextFormat::default();
    fmt.font_id = font;
    fmt.color = color;
    fmt.line_height = Some(line_h);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_w;
    job.wrap.max_rows = max_lines;
    job.wrap.overflow_character = Some('…');
    job.append(text, 0.0, fmt);
    ui.fonts(|f| f.layout_job(job))
}

/// 统一的便签文本输入框（坑 B / 坑 G 的落地）：稳定 Id + 透明底（无底色块）+
/// 选中淡薄荷 + 白色插入符 rgb(233,236,242) + 悬停 I-Beam 光标 + force_caret 兜底
/// （窗口 NOACTIVATE 时仍能绘制闪烁插入符）。所有便签输入框必须走这里，禁止各自重写。
#[allow(clippy::too_many_arguments)]
fn note_text_field(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    id: egui::Id,
    rect: Rect,
    text: &mut String,
    font_size: f32,
    ph_font: f32,
    multiline: bool,
    rows: usize,
    placeholder: &str,
    ph_x_off: f32,
    ph_y_off: f32,
    a: u8,
    show_placeholder: bool,
) -> egui::Response {
    let ink = Color32::from_rgba_unmultiplied(233, 236, 242, a);
    let te = if multiline {
        egui::TextEdit::multiline(text)
            .frame(false)
            .id(id)
            .font(FontId::proportional(font_size))
            .text_color(ink)
            .desired_width(rect.width())
            .desired_rows(rows)
    } else {
        egui::TextEdit::singleline(text)
            .frame(false)
            .id(id)
            .vertical_align(egui::Align::Center)
            .font(FontId::proportional(font_size))
            .text_color(ink)
            .desired_width(rect.width())
    };
    // 悬停输入区（含内边距）显示 I-Beam 文本光标
    if ui.ctx().pointer_hover_pos().map(|p| rect.expand(6.0).contains(p)).unwrap_or(false) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    // force_caret：窗口未聚焦但控件仍持焦点时，临时置 focused 以便绘制闪烁插入符（同待办）
    let vp_before = ui.ctx().input(|i| i.focused);
    let mem0 = ui.ctx().memory(|m| m.has_focus(id));
    let force_caret = !vp_before && mem0;
    if force_caret { ui.ctx().input_mut(|i| i.focused = true); }
    let resp = ui
        .scope(|ui| {
            ui.style_mut().visuals.extreme_bg_color = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.hovered.bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.active.bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(52, 211, 153, 56);
            ui.style_mut().visuals.text_cursor.stroke = Stroke::new(1.5, Color32::from_rgb(233, 236, 242));
            ui.put(rect, te)
        })
        .inner;
    if force_caret { ui.ctx().input_mut(|i| i.focused = vp_before); }
    // 占位文字：仅在空且未聚焦时绘制（避免与插入符重叠）
    let mem_now = ui.ctx().memory(|m| m.has_focus(id));
    // 离屏 caret 自检入口（坑 C）：有输入框获得焦点即打印一次，供离屏验证聚焦/插入符机制
    if mem_now && std::env::var_os("WINISLE_NOTE_CARET_CHECK").is_some() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static CARET_LOGGED: AtomicBool = AtomicBool::new(false);
        if !CARET_LOGGED.swap(true, Ordering::Relaxed) {
            eprintln!("[note-caret] input focused -> caret should render (id={:?})", id);
        }
    }
    if text.trim().is_empty() && !mem_now && show_placeholder && !placeholder.is_empty() {
        let (pos, al) = if multiline {
            (Pos2::new(rect.left() + ph_x_off, rect.top() + ph_y_off), Align2::LEFT_TOP)
        } else {
            (Pos2::new(rect.left() + ph_x_off, rect.center().y), Align2::LEFT_CENTER)
        };
        painter.text(pos, al, placeholder, FontId::proportional(ph_font), Color32::from_rgba_unmultiplied(107, 113, 128, a));
    }
    resp
}

/// 手绘小图钉（头部圆 + 针杆），用于置顶便签右下角。
fn draw_pin(painter: &egui::Painter, tip: Pos2, rgb: (u8, u8, u8), a: u8) {
    let c = Color32::from_rgba_unmultiplied(rgb.0, rgb.1, rgb.2, (a as f32 * 0.85) as u8);
    let head = Pos2::new(tip.x, tip.y - 2.5);
    painter.circle_filled(head, 2.3, c);
    painter.line_segment([Pos2::new(head.x, head.y + 2.0), Pos2::new(head.x, tip.y + 4.0)], Stroke::new(1.3, c));
}

/// 虚线圆角矩形（空状态占位框），沿圆角矩形周长采样后用 dashed_line 绘制。
fn draw_dashed_round_rect(painter: &egui::Painter, rect: Rect, radius: f32, stroke: Stroke, dash: f32, gap: f32) {
    let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let mn = rect.min;
    let mx = rect.max;
    let mut pts: Vec<Pos2> = Vec::new();
    let mut line = |pts: &mut Vec<Pos2>, a: Pos2, b: Pos2| {
        let d = (b - a).length();
        let n = (d / 3.0).ceil().max(1.0) as usize;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            pts.push(a + (b - a) * t);
        }
    };
    let mut arc = |pts: &mut Vec<Pos2>, cx: f32, cy: f32, s: f32, e: f32| {
        let n = 8usize;
        for i in 0..=n {
            let t = s + (e - s) * i as f32 / n as f32;
            pts.push(Pos2::new(cx + r * t.cos(), cy + r * t.sin()));
        }
    };
    let pi = std::f32::consts::PI;
    line(&mut pts, Pos2::new(mn.x + r, mn.y), Pos2::new(mx.x - r, mn.y));
    arc(&mut pts, mx.x - r, mn.y + r, -pi * 0.5, 0.0);
    line(&mut pts, Pos2::new(mx.x, mn.y + r), Pos2::new(mx.x, mx.y - r));
    arc(&mut pts, mx.x - r, mx.y - r, 0.0, pi * 0.5);
    line(&mut pts, Pos2::new(mx.x - r, mx.y), Pos2::new(mn.x + r, mx.y));
    arc(&mut pts, mn.x + r, mx.y - r, pi * 0.5, pi);
    line(&mut pts, Pos2::new(mn.x, mx.y - r), Pos2::new(mn.x, mn.y + r));
    arc(&mut pts, mn.x + r, mn.y + r, pi, pi * 1.5);
    painter.extend(egui::Shape::dashed_line(&pts, stroke, dash, gap));
}

/// 便签纸图标（文档轮廓 + 折角 + 三横线），用于空状态。
fn draw_doc_icon(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let w = size * 0.74;
    let h = size;
    let rect = Rect::from_center_size(center, Vec2::new(w, h));
    painter.rect_stroke(rect, size * 0.1, Stroke::new(1.4, color), StrokeKind::Inside);
    let f = size * 0.26;
    painter.line_segment([Pos2::new(rect.right() - f, rect.top()), Pos2::new(rect.right() - f, rect.top() + f)], Stroke::new(1.3, color));
    painter.line_segment([Pos2::new(rect.right() - f, rect.top() + f), Pos2::new(rect.right(), rect.top() + f)], Stroke::new(1.3, color));
    for k in 0..3 {
        let y = rect.top() + h * 0.40 + k as f32 * h * 0.17;
        painter.line_segment([Pos2::new(rect.left() + w * 0.16, y), Pos2::new(rect.right() - w * 0.16, y)], Stroke::new(1.1, color));
    }
}

#[derive(Clone, Debug)]
pub struct NoteImage {
    pub file: String,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub id: u64,
    pub text: String,
    pub color: u8,
    pub pinned: bool,
    pub updated: i64,
    pub images: Vec<NoteImage>,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 轻量调试日志（便签贴图链路排障用）。
fn note_dbg(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("D:\\LingDao\\_note_debug.log")
    {
        let _ = writeln!(f, "[{}] {}", unix_now(), msg);
    }
}

fn notes_path() -> std::path::PathBuf {
    crate::paths::file("notes.json")
}

fn media_dir(id: u64) -> std::path::PathBuf {
    crate::paths::ensure().join("notes_media").join(id.to_string())
}

fn load_notes() -> (Vec<Note>, u64) {
    let mut notes: Vec<Note> = Vec::new();
    let mut max_id = 0u64;
    if let Ok(txt) = std::fs::read_to_string(notes_path()) {
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&txt) {
            for v in arr {
                let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
                if id == 0 {
                    continue;
                }
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let color = v.get("color").and_then(|x| x.as_u64()).unwrap_or(0).min(4) as u8;
                let pinned = v.get("pinned").and_then(|x| x.as_bool()).unwrap_or(false);
                let updated = v.get("updated").and_then(|x| x.as_i64()).unwrap_or(0);
                let mut images: Vec<NoteImage> = Vec::new();
                if let Some(serde_json::Value::Array(imgs)) = v.get("images") {
                    for im in imgs {
                        if let Some(f) = im.get("file").and_then(|x| x.as_str()) {
                            let w = im.get("w").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                            let h = im.get("h").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                            images.push(NoteImage { file: f.to_string(), w, h });
                        }
                    }
                }
                max_id = max_id.max(id);
                notes.push(Note { id, text, color, pinned, updated, images });
            }
        }
    }
    (notes, max_id)
}

fn save_notes(notes: &[Note]) {
    let arr: Vec<serde_json::Value> = notes
        .iter()
        .map(|n| {
            let imgs: Vec<serde_json::Value> = n
                .images
                .iter()
                .map(|im| serde_json::json!({"file": im.file.clone(), "w": im.w, "h": im.h}))
                .collect();
            serde_json::json!({
                "id": n.id,
                "text": n.text,
                "color": n.color,
                "pinned": n.pinned,
                "updated": n.updated,
                "images": imgs,
            })
        })
        .collect();
    if let Ok(s) = serde_json::to_string_pretty(&serde_json::Value::Array(arr)) {
        let _ = std::fs::write(notes_path(), s);
    }
}

// ─────────────────────────── 独立窗口查看器状态 ───────────────────────────

struct ViewerData {
    images: Vec<NoteImage>,
    idx: usize,
    zoom: f32,
    pan: Vec2,
    tex: HashMap<String, TextureHandle>,
    request_close: bool,
}

impl Default for ViewerData {
    fn default() -> Self {
        Self {
            images: Vec::new(),
            idx: 0,
            zoom: 1.0,
            pan: Vec2::ZERO,
            tex: HashMap::new(),
            request_close: false,
        }
    }
}

// ─────────────────────────── 模块状态 ───────────────────────────

pub struct NoteState {
    pub notes: Vec<Note>,
    pub next_id: u64,
    pub filter: i8, // -1 全部，0..4 按颜色

    // 搜索（点击工具栏搜索图标展开，实时按文本过滤）
    search_open: bool,
    search_q: String,
    search_need_focus: bool,
    search_focus_tries: u8,

    // 快捷记输入
    input: String,
    input_need_focus: bool,
    input_focus_tries: u8,

    // 键盘 / 激活态（窗口默认 NOACTIVATE，需据此软激活并请求焦点）
    pub quick_editing: bool,
    pub hover_keyboard_area: bool,

    // 编辑器
    editor_open: bool,
    edit_id: Option<u64>,
    edit_text: String,
    edit_color: u8,
    edit_pinned: bool,
    edit_images: Vec<NoteImage>,
    edit_need_focus: bool,
    edit_focus_tries: u8,
    edit_reset_cursor: bool,
    last_color: u8,
    // 打开编辑器时的“是否已有便签”——用于固定按钮布局，避免保存新建便签后
    // 笔记已存在导致「删除」按钮在关闭动画里突然冒出来闪一下。
    edit_was_existing: bool,

    // 滚动（隐藏滚动条）
    scroll: f32,
    scroll_target: f32,
    scroll_vis: f32,
    scroll_act: Instant,

    // 灯箱
    lb_open: bool,
    lb_id: Option<u64>,
    lb_idx: usize,

    // 纹理缓存（缩略图 / 灯箱）
    tex: HashMap<String, TextureHandle>,

    // 独立查看器
    viewer_open: bool,
    viewer: Arc<Mutex<ViewerData>>,

    // 开关：贴图/编辑器键位状态
    needs_refocus_on_show: bool,

    // 调试注入
    pending_debug: Option<String>,

    // 贴图按键去重（同一次 Ctrl+V 可能被窗口过程与键盘轮询同时看到）
    paste_guard: Instant,

    // 编辑器开合动画（对齐“提醒”编辑页：指数缓动 0.09s，向上滑入 10px + 淡入）
    editor_anim: f32,
    // 返回/保存后列表淡入（0.18s，同提醒保存后）
    list_anim: f32,
}

impl NoteState {
    pub fn load() -> Self {
        let (mut notes, max_id) = load_notes();
        if notes.is_empty() && std::env::var("WINISLE_NOTE_DEMO").is_ok() {
            seed_demo(&mut notes);
        }
        let max_id = notes.iter().map(|n| n.id).max().unwrap_or(max_id);
        let pending_debug = std::env::var("WINISLE_NOTE_STATE").ok();
        Self {
            notes,
            next_id: max_id + 1,
            filter: -1,
            search_open: false,
            search_q: String::new(),
            search_need_focus: false,
            search_focus_tries: 0,
            input: String::new(),
            input_need_focus: false,
            input_focus_tries: 0,
            quick_editing: false,
            hover_keyboard_area: false,
            editor_open: false,
            edit_id: None,
            edit_text: String::new(),
            edit_color: 0,
            edit_pinned: false,
            edit_images: Vec::new(),
            edit_need_focus: false,
            edit_focus_tries: 0,
            edit_reset_cursor: false,
            last_color: 0,
            edit_was_existing: false,
            scroll: 0.0,
            scroll_target: 0.0,
            scroll_vis: 0.0,
            scroll_act: Instant::now() - std::time::Duration::from_secs(10),
            lb_open: false,
            lb_id: None,
            lb_idx: 0,
            tex: HashMap::new(),
            viewer_open: false,
            viewer: Arc::new(Mutex::new(ViewerData::default())),
            pending_debug,
            paste_guard: Instant::now() - std::time::Duration::from_secs(10),
            editor_anim: 0.0,
            list_anim: 1.0,
            needs_refocus_on_show: false,
        }
    }

    fn save(&self) {
        save_notes(&self.notes);
    }

    fn visible_notes(&self) -> Vec<Note> {
        let q = self.search_q.trim().to_lowercase();
        let mut v: Vec<Note> = self
            .notes
            .iter()
            .filter(|n| {
                (self.filter < 0 || n.color == self.filter as u8)
                    && (q.is_empty() || n.text.to_lowercase().contains(&q))
            })
            .cloned()
            .collect();
        v.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.updated.cmp(&a.updated)));
        v
    }

    /// 便签是否需要键盘输入（决定窗口是否可被激活、抢焦点）。
    pub fn wants_keyboard(&self) -> bool {
        self.quick_editing || self.editor_open || self.search_open
    }

    /// 离开便签模块时调用：释放键盘焦点与瞬时输入态，但**保留**编辑器/搜索页状态，
    /// 使切回便签时仍停留当前编辑页，而不是退回列表。
    pub fn leave_module(&mut self) {
        self.quick_editing = false;
        self.input_need_focus = false;
        self.edit_need_focus = false;
        self.search_need_focus = false;
        if self.editor_open || self.search_open {
            self.needs_refocus_on_show = true;
        }
        // 若编辑器已关闭，动画直接到位，避免切回来时残留半透明过渡
        if !self.editor_open {
            self.editor_anim = 0.0;
            self.list_anim = 1.0;
        }
    }

    /// 离开便签模块时复位所有输入/编辑态，避免窗口错误地保持可激活。
    pub fn clear_editing(&mut self) {
        self.quick_editing = false;
        self.input_need_focus = false;
        self.editor_open = false;
        self.search_open = false;
    }

    fn thumb(&mut self, ctx: &egui::Context, file: &str) -> Option<TextureHandle> {
        if let Some(t) = self.tex.get(file) {
            return Some(t.clone());
        }
        let img = image::open(std::path::Path::new(file)).ok()?;
        let th = img.thumbnail(360, 360).to_rgba8();
        let (w, h) = th.dimensions();
        let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], th.as_raw());
        let tex = ctx.load_texture(format!("note_thumb_{}", file), ci, egui::TextureOptions::LINEAR);
        self.tex.insert(file.to_string(), tex.clone());
        Some(tex)
    }

    fn open_editor_new(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        self.editor_open = true;
        self.edit_id = Some(id);
        self.edit_text.clear();
        self.edit_color = if self.filter >= 0 { self.filter as u8 } else { self.last_color };
        self.edit_pinned = false;
        self.edit_images.clear();
        self.edit_need_focus = true;
        self.edit_focus_tries = 0;
        self.edit_reset_cursor = true;
        self.edit_was_existing = false;
    }

    fn open_editor_note(&mut self, n: &Note) {
        self.editor_open = true;
        self.edit_id = Some(n.id);
        self.edit_text = n.text.clone();
        self.edit_color = n.color;
        self.edit_pinned = n.pinned;
        self.edit_images = n.images.clone();
        self.edit_need_focus = true;
        self.edit_focus_tries = 0;
        self.edit_reset_cursor = true;
        self.edit_was_existing = true;
    }

    fn commit_editor(&mut self) {
        let text = self.edit_text.trim().to_string();
        if text.is_empty() && self.edit_images.is_empty() {
            self.editor_open = false;
            return;
        }
        let id = self.edit_id.unwrap_or(0);
        let now = unix_now();
        if let Some(n) = self.notes.iter_mut().find(|n| n.id == id) {
            n.text = text;
            n.color = self.edit_color;
            n.pinned = self.edit_pinned;
            n.updated = now;
            n.images = self.edit_images.clone();
        } else {
            self.notes.push(Note {
                id,
                text,
                color: self.edit_color,
                pinned: self.edit_pinned,
                updated: now,
                images: self.edit_images.clone(),
            });
            if id >= self.next_id {
                self.next_id = id + 1;
            }
        }
        self.last_color = self.edit_color;
        self.save();
        self.editor_open = false;
    }

    fn delete_note(&mut self, id: u64) {
        self.notes.retain(|n| n.id != id);
        let dir = media_dir(id);
        let _ = std::fs::remove_dir_all(dir);
        self.save();
    }

    fn quick_save(&mut self) {
        let t = self.input.trim().to_string();
        if t.is_empty() {
            return;
        }
        let color = if self.filter >= 0 { self.filter as u8 } else { self.last_color };
        let id = self.next_id;
        self.next_id += 1;
        self.notes.push(Note {
            id,
            text: t,
            color,
            pinned: false,
            updated: unix_now(),
            images: Vec::new(),
        });
        self.save();
        self.input.clear();
    }

    fn lb_open_for(&mut self, n: &Note) {
        if n.images.is_empty() {
            self.open_editor_note(n);
            return;
        }
        self.lb_open = true;
        self.lb_id = Some(n.id);
        self.lb_idx = 0;
    }

    fn open_viewer(&mut self) {
        let Some(id) = self.lb_id else { return };
        let Some(n) = self.notes.iter().find(|n| n.id == id) else { return };
        if n.images.is_empty() {
            return;
        }
        if let Ok(mut v) = self.viewer.lock() {
            v.images = n.images.clone();
            v.idx = self.lb_idx;
            v.zoom = 1.0;
            v.pan = Vec2::ZERO;
            v.request_close = false;
            v.tex.clear();
        }
        self.viewer_open = true;
    }

    fn pump_viewer(&self, ctx: &egui::Context) {
        if !self.viewer_open {
            return;
        }
        let v = self.viewer.clone();
        let builder = egui::ViewportBuilder::default()
            .with_title("WinIsle · 图片查看")
            .with_inner_size([920.0, 660.0])
            .with_min_inner_size([360.0, 260.0])
            .with_resizable(true)
            .with_always_on_top();
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("winisle_note_viewer"),
            builder,
            move |ctx, _class| {
                let mut data = match v.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if ctx.input(|i| i.viewport().close_requested()) {
                    data.request_close = true;
                }
                draw_viewer(ctx, &mut data);
            },
        );
    }
}

// ─────────────────────────── 图片载入 ───────────────────────────

fn save_rgba_png(path: &std::path::Path, w: u32, h: u32, raw: Vec<u8>) -> bool {
    let Some(buf) = image::RgbaImage::from_raw(w, h, raw) else {
        return false;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    image::DynamicImage::ImageRgba8(buf).save(path).is_ok()
}

fn add_dynamic_image(st: &mut NoteState, id: u64, dynimg: image::DynamicImage) -> bool {
    let rgba = dynimg.to_rgba8();
    let (w, h) = rgba.dimensions();
    let n = st.edit_images.len();
    let path = media_dir(id).join(format!("{}.png", n));
    if save_rgba_png(&path, w, h, rgba.into_raw()) {
        st.edit_images.push(NoteImage {
            file: path.to_string_lossy().to_string(),
            w,
            h,
        });
        true
    } else {
        false
    }
}

/// 弹出系统「打开文件」对话框，选择一张图片；取消返回 None。
fn pick_image_file() -> Option<std::path::PathBuf> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OPENFILENAMEW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
        OFN_PATHMUSTEXIST,
    };
    unsafe {
        let mut file_buf = vec![0u16; 2048];
        let filter: Vec<u16> =
            "图片文件 (*.png;*.jpg;*.jpeg)\0*.png;*.jpg;*.jpeg\0所有文件 (*.*)\0*.*\0\0"
                .encode_utf16()
                .collect();
        let title: Vec<u16> = "选择要贴进便签的图片"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let owner = HWND(crate::platform::main_hwnd() as *mut core::ffi::c_void);
        let mut ofn = OPENFILENAMEW::default();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = owner;
        ofn.lpstrFilter = PCWSTR(filter.as_ptr());
        ofn.lpstrFile = PWSTR(file_buf.as_mut_ptr());
        ofn.nMaxFile = file_buf.len() as u32;
        ofn.lpstrTitle = PCWSTR(title.as_ptr());
        ofn.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
        if GetOpenFileNameW(&mut ofn).as_bool() {
            let len = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
            let p = std::path::PathBuf::from(String::from_utf16_lossy(&file_buf[..len]));
            note_dbg(&format!("pick -> {}", p.display()));
            Some(p)
        } else {
            note_dbg("pick canceled/failed");
            None
        }
    }
}

/// 从磁盘文件载入图片加入编辑器。
fn add_image_from_path(st: &mut NoteState, id: u64, path: &std::path::Path) -> bool {
    match image::open(path) {
        Ok(d) => {
            let ok = add_dynamic_image(st, id, d);
            note_dbg(&format!("addpath {} -> {}", path.display(), ok));
            ok
        }
        Err(e) => {
            note_dbg(&format!("addpath {} ERR {}", path.display(), e));
            false
        }
    }
}

fn is_image_ext(p: &std::path::Path) -> bool {
    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        Some(e) => matches!(e.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp"),
        None => false,
    }
}

/// 剪贴板里是「在资源管理器里复制的图片文件」(CF_HDROP) 时，取第一个图片路径。
fn clipboard_image_file() -> Option<std::path::PathBuf> {
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP) == 0 {
            return None;
        }
        if OpenClipboard(0) == 0 {
            return None;
        }
        let h = GetClipboardData(CF_HDROP);
        let mut best: Option<std::path::PathBuf> = None;
        if h != 0 {
            let n = DragQueryFileW(h, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
            for i in 0..n {
                let len = DragQueryFileW(h, i, std::ptr::null_mut(), 0);
                if len == 0 {
                    continue;
                }
                let mut buf = vec![0u16; (len + 1) as usize];
                let got = DragQueryFileW(h, i, buf.as_mut_ptr(), len + 1);
                if got == 0 {
                    continue;
                }
                let s = String::from_utf16_lossy(&buf[..got as usize]);
                let p = std::path::PathBuf::from(s);
                if is_image_ext(&p) {
                    best = Some(p);
                    break;
                }
            }
        }
        CloseClipboard();
        best
    }
}

fn push_rgba_image(st: &mut NoteState, id: u64, w: u32, h: u32, raw: Vec<u8>) -> bool {
    if w == 0 || h == 0 {
        return false;
    }
    if raw.len() < (w as usize) * (h as usize) * 4 {
        return false;
    }
    let n = st.edit_images.len();
    let path = media_dir(id).join(format!("{}.png", n));
    if save_rgba_png(&path, w, h, raw) {
        st.edit_images.push(NoteImage {
            file: path.to_string_lossy().to_string(),
            w,
            h,
        });
        true
    } else {
        false
    }
}

fn add_clipboard_image(st: &mut NoteState, id: u64) -> bool {
    match arboard::Clipboard::new() {
        Ok(mut cb) => match cb.get_image() {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                let ok = push_rgba_image(st, id, w, h, img.bytes.into_owned());
                note_dbg(&format!("clip image {}x{} -> {}", w, h, ok));
                ok
            }
            Err(e) => {
                let (dib, bmp, hdrop) = unsafe {
                    (IsClipboardFormatAvailable(CF_DIB), IsClipboardFormatAvailable(CF_BITMAP), IsClipboardFormatAvailable(CF_HDROP))
                };
                note_dbg(&format!("clip: no bitmap image ({e}); DIB={dib} BITMAP={bmp} HDROP={hdrop}"));
                // 兜底：用户是在资源管理器里“复制了图片文件”，剪贴板是文件列表而非位图。
                if let Some(p) = clipboard_image_file() {
                    note_dbg(&format!("clip HDROP -> {}", p.display()));
                    return add_image_from_path(st, id, &p);
                }
                false
            }
        },
        Err(e) => {
            note_dbg(&format!("clip: open failed {e}"));
            false
        }
    }
}

fn add_dropped_files(st: &mut NoteState, id: u64, files: &[egui::DroppedFile]) -> bool {
    let mut added = false;
    for f in files {
        let dynimg = if let Some(p) = &f.path {
            image::open(p).ok()
        } else if let Some(bytes) = &f.bytes {
            image::load_from_memory(bytes.as_ref()).ok()
        } else {
            None
        };
        if let Some(d) = dynimg {
            added |= add_dynamic_image(st, id, d);
        }
    }
    added
}

// ─────────────────────────── 主绘制入口 ───────────────────────────

pub fn draw(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    rect: Rect,
    e: f32,
    st: &mut NoteState,
    bg: [u8; 3],
    radius: f32,
) {
    let a = (((e - 0.5) * 2.0) * 255.0).clamp(0.0, 255.0) as u8;

    // 每帧重置：输入区是否被鼠标悬停（供主循环判定软激活）。
    st.hover_keyboard_area = false;

    // 从别的模块切回便签：若之前正在编辑/搜索，恢复焦点，继续保持在该页。
    if st.needs_refocus_on_show {
        st.needs_refocus_on_show = false;
        if st.editor_open {
            st.edit_need_focus = true;
            st.edit_focus_tries = 30;
        }
        if st.search_open {
            st.search_need_focus = true;
            st.search_focus_tries = 30;
        }
    }

    // 编辑器未打开时清掉 Ctrl+V 贴图意图，避免旧意图带进下一次编辑。
    if !st.editor_open {
        crate::platform::clear_paste_image_req();
    }

    // 调试注入：按环境变量打开某个状态（仅截图核对用）。
    if let Some(db) = st.pending_debug.take() {
        apply_debug(ui, st, &db);
    }

    // 独立查看器窗口每个可见帧都需注册一次。
    st.pump_viewer(ui.ctx());
    let mut need_close = false;
    if let Ok(v) = st.viewer.lock() {
        if v.request_close {
            need_close = true;
        }
    }
    if need_close {
        st.viewer_open = false;
        if let Ok(mut v) = st.viewer.lock() {
            v.request_close = false;
        }
    }

    // 编辑器开合：对齐「提醒」编辑页——打开时指数缓动(0.09s)向上滑入 10px + 淡入；
    // 返回/保存时立即撤掉编辑页（与提醒一致，不留任何残影），列表 180ms 淡入。
    let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
    if st.editor_open {
        st.editor_anim += (1.0 - st.editor_anim) * (1.0 - (-dt / 0.09).exp());
        if (1.0 - st.editor_anim).abs() < 0.01 {
            st.editor_anim = 1.0;
        }
        let fa = st.editor_anim.clamp(0.0, 1.0);
        st.list_anim = 0.0;
        let ed_rect = rect.translate(Vec2::new(0.0, (1.0 - fa) * 10.0));
        let ed_clip = painter.with_clip_rect(rect);
        let ae = ((a as f32) * fa).round() as u8;
        draw_editor(ui, &ed_clip, ed_rect, ae, st, bg, fa);
        if fa < 0.999 {
            ui.ctx().request_repaint();
        }
        return;
    }
    // 关闭态：编辑器立即消失（不叠残影），列表从头淡入（同提醒保存后）。
    st.editor_anim = 0.0;
    st.list_anim = (st.list_anim + dt / 0.18).min(1.0);
    let la = ((a as f32) * st.list_anim).round() as u8;
    if st.lb_open {
        draw_lightbox(ui, painter, rect, a, st, radius);
        return;
    }
    draw_wall(ui, painter, rect, la, st);
    if st.list_anim < 1.0 {
        ui.ctx().request_repaint();
    }
}

fn apply_debug(ui: &egui::Ui, st: &mut NoteState, db: &str) {
    match db {
        "editor" => st.open_editor_new(),
        "editnote" => {
            if let Some(n) = st.visible_notes().into_iter().next() {
                st.open_editor_note(&n);
            }
        }
        "editimg" => {
            st.open_editor_new();
            // 演示：给编辑中的便签附一张内存生成的图片
            if let Some(id) = st.edit_id {
                if st.edit_images.is_empty() {
                    let path = media_dir(id).join("demo.png");
                    make_grad_png(&path, 560, 360, (52, 211, 153), (91, 182, 230));
                    st.edit_images.push(NoteImage {
                        file: path.to_string_lossy().to_string(),
                        w: 560,
                        h: 360,
                    });
                }
            }
        }
        "lightbox" => {
            if let Some(n) = st.visible_notes().into_iter().find(|n| !n.images.is_empty()) {
                st.lb_open = true;
                st.lb_id = Some(n.id);
                st.lb_idx = 0;
            }
        }
        "viewer" => {
            if let Some(n) = st.visible_notes().into_iter().find(|n| !n.images.is_empty()) {
                st.lb_id = Some(n.id);
                st.lb_idx = 0;
                st.open_viewer();
            }
        }
        "empty" => st.notes.clear(),
        "paste" => {
            st.open_editor_new();
            if let Some(id) = st.edit_id {
                add_clipboard_image(st, id);
            }
        }
        "pastetake" => {
            // 走真实通路：置一次 Ctrl+V 意图，交给 draw_editor 自行 take 并贴图。
            st.open_editor_new();
            crate::platform::request_paste_for_test();
        }
        "addfile" => {
            st.open_editor_new();
            if let Some(id) = st.edit_id {
                if let Ok(p) = std::env::var("WINISLE_NOTE_FILE") {
                    add_image_from_path(st, id, std::path::Path::new(&p));
                }
            }
        }
        _ => {}
    }
    ui.ctx().request_repaint();
}

// ─────────────────────────── 便签墙 ───────────────────────────

fn draw_wall(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, a: u8, st: &mut NoteState) {
    let ink = Color32::from_rgba_unmultiplied(233, 236, 242, a);
    let muted = Color32::from_rgba_unmultiplied(150, 158, 175, a);
    let muted2 = Color32::from_rgba_unmultiplied(107, 113, 128, a);
    let accent = Color32::from_rgba_unmultiplied(52, 211, 153, a);

    let pad_x = 16.0;
    let pad_bottom = 14.0;
    let left = rect.left() + pad_x;
    let right = rect.right() - pad_x;
    let hd_top = rect.top() + 16.0;
    let hd_h = 26.0;

    // 标题 + 计数 pill（右上「N 张」）
    painter.text(Pos2::new(left, hd_top + hd_h * 0.5), Align2::LEFT_CENTER, "便签", crate::bold_font(17.0), ink);
    {
        let cnt = st.visible_notes().len();
        let pf = crate::bold_font(11.5);
        let txt = format!("{} 张", cnt);
        let pw = tw(ui, &txt, &pf) + 20.0;
        let ph = 19.5;
        let pr = Rect::from_min_size(Pos2::new(right - pw, hd_top + hd_h * 0.5 - ph * 0.5), Vec2::new(pw, ph));
        painter.rect_filled(pr, ph * 0.5, lay((255, 255, 255), 0.06, a));
        painter.text(pr.center(), Align2::CENTER_CENTER, txt, pf, muted);
    }

    // 工具栏：全部 chip + 5 色点 + 搜索（点击搜索图标展开搜索框）
    let toolbar_top = hd_top + hd_h + 8.0;
    let row_h = 24.0;
    let dots_cy = toolbar_top + row_h * 0.5;
    let chip_h = 19.0;
    let chips_y = dots_cy - chip_h * 0.5;
    if st.search_open {
        // ---- 搜索态：占满工具栏的搜索框 + 关闭 ----
        let field = Rect::from_min_max(Pos2::new(left, chips_y - 2.0), Pos2::new(right - 28.0, chips_y + chip_h + 2.0));
        painter.rect_filled(field, 9.0, lay((255, 255, 255), 0.05, a));
        painter.rect_stroke(field, 9.0, Stroke::new(1.0, lay((52, 211, 153), 0.30, a)), StrokeKind::Inside);
        let sid = egui::Id::new("note_search_input");
        let _sresp = note_text_field(ui, painter, sid, field.shrink2(Vec2::new(10.0, 0.0)), &mut st.search_q, 12.0, 12.0, false, 0, "搜索便签…", 2.0, 0.0, a, true);
        let mem_now = ui.ctx().memory(|m| m.has_focus(sid));
        if st.search_need_focus {
            if !mem_now {
                ui.ctx().memory_mut(|m| m.request_focus(sid));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                st.search_focus_tries = st.search_focus_tries.saturating_sub(1);
                if st.search_focus_tries == 0 { st.search_need_focus = false; }
            } else {
                st.search_need_focus = false;
            }
        }
        let xc = Pos2::new(right - 12.0, dots_cy);
        painter.line_segment([Pos2::new(xc.x - 4.0, xc.y - 4.0), Pos2::new(xc.x + 4.0, xc.y + 4.0)], Stroke::new(1.4, muted));
        painter.line_segment([Pos2::new(xc.x - 4.0, xc.y + 4.0), Pos2::new(xc.x + 4.0, xc.y - 4.0)], Stroke::new(1.4, muted));
        let xr = Rect::from_center_size(xc, Vec2::new(22.0, 22.0));
        if ui.interact(xr, ui.id().with("note_search_close"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.search_open = false;
            st.search_q.clear();
            st.search_need_focus = false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            st.search_open = false;
            st.search_q.clear();
        }
    } else {
        // ---- 常态：全部 chip + 5 色点 + 搜索按钮 ----
        let chip_font = FontId::proportional(11.0);
        let w = tw(ui, "全部", &chip_font) + 20.0;
        let r = Rect::from_min_size(Pos2::new(left, chips_y), Vec2::new(w, chip_h));
        let on = st.filter < 0;
        let fill = |al: f32| lay((52, 211, 153), al, a);
        painter.rect_filled(r, chip_h * 0.5, if on { fill(0.14) } else { lay((255, 255, 255), 0.05, a) });
        painter.rect_stroke(r, chip_h * 0.5, Stroke::new(1.0, if on { fill(0.26) } else { lay((255, 255, 255), 0.10, a) }), StrokeKind::Inside);
        painter.text(r.center(), Align2::CENTER_CENTER, "全部", chip_font.clone(), if on { accent } else { muted2 });
        if ui.interact(r, ui.id().with("chip_all"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.filter = -1;
        }
        let dots_x0 = r.right() + 9.0;
        for (i, col) in COLORS.iter().enumerate() {
            let cc = Pos2::new(dots_x0 + 6.0 + i as f32 * 20.0, dots_cy);
            let on = st.filter == i as i8;
            let hr = Rect::from_center_size(cc, Vec2::new(20.0, 20.0));
            if on {
                painter.circle_stroke(cc, 7.6, Stroke::new(1.6, Color32::from_rgba_unmultiplied(col.0, col.1, col.2, a)));
            }
            let a_dot = if on { a } else { (a as f32 * 0.78) as u8 };
            painter.circle_filled(cc, 6.0, Color32::from_rgba_unmultiplied(col.0, col.1, col.2, a_dot));
            if ui.interact(hr, ui.id().with(("cdot", i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                st.filter = i as i8;
            }
        }
        // 搜索按钮（放大镜）
        let sb = Rect::from_center_size(Pos2::new(right - 12.0, dots_cy), Vec2::new(24.0, 24.0));
        painter.rect_filled(sb, 8.0, lay((255, 255, 255), 0.04, a));
        painter.rect_stroke(sb, 8.0, Stroke::new(1.0, lay((255, 255, 255), 0.06, a)), StrokeKind::Inside);
        let sc = Pos2::new(sb.center().x - 1.0, sb.center().y - 1.0);
        painter.circle_stroke(sc, 4.6, Stroke::new(1.4, muted));
        painter.line_segment([Pos2::new(sc.x + 3.4, sc.y + 3.4), Pos2::new(sc.x + 7.0, sc.y + 7.0)], Stroke::new(1.4, muted));
        if ui.interact(sb, ui.id().with("note_search_btn"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.search_open = true;
            st.search_need_focus = true;
            st.search_focus_tries = 0;
        }
    }

    // 列表占满内容区高度（底部快捷记行已移除，改为网格末尾「新建便签」占位卡）
    let list_top = toolbar_top + row_h + 9.0;
    let list_bottom = rect.bottom() - pad_bottom;
    let list_rect = Rect::from_min_max(Pos2::new(left, list_top), Pos2::new(right, list_bottom));

    let visible = st.visible_notes();

    // 滚动几何
    let cols = 2usize;
    let card_w = (list_rect.width() - GRID_GAP) / 2.0;
    // 网格卡片数 = 便签数 + 1 个末尾「新建便签」占位卡（空态用整块 empty-wall，不计）
    let card_count = if visible.is_empty() { 0 } else { visible.len() + 1 };
    let rows = (card_count + cols - 1) / cols;
    let content_h = if rows == 0 {
        0.0
    } else {
        rows as f32 * CARD_H + (rows.saturating_sub(1)) as f32 * GRID_GAP
    };
    let max_scroll = (content_h - list_rect.height()).max(0.0);
    // 调试：强制显示滚动条（仅截图核对用）
    if std::env::var("WINISLE_NOTE_SCROLL").is_ok() {
        st.scroll = max_scroll * 0.6;
        st.scroll_target = st.scroll;
        st.scroll_vis = 1.0;
    }
    if ui.rect_contains_pointer(list_rect) {
        let sd = ui.input(|i| i.smooth_scroll_delta.y);
        if sd.abs() > 0.1 {
            st.scroll_target = (st.scroll_target - sd).clamp(0.0, max_scroll);
            st.scroll_act = Instant::now();
        }
    }
    st.scroll_target = st.scroll_target.clamp(0.0, max_scroll);
    let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
    st.scroll += (st.scroll_target - st.scroll) * (1.0 - (-dt / 0.07).exp());
    let scrolling = (st.scroll_target - st.scroll).abs() > 0.4 || st.scroll_act.elapsed().as_secs_f32() < 1.2;
    let want_vis = if scrolling { 1.0 } else { 0.0 };
    st.scroll_vis += (want_vis - st.scroll_vis) * (1.0 - (-dt / 0.12).exp());
    if scrolling {
        ui.ctx().request_repaint();
    }

    // 空状态：虚线圆角框 + 便签图标 + 提示（点击即新建，对齐原型 .empty-wall）
    if visible.is_empty() {
        draw_dashed_round_rect(painter, list_rect.shrink(0.75), 13.0, Stroke::new(1.5, lay((255, 255, 255), 0.14, a)), 6.0, 5.0);
        let cx = list_rect.center().x;
        let cy = list_rect.center().y;
        draw_doc_icon(painter, Pos2::new(cx, cy - 24.0), 30.0, Color32::from_rgba_unmultiplied(107, 113, 128, (a as f32 * 0.85) as u8));
        painter.text(Pos2::new(cx, cy + 9.0), Align2::CENTER_CENTER, if st.filter < 0 { "还没有便签" } else { "该颜色暂无便签" }, FontId::proportional(12.0), muted);
        painter.text(Pos2::new(cx, cy + 30.0), Align2::CENTER_CENTER, "点这里新建第一张便签", FontId::proportional(10.5), Color32::from_rgba_unmultiplied(107, 113, 128, (a as f32 * 0.9) as u8));
        if ui.interact(list_rect, ui.id().with("note_empty_add"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.open_editor_new();
        }
    }

    // 卡片
    let clip = painter.with_clip_rect(list_rect);
    let mut actions: Vec<(u64, bool)> = Vec::new(); // (id, open_lightbox)
    let mut delete_id: Option<u64> = None;
    for (i, n) in visible.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = list_rect.left() + col as f32 * (card_w + GRID_GAP);
        let y = list_rect.top() + row as f32 * (CARD_H + GRID_GAP) - st.scroll;
        let card = Rect::from_min_size(Pos2::new(x, y), Vec2::new(card_w, CARD_H));
        if card.bottom() < list_rect.top() - 4.0 || card.top() > list_rect.bottom() + 4.0 {
            continue;
        }
        let col_rgb = COLORS[n.color as usize % 5];
        let (tf, tring) = if n.pinned { (0.16f32, 0.55f32) } else { (0.11f32, 0.42f32) };
        let fillc = pre_blend(col_rgb, tf);
        let ringc = pre_blend(col_rgb, tring);
        clip.rect_filled(card, 11.0, Color32::from_rgba_unmultiplied(fillc.0, fillc.1, fillc.2, a));
        clip.rect_stroke(card, 11.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(ringc.0, ringc.1, ringc.2, a)), StrokeKind::Inside);

        let inner = card.shrink2(Vec2::new(11.0, 9.0));
        let has_img = !n.images.is_empty();
        let mut text_left = inner.left();
        let mut thumb_rect: Option<Rect> = None;
        let meta_h = 13.0;
        if has_img {
            let tw_ = 56.0;
            let tr = Rect::from_min_size(Pos2::new(inner.left(), inner.top()), Vec2::new(tw_, inner.height()));
            let tex = st.thumb(ui.ctx(), &n.images[0].file);
            if let Some(t) = tex {
                clip.image(t.id(), tr, Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)), Color32::WHITE);
            } else {
                clip.rect_filled(tr, 8.0, lay(col_rgb, 0.22, a));
            }
            if n.images.len() > 1 {
                let badge = Rect::from_min_size(Pos2::new(tr.right() - 24.0, tr.bottom() - 15.0), Vec2::new(22.0, 13.0));
                clip.rect_filled(badge, 5.0, Color32::from_rgba_unmultiplied(9, 11, 15, (a as f32 * 0.66) as u8));
                clip.text(badge.center(), Align2::CENTER_CENTER, format!("+{}", n.images.len() - 1), FontId::proportional(8.5), Color32::from_rgba_unmultiplied(233, 236, 242, a));
            }
            thumb_rect = Some(tr);
            text_left = tr.right() + 9.0;
        }

        // 文本（顶部对齐，最低到 meta 上沿，超出裁剪）
        let text_rect = Rect::from_min_max(
            Pos2::new(text_left, inner.top()),
            Pos2::new(inner.right(), inner.bottom() - meta_h - 1.0),
        );
        let wrap_w = (inner.right() - text_left).max(20.0);
        let galley = clamped_galley(ui, &n.text, FontId::proportional(11.5), ink, wrap_w, text_rect.height());
        clip.with_clip_rect(text_rect).galley(text_rect.left_top(), galley, ink);

        // 底部 meta：色点 + 相对时间 +（置顶时）右下角图钉
        let meta_cy = inner.bottom() - meta_h * 0.5;
        let dot_c = Pos2::new(text_left + 2.5, meta_cy);
        clip.circle_filled(dot_c, 2.5, Color32::from_rgba_unmultiplied(col_rgb.0, col_rgb.1, col_rgb.2, a));
        clip.text(Pos2::new(text_left + 10.0, meta_cy), Align2::LEFT_CENTER, rel_time(n.updated), FontId::proportional(9.5), muted2);
        if n.pinned {
            draw_pin(&clip, Pos2::new(inner.right() - 3.0, meta_cy), col_rgb, a);
        }

        // 右上角删除按钮（无填充；悬停显深底圆 + 红叉）
        let del_c = Pos2::new(card.right() - 12.0, card.top() + 12.0);
        let del_r = Rect::from_center_size(del_c, Vec2::new(22.0, 22.0));
        let del_hover = ui.rect_contains_pointer(del_r);
        if del_hover {
            clip.circle_filled(del_c, 10.0, Color32::from_rgba_unmultiplied(9, 11, 15, (a as f32 * 0.55) as u8));
        }
        let xc = if del_hover {
            Color32::from_rgba_unmultiplied(242, 112, 92, a)
        } else {
            Color32::from_rgba_unmultiplied(150, 158, 175, (a as f32 * 0.7) as u8)
        };
        clip.line_segment([Pos2::new(del_c.x - 3.2, del_c.y - 3.2), Pos2::new(del_c.x + 3.2, del_c.y + 3.2)], Stroke::new(1.4, xc));
        clip.line_segment([Pos2::new(del_c.x - 3.2, del_c.y + 3.2), Pos2::new(del_c.x + 3.2, del_c.y - 3.2)], Stroke::new(1.4, xc));

        let resp = ui.interact(card, ui.id().with(("note_card", n.id)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        // 删除按钮最后注册（层级更高，点在叉上优先触发删除而非打开）
        let del_clicked = ui.interact(del_r, ui.id().with(("note_del", n.id)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        if del_clicked {
            delete_id = Some(n.id);
        } else if resp.clicked() {
            let on_thumb = thumb_rect
                .map(|tr| resp.interact_pointer_pos().map(|p| tr.contains(p)).unwrap_or(false))
                .unwrap_or(false);
            actions.push((n.id, on_thumb && has_img));
        }
    }

    if let Some(did) = delete_id {
        st.delete_note(did);
    }

    for (id, open_lb) in actions {
        if let Some(n) = st.notes.iter().find(|n| n.id == id).cloned() {
            if open_lb {
                st.lb_open_for(&n);
            } else {
                st.open_editor_note(&n);
            }
        }
    }

    // 末尾「新建便签」占位卡（点击进入编辑器，替代原底部快捷记行）
    if !visible.is_empty() {
        let i = visible.len();
        let col = i % cols;
        let row = i / cols;
        let x = list_rect.left() + col as f32 * (card_w + GRID_GAP);
        let y = list_rect.top() + row as f32 * (CARD_H + GRID_GAP) - st.scroll;
        let card = Rect::from_min_size(Pos2::new(x, y), Vec2::new(card_w, CARD_H));
        if card.bottom() >= list_rect.top() - 4.0 && card.top() <= list_rect.bottom() + 4.0 {
            clip.rect_filled(card, 11.0, lay((255, 255, 255), 0.03, a));
            draw_dashed_round_rect(&clip, card.shrink(0.75), 11.0, Stroke::new(1.4, lay((255, 255, 255), 0.16, a)), 6.0, 5.0);
            let cc = card.center();
            let pc = Pos2::new(cc.x, cc.y - 8.0);
            clip.circle_stroke(pc, 11.0, Stroke::new(1.6, muted2));
            clip.line_segment([Pos2::new(pc.x - 5.0, pc.y), Pos2::new(pc.x + 5.0, pc.y)], Stroke::new(1.6, muted2));
            clip.line_segment([Pos2::new(pc.x, pc.y - 5.0), Pos2::new(pc.x, pc.y + 5.0)], Stroke::new(1.6, muted2));
            clip.text(Pos2::new(cc.x, cc.y + 16.0), Align2::CENTER_CENTER, "新建便签", FontId::proportional(10.5), muted2);
            if ui.interact(card, ui.id().with("note_addcard"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                st.open_editor_new();
            }
        }
    }

    // 隐藏滚动条（仅滚动/惯性/1.2s 内出现）——贴内容区最右侧，与天气页/待办页一致
    if max_scroll > 4.0 && st.scroll_vis > 0.01 {
        let sv = st.scroll_vis.clamp(0.0, 1.0);
        let tx = rect.right() - 4.0;
        let track = Rect::from_min_max(Pos2::new(tx, list_rect.top() + 2.0), Pos2::new(tx + 2.0, list_rect.bottom() - 2.0));
        painter.rect_filled(track, 1.0, Color32::from_rgba_unmultiplied(255, 255, 255, (12.0 * sv) as u8));
        let frac = list_rect.height() / content_h.max(1.0);
        let th = (track.height() * frac).max(14.0);
        let range = (track.height() - th).max(1.0);
        let tp = (st.scroll / max_scroll.max(1.0)) * range;
        let thumb = Rect::from_min_size(Pos2::new(tx, track.top() + tp), Vec2::new(2.0, th));
        painter.rect_filled(thumb, 1.0, Color32::from_rgba_unmultiplied(255, 255, 255, (60.0 * sv) as u8));
    }
}

// ─────────────────────────── 编辑器 ───────────────────────────

fn draw_editor(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, a: u8, st: &mut NoteState, bg: [u8; 3], t: f32) {
    let ink = Color32::from_rgba_unmultiplied(233, 236, 242, a);
    let muted = Color32::from_rgba_unmultiplied(150, 158, 175, a);
    let muted2 = Color32::from_rgba_unmultiplied(107, 113, 128, a);
    let accent = Color32::from_rgba_unmultiplied(52, 211, 153, a);
    let danger = Color32::from_rgba_unmultiplied(242, 112, 92, a);

    // 覆盖层：内容区右侧圆角随岛体，左侧直角（AGENTS 硬约定第 8 条）。
    // 透明度随过渡动画 t 变化，实现打开/返回的淡入淡出。
    let cr = (rect.height() * 0.5).min(16.0).min(rect.width() * 0.5).round() as u8;
    let bg_a = (255.0 * t).clamp(0.0, 255.0) as u8;
    painter.rect_filled(rect, CornerRadius { nw: 0, ne: cr, sw: 0, se: cr }, Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg_a));

    let pad_x = 16.0;
    let pad_bottom = 14.0;
    let left = rect.left() + pad_x;
    let right = rect.right() - pad_x;
    let hd_top = rect.top() + 16.0;
    let hd_h = 26.0;

    let is_existing = st.edit_was_existing;
    let title = if is_existing { "编辑便签" } else { "新建便签" };
    painter.text(Pos2::new(left, hd_top + hd_h * 0.5), Align2::LEFT_CENTER, title, crate::bold_font(17.0), ink);

    // 右上返回箭头（‹）
    let bx = Pos2::new(right - 10.0, hd_top + hd_h * 0.5);
    painter.line_segment([Pos2::new(bx.x + 3.0, bx.y - 6.0), Pos2::new(bx.x - 3.0, bx.y)], Stroke::new(1.7, muted));
    painter.line_segment([Pos2::new(bx.x - 3.0, bx.y), Pos2::new(bx.x + 3.0, bx.y + 6.0)], Stroke::new(1.7, muted));
    if ui.interact(Rect::from_center_size(bx, Vec2::new(28.0, 28.0)), ui.id().with("ed_close"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        st.editor_open = false;
        return;
    }

    // ---- 纵向布局 ----
    let btn_h = 34.0;
    let gap = 8.0;
    let btn_y = rect.bottom() - pad_bottom - btn_h;
    let show_del = is_existing;
    // 三个按钮统一尺寸、作为一组靠右对齐（与左侧「置顶」形成左右呼应）
    let bw = 84.0;
    let count = if show_del { 3.0 } else { 2.0 };
    let group_w = bw * count + gap * (count - 1.0);
    let start = right - group_w;
    let del_rect = Rect::from_min_size(Pos2::new(start, btn_y), Vec2::new(bw, btn_h));
    let cancel_rect = Rect::from_min_size(Pos2::new(start + if show_del { bw + gap } else { 0.0 }, btn_y), Vec2::new(bw, btn_h));
    let save_rect = Rect::from_min_size(Pos2::new(start + (if show_del { 2.0 } else { 1.0 }) * (bw + gap), btn_y), Vec2::new(bw, btn_h));
    let efoot_h = 23.0;
    let efoot_top = btn_y - 10.0 - efoot_h;
    let has_strip = false; // 贴图功能暂停：编辑器不再显示图片条/添加入口
    let strip_h = 50.0;
    let strip_top = efoot_top - 8.0 - strip_h;
    let ta_bottom = if has_strip { strip_top - 8.0 } else { efoot_top - 8.0 };
    let ed_top = hd_top + hd_h + 11.0;
    let prow_h = 20.0;
    let ta_top = ed_top + prow_h + 10.0;
    let ta_rect = Rect::from_min_max(Pos2::new(left, ta_top), Pos2::new(right, ta_bottom.max(ta_top + 30.0)));

    // 颜色行（便签颜色 + 5 色点）——放在顶部（对齐原型 .prow）
    let prow_cy = ed_top + prow_h * 0.5;
    let lab_font = FontId::proportional(10.0);
    painter.text(Pos2::new(left, prow_cy), Align2::LEFT_CENTER, "便签颜色", lab_font.clone(), muted2);
    let mut dx = left + tw(ui, "便签颜色", &lab_font) + 12.0;
    for (i, col) in COLORS.iter().enumerate() {
        let c = Pos2::new(dx + 8.5, prow_cy);
        let rr = Rect::from_center_size(c, Vec2::new(26.0, 26.0));
        let cc = Color32::from_rgba_unmultiplied(col.0, col.1, col.2, a);
        if st.edit_color == i as u8 {
            painter.circle_stroke(c, 9.5, Stroke::new(1.6, cc));
            painter.circle_filled(c, 6.0, cc);
        } else {
            painter.circle_filled(c, 8.5, Color32::from_rgba_unmultiplied(col.0, col.1, col.2, (a as f32 * 0.85) as u8));
        }
        if ui.interact(rr, ui.id().with(("ed_color", i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.edit_color = i as u8;
        }
        dx += 26.0 + 10.0;
    }

    // 文本域（.ta：1px 内描边 + 11/13 内边距）；统一走 note_text_field（坑 G）
    painter.rect_stroke(ta_rect, 11.0, Stroke::new(1.0, lay((255, 255, 255), 0.14, a)), StrokeKind::Inside);
    let text_rect = ta_rect.shrink2(Vec2::new(13.0, 11.0));
    let ed_id = egui::Id::new(("note_editor_input", st.edit_id.unwrap_or(0)));
    // 每次打开便签都清掉该输入框的 TextEdit 光标状态，避免沿用上一条便签的光标位置（默认落到末尾）
    if st.edit_reset_cursor {
        ui.ctx().data_mut(|d| d.remove::<egui::widgets::text_edit::TextEditState>(ed_id));
        st.edit_reset_cursor = false;
    }
    let ph = "";
    let show_ph = false;
    // 过渡（打开/返回淡入淡出）期间不画占位文字；且只有在编辑器真正打开时才允许画，
    // 避免“返回”那一帧 editor_open 已为 false 但动画值仍≈1 时占位字闪一下。
    let _resp = note_text_field(ui, painter, ed_id, text_rect, &mut st.edit_text, 12.5, 12.5, true, 3, ph, 0.0, 2.0, a, show_ph);
    let mem_now = ui.ctx().memory(|m| m.has_focus(ed_id));
    if st.edit_need_focus {
        if !mem_now {
            ui.ctx().memory_mut(|m| m.request_focus(ed_id));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            st.edit_focus_tries = st.edit_focus_tries.saturating_sub(1);
            if st.edit_focus_tries == 0 { st.edit_need_focus = false; }
        } else {
            st.edit_need_focus = false;
        }
    }

    // 图片条（仅在有图时显示，对齐原型 .imgs）
    if has_strip {
        let mut ix = left;
        let mut remove_idx: Option<usize> = None;
        let cur_imgs = st.edit_images.clone();
        for (i, im) in cur_imgs.iter().enumerate() {
            let tr = Rect::from_min_size(Pos2::new(ix, strip_top), Vec2::new(strip_h, strip_h));
            let tex = st.thumb(ui.ctx(), &im.file);
            if let Some(t) = tex {
                painter.image(t.id(), tr, Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)), Color32::WHITE);
            } else {
                painter.rect_filled(tr, 9.0, lay((120, 126, 142), 0.25, a));
            }
            painter.rect_stroke(tr, 9.0, Stroke::new(1.0, lay((255, 255, 255), 0.10, a)), StrokeKind::Inside);
            let xr = Rect::from_min_size(Pos2::new(tr.right() - 16.0, tr.top() + 2.0), Vec2::new(14.0, 14.0));
            painter.circle_filled(xr.center(), 7.0, Color32::from_rgba_unmultiplied(9, 11, 15, (a as f32 * 0.72) as u8));
            let xp = xr.center();
            painter.line_segment([Pos2::new(xp.x - 2.6, xp.y - 2.6), Pos2::new(xp.x + 2.6, xp.y + 2.6)], Stroke::new(1.2, Color32::from_rgba_unmultiplied(223, 228, 238, a)));
            painter.line_segment([Pos2::new(xp.x - 2.6, xp.y + 2.6), Pos2::new(xp.x + 2.6, xp.y - 2.6)], Stroke::new(1.2, Color32::from_rgba_unmultiplied(223, 228, 238, a)));
            if ui.interact(xr, ui.id().with(("ed_rmimg", i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                remove_idx = Some(i);
            }
            ix += strip_h + 8.0;
        }
        if let Some(ri) = remove_idx {
            st.edit_images.remove(ri);
        }
        // 添加图片块（虚线感方块 + 图标 + 文案）
        let add_rect = Rect::from_min_size(Pos2::new(ix, strip_top), Vec2::new(strip_h, strip_h));
        let ad = Color32::from_rgba_unmultiplied(150, 158, 175, (a as f32 * 0.7) as u8);
        draw_dashed_round_rect(painter, add_rect, 9.0, Stroke::new(1.0, lay((255, 255, 255), 0.22, a)), 4.0, 3.0);
        let ac = add_rect.center();
        painter.line_segment([Pos2::new(ac.x - 5.0, ac.y - 4.0), Pos2::new(ac.x + 5.0, ac.y - 4.0)], Stroke::new(1.3, ad));
        painter.line_segment([Pos2::new(ac.x, ac.y - 9.0), Pos2::new(ac.x, ac.y + 1.0)], Stroke::new(1.3, ad));
        painter.text(Pos2::new(ac.x, ac.y + 10.0), Align2::CENTER_CENTER, "添加图片", FontId::proportional(8.5), muted2);
        if ui.interact(add_rect, ui.id().with("ed_addimg"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if let Some(id) = st.edit_id {
                if let Some(p) = pick_image_file() {
                    add_image_from_path(st, id, &p);
                }
            }
        }
        let px = ix + strip_h + 10.0;
        let phf = FontId::proportional(9.5);
        painter.text(Pos2::new(px, strip_top + strip_h * 0.5 - 8.0), Align2::LEFT_CENTER, "点「添加图片」选文件", phf.clone(), muted2);
        painter.text(Pos2::new(px, strip_top + strip_h * 0.5 + 8.0), Align2::LEFT_CENTER, "或 Ctrl+V 粘贴截图", phf, muted2);
    }

    // 底部 efoot：置顶 chip（左）+ 字数（右）
    let pin_chip = Rect::from_min_size(Pos2::new(left, efoot_top + (efoot_h - 22.0) * 0.5), Vec2::new(64.0, 22.0));
    let on = st.edit_pinned;
    // 未置顶：中性灰（不染绿）；置顶后：绿色点亮
    let pin_col: (u8, u8, u8) = if on { (52, 211, 153) } else { (150, 158, 175) };
    painter.rect_filled(pin_chip, 11.0, if on { lay((52, 211, 153), 0.12, a) } else { lay((255, 255, 255), 0.04, a) });
    painter.rect_stroke(pin_chip, 11.0, Stroke::new(1.0, if on { lay((52, 211, 153), 0.22, a) } else { lay((255, 255, 255), 0.08, a) }), StrokeKind::Inside);
    draw_pin(painter, Pos2::new(pin_chip.left() + 14.0, pin_chip.center().y), pin_col, a);
    painter.text(Pos2::new(pin_chip.left() + 24.0, pin_chip.center().y), Align2::LEFT_CENTER, "置顶", FontId::proportional(11.0), if on { accent } else { muted });
    if ui.interact(pin_chip, ui.id().with("ed_pin"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        st.edit_pinned = !on;
    }
    // 空便签：常显「添加图片」入口（贴图功能暂停，入口一并移除）
    let _ = has_strip;
    let cc_txt = format!("{} / 500", st.edit_text.chars().count());
    painter.text(Pos2::new(right, efoot_top + efoot_h * 0.5), Align2::RIGHT_CENTER, cc_txt, FontId::proportional(10.0), muted2);

    // 删除（无填充，仅危险色文字）| 取消 | 保存便签（仅编辑已有便签时显示删除）
    if show_del {
        let del_hover = ui.rect_contains_pointer(del_rect);
        let dcol = if del_hover { danger } else { Color32::from_rgba_unmultiplied(242, 112, 92, (a as f32 * 0.85) as u8) };
        // 无填充，仅危险色描边 + 文字（与取消/保存按钮同尺寸同圆角）
        painter.rect_stroke(del_rect, 10.0, Stroke::new(1.0, lay((242, 112, 92), if del_hover { 0.44 } else { 0.26 }, a)), StrokeKind::Inside);
        painter.text(del_rect.center(), Align2::CENTER_CENTER, "删除", crate::bold_font(12.5), dcol);
        if ui.interact(del_rect, ui.id().with("ed_delete"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if let Some(id) = st.edit_id { st.delete_note(id); }
            st.editor_open = false;
            return;
        }
    }
    // 取消 / 保存便签
    button(ui, painter, cancel_rect, "取消", false, a, &muted, &ink);
    if ui.interact(cancel_rect, ui.id().with("ed_cancel"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        st.editor_open = false;
        return;
    }
    button(ui, painter, save_rect, "保存便签", true, a, &accent, &ink);
    if ui.interact(save_rect, ui.id().with("ed_save"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        st.commit_editor();
        return;
    }

    // Esc 关闭
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.editor_open = false;
    }
}

fn button(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, label: &str, filled: bool, a: u8, accent: &Color32, ink: &Color32) {
    let hovered = ui.rect_contains_pointer(rect);
    if filled {
        // primary：薄荷 16% 底 + 28% 描边 + 薄荷文字（对齐原型 .btn.primary）
        let bg = if hovered { 0.22 } else { 0.16 };
        let ring = if hovered { 0.36 } else { 0.28 };
        painter.rect_filled(rect, 10.0, lay((52, 211, 153), bg, a));
        painter.rect_stroke(rect, 10.0, Stroke::new(1.0, lay((52, 211, 153), ring, a)), StrokeKind::Inside);
        painter.text(rect.center(), Align2::CENTER_CENTER, label, crate::bold_font(12.5), *accent);
    } else {
        // ghost：白 5% 底 + 8% 描边 + muted 文字（对齐原型 .btn.ghost）
        let bg = if hovered { 0.08 } else { 0.05 };
        painter.rect_filled(rect, 10.0, lay((255, 255, 255), bg, a));
        painter.rect_stroke(rect, 10.0, Stroke::new(1.0, lay((255, 255, 255), 0.08, a)), StrokeKind::Inside);
        painter.text(rect.center(), Align2::CENTER_CENTER, label, crate::bold_font(12.5), Color32::from_rgba_unmultiplied(150, 158, 175, a));
    }
    let _ = ink;
}

// ─────────────────────────── 岛内灯箱 ───────────────────────────

fn draw_lightbox(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, a: u8, st: &mut NoteState, radius: f32) {
    let ink = Color32::from_rgba_unmultiplied(233, 236, 242, a);
    let muted = Color32::from_rgba_unmultiplied(150, 158, 175, a);
    let muted2 = Color32::from_rgba_unmultiplied(107, 113, 128, a);

    let cr = radius.min(rect.height() * 0.5).min(rect.width() * 0.5).round() as u8;
    painter.rect_filled(rect, CornerRadius { nw: 0, ne: cr, sw: 0, se: cr }, Color32::from_rgba_unmultiplied(8, 10, 14, (a as f32 * 0.80) as u8));

    let Some(id) = st.lb_id else { st.lb_open = false; return };
    let Some(nimg) = st.notes.iter().find(|n| n.id == id).map(|n| n.images.clone()) else {
        st.lb_open = false;
        return;
    };
    if nimg.is_empty() {
        st.lb_open = false;
        return;
    }
    let n = nimg.len();
    if st.lb_idx >= n {
        st.lb_idx = 0;
    }

    // 顶栏
    let topbar_h = 30.0;
    painter.text(Pos2::new(rect.left() + 14.0, rect.top() + topbar_h * 0.5), Align2::LEFT_CENTER, format!("图片 {}/{}", st.lb_idx + 1, n), FontId::proportional(11.5), muted);

    // 关闭 ×
    let xc = Pos2::new(rect.right() - 16.0, rect.top() + topbar_h * 0.5);
    painter.line_segment([Pos2::new(xc.x - 5.0, xc.y - 5.0), Pos2::new(xc.x + 5.0, xc.y + 5.0)], Stroke::new(1.6, muted));
    painter.line_segment([Pos2::new(xc.x - 5.0, xc.y + 5.0), Pos2::new(xc.x + 5.0, xc.y - 5.0)], Stroke::new(1.6, muted));
    if ui.interact(Rect::from_center_size(xc, Vec2::new(26.0, 26.0)), ui.id().with("lb_close"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        st.lb_open = false;
        return;
    }
    // 在新窗口打开
    let win_label = "⤢ 在新窗口打开";
    let wf = FontId::proportional(10.0);
    let ww = tw(ui, win_label, &wf) + 18.0;
    let win_rect = Rect::from_min_size(Pos2::new(rect.right() - 30.0 - ww, rect.top() + topbar_h * 0.5 - 11.0), Vec2::new(ww, 22.0));
    painter.rect_filled(win_rect, 11.0, lay((52, 211, 153), 0.14, a));
    painter.rect_stroke(win_rect, 11.0, Stroke::new(1.0, lay((52, 211, 153), 0.34, a)), StrokeKind::Inside);
    painter.text(win_rect.center(), Align2::CENTER_CENTER, win_label, wf, Color32::from_rgba_unmultiplied(52, 211, 153, a));
    if ui.interact(win_rect, ui.id().with("lb_win"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        st.open_viewer();
    }

    // 底部 caption 区
    let cap_h = 26.0;
    let stage = Rect::from_min_max(
        Pos2::new(rect.left() + 8.0, rect.top() + topbar_h + 4.0),
        Pos2::new(rect.right() - 8.0, rect.bottom() - cap_h),
    );

    // 图片（contain 撑满）
    let ni = &nimg[st.lb_idx];
    let texx = st.thumb(ui.ctx(), &ni.file);
    let (iw, ih) = ((ni.w.max(1)) as f32, (ni.h.max(1)) as f32);
    let fit = (stage.width() / iw).min(stage.height() / ih);
    let disp = Vec2::new(iw * fit, ih * fit);
    let ir = Rect::from_center_size(stage.center(), disp);
    if let Some(t) = texx {
        painter.image(t.id(), ir, Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)), Color32::WHITE);
    } else {
        painter.rect_filled(ir, 8.0, Color32::from_rgba_unmultiplied(40, 44, 54, a));
    }

    // 左右切换（多图）
    if n > 1 {
        // 左
        let lc = Pos2::new(rect.left() + 18.0, stage.center().y);
        let lr = Rect::from_center_size(lc, Vec2::new(30.0, 44.0));
        painter.circle_filled(lc, 15.0, Color32::from_rgba_unmultiplied(12, 14, 18, (a as f32 * 0.55) as u8));
        painter.line_segment([Pos2::new(lc.x + 3.0, lc.y - 6.0), Pos2::new(lc.x - 3.0, lc.y)], Stroke::new(1.6, ink));
        painter.line_segment([Pos2::new(lc.x - 3.0, lc.y), Pos2::new(lc.x + 3.0, lc.y + 6.0)], Stroke::new(1.6, ink));
        if ui.interact(lr, ui.id().with("lb_prev"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.lb_idx = (st.lb_idx + n - 1) % n;
        }
        // 右
        let rc = Pos2::new(rect.right() - 18.0, stage.center().y);
        let rr = Rect::from_center_size(rc, Vec2::new(30.0, 44.0));
        painter.circle_filled(rc, 15.0, Color32::from_rgba_unmultiplied(12, 14, 18, (a as f32 * 0.55) as u8));
        painter.line_segment([Pos2::new(rc.x - 3.0, rc.y - 6.0), Pos2::new(rc.x + 3.0, rc.y)], Stroke::new(1.6, ink));
        painter.line_segment([Pos2::new(rc.x - 3.0, rc.y), Pos2::new(rc.x + 3.0, rc.y + 6.0)], Stroke::new(1.6, ink));
        if ui.interact(rr, ui.id().with("lb_next"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.lb_idx = (st.lb_idx + 1) % n;
        }
    }

    // 底部圆点
    let dots_y = rect.bottom() - cap_h * 0.5;
    if n > 1 {
        let total = n as f32 * 10.0;
        let mut dx = rect.center().x - total * 0.5 + 5.0;
        for k in 0..n {
            let on = k == st.lb_idx;
            painter.circle_filled(Pos2::new(dx, dots_y), if on { 3.2 } else { 2.4 }, if on { Color32::from_rgba_unmultiplied(52, 211, 153, a) } else { lay((255, 255, 255), 0.35, a) });
            dx += 10.0;
        }
    } else {
        painter.text(Pos2::new(rect.center().x, dots_y), Align2::CENTER_CENTER, format!("{} × {}", ni.w, ni.h), FontId::proportional(9.5), muted2);
    }

    // 点空白关闭 / Esc
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.lb_open = false;
        return;
    }
    // 点击图片外空白关闭
    let bg_resp = ui.interact(stage, ui.id().with("lb_stage"), egui::Sense::click());
    if bg_resp.clicked() {
        let p = bg_resp.interact_pointer_pos().unwrap_or(Pos2::new(-1.0, -1.0));
        if !ir.contains(p) {
            st.lb_open = false;
        }
    }
    let _ = muted;
}

// ─────────────────────────── 独立查看器 ───────────────────────────

fn draw_viewer(ctx: &egui::Context, v: &mut ViewerData) {
    let dark = Color32::from_rgb(16, 17, 21);
    let bar = Color32::from_rgb(26, 27, 33);
    egui::TopBottomPanel::top("vtool")
        .frame(egui::Frame::default().fill(bar))
        .show(ctx, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("图片 {}/{}", v.idx + 1, v.images.len().max(1)))
                        .color(Color32::from_rgb(233, 236, 242))
                        .size(13.0),
                );
                ui.add_space(6.0);
                if v.images.len() > 1 {
                    if ui.button("‹").clicked() {
                        v.idx = (v.idx + v.images.len() - 1) % v.images.len();
                        v.zoom = 1.0;
                        v.pan = Vec2::ZERO;
                    }
                    if ui.button("›").clicked() {
                        v.idx = (v.idx + 1) % v.images.len();
                        v.zoom = 1.0;
                        v.pan = Vec2::ZERO;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("关闭").clicked() {
                        v.request_close = true;
                    }
                    if ui.button("1:1").clicked() {
                        v.zoom = 1.0;
                        v.pan = Vec2::ZERO;
                    }
                    if ui.button("＋").clicked() {
                        v.zoom = (v.zoom * 1.25).clamp(0.1, 8.0);
                    }
                    if ui.button("－").clicked() {
                        v.zoom = (v.zoom / 1.25).clamp(0.1, 8.0);
                    }
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", v.zoom * 100.0))
                            .color(Color32::from_rgb(150, 158, 175))
                            .size(12.0),
                    );
                });
            });
            ui.add_space(3.0);
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(dark))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, dark);
            let resp = ui.interact(rect, ui.id().with("vstage"), egui::Sense::click_and_drag());
            if resp.dragged() {
                v.pan += resp.drag_delta();
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                v.zoom = (v.zoom * (1.0 + scroll * 0.0015)).clamp(0.1, 8.0);
            }
            if resp.double_clicked() {
                v.zoom = 1.0;
                v.pan = Vec2::ZERO;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                v.request_close = true;
            }
            if let Some(ni) = v.images.get(v.idx).cloned() {
                if !v.tex.contains_key(&ni.file) {
                    if let Ok(img) = image::open(std::path::Path::new(&ni.file)) {
                        let rgba = img.to_rgba8();
                        let (w, h) = rgba.dimensions();
                        let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
                        let tex = ctx.load_texture(format!("note_view_{}", ni.file), ci, egui::TextureOptions::LINEAR);
                        v.tex.insert(ni.file.clone(), tex);
                    }
                }
                if let Some(tex) = v.tex.get(&ni.file).cloned() {
                    let sz = tex.size();
                    let (tw_, th_) = (sz[0] as f32, sz[1] as f32);
                    let fit = (rect.width() / tw_).min(rect.height() / th_) * 0.94;
                    let s = fit * v.zoom;
                    let disp = Vec2::new(tw_ * s, th_ * s);
                    let center = rect.center() + v.pan;
                    let ir = Rect::from_center_size(center, disp);
                    p.image(tex.id(), ir, Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                p.text(
                    Pos2::new(rect.left() + 12.0, rect.bottom() - 18.0),
                    Align2::LEFT_BOTTOM,
                    format!("{} × {}   ·   滚轮缩放 / 拖动平移 / 双击 1:1", ni.w, ni.h),
                    FontId::proportional(11.0),
                    Color32::from_rgb(120, 126, 142),
                );
            } else {
                p.text(rect.center(), Align2::CENTER_CENTER, "没有图片", FontId::proportional(14.0), Color32::from_rgb(150, 158, 175));
            }
        });
}

// ─────────────────────────── 工具 ───────────────────────────

fn tw(ui: &egui::Ui, s: &str, font: &FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x)
}

fn rel_time(updated: i64) -> String {
    if updated <= 0 {
        return String::new();
    }
    let now = unix_now();
    let d = (now - updated).max(0);
    if d < 60 {
        "刚刚".to_string()
    } else if d < 3600 {
        format!("{} 分钟前", d / 60)
    } else if d < 86400 {
        format!("{} 小时前", d / 3600)
    } else if d < 86400 * 2 {
        "昨天".to_string()
    } else if d < 86400 * 30 {
        format!("{} 天前", d / 86400)
    } else {
        // 日期
        let days = d / 86400;
        format!("{} 个月前", (days / 30).max(1))
    }
}

fn make_grad_png(path: &std::path::Path, w: u32, h: u32, c0: (u8, u8, u8), c1: (u8, u8, u8)) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut buf = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let t = (x as f32 / w as f32) * 0.6 + (y as f32 / h as f32) * 0.4;
            let lerp = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8 };
            buf.put_pixel(x, y, image::Rgba([lerp(c0.0, c1.0), lerp(c0.1, c1.1), lerp(c0.2, c1.2), 255]));
        }
    }
    let _ = image::DynamicImage::ImageRgba8(buf).save(path);
}

fn seed_demo(notes: &mut Vec<Note>) {
    let now = unix_now();
    notes.push(Note { id: 1, text: "买牛奶 + 鸡蛋，顺路取快递".into(), color: 0, pinned: false, updated: now - 120, images: Vec::new() });
    notes.push(Note { id: 2, text: "周五前把设计稿定稿\n配色再调一版，别太花".into(), color: 1, pinned: true, updated: now - 3600, images: Vec::new() });
    notes.push(Note { id: 3, text: "看到一句话：慢就是快。".into(), color: 3, pinned: false, updated: now - 7200, images: Vec::new() });
    notes.push(Note { id: 4, text: "想去的三个地方：京都 / 冰岛 / 大理".into(), color: 4, pinned: false, updated: now - 86400, images: Vec::new() });
    // 带贴图的便签（内存生成两张渐变图）
    let dir = media_dir(5);
    let p1 = dir.join("0.png");
    let p2 = dir.join("1.png");
    make_grad_png(&p1, 640, 400, (242, 112, 92), (230, 178, 60));
    make_grad_png(&p2, 420, 560, (91, 182, 230), (181, 140, 232));
    notes.push(Note {
        id: 5,
        text: "周末露台的花开了".into(),
        color: 2,
        pinned: false,
        updated: now - 600,
        images: vec![
            NoteImage { file: p1.to_string_lossy().to_string(), w: 640, h: 400 },
            NoteImage { file: p2.to_string_lossy().to_string(), w: 420, h: 560 },
        ],
    });
}
