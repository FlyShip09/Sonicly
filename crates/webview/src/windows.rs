//! The Windows backend: a Win32 window hosting an InPrivate WebView2 controller. WebView2 and the
//! window both live on GPUI's main STA thread; its asynchronous setup and cookie callbacks are
//! dispatched by that thread's existing message loop.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use anyhow::{Context as _, Result};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2_2, ICoreWebView2_8,
    ICoreWebView2Controller, ICoreWebView2CookieManager, ICoreWebView2Environment10,
    ICoreWebView2EnvironmentOptions,
};
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, CoTaskMemPWSTR,
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler,
    GetCookiesCompletedHandler, WebMessageReceivedEventHandler,
};
use windows::Win32::Foundation::{E_POINTER, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetSystemMetrics, IsWindow, MINMAXINFO, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
    ShowWindow, WINDOW_EX_STYLE, WM_CLOSE, WM_GETMINMAXINFO, WM_NCDESTROY, WM_SIZE, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::{GUID, Interface as _, PCWSTR, PWSTR, w};

use crate::native::{Fetch, HEIGHT, MIN_HEIGHT, MIN_WIDTH, Reading, WIDTH};
use crate::{Cookie, Target};

pub(crate) fn supported() -> bool {
    true
}

/// WebView2 is Chromium with Microsoft's licensed media modules, so a player page plays here.
pub(crate) fn plays() -> bool {
    true
}

/// What lets a player page start sound with no click behind it. Chromium takes it only as a
/// launch argument, so player pages get a browser process of their own.
const AUTOPLAY: &str = "--autoplay-policy=no-user-gesture-required";

/// What a player page's scripts call to reach the app.
const POST: &str = "window.soniclyPost=(text)=>window.chrome.webview.postMessage(String(text));";

/// Whoever listens to a player page.
type Listener = Rc<RefCell<Option<Box<dyn Fn(String)>>>>;

thread_local! {
    /// Controllers are keyed by their host window so the Win32 callback can resize them.
    static CONTROLLERS: RefCell<HashMap<isize, ICoreWebView2Controller>> = RefCell::new(HashMap::new());
}

pub(crate) struct Window {
    hwnd: HWND,
    browser: Rc<RefCell<Option<Browser>>>,
    fetch: Rc<RefCell<Fetch>>,
    listener: Listener,
    /// Whether the page is to be silent, which a page that is still opening starts as.
    muted: Rc<Cell<bool>>,
}

/// What the asynchronous setup carries from `open` to the finished controller.
#[derive(Clone)]
struct Setup {
    target: Target,
    browser: Weak<RefCell<Option<Browser>>>,
    listener: Listener,
    muted: Rc<Cell<bool>>,
}

/// COM objects that must stay alive for as long as the sign-in window is usable.
struct Browser {
    controller: ICoreWebView2Controller,
    view: ICoreWebView2,
    cookies: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CookieManager,
}

impl Window {
    pub(crate) fn open(target: &Target) -> Result<Self> {
        let hwnd = create_window(&target.title)?;
        let browser = Rc::new(RefCell::new(None));
        let listener = Listener::default();
        let muted = Rc::new(Cell::new(false));
        let setup = Setup {
            target: target.clone(),
            browser: Rc::downgrade(&browser),
            listener: listener.clone(),
            muted: muted.clone(),
        };
        if let Err(error) = begin_environment(hwnd, setup) {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(error);
        }
        // A scripted window is never looked at. WebView2 renders into a host that was never
        // shown all the same, so it stays hidden.
        if !target.scripted() {
            let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };
        }

        Ok(Self {
            hwnd,
            browser,
            fetch: Rc::new(RefCell::new(Fetch::Idle)),
            listener,
            muted,
        })
    }

    /// Hands every message a player page posts to `heard`, on this thread.
    pub(crate) fn listen(&self, heard: Box<dyn Fn(String)>) {
        *self.listener.borrow_mut() = Some(heard);
    }

    /// Silences the page at the engine or lets it sound. A page still opening starts that way.
    pub(crate) fn mute(&self, muted: bool) {
        self.muted.set(muted);
        if let Some(browser) = self.browser.borrow().as_ref() {
            apply_mute(&browser.view, muted);
        }
    }

    /// True once the user closed and destroyed the host window.
    pub(crate) fn closed(&self) -> bool {
        !unsafe { IsWindow(Some(self.hwnd)) }.as_bool()
    }

    pub(crate) fn host(&self) -> Option<String> {
        let view = self.browser.borrow().as_ref()?.view.clone();
        let mut source = PWSTR::null();
        unsafe { view.Source(&mut source) }.ok()?;
        let source = CoTaskMemPWSTR::from(source).to_string();
        crate::native::host(&source)
    }

    /// Hands back a finished cookie fetch, or starts one when none is in flight.
    pub(crate) fn fetch(&mut self) -> Option<Vec<Cookie>> {
        let reading = self.fetch.borrow_mut().take();
        match reading {
            Reading::Done(cookies) => return Some(cookies),
            Reading::Waiting => return None,
            Reading::Start => {}
        }
        // The controller may not have arrived yet; the claimed slot goes back if it has not.
        let Some(cookies) = self
            .browser
            .borrow()
            .as_ref()
            .map(|browser| browser.cookies.clone())
        else {
            *self.fetch.borrow_mut() = Fetch::Idle;
            return None;
        };
        let slot = self.fetch.clone();
        let handler = GetCookiesCompletedHandler::create(Box::new(move |result, found| {
            let cookies = match result
                .and_then(|()| found.ok_or_else(|| windows::core::Error::from(E_POINTER)))
            {
                Ok(found) => read_cookies(&found),
                Err(error) => {
                    log::warn!("webview: cannot read sign-in cookies: {error}");
                    *slot.borrow_mut() = Fetch::Idle;
                    return Ok(());
                }
            };
            match cookies {
                Ok(cookies) => *slot.borrow_mut() = Fetch::Done(cookies),
                Err(error) => {
                    log::warn!("webview: cannot decode sign-in cookies: {error}");
                    *slot.borrow_mut() = Fetch::Idle;
                }
            }
            Ok(())
        }));
        if let Err(error) = unsafe { cookies.GetCookies(w!(""), &handler) } {
            log::warn!("webview: cannot start sign-in cookie fetch: {error}");
            *self.fetch.borrow_mut() = Fetch::Idle;
        }
        None
    }

    /// Navigates the view to `url`. Nothing happens before the controller has arrived.
    pub(crate) fn load(&self, url: &str) {
        let Some(view) = self
            .browser
            .borrow()
            .as_ref()
            .map(|browser| browser.view.clone())
        else {
            return;
        };
        let url = wide(url);
        if let Err(error) = unsafe { view.Navigate(PCWSTR(url.as_ptr())) } {
            log::warn!("webview: cannot load the sign-in url again: {error}");
        }
    }

    /// Runs `script` in the page, in the world the document-created script runs in. Nothing
    /// happens before the controller has arrived.
    pub(crate) fn run(&self, script: &str) {
        let Some(view) = self
            .browser
            .borrow()
            .as_ref()
            .map(|browser| browser.view.clone())
        else {
            return;
        };
        let script = wide(script);
        let handler = ExecuteScriptCompletedHandler::create(Box::new(|result, _value| {
            if let Err(error) = result {
                log::warn!("webview: the page refused a script: {error}");
            }
            Ok(())
        }));
        if let Err(error) = unsafe { view.ExecuteScript(PCWSTR(script.as_ptr()), &handler) } {
            log::warn!("webview: cannot run a script in the page: {error}");
        }
    }

    pub(crate) fn close(&self) {
        if let Some(browser) = self.browser.borrow_mut().take() {
            let _ = unsafe { browser.controller.Close() };
        }
        if !self.closed() {
            let _ = unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.close();
    }
}

/// Starts WebView2 without pumping a nested message loop inside GPUI's click handler.
fn begin_environment(hwnd: HWND, setup: Setup) -> Result<()> {
    let player = setup.target.player;
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
        move |result, environment| {
            let result = result
                .context("WebView2 could not create an environment")
                .and_then(|()| environment.context("WebView2 returned no environment"))
                .and_then(|environment| begin_controller(environment, hwnd, setup));
            if let Err(error) = result {
                fail(hwnd, error);
            }
            Ok(())
        },
    ));
    let folder = user_data_folder(player)?;
    // Every environment over one folder has to ask for the same arguments, so only the player
    // pages' folder is started with the autoplay allowance.
    let options = player.then(|| {
        let options = CoreWebView2EnvironmentOptions::default();
        unsafe { options.set_additional_browser_arguments(AUTOPLAY.to_owned()) };
        ICoreWebView2EnvironmentOptions::from(options)
    });
    unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            PCWSTR::null(),
            PCWSTR(folder.as_ptr()),
            options.as_ref(),
            &handler,
        )
    }
    .context("cannot start the WebView2 environment")
}

