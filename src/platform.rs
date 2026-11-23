//! Windows 平台适配：把 eframe/winit 创建出来的普通窗口，改造成"灵动岛"。
//! 全部使用原始 Win32 / DWM / 未文档化 user32 FFI，不引入额外 crate。
#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicUsize, Ordering};

pub type HWND = isize;
pub type WNDENUMPROC = Option<unsafe extern "system" fn(HWND, isize) -> i32>;

#[repr(C)]
pub struct MARGINS {
    pub cxLeftWidth: i32,
    pub cxRightWidth: i32,
    pub cyTopHeight: i32,
    pub cyBottomHeight: i32,
}

#[repr(C)]
#[derive(Default)]
struct AccentPolicy {
    AccentState: i32,
    AccentFlags: i32,
    GradientColor: u32,
    AnimationId: i32,
}

#[repr(C)]
struct WindowCompositionAttributeData {
    Attribute: i32,
    Data: *mut c_void,
    SizeOfData: usize,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SYSTEMTIME {
    pub wYear: u16,
    pub wMonth: u16,
    pub wDayOfWeek: u16,
    pub wDay: u16,
    pub wHour: u16,
    pub wMinute: u16,
    pub wSecond: u16,
    pub wMilliseconds: u16,
}

#[link(name = "user32")]
extern "system" {
    fn GetWindowLongPtrW(hWnd: HWND, nIndex: i32) -> isize;
    fn SetWindowLongPtrW(hWnd: HWND, nIndex: i32, dwNewLong: isize) -> isize;
    fn SetWindowPos(hWnd: HWND, hWndInsertAfter: HWND, X: i32, Y: i32, cx: i32, cy: i32, uFlags: u32) -> i32;
    fn SetWindowCompositionAttribute(hWnd: HWND, pData: *mut WindowCompositionAttributeData) -> i32;
    fn EnumWindows(lpEnumFunc: WNDENUMPROC, lParam: isize) -> i32;
    fn GetWindowTextW(hWnd: HWND, lpString: *mut u16, nMaxCount: i32) -> i32;
    fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut u32) -> u32;
    fn GetSystemMetrics(nIndex: i32) -> i32;
    fn GetDpiForWindow(hwnd: HWND) -> u32;
    fn ShowWindow(hwnd: HWND, nCmdShow: i32) -> i32;
    fn SetTimer(hWnd: HWND, nIDEvent: usize, uElapse: u32, lpTimerFunc: usize) -> usize;
    fn KillTimer(hWnd: HWND, uIDEvent: usize) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, dwAttribute: u32, pvAttribute: *const c_void, cbAttribute: u32) -> i32;
    fn DwmExtendFrameIntoClientArea(hwnd: HWND, pMarInset: *const MARGINS) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(lpSystemTime: *mut SYSTEMTIME);
    fn CreateMutexW(lpMutexAttributes: *mut c_void, bInitialOwner: i32, lpName: *const u16) -> *mut c_void;
    fn GetLastError() -> u32;
    fn GetCurrentProcessId() -> u32;
}

const ERROR_ALREADY_EXISTS: u32 = 183;

/// 单实例保护：创建一个命名互斥体。
/// 返回 true 表示"已经有一个 WinIsle 在运行"（本次为新启动的重复实例，应立即退出）。
/// 句柄故意不关闭，随进程存活保持有效；进程结束时由系统释放。
pub fn ensure_single_instance() -> bool {
    // 仅供开发/自动化测试：允许临时并行运行（正式版与开发版互不干扰地截图验证）。
    if std::env::var_os("WINISLE_ALLOW_MULTI").is_some() {
        return false;
    }
    // Local\ 前缀：同一登录会话内唯一，不影响其他用户/会话。
    let name: Vec<u16> = "Local\\WinIsle_SingleInstance_v1\0".encode_utf16().collect();
    let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr()) };
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let _ = handle; // 不关闭，进程存活期间持有
    already
}

const GWL_EXSTYLE: i32 = -20;
const GWL_STYLE: i32 = -16;
const WS_CAPTION: isize = 0x00C0_0000;
const WS_THICKFRAME: isize = 0x0004_0000;
const WS_SYSMENU: isize = 0x0008_0000;
const WS_MINIMIZEBOX: isize = 0x0002_0000;
const WS_MAXIMIZEBOX: isize = 0x0001_0000;
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
const WS_EX_NOACTIVATE: isize = 0x0800_0000;
const WS_EX_TRANSPARENT: isize = 0x0000_0020;
const WS_EX_LAYERED: isize = 0x0008_0000;

const HWND_TOPMOST: HWND = -1;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_DONOTROUND: i32 = 1;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_VISIBLE_FRAME_BORDER_THICKNESS: u32 = 37;
const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct MONITORINFO {
    pub cbSize: u32,
    pub rcMonitor: RECT,
    pub rcWork: RECT,
    pub dwFlags: u32,
}

// MonitorFromPoint / MonitorFromWindow 的标志位
const MONITOR_DEFAULTTONEAREST: u32 = 2;

#[link(name = "user32")]
extern "system" {
    fn GetCursorPos(p: *mut POINT) -> i32;
    fn GetWindowRect(h: HWND, r: *mut RECT) -> i32;
    fn MonitorFromPoint(pt: POINT, dwFlags: u32) -> isize;
    fn MonitorFromWindow(hwnd: HWND, dwFlags: u32) -> isize;
    fn GetMonitorInfoW(hMonitor: isize, lpmi: *mut MONITORINFO) -> i32;
    fn GetAsyncKeyState(vkey: i32) -> i16;
    fn CallWindowProcW(prev: isize, h: HWND, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn SetCapture(h: HWND) -> HWND;
    fn ReleaseCapture() -> i32;
    fn GetClientRect(h: HWND, r: *mut RECT) -> i32;
    fn FillRect(hdc: isize, r: *const RECT, brush: isize) -> i32;
    fn GetDC(h: HWND) -> isize;
    fn ReleaseDC(h: HWND, hdc: isize) -> i32;
    fn SetClassLongPtrW(h: HWND, nIndex: i32, v: isize) -> isize;
    fn SetLayeredWindowAttributes(h: HWND, key: u32, alpha: u8, flags: u32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> isize;
    fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> isize;
    fn CombineRgn(dst: isize, a: isize, b: isize, mode: i32) -> i32;
    fn DeleteObject(o: isize) -> i32;
    fn CreateSolidBrush(color: u32) -> isize;
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowRgn(hwnd: HWND, hrgn: isize, bRedraw: i32) -> i32;
}

// 命中测试：只有岛体矩形接收鼠标，其余区域穿透。矩形用物理像素、相对窗口左上角。
static HIT_L: AtomicI32 = AtomicI32::new(0);
static HIT_T: AtomicI32 = AtomicI32::new(0);
static HIT_R: AtomicI32 = AtomicI32::new(0);
static HIT_B: AtomicI32 = AtomicI32::new(0);
static HIT_ON: AtomicI32 = AtomicI32::new(0);
static mut ORIG_WNDPROC: isize = 0;

const GWLP_WNDPROC: i32 = -4;
const WM_NCHITTEST: u32 = 0x0084;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_MOUSEACTIVATE: u32 = 0x0021;
const WM_NCACTIVATE: u32 = 0x0086;
const MA_NOACTIVATE: isize = 3;
const MA_ACTIVATE: isize = 1;
const HTCLIENT: isize = 1;
const HTTRANSPARENT: isize = -1;
const VK_LBUTTON: i32 = 0x01;

// 动态整窗鼠标穿透：WS_EX_TRANSPARENT 使窗口对鼠标完全透明（点击落到下方窗口，跨进程有效）。
// 通过定时器按光标是否落在岛体/面板矩形内来开关，从而“岛体可交互、透明边距可穿透”。
// 注意：WS_EX_TRANSPARENT 常量已在文件上部定义（见上方）。
const WM_TIMER: u32 = 0x0113;
const CLICK_TIMER_ID: usize = 0x0B15;
const CLICK_TIMER_MS: u32 = 8;

/// 是否允许本窗口被点击激活（仅在需要键盘输入的搜索页临时打开）。
static ACTIVATABLE: AtomicI32 = AtomicI32::new(0);

/// 穿透轮询线程是否已启动（保证只启动一次）。
static CLICK_POLL_STARTED: AtomicI32 = AtomicI32::new(0);

/// 是否正在拖动岛体（由 UI 线程每帧写入）。拖动期间需保持窗口可交互，即使光标离开岛体。
static DRAGGING: AtomicI32 = AtomicI32::new(0);

/// 设置“正在拖动”状态（由主线程调用）。
pub fn set_dragging(on: bool) {
    DRAGGING.store(on as i32, Ordering::Relaxed);
}

/// 每帧更新"岛体"命中矩形（物理像素，窗口内相对坐标）。
pub fn set_hit_pill(l: i32, t: i32, r: i32, b: i32) {
    HIT_L.store(l, Ordering::Relaxed);
    HIT_T.store(t, Ordering::Relaxed);
    HIT_R.store(r, Ordering::Relaxed);
    HIT_B.store(b, Ordering::Relaxed);
    HIT_ON.store(1, Ordering::Relaxed);
}

/// 判断给定屏幕物理点 (px,py) 是否落在“岛体/面板”命中矩形内。
unsafe fn hit_contains(hwnd: HWND, px: i32, py: i32) -> bool {
    if HIT_ON.load(Ordering::Relaxed) == 0 {
        return false;
    }
    let mut rc = RECT::default();
    GetWindowRect(hwnd, &mut rc);
    let lx = px - rc.left;
    let ly = py - rc.top;
    let (l, t, r, b) = (
        HIT_L.load(Ordering::Relaxed),
        HIT_T.load(Ordering::Relaxed),
        HIT_R.load(Ordering::Relaxed),
        HIT_B.load(Ordering::Relaxed),
    );
    lx >= l && lx <= r && ly >= t && ly <= b
}

/// 光标当前是否落在“岛体/面板”命中矩形内（物理像素、窗口内相对坐标）。
unsafe fn cursor_inside_pill(hwnd: HWND) -> bool {
    let mut p = POINT::default();
    GetCursorPos(&mut p);
    hit_contains(hwnd, p.x, p.y)
}

static mut ORIG_ERASE: isize = 0;
static mut OUR_ERASE: isize = 0;
static mut DARK_BRUSH: isize = 0;
const GCLP_HBRBACKGROUND: i32 = -10;
// 岛体背景色 (11,14,19) -> COLORREF 0x00BBGGRR
const DARK_COLORREF: u32 = 0x00_13_0E_0B;

unsafe fn dark_brush() -> isize {
    if DARK_BRUSH == 0 {
        DARK_BRUSH = CreateSolidBrush(DARK_COLORREF);
    }
    DARK_BRUSH
}

unsafe extern "system" fn erase_wndproc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    // 命中测试（兜底）：岛体/面板内返回 HTCLIENT 可交互；其余返回 HTTRANSPARENT。
    // 真正的整窗跨进程穿透由后台 8ms 轮询线程通过 WS_EX_TRANSPARENT 开关（见 start_click_through_poller）。
    if msg == WM_NCHITTEST && HIT_ON.load(Ordering::Relaxed) != 0 {
        if cursor_inside_pill(hwnd) {
            return HTCLIENT;
        }
        return HTTRANSPARENT;
    }
    // 点击时不要让窗口被激活——否则 Windows 会为"活动窗口"画一圈高亮边框（偶发白边的来源）。
    // 例外：城市搜索页需要键盘输入，此时允许激活，键盘事件才能进入 egui。
    if msg == WM_MOUSEACTIVATE {
        return if ACTIVATABLE.load(Ordering::Relaxed) != 0 {
            MA_ACTIVATE
        } else {
            MA_NOACTIVATE // 3 = 不激活，也不吃掉这次点击
        };
    }
    // 非客户区激活态变化：直接吞掉，不重画标题栏/边框。
    if msg == WM_NCACTIVATE {
        return 1;
    }
    // 不在这里做任何 GDI 填充（GDI 直接画在 GL 窗口上会造成"点一下闪一下"）。
    // 背景由窗口类背景刷（已设为岛体同色）负责，绘制交给 GL 每帧覆盖。
    if msg == WM_ERASEBKGND {
        return 1;
    }
    CallWindowProcW(ORIG_ERASE, hwnd, msg, wparam, lparam)
}


/// 试验：改用分层窗口（WS_EX_LAYERED + 常量 alpha 255），验证是否能消除系统补白帧且保持透明。
pub fn set_layered(hwnd: HWND) {
    if hwnd == 0 { return; }
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED);
        SetLayeredWindowAttributes(hwnd, 0, 255, 0x0000_0002 /*LWA_ALPHA*/);
    }
}
/// 设置整窗不透明度（需已 WS_EX_LAYERED）。alpha=0 时整窗完全不可见（含所有内容），
/// 用于启动阶段「先隐身、等首帧深色内容就绪后再显形」，避免空白/白底窗口一闪。
pub fn set_window_alpha(hwnd: HWND, alpha: u8) {
    if hwnd == 0 { return; }
    unsafe {
        SetLayeredWindowAttributes(hwnd, 0, alpha, 0x0000_0002 /*LWA_ALPHA*/);
    }
}
/// 立即把窗口客户区同步刷成深色（用于"久置后第一次刷新"前预先盖住可能露出的白底）。
pub fn fill_dark_now(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let hdc = GetDC(hwnd);
        if hdc != 0 {
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);
            FillRect(hdc, &rc, dark_brush());
            ReleaseDC(hwnd, hdc);
        }
    }
}

