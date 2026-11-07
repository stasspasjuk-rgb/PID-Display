#![allow(non_snake_case)]
use std::ffi::c_void;

pub type Handle = *mut c_void;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct Paint {
    dc: Handle,
    erase: i32,
    rect: Rect,
    restore: i32,
    update: i32,
    reserved: [u8; 32],
}

#[repr(C)]
#[derive(Default)]
pub struct Msg {
    hwnd: Handle,
    message: u32,
    w: usize,
    l: isize,
    time: u32,
    point: Point,
    private: u32,
}

#[repr(C)]
pub struct WindowClass {
    pub style: u32,
    pub proc: Option<unsafe extern "system" fn(Handle, u32, usize, isize) -> isize>,
    pub cls_extra: i32,
    pub wnd_extra: i32,
    pub instance: Handle,
    pub icon: Handle,
    pub cursor: Handle,
    pub background: Handle,
    pub menu: *const u16,
    pub name: *const u16,
}

#[repr(C)]
#[derive(Default)]
pub struct MonitorInfo {
    pub size: u32,
    pub monitor: Rect,
    pub work: Rect,
    pub flags: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct SystemTime {
    pub year: u16,
    pub month: u16,
    pub weekday: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(name: *const u16) -> Handle;
    pub fn GetLocalTime(time: *mut SystemTime);
}

#[link(name = "user32")]
extern "system" {
    pub fn SystemParametersInfoW(action: u32, parameter: u32, data: Handle, update: u32) -> i32;
    pub fn SetProcessDPIAware() -> i32;
    pub fn RegisterClassW(class: *const WindowClass) -> u16;
    pub fn CreateWindowExW(
        extra: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: Handle,
    ) -> Handle;
    pub fn LoadCursorW(instance: Handle, cursor: *const u16) -> Handle;
    pub fn ShowWindow(window: Handle, cmd: i32) -> i32;
    pub fn GetMessageW(msg: *mut Msg, window: Handle, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(msg: *const Msg) -> i32;
    pub fn DispatchMessageW(msg: *const Msg) -> isize;
    pub fn DefWindowProcW(window: Handle, message: u32, w: usize, l: isize) -> isize;
    pub fn PostQuitMessage(code: i32);
    pub fn SetTimer(window: Handle, id: usize, milliseconds: u32, callback: Handle) -> usize;
    pub fn InvalidateRect(window: Handle, rect: *const Rect, erase: i32) -> i32;
    pub fn BeginPaint(window: Handle, paint: *mut Paint) -> Handle;
    pub fn EndPaint(window: Handle, paint: *const Paint) -> i32;
    pub fn GetClientRect(window: Handle, rect: *mut Rect) -> i32;
    pub fn GetWindowRect(window: Handle, rect: *mut Rect) -> i32;
    pub fn FillRect(dc: Handle, rect: *const Rect, brush: Handle) -> i32;
    pub fn DrawTextW(dc: Handle, text: *const u16, count: i32, rect: *mut Rect, format: u32)
        -> i32;
    pub fn MonitorFromWindow(window: Handle, flags: u32) -> Handle;
    pub fn GetMonitorInfoW(monitor: Handle, info: *mut MonitorInfo) -> i32;
    pub fn SetWindowLongW(window: Handle, index: i32, value: i32) -> i32;
    pub fn SetWindowPos(
        window: Handle,
        after: Handle,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn SaveDC(dc: Handle) -> i32;
    pub fn RestoreDC(dc: Handle, saved: i32) -> i32;
    pub fn IntersectClipRect(dc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    pub fn Polygon(dc: Handle, points: *const Point, count: i32) -> i32;
    pub fn TextOutW(dc: Handle, x: i32, y: i32, text: *const u16, count: i32) -> i32;
    pub fn GetTextExtentPoint32W(dc: Handle, text: *const u16, count: i32, size: *mut Point)
        -> i32;
    pub fn GetStockObject(index: i32) -> Handle;
    pub fn Ellipse(dc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    pub fn CreateCompatibleDC(dc: Handle) -> Handle;
    pub fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    pub fn CreateSolidBrush(color: u32) -> Handle;
    pub fn SelectObject(dc: Handle, object: Handle) -> Handle;
    pub fn DeleteDC(dc: Handle) -> i32;
    pub fn DeleteObject(object: Handle) -> i32;
    pub fn BitBlt(
        dst: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        src: Handle,
        src_x: i32,
        src_y: i32,
        operation: u32,
    ) -> i32;
    pub fn SetTextColor(dc: Handle, color: u32) -> u32;
    pub fn SetBkMode(dc: Handle, mode: i32) -> i32;
    pub fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        precision: u32,
        clip: u32,
        quality: u32,
        family: u32,
        face: *const u16,
    ) -> Handle;
}

#[link(name = "winmm")]
extern "system" {
    pub fn PlaySoundW(sound: *const u16, module: Handle, flags: u32) -> i32;
}
