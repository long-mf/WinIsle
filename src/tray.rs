//! 系统托盘小图标（基于 tray-icon / muda 跨平台封装）。
//! 常驻通知区域；右键 → 菜单（显示 / 退出）。
//!
//! 关键：菜单事件由一个**独立后台线程**阻塞监听，而不是依赖 egui 的 update 轮询。
//! 这样即使窗口被隐藏/暂停重绘（例如“岛不见了”或系统繁忙导致某帧卡住），
//! 托盘菜单依然可点、可退出、可重新唤出窗口，不会出现“托盘也退出不了”。

use tray_icon::menu::{Menu, MenuEvent, MenuItem, MenuId};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use std::sync::atomic::{AtomicBool, Ordering};

/// 托盘「退出」只置这个标志，由 UI 线程播放一个淡出+上滑的退场动画后再真正退出进程。
/// （直接在托盘线程 process::exit 会让岛体瞬间消失，没有退场过渡。）
pub static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub enum TrayAction {
    Exit,
}

pub struct Tray {
    _icon: TrayIcon,
}

impl Tray {
    /// 创建托盘图标；失败（极少数环境）返回 None，应用照常以灵动岛形式运行。
    pub fn new() -> Option<Tray> {
        let menu = Menu::new();
        // 托盘菜单只保留「退出」一项（用户要求：不再显示「显示 WinIsle」）。
        let exit = MenuItem::new("退出", true, None);
        menu.append_items(&[&exit]).ok()?;

        let (rgba, w, h) = make_icon_rgba(32, 32);
        let icon = Icon::from_rgba(rgba, w, h).ok()?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false) // 只有右键弹菜单；左键无动作
            .with_tooltip(if cfg!(feature = "dev-icon") { "WinIsle 开发版" } else { "WinIsle" })
            .with_icon(icon)
            .build()
            .ok()?;

        // 独立线程监听菜单事件：不依赖 UI 帧循环，隐藏/卡顿时也能响应。
        let exit_id = exit.id().clone();
        std::thread::spawn(move || loop {
            if let Ok(me) = MenuEvent::receiver().recv() {
                if me.id == exit_id {
                    QUIT_REQUESTED.store(true, Ordering::SeqCst);
                }
            }
        });

        Some(Tray { _icon: tray })
    }

    /// 非阻塞读取一次托盘图标事件（仅左键图标点击等，不消耗菜单事件）。
    pub fn poll(&self) -> Option<TrayAction> {
        while let Ok(_ev) = TrayIconEvent::receiver().try_recv() { /* 左键点击不触发任何动作 */ }
        None
    }
}

/// 托盘图标：与 exe 图标同一套「WinIsle」品牌（深色圆角方块 + M 立于青绿孤岛）。
/// 32×32 RGBA 由 assets 在构建期嵌入，避免运行时绘制差异。
/// 开发版（`--features dev-icon`）使用带琥珀色 DEV 角标的图标，便于与正式版区分。
#[cfg(not(feature = "dev-icon"))]
fn make_icon_rgba(_w: u32, _h: u32) -> (Vec<u8>, u32, u32) {
    (include_bytes!("../assets/winisle_32.rgba").to_vec(), 32, 32)
}

#[cfg(feature = "dev-icon")]
fn make_icon_rgba(_w: u32, _h: u32) -> (Vec<u8>, u32, u32) {
    (include_bytes!("../assets/winisle_dev_32.rgba").to_vec(), 32, 32)
}