/// Sonicly's own cache folder for WebView2's browser state, as a wide string. WebView2 otherwise
/// writes beside the executable, which an installed copy under Program Files cannot do. Player
/// pages keep theirs apart, since their browser process runs with other arguments.
fn user_data_folder(player: bool) -> Result<Vec<u16>> {
    let folder = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("sonicly")
        .join(match player {
            true => "webview2-player",
            false => "webview2",
        });
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("cannot create {}", folder.display()))?;
    Ok(wide(&folder.to_string_lossy()))
}

/// Gives each controller its own InPrivate profile so sign-in and background pages never
/// share cookies, even while their browser processes overlap.
fn begin_controller(
    environment: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment,
    hwnd: HWND,
    setup: Setup,
) -> Result<()> {
    let environment = environment
        .cast::<ICoreWebView2Environment10>()
        .context("the installed WebView2 runtime does not support private profiles")?;
    let options = unsafe { environment.CreateCoreWebView2ControllerOptions() }
        .context("cannot configure the private sign-in profile")?;
    let profile = GUID::new().context("cannot identify the private sign-in profile")?;
    let profile = wide(&format!("Sonicly-{profile:?}"));
    unsafe { options.SetProfileName(PCWSTR(profile.as_ptr())) }
        .context("cannot name the sign-in profile")?;
    unsafe { options.SetIsInPrivateModeEnabled(true) }
        .context("cannot make the sign-in profile private")?;

    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
        move |result, controller| {
            let result = result
                .context("WebView2 could not create a controller")
                .and_then(|()| controller.context("WebView2 returned no controller"))
                .and_then(|controller| finish_controller(controller, hwnd, &setup));
            if let Err(error) = result {
                fail(hwnd, error);
            }
            Ok(())
        },
    ));
    unsafe { environment.CreateCoreWebView2ControllerWithOptions(hwnd, &options, &handler) }
        .context("cannot start the private WebView2 controller")
}