/// 安装背景擦除护栏子类：消除 SetWindowRgn 区域变化时的白闪。
pub fn install_erase_guard(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        // 窗口类背景刷设为岛体同色（系统补帧时画深色而非白色）。
        SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, dark_brush());
        let me = erase_wndproc as *const () as usize as isize;
        OUR_ERASE = me;
        let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, me);
        ORIG_ERASE = prev;
    }
}

/// 每帧调用：如果窗口过程被 winit/eframe 重新设置过（会覆盖掉我们的守卫），就重新装回去。
pub fn ensure_erase_guard(hwnd: HWND) {
    if hwnd == 0 { return; }
    unsafe {
        let cur = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        if OUR_ERASE == 0 || cur != OUR_ERASE {
            install_erase_guard(hwnd);
        }
    }
}

/// 后台 8ms 轮询：按光标是否落在岛体/面板矩形内，动态开关整窗 WS_EX_TRANSPARENT 穿透。
/// 不依赖消息循环（WM_TIMER 在此框架下不稳定），也不依赖 egui 重绘节流，所以没有延迟。
/// - 光标在岛体/面板内 => 关闭穿透（可悬停、拖动、点击）
/// - 光标在透明边距/窗口外 => 打开穿透（点击落到下方窗口，跨进程生效）
/// 若按住左键且当前可交互，则保持可交互，避免拖拽途中被切成穿透。
/// 同时每 16ms 重新断言一次，抵消 winit 在 set_outer_position/set_inner_size 时重置扩展样式。
pub fn start_click_through_poller(hwnd: HWND) {
    if hwnd == 0 { return; }
    if std::env::var_os("WINISLE_NOCTP").is_some() { return; }
    unsafe {
        if CLICK_POLL_STARTED.swap(1, Ordering::SeqCst) != 0 {
            return;
        }
    }
    std::thread::spawn(move || {
        let dbg = std::env::var_os("WINISLE_CLICK_DEBUG").is_some();
        // 仅自测用：用窗口中心代替真实光标，验证展开→收起后的穿透状态恢复。
        let fake_center = std::env::var_os("WINISLE_FAKE_CENTER").is_some();
        // 仅自测用：用固定点替代真实光标（不移动用户鼠标），验证命中/穿透逻辑。
        let fake: Option<(i32, i32)> = std::env::var("WINISLE_FAKE_CURSOR")
            .ok()
            .and_then(|s| {
                let mut it = s.split(',');
                let x = it.next()?.trim().parse::<i32>().ok()?;
                let y = it.next()?.trim().parse::<i32>().ok()?;
                Some((x, y))
            });
        let mut n: u32 = 0;
        loop {
            unsafe {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if (ex & WS_EX_LAYERED) == 0 {
                    // winit 有时会重置扩展样式，导致分层位丢失；丢失后 WS_EX_TRANSPARENT 将不再穿透，需补回。
                    set_layered(hwnd);
                }
                let dragging = DRAGGING.load(Ordering::Relaxed) != 0;
                // 兜底：某些环境下窗口创建后会被系统/winit 停留在隐藏状态（“岛不见了”）。
                // 周期性检查，若不可见就重新显示（不抢焦点）。约每 320ms 一次。
                if n % 20 == 0 && IsWindowVisible(hwnd) == 0 {
                    show_island(hwnd);
                }
                // 周期/拖动时重新断言无边框样式，抵消跨 DPI 显示器移动时系统画回标题栏的问题
                if dragging || (n % 12 == 0) { ensure_island_style(hwnd); }
                let (px, py) = if fake_center {
                    let mut rc = RECT::default();
                    GetWindowRect(hwnd, &mut rc);
                    ((rc.left + rc.right) / 2, (rc.top + rc.bottom) / 2)
                } else {
                    cursor_pos()
                };
                let inside = hit_contains(hwnd, px, py);
                // 拖动时保持可交互（光标可能离开岛体）；其余情况纯按光标是否在岛体/面板内决定穿透，
                // 不做任何“按下锁存”，避免一次单击把窗口锁成可交互导致后续点击被吞。
                let want_on = if dragging { false } else { !inside };
                set_click_through(hwnd, want_on);
                n = n.wrapping_add(1);
                if dbg && (n % 60 == 0) {
                    let hit = (
                        HIT_L.load(Ordering::Relaxed),
                        HIT_T.load(Ordering::Relaxed),
                        HIT_R.load(Ordering::Relaxed),
                        HIT_B.load(Ordering::Relaxed),
                        HIT_ON.load(Ordering::Relaxed),
                    );
                    let ex2 = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    let line = format!(
                        "cur=({},{}) fake={} hit=({},{},{},{}) hiton={} inside={} transp={}\n",
                        px, py, fake.is_some(), hit.0, hit.1, hit.2, hit.3, hit.4, inside,
                        (ex2 & WS_EX_TRANSPARENT) != 0
                    );
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("D:\\LingDao\\_click_debug.txt")
                    {
                        let _ = f.write_all(line.as_bytes());
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    });
}

/// 左键当前是否按下（全局，用于拖动，不依赖窗口消息）。
pub fn lbutton_down() -> bool {
    // 自测钩子：env WINISLE_BUTTON_FILE（文件内容 "0"/"1"）可脚本化按键状态，不产生真实按键事件。
    if let Ok(path) = std::env::var("WINISLE_BUTTON_FILE") {
        if let Ok(s) = std::fs::read_to_string(&path) {
            return s.trim() == "1";
        }
    }
    unsafe { (GetAsyncKeyState(VK_LBUTTON) as u16 & 0x8000) != 0 }
}

/// 抓取/释放鼠标：拖动时把鼠标事件锁到本窗口，避免拖出岛体后误选桌面。
pub fn capture_mouse(hwnd: HWND) {
    unsafe {
        SetCapture(hwnd);
    }
}

pub fn release_mouse() {
    unsafe {
        ReleaseCapture();
    }
}

/// 全局光标位置（物理像素，屏幕坐标）。用于不依赖窗口消息的悬停检测。
/// 自测钩子：env `WINISLE_CURSOR_FILE`（文件内容 "x,y"，可动态改写以脚本化光标）或
/// `WINISLE_FAKE_CURSOR`（固定 "x,y"）可替代真实光标，用于无鼠标自动验证，不移动用户鼠标。
pub fn cursor_pos() -> (i32, i32) {
    if let Ok(path) = std::env::var("WINISLE_CURSOR_FILE") {
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Some(v) = parse_xy(&s) {
                return v;
            }
        }
    }
    if let Ok(s) = std::env::var("WINISLE_FAKE_CURSOR") {
        if let Some(v) = parse_xy(&s) {
            return v;
        }
    }
    let mut p = POINT::default();
    unsafe {
        GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

fn parse_xy(s: &str) -> Option<(i32, i32)> {
    let mut it = s.trim().split(',');
    let x = it.next()?.trim().parse::<i32>().ok()?;
    let y = it.next()?.trim().parse::<i32>().ok()?;
    Some((x, y))
}

/// 窗口在屏幕上的物理矩形 (left, top, right, bottom)。
pub fn window_rect(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut r = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut r);
    }
    (r.left, r.top, r.right, r.bottom)
}

unsafe fn monitor_info_rect(hmon: isize) -> Option<(i32, i32, i32, i32)> {
    if hmon == 0 {
        return None;
    }
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(hmon, &mut mi) == 0 {
        return None;
    }
    Some((
        mi.rcMonitor.left,
        mi.rcMonitor.top,
        mi.rcMonitor.right,
        mi.rcMonitor.bottom,
    ))
}

/// 包含指定物理点 (x, y) 的显示器矩形 (left, top, right, bottom)，虚拟屏坐标、物理像素。
/// 该点不在任何显示器上时，返回距离最近的显示器（MONITOR_DEFAULTTONEAREST）。
pub fn monitor_rect_at(x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let hmon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        monitor_info_rect(hmon)
    }
}

/// 窗口当前所在（或最近）的显示器矩形 (left, top, right, bottom)，物理像素。
pub fn monitor_rect_of_window(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    if hwnd == 0 {
        return None;
    }
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        monitor_info_rect(hmon)
    }
}

/// 整个虚拟桌面矩形 (left, top, right, bottom)，覆盖所有显示器，物理像素。
/// 使用 GetSystemMetrics 的 SM_XVIRTUALSCREEN(76)/SM_YVIRTUALSCREEN(77)/
/// SM_CXVIRTUALSCREEN(78)/SM_CYVIRTUALSCREEN(79)。
pub fn virtual_screen_rect() -> (i32, i32, i32, i32) {
    unsafe {
        let x = GetSystemMetrics(76);
        let y = GetSystemMetrics(77);
        let w = GetSystemMetrics(78);
        let h = GetSystemMetrics(79);
        if w <= 0 || h <= 0 {
            // 极端兜底：退化成主屏
            return (0, 0, GetSystemMetrics(0).max(1), GetSystemMetrics(1).max(1));
        }
        (x, y, x + w, y + h)
    }
}

/// 用"圆角区域"把窗口裁剪成岛体形状（物理像素，相对窗口本地坐标）。
/// 圆角区域必须与绘制形状一致（同样半径、同样哪两个角是直角），否则圆角外侧会闪白。
/// sq_* 为 true 的角是"直角"（贴边那两条边），其余角按 rad 圆角。
pub fn set_window_region_round(
    hwnd: HWND,
    l: i32,
    t: i32,
    r: i32,
    b: i32,
    rad: i32,
    sq_nw: bool,
    sq_ne: bool,
    sq_sw: bool,
    sq_se: bool,
) {
    if hwnd == 0 {
        return;
    }
    let l = l;
    let r = r.max(l + 1);
    let b = b.max(t + 1);
    unsafe {
        if rad <= 0 {
            let hrgn = CreateRectRgn(l, t, r, b);
            SetWindowRgn(hwnd, hrgn, 1);
            return;
        }
        let base = CreateRoundRectRgn(l, t, r, b, rad * 2, rad * 2);
        // 把需要直角的角补成方块（圆角矩形默认四角都圆）
        let corners = [
            (sq_nw, l, t, l + rad, t + rad),
            (sq_ne, r - rad, t, r, t + rad),
            (sq_sw, l, b - rad, l + rad, b),
            (sq_se, r - rad, b - rad, r, b),
        ];
        for (sq, x1, y1, x2, y2) in corners {
            if sq {
                let c = CreateRectRgn(x1, y1, x2, y2);
                CombineRgn(base, base, c, 2 /* RGN_OR */);
                DeleteObject(c);
            }
        }
        SetWindowRgn(hwnd, base, 1);
    }
}

/// 开/关整窗鼠标穿透(WS_EX_TRANSPARENT)：开启后鼠标与点击都穿透到下层程序/桌面。
/// 收起静止时开启(不占用桌面)，展开/拖动时关闭(面板可交互)。
pub fn set_click_through(hwnd: HWND, on: bool) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new = if on {
            ex | WS_EX_TRANSPARENT
        } else {
            ex & !WS_EX_TRANSPARENT
        };
        if new != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new);
        }
    }
}

