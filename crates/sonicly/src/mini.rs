//! The mini player's window: a small, borderless, always-on-top window that stands in for the
//! main one. Opening it hands the main window to the taskbar or the Dock; closing it, or its
//! way back, brings the main window back. One opens at a time, and asking again focuses it.
//!
//! What each platform gives it, all from the gpui fork unless noted:
//! - Windows: `WindowKind::PopUp` is a `WS_EX_TOPMOST | WS_EX_TOOLWINDOW` window with no frame,
//!   so it stays on top and keeps out of the taskbar and Alt+Tab. DWM rounds its corners.
//! - macOS: `PopUp` is a non-activating `NSPanel` at the pop-up level that joins every Space and
//!   sits over full-screen apps. It is told here not to hide when Sonicly is in the background,
//!   and its title bar buttons are hidden.
//! - Linux: `PopUp` would be an override-redirect notification the window manager can neither
//!   move nor focus, so it is a normal window with client decorations. On X11 it asks the window
//!   manager to keep it above and out of the taskbar. Wayland has no way to ask for either.

use gpui::{
    App, AppContext as _, Bounds, Pixels, Point, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, point,
};
use i18n::t;
use input::{LeaveMiniPlayer, ToggleMiniPlayer};
use state::Sonicly;
use ui::ActiveTheme as _;
use views::MiniPlayer;

/// Hooks the two mini player actions up. The shortcut opens the mini player from the main
/// window and returns from the mini player when it has focus.
pub fn register(cx: &mut App) {
    cx.on_action(|_: &ToggleMiniPlayer, cx: &mut App| {
        let active = cx.active_window();
        match find(cx) {
            Some(mini) if active == Some(mini.into()) => leave(cx),
            _ => open(cx),
        }
    });
    cx.on_action(|_: &LeaveMiniPlayer, cx: &mut App| leave(cx));
}

/// Opens the mini player, or focuses the one already open, and minimizes the main window.
pub fn open(cx: &mut App) {
    if let Some(mini) = find(cx) {
        mini.update(cx, |_, window, _| window.activate_window())
            .ok();
        return;
    }
    let bounds = placement(cx);
    let shot = std::env::var_os("SONICLY_SHOT").is_some();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds.0)),
        display_id: bounds.1,
        titlebar: Some(TitlebarOptions {
            title: Some(t!("mini-player-title")),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        kind: match cfg!(any(target_os = "linux", target_os = "freebsd")) {
            true => WindowKind::Normal,
            false => WindowKind::PopUp,
        },
        is_movable: true,
        app_owns_titlebar_drag: true,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Opaque,
        app_id: Some(crate::assets::app_id().into()),
        // capture mode: scripts/shot.ps1 photographs the window without stealing focus
        focus: !shot,
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        settle(window, cx);
        state::remember_mini_player(window, cx);
        // A close the platform asks for (Alt+F4, the Window menu) brings the main window back
        // first, so the app never finds itself without a window and quits.
        window.on_window_should_close(cx, |_, cx| {
            crate::show_window(cx);
            true
        });
        cx.new(|cx| MiniPlayer::new(window, cx))
    });
    if let Err(error) = opened {
        log::warn!("mini player: cannot open the window: {error:#}");
        return;
    }
    if let Some(main) = crate::main_window(cx) {
        main.update(cx, |_, window, _| window.minimize_window())
            .ok();
    }
}

/// Closes the mini player and brings the main window back, opening it again if it was closed.
/// The main window comes first, so closing the last window never quits the app.
pub fn leave(cx: &mut App) {
    crate::show_window(cx);
    if let Some(mini) = find(cx) {
        mini.update(cx, |_, window, _| window.remove_window()).ok();
    }
}

/// The mini player's window, when it is open.
pub fn find(cx: &App) -> Option<WindowHandle<MiniPlayer>> {
    cx.windows()
        .into_iter()
        .find_map(|window| window.downcast::<MiniPlayer>())
}