/// Attaches a completed controller to the live window and starts navigation.
fn finish_controller(controller: ICoreWebView2Controller, hwnd: HWND, setup: &Setup) -> Result<()> {
    let target = &setup.target;
    let script = target.script.as_deref();
    let Some(browser) = setup.browser.upgrade() else {
        let _ = unsafe { controller.Close() };
        return Ok(());
    };
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        let _ = unsafe { controller.Close() };
        return Ok(());
    }

    let view = unsafe { controller.CoreWebView2() }.context("cannot open the sign-in webview")?;
    // A scripted page keeps the runtime's own agent. Google's attestation checks the agent
    // against the engine, and a rewritten one gets tokens the stream host refuses.
    if script.is_none() {
        let settings = unsafe { view.Settings() }.context("cannot read webview settings")?;
        let settings = settings
            .cast::<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings2>()
            .context("the installed WebView2 runtime cannot set a user agent")?;
        let user_agent = browser_user_agent(&settings)?;
        unsafe { settings.SetUserAgent(PCWSTR(user_agent.as_ptr())) }
            .context("cannot set the sign-in user agent")?;
    }
    let view2 = view
        .cast::<ICoreWebView2_2>()
        .context("the installed WebView2 runtime cannot read cookies")?;
    let cookies = unsafe { view2.CookieManager() }.context("cannot open the cookie store")?;

    for (name, value) in &target.cookies {
        set_cookie(&cookies, name, value, &target.domain)?;
    }
    match (script, target.player) {
        (Some(script), true) => {
            apply_mute(&view, setup.muted.get());
            hear(&view, &target.landing, setup.listener.clone())?;
            inject(&view, &format!("{POST}{script}"))?;
        }
        (Some(script), false) => inject(&view, script)?,
        (None, _) => {}
    }
    resize(controller.clone(), hwnd);
    unsafe { controller.SetIsVisible(true) }.context("cannot show the sign-in webview")?;
    let url = wide(&target.url);
    unsafe { view.Navigate(PCWSTR(url.as_ptr())) }.context("cannot load the sign-in url")?;
    CONTROLLERS.with(|controllers| {
        controllers
            .borrow_mut()
            .insert(key(hwnd), controller.clone());
    });
    *browser.borrow_mut() = Some(Browser {
        controller,
        view,
        cookies,
    });
    Ok(())
}