/// 临时允许/禁止窗口被点击激活。搜索页需要文本输入时打开，离开后恢复灵动岛的不抢焦点行为。
pub fn set_activatable(hwnd: HWND, on: bool) {
    if hwnd == 0 {
        return;
    }
    ACTIVATABLE.store(if on { 1 } else { 0 }, Ordering::Relaxed);
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new = if on {
            ex & !WS_EX_NOACTIVATE
        } else {
            ex | WS_EX_NOACTIVATE
        };
        if new != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new);
        }
        if on {
            // 允许激活后主动把键盘焦点拿过来：用户点开搜索页即可直接输入。
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
        }
    }
}

/// 只放开/收回「可被点击激活」这一属性（WS_EX_NOACTIVATE），**不主动抢焦点**。
/// 用于鼠标悬停在“添加待办”输入行上时预先放开：这样随后这一下点击会以 MA_ACTIVATE
/// 送达窗口，窗口在点击瞬间即被系统激活、键盘焦点落到输入框，用户点开就能直接打字；
/// 而悬停本身不会抢走前台焦点（此处不调用 SetForegroundWindow/SetFocus）。
pub fn set_activatable_soft(hwnd: HWND, on: bool) {
    if hwnd == 0 {
        return;
    }
    ACTIVATABLE.store(if on { 1 } else { 0 }, Ordering::Relaxed);
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new = if on {
            ex & !WS_EX_NOACTIVATE
        } else {
            ex | WS_EX_NOACTIVATE
        };
        if new != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new);
        }
    }
}

/// 当前是否已放开「可被点击激活」（即悬停预放开后仍处于打开状态）。
pub fn is_activatable() -> bool {
    ACTIVATABLE.load(Ordering::Relaxed) != 0
}

/// 强制把本窗口切到前台并交给键盘焦点（用于用户点开需要打字的输入框时）。
/// 采用 AttachThreadInput 规避 Windows 对非前台进程 SetForegroundWindow 的限制：
/// 临时把本线程输入队列挂到当前前台线程上，置前后再解绑。仅在用户主动进入输入态时调用。
pub fn force_foreground(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        // 先确保窗口不再带 WS_EX_NOACTIVATE，否则 SetForegroundWindow/SetFocus 会被系统忽略，
        // winit 也不会收到 Focused(true)，egui 的 i.focused 一直为 false（插入符因此不画）。
        ACTIVATABLE.store(1, Ordering::Relaxed);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if (ex & WS_EX_NOACTIVATE) != 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !WS_EX_NOACTIVATE);
        }
        let fg = GetForegroundWindow();
        let fg_tid = if fg == 0 { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
        let me_tid = GetCurrentThreadId();
        let attached = fg_tid != 0 && fg_tid != me_tid && AttachThreadInput(me_tid, fg_tid, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        if attached {
            AttachThreadInput(me_tid, fg_tid, 0);
        }
        if std::env::var_os("WINISLE_FOCUS_LOG").is_some() {
            eprintln!("[fgdbg] hwnd={} old_fg={} fg_tid={} me_tid={} attached={} new_fg={}",
                hwnd, fg, fg_tid, me_tid, attached, GetForegroundWindow());
        }
    }
}

/// 清除窗口区域（恢复整窗矩形）。展开态使用。
pub fn clear_window_region(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        SetWindowRgn(hwnd, 0, 1);
    }
}

/// 当前本地时间（年月日时分秒星期）。
pub fn local_time() -> SYSTEMTIME {
    let mut st = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut st) };
    st
}

/// 把窗口改造为灵动岛：不进任务栏/不抢焦点/置顶；关闭 DWM 自带圆角与边框，
/// 圆角与描边完全由 egui 自绘（更干净，不会出现那圈浅色方框）。
pub fn make_island(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        // 注：整个窗口的鼠标穿透不在这里做，而是用 WM_NCHITTEST 子类化：
        // 只有"岛体"区域接收鼠标(可抓取拖动)，其余透明边距穿透到桌面。
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE);

        // 置顶
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);

        // 关闭 DWM 自带圆角（窗口保持矩形，四角由 egui 的透明+自绘圆角处理）
        let corner = DWMWCP_DONOTROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );

        // 关闭 DWM 窗口边框（去掉系统画的那圈浅色描边）
        let no_border = DWMWA_COLOR_NONE;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &no_border as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        // 关闭 Win11 可见边框厚度（拖动/置顶瞬间系统可能画一圈 1px 白边）
        let zero_border: u32 = 0;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_VISIBLE_FRAME_BORDER_THICKNESS,
            &zero_border as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        // 彻底关闭非客户区渲染（标题栏/边框/窗口阴影都不画，拖动时也不会闪白边）
        // DWMWA_NCRENDERING_POLICY = 2, DWMNCRP_DISABLED = 1
        let ncrp: u32 = 1;
        DwmSetWindowAttribute(hwnd, 2u32, &ncrp as *const u32 as *const c_void, 4);
    }
}

