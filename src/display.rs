//! Linux only: which display system the window uses.
//!
//! The window library (winit 0.30) receives dropped files on X11 but not on Wayland,
//! the default on recent Ubuntu. Wayland desktops also offer X11 (XWayland), so
//! TextCraft uses that when everything it needs is there, and stays on Wayland
//! otherwise: it must never fail to start because of this.
//! TEXTCRAFT_WAYLAND=1 keeps Wayland (sharper with fractional scaling, no file drops).

/// What was found on this computer.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy)]
pub struct Found {
    pub wayland: bool,
    pub x11_answers: bool,
    pub x11_libraries: bool,
    pub keep_wayland: bool,
}

/// Whether to ask for X11 instead of the library's own choice.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn prefer_x11(f: Found) -> bool {
    f.wayland && f.x11_answers && f.x11_libraries && !f.keep_wayland
}

/// ":0" / ":1.0" → the local X11 socket number (None for a remote display).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn local_display_number(display: &str) -> Option<u32> {
    let rest = display.strip_prefix("unix").unwrap_or(display).strip_prefix(':')?;
    rest.split('.').next()?.parse().ok()
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::sync::OnceLock;

    static BACKEND: OnceLock<&'static str> = OnceLock::new();

    fn env_set(name: &str) -> bool {
        std::env::var_os(name).is_some_and(|v| !v.is_empty())
    }

    /// The libraries winit opens when it starts on X11; without one of them it can't start.
    fn x11_libraries() -> bool {
        ["libX11.so.6", "libX11-xcb.so.1", "libXcursor.so.1", "libXi.so.6", "libxkbcommon.so.0", "libxkbcommon-x11.so.0"]
            .iter()
            // SAFETY: only opens well-known system libraries, as winit does itself right after.
            .all(|l| unsafe { libloading::Library::new(l) }.is_ok())
    }

    fn x11_answers() -> bool {
        std::env::var("DISPLAY")
            .ok()
            .and_then(|d| local_display_number(&d))
            .is_some_and(|n| std::os::unix::net::UnixStream::connect(format!("/tmp/.X11-unix/X{n}")).is_ok())
    }

    /// Sets the display system before the window opens.
    pub fn choose(options: &mut eframe::NativeOptions) {
        let found = Found {
            wayland: env_set("WAYLAND_DISPLAY") || env_set("WAYLAND_SOCKET"),
            x11_answers: x11_answers(),
            x11_libraries: x11_libraries(),
            keep_wayland: env_set("TEXTCRAFT_WAYLAND"),
        };
        if prefer_x11(found) {
            options.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::x11::EventLoopBuilderExtX11;
                b.with_x11();
            }));
        }
        let _ = BACKEND.set(match (found.wayland, prefer_x11(found)) {
            (true, true) => "X11 (via Wayland)",
            (true, false) if !found.x11_libraries => "Wayland, sans glisser-déposer (il manque libxkbcommon-x11-0)",
            (true, false) => "Wayland, sans glisser-déposer",
            (false, _) => "X11",
        });
    }

    pub fn backend() -> Option<&'static str> {
        BACKEND.get().copied()
    }
}

#[cfg(target_os = "linux")]
pub use linux::{backend, choose};

#[cfg(not(target_os = "linux"))]
pub fn backend() -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_is_used_only_when_it_will_really_start() {
        let all = Found { wayland: true, x11_answers: true, x11_libraries: true, keep_wayland: false };
        assert!(prefer_x11(all));
        assert!(!prefer_x11(Found { x11_answers: false, ..all }));
        assert!(!prefer_x11(Found { x11_libraries: false, ..all }));
        assert!(!prefer_x11(Found { keep_wayland: true, ..all }));
        assert!(!prefer_x11(Found { wayland: false, ..all }), "already X11: nothing to force");
    }

    #[test]
    fn display_names_are_understood() {
        assert_eq!(local_display_number(":0"), Some(0));
        assert_eq!(local_display_number(":1.0"), Some(1));
        assert_eq!(local_display_number("unix:2"), Some(2));
        assert_eq!(local_display_number("localhost:10.0"), None);
        assert_eq!(local_display_number(""), None);
    }
}