/// Puts one cookie into the view's session for `domain` and everything under it, sent over
/// https alone. The session is new and private to this window, so nothing is overwritten.
fn set_cookie(
    cookies: &ICoreWebView2CookieManager,
    name: &str,
    value: &str,
    domain: &str,
) -> Result<()> {
    let (name, value, domain) = (wide(name), wide(value), wide(&format!(".{domain}")));
    let cookie = unsafe {
        cookies.CreateCookie(
            PCWSTR(name.as_ptr()),
            PCWSTR(value.as_ptr()),
            PCWSTR(domain.as_ptr()),
            w!("/"),
        )
    }
    .context("cannot make a session cookie")?;
    unsafe { cookie.SetIsSecure(true) }.context("cannot restrict a session cookie to https")?;
    unsafe { cookies.AddOrUpdateCookie(&cookie) }.context("cannot set a session cookie")
}

/// Hands what the top document of `host` posts to the listener. A page is free to post anything,
/// and a document of another site is not listened to at all.
fn hear(view: &ICoreWebView2, host: &str, listener: Listener) -> Result<()> {
    let host = host.to_owned();
    let handler = WebMessageReceivedEventHandler::create(Box::new(move |_view, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut source = PWSTR::null();
        unsafe { args.Source(&mut source) }?;
        let source = CoTaskMemPWSTR::from(source).to_string();
        if !source.starts_with("https://") || crate::native::host(&source).as_deref() != Some(&host)
        {
            return Ok(());
        }
        let mut message = PWSTR::null();
        // a message that is not a string is not one of ours
        if unsafe { args.TryGetWebMessageAsString(&mut message) }.is_err() {
            return Ok(());
        }
        let message = CoTaskMemPWSTR::from(message).to_string();
        if let Some(heard) = listener.borrow().as_ref() {
            heard(message);
        }
        Ok(())
    }));
    let mut token = 0;
    unsafe { view.add_WebMessageReceived(&handler, &mut token) }
        .context("cannot listen to the player page")
}

fn apply_mute(view: &ICoreWebView2, muted: bool) {
    let applied = view
        .cast::<ICoreWebView2_8>()
        .and_then(|view| unsafe { view.SetIsMuted(muted) });
    if let Err(error) = applied {
        log::warn!("webview: cannot mute the player page: {error}");
    }
}