/// 主窗口句柄（供托盘线程唤醒/显示窗口使用）。
static MAIN_HWND: AtomicI64 = AtomicI64::new(0);
pub fn set_main_hwnd(h: HWND) { MAIN_HWND.store(h as i64, Ordering::Relaxed); }
pub fn main_hwnd() -> i64 { MAIN_HWND.load(Ordering::Relaxed) }

/// 显示并置顶岛体窗口（托盘「显示」/启动兜底调用）。不抢焦点、不激活。
pub fn show_island(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        // SW_SHOWNOACTIVATE = 4
        ShowWindow(hwnd, 4);
        // SWP_NOSIZE(1)|SWP_NOZORDER(4)|SWP_NOACTIVATE(0x10)|SWP_SHOWWINDOW(0x40)
        SetWindowPos(hwnd, 0, 0, 0, 0, 0, 0x0055);
        // 再确保置顶（不重复设置时开销很小）
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        const WS_EX_TOPMOST: isize = 0x0000_0008;
        if ex & WS_EX_TOPMOST == 0 {
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// 窗口当前是否已真正显示在屏幕上（用来判定“首帧已呈现”的准确时刻，作为入场淡入的起点）。
pub fn is_window_visible(hwnd: HWND) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe { IsWindowVisible(hwnd) != 0 }
}

/// 用 DWM 的 DWMWA_CLOAK(13) 把窗口从合成器临时摘除/恢复。
/// 启动时先 cloak，等首帧深色内容真正 swap 出去后再取消 cloak，
/// 可彻底避免「双击启动瞬间左上角闪一下白色空窗口」。
pub fn set_cloak(hwnd: HWND, on: bool) {
    if hwnd == 0 {
        return;
    }
    let v: i32 = if on { 1 } else { 0 };
    unsafe {
        DwmSetWindowAttribute(hwnd, 13u32, &v as *const i32 as *const c_void, 4);
    }
}

/// 兜底：跨显示器（不同 DPI 缩放）移动或 winit 重置样式时，系统可能把标题栏/边框/非客户区
/// 重新画回来（拖动穿屏时会“卡出”一个带标题栏的白框）。这里把「无标题栏 + 工具窗口 + 不激活
/// + 不渲染非客户区」重新断言一遍。开销很小，适合每帧/每轮调用。
pub fn ensure_island_style(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    // DWM 属性不必每帧重设（每次 DwmSetWindowAttribute 都会触发一次合成/重排，
    // 拖动时每帧调用正是掉帧的主因）。样式一旦被系统改回去才重设一次。
    static STYLE_READY: AtomicBool = AtomicBool::new(false);
    unsafe {
        let st = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let strip = WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;
        let new_st = st & !strip;
        if new_st != st {
            SetWindowLongPtrW(hwnd, GWL_STYLE, new_st);
            // 通知系统重算非客户区（否则样式改了要等下一次尺寸变化才生效）
            // SWP_NOSIZE(0x1)|SWP_NOMOVE(0x2)|SWP_NOZORDER(0x4)|SWP_NOACTIVATE(0x10)|SWP_FRAMECHANGED(0x20)
            SetWindowPos(hwnd, 0, 0, 0, 0, 0, 0x0037);
            STYLE_READY.store(false, Ordering::Relaxed);
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        // 尊重当前「可激活」状态：输入/搜索需要键盘时不能把这期间打开的激活又关回去。
        // 否则后台穿透轮询每 ~192ms（n%12）重断言样式时会把 WS_EX_NOACTIVATE 重新加上，
        // 窗口被周期性去激活 → egui 的 i.focused 变 false → 输入框光标消失、打不了字。
        let activatable = ACTIVATABLE.load(Ordering::Relaxed) != 0;
        let mut new_ex = ex | WS_EX_TOOLWINDOW;
        if activatable {
            new_ex &= !WS_EX_NOACTIVATE;
        } else {
            new_ex |= WS_EX_NOACTIVATE;
        }
        if new_ex != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
            STYLE_READY.store(false, Ordering::Relaxed);
        }
        if !STYLE_READY.load(Ordering::Relaxed) {
            // 彻底关闭非客户区渲染（标题栏/边框/阴影都不画）
            let ncrp: u32 = 1; // DWMNCRP_DISABLED
            DwmSetWindowAttribute(hwnd, 2u32, &ncrp as *const u32 as *const c_void, 4);
            // 关闭 Win11 圆角与可见边框
            let corner = DWMWCP_DONOTROUND;
            DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const i32 as *const c_void, 4);
            let no_border = DWMWA_COLOR_NONE;
            DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &no_border as *const u32 as *const c_void, 4);
            let zero: u32 = 0;
            DwmSetWindowAttribute(hwnd, DWMWA_VISIBLE_FRAME_BORDER_THICKNESS, &zero as *const u32 as *const c_void, 4);
            STYLE_READY.store(true, Ordering::Relaxed);
        }
    }
}

/// 重新置顶（某些全屏应用会把我们压下去后调用）。
pub fn keep_on_top(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        // 已经是 TOPMOST 就不用每帧再 SetWindowPos（重复置顶也会打断合成、掉帧）
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        const WS_EX_TOPMOST: isize = 0x0000_0008;
        if ex & WS_EX_TOPMOST != 0 {
            return;
        }
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// 给窗口开启 Win11 原生圆角（DWMWA_WINDOW_CORNER_PREFERENCE=33, DWMWCP_ROUND=2）。
pub fn set_round_corners(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let pref: i32 = 2;
        DwmSetWindowAttribute(hwnd, 33u32, &pref as *const _ as *const c_void, 4);
    }
}

unsafe extern "system" fn enum_title_cb(hwnd: HWND, l: isize) -> i32 {
    let v = &mut *(l as *mut Vec<(HWND, String)>);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() {
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
        if n > 0 {
            v.push((hwnd, String::from_utf16_lossy(&buf[..n as usize])));
        }
    }
    1
}

/// 在当前进程里按窗口标题（完全相等）查找顶层窗口句柄；找不到返回 0。
pub fn find_process_window(title: &str) -> HWND {
    let mut v: Vec<(HWND, String)> = Vec::new();
    unsafe {
        let p = &mut v as *mut _ as isize;
        EnumWindows(Some(enum_title_cb), p);
    }
    for (h, t) in v {
        if t == title {
            return h;
        }
    }
    0
}
/// 把窗口移动到主屏幕正中。
/// 以“物理像素整数”精确设置窗口外框左上角位置（不改尺寸、不改 Z 序、不激活）。
/// 直接调用 Win32 SetWindowPos，绕开 winit OuterPosition 在 DPI 缩放下的取整/锚点偏差，
/// 用于消除展开↔塌收大幅移动后窗口少走几像素导致的贴边留缝，并保证逐帧位移连续。
pub fn set_outer_position_px(hwnd: HWND, x: i32, y: i32) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        // SWP_NOSIZE(0x1) | SWP_NOZORDER(0x4) | SWP_NOACTIVATE(0x10)
        SetWindowPos(hwnd, 0, x, y, 0, 0, 0x0015);
    }
}

pub fn center_window(hwnd: HWND) {    if hwnd == 0 {
        return;
    }
    unsafe {
        let (l, t, r, b) = window_rect(hwnd);
        let w = r - l;
        let h = b - t;
        let sw = GetSystemMetrics(0); // SM_CXSCREEN
        let sh = GetSystemMetrics(1); // SM_CYSCREEN
        let x = (sw - w) / 2;
        let y = (sh - h) / 2;
        // SWP_NOSIZE(0x1) | SWP_NOZORDER(0x4) | SWP_NOACTIVATE(0x10)
        SetWindowPos(hwnd, 0, x, y, 0, 0, 0x0015);
    }
}
/// 设置窗初始是隐藏的：先用 OS 强制设定"居中 + 正确尺寸"，再显示，
/// 避免窗口先以默认大尺寸在角落闪一下再跳到中间。
pub fn reveal_centered(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        let ppp = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
        let w = (440.0 * ppp).round() as i32;
        let h = (320.0 * ppp).round() as i32;
        let sw = GetSystemMetrics(0);
        let sh = GetSystemMetrics(1);
        let x = (sw - w) / 2;
        let y = (sh - h) / 2;
        // 同时设定位置+尺寸；SWP_NOZORDER(0x4) | SWP_NOACTIVATE(0x10)
        SetWindowPos(hwnd, 0, x, y, w, h, 0x0014);
        // SW_SHOWNOACTIVATE = 4
        ShowWindow(hwnd, 4);
    }
}
// ===== 兜底：当 SMTC 不可用时，从已知播放器窗口标题抓 “歌名 - 歌手” =====
#[link(name = "user32")]
extern "system" {
    fn IsWindowVisible(hWnd: HWND) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
    fn QueryFullProcessImageNameW(hProcess: isize, dwFlags: u32, lpExeName: *mut u16, lpdwSize: *mut u32) -> i32;
    fn CloseHandle(hObject: isize) -> i32;
}