/// Where the mini player opens: where it was last left, or else tucked into the bottom right
/// corner of the main window's display, clear of its edge by the page inset.
fn placement(cx: &mut App) -> (Bounds<Pixels>, Option<gpui::DisplayId>) {
    let bar = Sonicly::global(cx).settings.read(cx).mini_player_bar();
    let size = MiniPlayer::size(bar, cx);
    if let Some((origin, display)) = state::mini_player_placement(cx) {
        return (Bounds::new(origin, size), Some(display));
    }
    let inset = cx.theme().metrics.inset;
    let display = crate::main_window(cx)
        .and_then(|main| main.update(cx, |_, window, cx| window.display(cx)).ok())
        .flatten()
        .or_else(|| cx.primary_display());
    match display {
        Some(display) => {
            let room = display.visible_bounds();
            let origin: Point<Pixels> = point(
                room.right() - size.width - inset,
                room.bottom() - size.height - inset,
            );
            (Bounds::new(origin, size), Some(display.id()))
        }
        None => (Bounds::centered(None, size, cx), None),
    }
}

/// What the platform needs beyond the window options: rounded corners on Windows, staying put
/// over other apps on macOS, staying above other windows on X11.
fn settle(window: &mut Window, cx: &mut App) {
    window.set_rem_size(cx.theme().font_size);
    #[cfg(target_os = "windows")]
    crate::set_corner_preference(
        window,
        Sonicly::global(cx).settings.read(cx).window_rounding(),
    );
    #[cfg(target_os = "macos")]
    float_over_apps(window);
    #[cfg(target_os = "linux")]
    if let Err(error) = keep_above(window) {
        log::warn!("mini player: cannot keep the window above others: {error:#}");
    }
}

/// Keeps the panel on screen while another app is active, and hides the title bar buttons a
/// titled panel would otherwise draw over the cover.
#[cfg(target_os = "macos")]
fn float_over_apps(window: &Window) {
    use objc2_app_kit::{NSView, NSWindowButton};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(RawWindowHandle::AppKit(handle)) =
        HasWindowHandle::window_handle(window).map(|handle| handle.as_raw())
    else {
        return;
    };
    // SAFETY: GPUI hands out its live `NSView` for the window, and this runs on the main thread.
    let view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let Some(native) = view.window() else {
        return;
    };
    native.setHidesOnDeactivate(false);
    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = native.standardWindowButton(kind) {
            button.setHidden(true);
        }
    }
}

/// Asks an EWMH window manager to keep the window above the others and out of the taskbar,
/// borrowing GPUI's own connection the way `x11_icon` does. Wayland gives no such hint.
#[cfg(target_os = "linux")]
fn keep_above(window: &Window) -> anyhow::Result<()> {
    use anyhow::Context as _;
    use raw_window_handle::{HasDisplayHandle as _, RawDisplayHandle, RawWindowHandle};
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};
    use x11rb::xcb_ffi::XCBConnection;

    let RawWindowHandle::Xcb(handle) =
        raw_window_handle::HasWindowHandle::window_handle(window)?.as_raw()
    else {
        return Ok(());
    };
    let RawDisplayHandle::Xcb(display) = window.display_handle()?.as_raw() else {
        return Ok(());
    };
    let connection = display
        .connection
        .context("cannot find the X11 connection")?;
    let connection = unsafe { XCBConnection::from_raw_xcb_connection(connection.as_ptr(), false) }
        .context("cannot borrow the X11 connection")?;
    let atom = |name: &[u8]| -> anyhow::Result<u32> {
        Ok(connection.intern_atom(false, name)?.reply()?.atom)
    };
    let (state, above, skip_taskbar) = (
        atom(b"_NET_WM_STATE")?,
        atom(b"_NET_WM_STATE_ABOVE")?,
        atom(b"_NET_WM_STATE_SKIP_TASKBAR")?,
    );
    let target = handle.window.get();
    let root = connection.get_geometry(target)?.reply()?.root;
    // _NET_WM_STATE_ADD for two properties at once, sent as a normal application
    let message = ClientMessageEvent::new(32, target, state, [1, above, skip_taskbar, 1, 0]);
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            message,
        )?
        .check()?;
    connection.flush().context("cannot flush the window state")
}