/// Adds a script to run in every page the view loads, before the page's own scripts. WebView2
/// registers it asynchronously, and the navigation that follows waits on the same message loop,
/// so the script is in place by the time the first page parses.
fn inject(view: &ICoreWebView2, script: &str) -> Result<()> {
    let script = wide(script);
    let handler =
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|result, _id| {
            if let Err(error) = result {
                log::warn!("webview: the page script was refused: {error}");
            }
            Ok(())
        }));
    unsafe { view.AddScriptToExecuteOnDocumentCreated(PCWSTR(script.as_ptr()), &handler) }
        .context("cannot add the page script")
}

/// Closes an unusable host after an asynchronous setup failure.
fn fail(hwnd: HWND, error: anyhow::Error) {
    log::error!("webview: cannot prepare the sign-in window: {error:#}");
    let _ = unsafe { DestroyWindow(hwnd) };
}

/// Registers and creates the standalone Win32 host window with a centered client area.
fn create_window(title: &str) -> Result<HWND> {
    let instance = unsafe { GetModuleHandleW(None) }.context("cannot find the app module")?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: HINSTANCE(instance.0),
        lpszClassName: w!("SoniclySignInWebView"),
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };

    let mut bounds = RECT {
        left: 0,
        top: 0,
        right: WIDTH,
        bottom: HEIGHT,
    };
    unsafe { AdjustWindowRectEx(&mut bounds, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0)) }
        .context("cannot size the sign-in window")?;
    let width = bounds.right - bounds.left;
    let height = bounds.bottom - bounds.top;
    let x = (unsafe { GetSystemMetrics(SM_CXSCREEN) } - width) / 2;
    let y = (unsafe { GetSystemMetrics(SM_CYSCREEN) } - height) / 2;
    let title = wide(title);
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("SoniclySignInWebView"),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )
    }
    .context("cannot create the sign-in window")?;
    Ok(hwnd)
}

/// Copies the WebView2 cookie list before its completion callback returns.
fn read_cookies(
    found: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CookieList,
) -> Result<Vec<Cookie>> {
    let mut count = 0;
    unsafe { found.Count(&mut count) }.context("cannot count cookies")?;
    (0..count)
        .map(|index| {
            let cookie = unsafe { found.GetValueAtIndex(index) }.context("cannot read a cookie")?;
            Ok(Cookie {
                name: cookie_string(|value| unsafe { cookie.Name(value) })?,
                value: cookie_string(|value| unsafe { cookie.Value(value) })?,
                domain: cookie_string(|value| unsafe { cookie.Domain(value) })?,
            })
        })
        .collect()
}

/// Takes one COM-allocated string property from a WebView2 cookie.
fn cookie_string(read: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> Result<String> {
    let mut value = PWSTR::null();
    read(&mut value).context("cannot read a cookie property")?;
    Ok(CoTaskMemPWSTR::from(value).to_string())
}

/// Keeps the runtime's current Chromium version while removing the Edge token Google uses to
/// reject an otherwise capable embedded browser.
fn browser_user_agent(
    settings: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings2,
) -> Result<Vec<u16>> {
    let mut value = PWSTR::null();
    unsafe { settings.UserAgent(&mut value) }.context("cannot read the WebView2 user agent")?;
    let value = CoTaskMemPWSTR::from(value).to_string();
    let value = value
        .split_whitespace()
        .filter(|part| !part.starts_with("Edg/"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(wide(&value))
}

fn resize(controller: ICoreWebView2Controller, hwnd: HWND) {
    let mut bounds = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut bounds) }.is_ok() {
        let _ = unsafe { controller.SetBounds(bounds) };
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn key(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_SIZE => {
            CONTROLLERS.with(|controllers| {
                if let Some(controller) = controllers.borrow().get(&key(hwnd)) {
                    resize(controller.clone(), hwnd);
                }
            });
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let limits = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            limits.ptMinTrackSize.x = MIN_WIDTH;
            limits.ptMinTrackSize.y = MIN_HEIGHT;
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_NCDESTROY => {
            CONTROLLERS.with(|controllers| {
                controllers.borrow_mut().remove(&key(hwnd));
            });
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