fn exe_name_of_pid(pid: u32) -> String {
    unsafe {
        let h = OpenProcess(0x1000 /*PROCESS_QUERY_LIMITED_INFORMATION*/, 0, pid);
        if h == 0 {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut sz: u32 = 1024;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut sz);
        CloseHandle(h);
        if ok == 0 {
            return String::new();
        }
        let s = String::from_utf16_lossy(&buf[..sz as usize]);
        s.rsplit(|c| c == '\\' || c == '/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    }
}

unsafe extern "system" fn enum_media_cb(hwnd: HWND, l: isize) -> i32 {
    let v = &mut *(l as *mut Vec<(u32, String)>);
    // 不再要求“可见”：播放器最小化/隐藏到托盘时窗口仍在，标题可能仍可读取
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    let mut buf = [0u16; 512];
    let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
    if n > 2 {
        v.push((pid, String::from_utf16_lossy(&buf[..n as usize])));
    }
    1
}

/// 扫描已知音乐客户端的顶层窗口标题，
/// 若形如 “歌名 - 歌手” 则返回 (exe 名, 歌名, 歌手)。仅作 SMTC 的兜底。
pub fn media_title_fallback() -> Option<(String, String, String)> {
    const ALLOW: [&str; 8] = [
        "cloudmusic.exe",
        "sodamusic.exe",
        "qqmusic.exe",
        "kugou.exe",
        "kwmusic.exe",
        "spotify.exe",
        "applemusic.exe",
        "bodian_pc.exe",
    ];
    let mut v: Vec<(u32, String)> = Vec::new();
    unsafe {
        EnumWindows(Some(enum_media_cb), &mut v as *mut _ as isize);
    }
    for (pid, title) in v {
        // 过滤播放器的辅助/隐藏窗口（标题形如 "MediaPlayer SMTC window - {GUID}"）
        if title.contains("SMTC") || title.contains("MediaPlayer") || title.contains('{') || title.contains('}') {
            continue;
        }
        let exe = exe_name_of_pid(pid);
        if !ALLOW.contains(&exe.as_str()) {
            continue;
        }
        // 形如 “歌名 - 歌手 [- 播放器名]”：按 " - " 拆分，去掉末尾的播放器名
        let bad = |s: &str| {
            s.contains("网易云") || s.contains("QQ音乐") || s.contains("汽水")
                || s.eq_ignore_ascii_case("spotify") || s.contains("酷狗") || s.contains("酷我")
        };
        let parts: Vec<&str> = title.split(" - ").map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            let t = parts[0].to_string();
            let a = parts[1].to_string();
            if !bad(&t) && !bad(&a) && !t.is_empty() && !a.is_empty()
                && t.chars().count() <= 80 && a.chars().count() <= 60
            {
                return Some((exe, t, a));
            }
        }
    }
    None
}

#[link(name = "user32")]
extern "system" {
    fn keybd_event(bvk: u8, bscan: u8, dwflags: u32, dwextra: usize);
}

/// 发送系统多媒体键（全局生效，控制当前活动的播放器）。
pub fn media_key(vk: u8) {
    unsafe {
        keybd_event(vk, 0, 0, 0);
        keybd_event(vk, 0, 2, 0); // KEYEVENTF_KEYUP
    }
}

// ===== 网易云专用适配：SMTC 不提供时间轴，直接读其进度条像素 / 点击其进度条实现定位 =====
#[link(name = "gdi32")]
extern "system" {
    fn GetPixel(hdc: isize, x: i32, y: i32) -> u32;
}
#[link(name = "user32")]
extern "system" {
    fn ClientToScreen(h: HWND, p: *mut POINT) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
    fn SetForegroundWindow(h: HWND) -> i32;
    fn SetFocus(h: HWND) -> HWND;
    fn GetForegroundWindow() -> HWND;
    fn GetCurrentThreadId() -> u32;
    fn AttachThreadInput(idAttach: u32, idAttachTo: u32, fAttach: i32) -> i32;
    fn BringWindowToTop(h: HWND) -> i32;
    fn IsIconic(h: HWND) -> i32;
}
unsafe extern "system" fn enum_netease_cb(hwnd: HWND, l: isize) -> i32 {
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if exe_name_of_pid(pid) != "cloudmusic.exe" { return 1; }
    if IsWindowVisible(hwnd) == 0 { return 1; }
    let mut r = RECT::default();
    if GetWindowRect(hwnd, &mut r) == 0 { return 1; }
    if r.right - r.left > 500 && r.bottom - r.top > 400 {
        *(l as *mut isize) = hwnd as isize;
        return 0;
    }
    1
}
fn netease_hwnd() -> Option<HWND> {
    unsafe {
        let mut v: isize = 0;
        EnumWindows(Some(enum_netease_cb), &mut v as *mut isize as isize);
        if v == 0 { None } else { Some(v as HWND) }
    }
}
/// 读取网易云进度条（底部细线上高亮圆点）的位置比例 0..1。
pub fn netease_frac() -> Option<f64> {
    unsafe {
        let h = netease_hwnd()?;
        if IsIconic(h) != 0 { return None; }
        let mut r = RECT::default();
        if GetClientRect(h, &mut r) == 0 { return None; }
        let w = r.right - r.left;
        let ht = r.bottom - r.top;
        if w < 1400 || ht < 700 { return None; }
        let mut o = POINT { x: 0, y: 0 };
        if ClientToScreen(h, &mut o) == 0 { return None; }
        let hdc = GetDC(0);
        if hdc == 0 { return None; }
        // 进度条在窗口底部约 89~90% 高度处；控件按钮也在附近，故只取左侧 1%~84% 区间，
        // 并且按“亮=已播放填充 / 灰=未播放轨道 / 暗=背景”推断填充点与轨道终点。
        let base_y = o.y + (ht as f64 * 0.893) as i32;
        let x0 = o.x + (w as f64 * 0.012) as i32;
        let x1 = o.x + (w as f64 * 0.84) as i32;
        let step = (((x1 - x0) / 220).max(2)) as i32;
        let mut best_score = -1i64;
        let mut best_frac = -1.0f64;
        for dy in (-40i32..=40).step_by(2) {
            let y = base_y + dy;
            let mut bright_until = i64::MIN;
            let mut track_until = i64::MIN;
            let mut x = x0;
            while x <= x1 {
                let px = GetPixel(hdc, x, y);
                if px != 0xFFFF_FFFF {
                    let rr = (px & 0xff) as i64;
                    let gg = ((px >> 8) & 0xff) as i64;
                    let bb = ((px >> 16) & 0xff) as i64;
                    let lum = rr + gg + bb;
                    let sat = rr.max(gg).max(bb) - rr.min(gg).min(bb);
                    let is_track = lum > 130 && sat < 60; // 灰轨道 或 亮填充（都低饱和）
                    if is_track { track_until = x as i64; }
                    if lum > 420 && sat < 40 { bright_until = x as i64; } // 白色填充/圆点
                }
                x += step;
            }
            if track_until <= x0 as i64 || bright_until < x0 as i64 { continue; }
            let span = (track_until - x0 as i64) as f64;
            if span < 60.0 { continue; }
            let score = span as i64;
            let frac = ((bright_until - x0 as i64) as f64 / span).clamp(0.0, 1.0);
            if score > best_score { best_score = score; best_frac = frac; }
        }
        ReleaseDC(0, hdc);
        if best_score < 0 { None } else { Some(best_frac) }
    }
}
/// 通过真实鼠标点击网易云进度条来实现定位（SMTC Seek 对网易云无效）。
pub fn netease_seek_frac(frac: f64) -> bool {
    unsafe {
        let h = match netease_hwnd() { Some(h) => h, None => return false };
        if IsIconic(h) != 0 { return false; }
        let mut r = RECT::default();
        if GetClientRect(h, &mut r) == 0 { return false; }
        let w = r.right - r.left;
        let ht = r.bottom - r.top;
        if w < 1700 || ht < 900 { return false; }
        let cx = (w as f64 * 0.02 + w as f64 * 0.96 * frac.clamp(0.0, 1.0)) as i32;
        let cy = (ht as f64 * 0.891) as i32;
        let mut pt = POINT { x: cx, y: cy };
        if ClientToScreen(h, &mut pt) == 0 { return false; }
        let mut old = POINT::default();
        GetCursorPos(&mut old);
        SetForegroundWindow(h);
        SetCursorPos(pt.x, pt.y);
        mouse_event(0x0002, 0, 0, 0, 0);
        mouse_event(0x0004, 0, 0, 0, 0);
        SetCursorPos(old.x, old.y);
        true
    }
}

static NET_FRAC: AtomicI64 = AtomicI64::new(-1);
static NET_POS: AtomicI64 = AtomicI64::new(-1);
static NET_DUR: AtomicI64 = AtomicI64::new(-1);

static NET_READER_WANTED: AtomicBool = AtomicBool::new(false);
static NET_READER_RUN: AtomicBool = AtomicBool::new(false);

/// 按需启停网易云时间轴读取线程：只有当前播放器是网易云时才运行，其它时候完全退出，
/// 不常驻、不占用后台内存。可每帧调用（只是原子开关）。
pub fn set_netease_reader_enabled(on: bool) {
    NET_READER_WANTED.store(on, Ordering::Relaxed);
    if on && !NET_READER_RUN.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            netease_ensure_offsets();
            let mut last_log = std::time::Instant::now();
            while NET_READER_WANTED.load(Ordering::Relaxed) {
                let mut got = netease_pos_dur();
                if got.is_none() {
                    netease_ensure_offsets();
                    got = netease_pos_dur();
                }
                if let Some((p, d)) = got {
                    NET_POS.store((p * 1000.0) as i64, Ordering::Relaxed);
                    NET_DUR.store((d * 1000.0) as i64, Ordering::Relaxed);
                    NET_FRAC.store(-1, Ordering::Relaxed);
                    if last_log.elapsed() >= std::time::Duration::from_secs(2) {
                        last_log = std::time::Instant::now();
                        {
                            use std::io::Write;
                            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::paths::file("netease_tl.log")) {
                                let _ = writeln!(f, "pos={:.2} dur={:.2}", p, d);
                            }
                        }
                    }
                } else {
                    NET_POS.store(-1, Ordering::Relaxed);
                    NET_DUR.store(-1, Ordering::Relaxed);
                    let v = match netease_frac() { Some(f) => (f * 1_000_000.0) as i64, None => -1 };
                    NET_FRAC.store(v, Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            // 退出前清空缓存，避免残留旧歌进度被别处读到
            NET_POS.store(-1, Ordering::Relaxed);
            NET_DUR.store(-1, Ordering::Relaxed);
            NET_FRAC.store(-1, Ordering::Relaxed);
            NET_READER_RUN.store(false, Ordering::SeqCst);
            // 极短时间内的开关竞态兜底：若期间又需要，重新拉起
            if NET_READER_WANTED.load(Ordering::Relaxed) {
                set_netease_reader_enabled(true);
            }
        });
    }
}

/// 兼容旧调用：立即按需启动（内部仍受开关管理）。
pub fn start_netease_reader() {
    set_netease_reader_enabled(true);
}

/// 读取后台线程缓存的网易云进度比例（0..1），无则 None。
pub fn netease_frac_cached() -> Option<f64> {
    let v = NET_FRAC.load(Ordering::Relaxed);
    if v < 0 { None } else { Some(v as f64 / 1_000_000.0) }
}

/// 后台线程缓存的网易云 (position, duration) 秒；内存读取成功时优先使用。
pub fn netease_posdur_cached() -> Option<(f64, f64)> {
    let p = NET_POS.load(Ordering::Relaxed);
    let d = NET_DUR.load(Ordering::Relaxed);
    if p < 0 || d <= 0 { None } else { Some((p as f64 / 1000.0, d as f64 / 1000.0)) }
}

/// 后台执行点击定位（会短暂移动鼠标，不能放在 UI 线程）。
pub fn netease_seek_async(frac: f64) {
    std::thread::spawn(move || { let _ = netease_seek_frac(frac); });
}

// ===== 网易云内存时间轴：读 cloudmusic.dll 内的 position/duration（等同岛仔方案，最小化也能精确同步） =====
#[repr(C)]
struct ProcessEntry32W {
    dw_size: u32,
    cnt_usage: u32,
    th32_process_id: u32,
    th32_default_heap_id: usize,
    th32_module_id: u32,
    cnt_threads: u32,
    th32_parent_process_id: u32,
    pc_pri_class_base: i32,
    dw_flags: u32,
    sz_exe_file: [u16; 260],
}
#[link(name = "kernel32")]
extern "system" {
    fn ReadProcessMemory(h: isize, addr: usize, buf: *mut u8, size: usize, read: *mut usize) -> i32;
    fn K32EnumProcessModules(h: isize, mods: *mut usize, size: u32, needed: *mut u32) -> i32;
    fn K32GetModuleBaseNameW(h: isize, module: usize, name: *mut u16, size: u32) -> u32;
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
    fn Process32FirstW(snap: isize, pe: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snap: isize, pe: *mut ProcessEntry32W) -> i32;
}
fn read_f64(h: isize, addr: usize) -> Option<f64> {
    let mut b = [0u8; 8];
    let mut got = 0usize;
    let ok = unsafe { ReadProcessMemory(h, addr, b.as_mut_ptr(), 8, &mut got) };
    (ok != 0 && got == 8).then(|| f64::from_le_bytes(b))
}
/// 指定进程名是否在运行（供设置页展示连接状态）。
pub fn process_running(name: &str) -> bool {
    !pids_named(name).is_empty()
}

fn pids_named(name: &str) -> Vec<u32> {    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(0x00000002, 0);
        if snap == -1 { return out; }
        let mut pe: ProcessEntry32W = std::mem::zeroed();
        pe.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
        if Process32FirstW(snap, &mut pe) != 0 {
            loop {
                let end = pe.sz_exe_file.iter().position(|&c| c == 0).unwrap_or(260);
                let exe = String::from_utf16_lossy(&pe.sz_exe_file[..end]);
                if exe.eq_ignore_ascii_case(name) { out.push(pe.th32_process_id); }
                pe.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
                if Process32NextW(snap, &mut pe) == 0 { break; }
            }
        }
        CloseHandle(snap);
    }
    out
}
/// 是否有已知音乐客户端进程在运行（网易云/汽水/QQ/酷狗/酷我/Spotify/Apple Music/波点）。
/// 注意：这里用 exe 文件名粗判，可能有极短误判窗口，但足以用来决定“该不该保留上次的歌名”。
pub fn player_running() -> bool {
    static WATCH: std::sync::Once = std::sync::Once::new();
    WATCH.call_once(|| {
        // 首次同步采样一次：否则后台线程还没跑起来时 PLAYER_RUN 仍是初始 false，
        // 会让启动后第一秒内误判“播放器未运行”，导致音乐模块延迟出现，并在与“丢失去抖”
        // 叠加时造成收起条先展开→缩回→再展开的假象。一次进程快照仅几毫秒，且只发生一次。
        PLAYER_RUN.store(player_running_now(), std::sync::atomic::Ordering::Relaxed);
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            PLAYER_RUN.store(player_running_now(), std::sync::atomic::Ordering::Relaxed);
        });
    });
    PLAYER_RUN.load(std::sync::atomic::Ordering::Relaxed)
}

static PLAYER_RUN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn player_running_now() -> bool {
    const NAMES: [&str; 8] = [
        "cloudmusic.exe",
        "sodamusic.exe",
        "qqmusic.exe",
        "kugou.exe",
        "kwmusic.exe",
        "spotify.exe",
        "applemusic.exe",
        "bodian_pc.exe",
    ];
    // 只做一次进程快照，避免对每个名字都枚举一遍（原来 6 次快照单帧要 50~80ms）。
    unsafe {
        let snap = CreateToolhelp32Snapshot(0x00000002, 0);
        if snap == -1 {
            return false;
        }
        let mut pe: ProcessEntry32W = std::mem::zeroed();
        pe.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
        let mut found = false;
        if Process32FirstW(snap, &mut pe) != 0 {
            loop {
                let end = pe.sz_exe_file.iter().position(|&c| c == 0).unwrap_or(260);
                let exe = String::from_utf16_lossy(&pe.sz_exe_file[..end]);
                if NAMES.iter().any(|n| exe.eq_ignore_ascii_case(n)) {
                    found = true;
                    break;
                }
                pe.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
                if Process32NextW(snap, &mut pe) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

// ---------- 网易云内存时间轴：偏移自动发现 + 持久化 ----------
// 说明：网易云每次自动更新，cloudmusic.dll 里 position/duration 的偏移可能整体平移。
// 这里先查内置版本表/已保存偏移；都失败时后台线程用“两次快照差分”自动找出
// “每秒+1 的 f64”（position）和稳定的“总时长 f64”，校验通过后写入
// %APPDATA%\WinIsle\netease_offsets.txt，今后版本再用同一映像大小时直接复用。

static NET_POS_OFF: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static NET_DUR_OFF: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static NET_DISCOVER_AT: AtomicI64 = AtomicI64::new(0);

fn offsets_path() -> Option<std::path::PathBuf> {
    Some(crate::paths::file("netease_offsets.txt"))
}

fn saved_offsets(img: u32) -> Option<(usize, usize)> {
    let p = offsets_path()?;
    let txt = std::fs::read_to_string(p).ok()?;
    for line in txt.lines() {
        let mut it = line.split_whitespace();
        let (i, po, du) = (it.next()?, it.next()?, it.next()?);
        if i.parse::<u32>().ok() == Some(img) {
            return Some((usize::from_str_radix(po, 16).ok()?, usize::from_str_radix(du, 16).ok()?));
        }
    }
    None
}

fn save_offsets(img: u32, pos: usize, dur: usize) {
    if let Some(p) = offsets_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let body = format!("{:08x} {:x} {:x}\n", img, pos, dur);
        let _ = std::fs::write(p, body);
    }
}

unsafe fn module_base_img(h: isize) -> Option<(usize, u32)> {
    let mut mods = [0usize; 1024];
    let mut needed = 0u32;
    if K32EnumProcessModules(h, mods.as_mut_ptr(), std::mem::size_of_val(&mods) as u32, &mut needed) == 0 {
        return None;
    }
    let count = (needed as usize / std::mem::size_of::<usize>()).min(mods.len());
    for &base in mods.iter().take(count) {
        let mut nb = [0u16; 260];
        let n = K32GetModuleBaseNameW(h, base, nb.as_mut_ptr(), 260) as usize;
        if !String::from_utf16_lossy(&nb[..n]).eq_ignore_ascii_case("cloudmusic.dll") {
            continue;
        }
        let mut lfanew = [0u8; 4];
        let mut got = 0usize;
        let _ = ReadProcessMemory(h, base + 0x3c, lfanew.as_mut_ptr(), 4, &mut got);
        let pe_off = u32::from_le_bytes(lfanew) as usize;
        let mut szb = [0u8; 4];
        let _ = ReadProcessMemory(h, base + pe_off + 80, szb.as_mut_ptr(), 4, &mut got);
        return Some((base, u32::from_le_bytes(szb)));
    }
    None
}

unsafe fn read_bytes(h: isize, addr: usize, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut got = 0usize;
    if ReadProcessMemory(h, addr, buf.as_mut_ptr(), len, &mut got) == 0 || got < len {
        return None;
    }
    Some(buf)
}

fn f64_at(buf: &[u8], off: usize) -> f64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&buf[off..off + 8]);
    f64::from_le_bytes(b)
}

fn scan_log(s: &str) {
    {
        use std::io::Write as _;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::paths::file("netease_tl.log")) {
            let _ = writeln!(f, "scan {}", s);
        }
    }
}

/// 两次快照差分：找 position（每秒 +1）与 duration（稳定、>=position）的偏移。
unsafe fn discover_offsets() -> Option<(usize, usize, u32)> {
    // 只在已知偏移附近窗口内搜索（±2MB），避免动辄几十 MB 的整映像读取
    const WIN: usize = 0x20_0000;
    const POS_ANCHOR: usize = 0x21a_74d8;
    const DUR_ANCHOR: usize = 0x221_4778;
    for pid in pids_named("cloudmusic.exe") {
        let h = OpenProcess(0x410, 0, pid);
        if h == 0 {
            continue;
        }
        let out = (|| -> Option<(usize, usize, u32)> {
            let (base, img) = match module_base_img(h) {
                Some(v) => v,
                None => {
                    scan_log("module not found");
                    return None;
                }
            };
            let plo = POS_ANCHOR - WIN;
            let dlo = DUR_ANCHOR - WIN;
            let a = match read_bytes(h, base + plo, 2 * WIN) {
                Some(v) => v,
                None => {
                    scan_log("read a failed");
                    return None;
                }
            };
            let da = match read_bytes(h, base + dlo, 2 * WIN) {
                Some(v) => v,
                None => {
                    scan_log("read dur-a failed");
                    return None;
                }
            };
            std::thread::sleep(std::time::Duration::from_millis(1200));
            let b = match read_bytes(h, base + plo, 2 * WIN) {
                Some(v) => v,
                None => {
                    scan_log("read b failed");
                    return None;
                }
            };
            let db = match read_bytes(h, base + dlo, 2 * WIN) {
                Some(v) => v,
                None => {
                    scan_log("read dur-b failed");
                    return None;
                }
            };
            let n = (a.len().min(b.len())) / 8 * 8;
            let mut best_d = 0.0f64;
            let mut best_off = 0usize;
            let mut i = 0usize;
            while i + 8 <= n {
                let va = f64_at(&a, i);
                let vb = f64_at(&b, i);
                if va.is_finite() && vb.is_finite() && va > 0.5 && va < 20000.0 {
                    let d = vb - va;
                    if d > 0.55 && d < 1.8 && d > best_d {
                        best_d = d;
                        best_off = i;
                    }
                }
                i += 8;
            }
            scan_log(&format!("snap img={} best_d={:.3} pos_off=0x{:x}", img, best_d, plo + best_off));
            if best_d == 0.0 {
                return None;
            }
            let pos_abs = plo + best_off;
            let pos_val = f64_at(&b, best_off);

            let m = (da.len().min(db.len())) / 8 * 8;
            let mut best_dur: Option<(i64, usize, f64)> = None;
            let mut j = 0usize;
            while j + 8 <= m {
                if j + 8 <= m {
                    let va = f64_at(&da, j);
                    let vb = f64_at(&db, j);
                    if va.is_finite() && vb.is_finite() && (va - vb).abs() < 0.01 && vb >= 20.0 && vb < 20000.0 && vb >= pos_val + 5.0 {
                        let dur_abs = dlo + j;
                        let diff = dur_abs as i64 - pos_abs as i64;
                        if diff > 0 {
                            let score = (diff - 0x6F_2C0).abs();
                            if best_dur.map(|(s, _, _)| score < s).unwrap_or(true) {
                                best_dur = Some((score, j, vb));
                            }
                        }
                    }
                }
                j += 8;
            }
            scan_log(&format!("dur cands best={:?}", best_dur.map(|x| (x.0, dlo + x.1, x.2))));
            let (_, dur_rel, dur_expected) = best_dur?;
            let dur_abs = dlo + dur_rel;
            scan_log(&format!("dur_off=0x{:x} dur={:.2} pos={:.2}", dur_abs, dur_expected, pos_val));

            std::thread::sleep(std::time::Duration::from_millis(900));
            let c = read_bytes(h, base + plo, 2 * WIN)?;
            let dc = read_bytes(h, base + dlo, 2 * WIN)?;
            let pos2 = f64_at(&c, best_off);
            let dur2 = f64_at(&dc, dur_rel);
            scan_log(&format!("verify dpos={:.3} dur2={:.2}", pos2 - pos_val, dur2));
            if (pos2 - pos_val) >= 0.2 && (pos2 - pos_val) <= 4.0 && (dur2 - dur_expected).abs() < 0.05 {
                return Some((pos_abs, dur_abs, img));
            }
            None
        })();
        CloseHandle(h);
        if out.is_some() {
            return out;
        }
    }
    None
}

/// 后台调用：若还没偏移，尝试从已保存文件/内置表/自动发现中确定，并持久化。
pub fn netease_ensure_offsets() {
    let flag = Some(crate::paths::file("offset_scan.flag"));
    let force = std::env::var_os("WINISLE_OFFSET_SCAN").is_some()
        || flag.as_ref().map(|p| p.exists()).unwrap_or(false);
    if let Some(p) = &flag {
        if p.exists() {
            let _ = std::fs::remove_file(p);
        }
    }
    scan_log(&format!("ensure enter force={} pids={}", force, pids_named("cloudmusic.exe").len()));
    if !force && NET_POS_OFF.load(Ordering::Relaxed) != 0 {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if !force && now - NET_DISCOVER_AT.load(Ordering::Relaxed) < 10 {
        return;
    }
    NET_DISCOVER_AT.store(now, Ordering::Relaxed);
    unsafe {
        // 先看已保存偏移是否仍成立
        for pid in pids_named("cloudmusic.exe") {
            let h = OpenProcess(0x410, 0, pid);
            if h == 0 {
                continue;
            }
            let mut ok = None;
            if let Some((base, img)) = module_base_img(h) {
                if let Some((po, du)) = saved_offsets(img) {
                    if let (Some(p), Some(d)) = (read_f64(h, base + po), read_f64(h, base + du)) {
                        if p.is_finite() && d.is_finite() && d >= 20.0 && d < 86400.0 && p >= 0.0 && p <= d + 1.0 {
                            ok = Some((po, du));
                        }
                    }
                }
            }
            CloseHandle(h);
            if let Some((po, du)) = ok {
                NET_POS_OFF.store(po, Ordering::Relaxed);
                NET_DUR_OFF.store(du, Ordering::Relaxed);
                return;
            }
        }
        // 再尝试自动发现
        if let Some((po, du, img)) = discover_offsets() {
            NET_POS_OFF.store(po, Ordering::Relaxed);
            NET_DUR_OFF.store(du, Ordering::Relaxed);
            save_offsets(img, po, du);
        }
    }
}

/// 读网易云内存里的 (position, duration)，单位秒。仅在数值合理时返回。
/// 优先使用自动发现/持久化的偏移；其次按 cloudmusic.dll 映像大小查内置版本表。
/// 当前“活跃”的网易云进程：拥有“可见且有标题窗口”的那个。
/// （网易云常同时有 3 个进程，其它多是辅助/上次会话进程，内存里可能是别的歌，
///  逐个取第一个“数值合理”的会随机读到旧歌的 position/duration。）
fn active_cloudmusic_pid() -> Option<u32> {
    let mut v: Vec<(u32, String)> = Vec::new();
    unsafe {
        EnumWindows(Some(enum_media_cb), &mut v as *mut _ as isize);
    }
    for (pid, title) in v {
        if title.trim().is_empty()
            || title.contains("SMTC")
            || title.contains("MediaPlayer")
            || title.contains('{')
            || title.contains('}')
        {
            continue;
        }
        if exe_name_of_pid(pid).eq_ignore_ascii_case("cloudmusic.exe") {
            return Some(pid);
        }
    }
    None
}

pub fn netease_pos_dur() -> Option<(f64, f64)> {
    const IMG_OLD: u32 = 41054208; // 2.9.7.x（旧构建）
    const IMG_NEW: u32 = 41058304; // 应用自动更新后的构建
    const POS_OLD: usize = 0x21a74d8;
    const DUR_OLD: usize = 0x2214778;
    const POS_NEW: usize = 0x21a84d8;
    const DUR_NEW: usize = 0x2215798;
    let po = NET_POS_OFF.load(Ordering::Relaxed);
    let du = NET_DUR_OFF.load(Ordering::Relaxed);
    unsafe {
        // 优先读“有可见播放窗口”的那个云音乐进程，避免读到其它辅助进程里的旧歌数据
        let mut pids = pids_named("cloudmusic.exe");
        if let Some(ap) = active_cloudmusic_pid() {
            pids.sort_by_key(|&p| if p == ap { 0i32 } else { 1i32 });
        }
        for pid in pids {
            let h = OpenProcess(0x410, 0, pid);
            if h == 0 {
                continue;
            }
            let mut found: Option<(f64, f64)> = None;
            if let Some((base, img)) = module_base_img(h) {
                // 1) 已确定（自动发现或已保存）的偏移
                if po != 0 && du != 0 {
                    if let (Some(p), Some(d)) = (read_f64(h, base + po), read_f64(h, base + du)) {
                        if p.is_finite() && d.is_finite() && d >= 20.0 && d < 86400.0 && p >= 0.0 && p <= d + 1.0 {
                            found = Some((p, d));
                        }
                    }
                }
                // 2) 内置版本表
                if found.is_none() {
                    let pair = if img == IMG_NEW {
                        Some((POS_NEW, DUR_NEW))
                    } else if img == IMG_OLD {
                        Some((POS_OLD, DUR_OLD))
                    } else {
                        None
                    };
                    if let Some((pos_off, dur_off)) = pair {
                        if let (Some(p), Some(d)) = (read_f64(h, base + pos_off), read_f64(h, base + dur_off)) {
                            if p.is_finite() && d.is_finite() && d >= 20.0 && d < 86400.0 && p >= 0.0 && p <= d + 1.0 {
                                found = Some((p, d));
                            }
                        }
                    }
                }
            }
            CloseHandle(h);
            if found.is_some() {
                return found;
            }
        }
    }
    None
}

// ===== VIP 试听片段起点 =====
// 网易云播放 VIP 试听时只给一小段（如 20~40s），且可能从歌曲中间（副歌）开始。
// 播放器上报的 position 是“片段内相对时间”，歌词却按整首绝对时间排。
// 网易云会在播放资源加载完成后把带 `freeTrialInfo:{...,"start":S,"end":E}` 的 JSON
// 打进自己的内存日志里（例如 `[2026-11-16 16:50:21] 【playing】... "songId":"..." ...
// "freeTrialInfo":{"fragmentType":1018,"start":57,"end":77,...}`）。这里扫描该进程
// 已提交内存，取“本轮播放开始之后”最新一条记录的 start，作为歌词时间轴偏移。
#[repr(C)]
struct MemoryBasicInformation {
    base_address: usize,
    allocation_base: usize,
    allocation_protect: u32,
    _pad1: u32,
    region_size: usize,
    state: u32,
    protect: u32,
    mem_type: u32,
    _pad2: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn VirtualQueryEx(h: isize, addr: usize, mbi: *mut MemoryBasicInformation, len: usize) -> usize;
}

/// 当前本地时间 `YYYY-MM-DD HH:MM:SS`（与网易云内存日志里的时间戳格式一致）。
pub fn local_stamp() -> String {
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
}

fn find_sub(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() || from >= hay.len() {
        return None;
    }
    let mut i = from;
    while i + needle.len() <= hay.len() {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// 从 `from` 起 300 字节内找 `key`（如 `"start":`），返回其后的整数。
fn num_after_key(data: &[u8], from: usize, key: &[u8]) -> Option<i64> {
    let end = (from + 300).min(data.len());
    let slice = &data[from..end];
    let kp = find_sub(slice, key, 0)?;
    let mut j = kp + key.len();
    while j < slice.len() && !slice[j].is_ascii_digit() {
        j += 1;
    }
    let s = j;
    while j < slice.len() && slice[j].is_ascii_digit() {
        j += 1;
    }
    if s == j {
        return None;
    }
    std::str::from_utf8(&slice[s..j]).ok()?.parse().ok()
}

/// 在 `before` 之前 400 字节内向后找最近的 `[YYYY-MM-DD HH:MM:SS]`，返回时间串。
fn timestamp_before(data: &[u8], before: usize) -> Option<String> {
    let lo = before.saturating_sub(1200);
    let mut i = before;
    while i > lo {
        i -= 1;
        if data[i] == b'[' && i + 20 <= data.len() {
            let s = &data[i + 1..i + 20];
            if s[4] == b'-' && s[7] == b'-' && s[10] == b' ' && s[13] == b':' && s[16] == b':' {
                return std::str::from_utf8(s).ok().map(|x| x.to_string());
            }
        }
    }
    None
}

/// 扫描网易云进程内存里内存日志的 `freeTrialInfo` 记录，返回时间戳在 `since` 之后的
/// 最新一条的 start（秒）。`since` 用 `local_stamp()` 的格式；传空串表示不过滤。
fn trial_start_in(h: isize, since: &str) -> Option<f32> {
    const MEM_COMMIT: u32 = 0x1000;
    const PAGE_GUARD: u32 = 0x100;
    const PAGE_NOACCESS: u32 = 0x01;
    const CHUNK: usize = 1 << 20;
    let needle = b"freeTrialInfo\":{\"fragmentType\":";
    let mut addr: usize = 0;
    let mut buf = vec![0u8; CHUNK];
    let mut carry: Vec<u8> = Vec::new();
    let mut best_ts: Option<String> = None;
    let mut best_start: f32 = 0.0;
    let mut best_addr: usize = 0;
    let max_addr: usize = 0x0000_7fff_ffff_ffff;
    while addr < max_addr {
        let mut mbi: MemoryBasicInformation = unsafe { std::mem::zeroed() };
        let r = unsafe {
            VirtualQueryEx(
                h,
                addr,
                &mut mbi,
                std::mem::size_of::<MemoryBasicInformation>(),
            )
        };
        if r == 0 {
            break;
        }
        let base = mbi.base_address;
        let size = mbi.region_size.max(1);
        let readable = mbi.state == MEM_COMMIT
            && (mbi.protect & PAGE_GUARD) == 0
            && (mbi.protect & 0xFF) != PAGE_NOACCESS;
        if readable {
            let mut off = 0usize;
            while off < size {
                let n = CHUNK.min(size - off);
                let mut got = 0usize;
                let ok = unsafe { ReadProcessMemory(h, base + off, buf.as_mut_ptr(), n, &mut got) };
                if ok != 0 && got > 0 {
                    let carry_len = carry.len();
                    let mut data: Vec<u8> = Vec::with_capacity(carry_len + got);
                    data.extend_from_slice(&carry);
                    data.extend_from_slice(&buf[..got]);
                    let data_start = base + off - carry_len;
                    let mut idx = 0usize;
                    while let Some(pos) = find_sub(&data, needle, idx) {
                        if let Some(v) = num_after_key(&data, pos + needle.len(), b"\"start\":") {
                            let ts = timestamp_before(&data, pos);
                            let eligible = match &ts {
                                Some(t) => since.is_empty() || t.as_str() >= since,
                                None => since.is_empty(),
                            };
                            if eligible {
                                let addr_here = data_start + pos;
                                let better = match (&ts, &best_ts) {
                                    (Some(a), Some(b)) => a > b,
                                    (Some(_), None) => true,
                                    (None, Some(_)) => false,
                                    (None, None) => addr_here >= best_addr,
                                };
                                if better {
                                    best_start = v.max(0) as f32;
                                    best_addr = addr_here;
                                    best_ts = ts;
                                }
                            }
                        }
                        idx = pos + 1;
                    }
                    let keep = data.len().saturating_sub(2048);
                    carry.clear();
                    carry.extend_from_slice(&data[keep..]);
                } else {
                    carry.clear();
                }
                off += n;
            }
        }
        addr = base.saturating_add(size);
    }
    best_ts.map(|_| best_start)
}

/// 当前网易云“本轮播放开始（since）之后”最新试听片段的起点（秒）。非试听/找不到返回 None。
pub fn netease_trial_start_since(since: &str) -> Option<f32> {
    let pid = active_cloudmusic_pid()?;
    let h = unsafe { OpenProcess(0x410, 0, pid) };
    if h == 0 {
        return None;
    }
    let r = trial_start_in(h, since);
    unsafe { CloseHandle(h) };
    r
}

/// 调试/验证用：列出内存日志里所有 freeTrialInfo 的 (时间戳, start)，按时间升序。
pub fn debug_trial_entries() -> Vec<(String, f32)> {
    const MEM_COMMIT: u32 = 0x1000;
    const PAGE_GUARD: u32 = 0x100;
    const PAGE_NOACCESS: u32 = 0x01;
    const CHUNK: usize = 1 << 20;
    let needle = b"freeTrialInfo\":{\"fragmentType\":";
    let pid = match active_cloudmusic_pid() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let h = unsafe { OpenProcess(0x410, 0, pid) };
    if h == 0 {
        return Vec::new();
    }
    let mut out: Vec<(String, f32)> = Vec::new();
    let mut addr: usize = 0;
    let mut buf = vec![0u8; CHUNK];
    let mut carry: Vec<u8> = Vec::new();
    while addr < 0x0000_7fff_ffff_ffff {
        let mut mbi: MemoryBasicInformation = unsafe { std::mem::zeroed() };
        let r = unsafe {
            VirtualQueryEx(h, addr, &mut mbi, std::mem::size_of::<MemoryBasicInformation>())
        };
        if r == 0 {
            break;
        }
        let base = mbi.base_address;
        let size = mbi.region_size.max(1);
        if mbi.state == MEM_COMMIT && (mbi.protect & PAGE_GUARD) == 0 && (mbi.protect & 0xFF) != PAGE_NOACCESS {
            let mut off = 0usize;
            while off < size {
                let n = CHUNK.min(size - off);
                let mut got = 0usize;
                let ok = unsafe { ReadProcessMemory(h, base + off, buf.as_mut_ptr(), n, &mut got) };
                if ok != 0 && got > 0 {
                    let mut data: Vec<u8> = Vec::with_capacity(carry.len() + got);
                    data.extend_from_slice(&carry);
                    data.extend_from_slice(&buf[..got]);
                    let mut idx = 0usize;
                    while let Some(pos) = find_sub(&data, needle, idx) {
                        if let Some(v) = num_after_key(&data, pos + needle.len(), b"\"start\":") {
                            let ts = timestamp_before(&data, pos).unwrap_or_default();
                            let item = (ts, v.max(0) as f32);
                            if !out.contains(&item) {
                                out.push(item);
                            }
                        }
                        idx = pos + 1;
                    }
                    let keep = data.len().saturating_sub(2048);
                    carry.clear();
                    carry.extend_from_slice(&data[keep..]);
                } else {
                    carry.clear();
                }
                off += n;
            }
        }
        addr = base.saturating_add(size);
    }
    unsafe { CloseHandle(h) };
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)));
    out
}

/// 取屏幕某点的感知亮度（0..1），用于让霓虹边缘在浅色壁纸上也看得清。
pub fn screen_luminance_at(x: i32, y: i32) -> f32 {
    unsafe {
        let hdc = GetDC(0);
        if hdc == 0 { return 0.0; }
        let px = GetPixel(hdc, x, y);
        ReleaseDC(0, hdc);
        if px == 0xFFFF_FFFF { return 0.0; }
        let r = (px & 0xff) as f32;
        let g = ((px >> 8) & 0xff) as f32;
        let b = ((px >> 16) & 0xff) as f32;
        (0.299 * r + 0.587 * g + 0.114 * b) / 255.0
    }
}
