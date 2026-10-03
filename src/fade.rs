//! The menu's whole-surface opacity, through `wp_alpha_modifier_v1`.
//!
//! The compositor multiplies everything the surface draws — card, shadow,
//! text — by one value, so a fade costs no redraw at all: the content is
//! drawn once and the multiplier is stepped per frame. cosmic-comp supports
//! the protocol, but libcosmic only uses it for its own embedded surfaces and
//! offers no way to reach a layer surface's, so this module opens a second,
//! guest connection on libcosmic's own `wl_display` and drives the surface
//! from there.
//!
//! If any of that is unavailable — no protocol, no handle — the caller shows
//! the menu at once, exactly as before the fade existed. Nothing here may
//! ever be the reason the menu does not appear.
//!
//! # Invariants the unsafe code relies on
//!
//! 1. **The display outlives the guest connection.** The `wl_display` comes
//!    from libcosmic's window handle and lives as long as the event loop,
//!    which is the life of this resident process. A guest backend never
//!    closes the display it borrows.
//! 2. **The surface pointer is libcosmic's live `wl_surface` proxy**, taken
//!    from the window handle inside the same update that attaches it. That
//!    proxy was created through `wayland-client` — the tree has exactly one
//!    `wayland-backend` — so `ObjectId::from_ptr` recognises it as
//!    Rust-managed and shares its liveness flag: once libcosmic destroys the
//!    surface, [`WlSurface::is_alive`] turns false here too, and no request
//!    is ever sent through a dangling pointer.
//! 3. **The alpha object goes before the surface.** The protocol makes every
//!    request on it, `destroy` included, a fatal `no_surface` error once its
//!    `wl_surface` is gone — fatal for the whole shared connection, so for
//!    libcosmic as well. [`Fader::detach`] therefore runs before the app asks
//!    for the layer surface to be destroyed, and when the surface has died
//!    first (the compositor closed it) the object is dropped without a word.
//! 4. **Requests only from the UI thread.** Committing applies whatever state
//!    libcosmic has pending, so a commit must never land between its attach
//!    and its own commit. libcosmic renders on the UI thread, and every
//!    method here that sends a request is called from `update`, on that same
//!    thread. The background thread only reads this queue's events.

use std::sync::{Mutex, OnceLock};

use tokio::sync::mpsc;
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::{
    delegate_noop,
    protocol::{wl_callback, wl_registry, wl_surface::WlSurface},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::wp::alpha_modifier::v1::client::{
    wp_alpha_modifier_surface_v1::WpAlphaModifierSurfaceV1, wp_alpha_modifier_v1::WpAlphaModifierV1,
};

/// The two pointers a fade needs, as plain addresses so they can travel in a
/// message. Only [`Fader`] turns them back into pointers.
#[derive(Debug, Clone, Copy)]
pub struct Handles {
    display: usize,
    surface: usize,
}

/// Read the display and surface pointers off a window libcosmic is showing.
/// Produces nothing at all if `id` is not one of its windows yet; the caller
/// has a deadline for that.
pub fn handles(id: cosmic::iced::window::Id) -> cosmic::iced::Task<Option<Handles>> {
    cosmic::iced::window::run(id, |window| {
        use cosmic::iced::window::raw_window_handle::{RawDisplayHandle, RawWindowHandle};
        let display = match window.display_handle().ok()?.as_raw() {
            RawDisplayHandle::Wayland(h) => h.display.as_ptr() as usize,
            _ => return None,
        };
        let surface = match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Wayland(h) => h.surface.as_ptr() as usize,
            _ => return None,
        };
        Some(Handles { display, surface })
    })
}

/// A frame callback's serial, from the compositor to the app.
type Frames = (
    mpsc::UnboundedSender<u64>,
    Mutex<Option<mpsc::UnboundedReceiver<u64>>>,
);

/// Made on first use by either end, so the app's subscription and the fader
/// can come up in any order.
fn frames() -> &'static Frames {
    static FRAMES: OnceLock<Frames> = OnceLock::new();
    FRAMES.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded_channel();
        (tx, Mutex::new(Some(rx)))
    })
}

/// The frame callbacks, for the app's subscription: each serial is "the
/// compositor has shown the last step, draw the next". `None` after the first
/// call, as iced may build the subscription more than once.
pub fn frame_callbacks() -> Option<mpsc::UnboundedReceiver<u64>> {
    frames().1.lock().ok()?.take()
}

/// The guest queue's state. Lives on the reading thread after setup.
struct State {
    globals: Vec<(u32, String)>,
    frames: mpsc::UnboundedSender<u64>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            state.globals.push((name, interface));
        }
    }
}

impl Dispatch<wl_callback::WlCallback, u64> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        serial: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            tracing::trace!(serial, "frame callback");
            let _ = state.frames.send(*serial);
        }
    }
}

delegate_noop!(State: ignore WpAlphaModifierV1);
delegate_noop!(State: ignore WpAlphaModifierSurfaceV1);

/// The surface being faded, and its alpha object.
struct Attached {
    surface: WlSurface,
    alpha: WpAlphaModifierSurfaceV1,
}

/// The guest connection, bound to the alpha modifier, and the surface it is
/// fading if any. One per process, made on the first open.
pub struct Fader {
    connection: Connection,
    queue: QueueHandle<State>,
    manager: WpAlphaModifierV1,
    attached: Option<Attached>,
}

impl std::fmt::Debug for Fader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fader")
            .field("attached", &self.attached.is_some())
            .finish()
    }
}

impl Fader {
    /// Join libcosmic's connection and bind the alpha modifier, or `None` when
    /// the compositor does not offer it.
    pub fn connect(handles: Handles) -> Option<Self> {
        // SAFETY: invariant 1 — libcosmic's live display, borrowed as a guest.
        let backend = unsafe { Backend::from_foreign_display(handles.display as *mut _) };
        let connection = Connection::from_backend(backend);
        let mut events = connection.new_event_queue::<State>();
        let queue = events.handle();
        let registry = connection.display().get_registry(&queue, ());
        let mut state = State {
            globals: Vec::new(),
            frames: frames().0.clone(),
        };
        // One round trip on the UI thread, before the reading thread exists,
        // to learn the globals.
        events.roundtrip(&mut state).ok()?;
        let name = state
            .globals
            .iter()
            .find(|(_, interface)| interface == WpAlphaModifierV1::interface().name)
            .map(|(name, _)| *name)?;
        let manager = registry.bind::<WpAlphaModifierV1, _, _>(name, 1, &queue, ());
        // From here on the queue only carries frame callbacks. libwayland's
        // prepare/read protocol lets this thread and libcosmic's loop read
        // the shared socket side by side, each dispatching its own queue.
        std::thread::Builder::new()
            .name("start-menu-fade".into())
            .spawn(move || while events.blocking_dispatch(&mut state).is_ok() {})
            .ok()?;
        Some(Self {
            connection,
            queue,
            manager,
            attached: None,
        })
    }

    /// Take over the opacity of the surface in `handles`. False if the pointer
    /// is not a `wl_surface`, in which case nothing was sent.
    pub fn attach(&mut self, handles: Handles) -> bool {
        // A surface may only have one alpha object.
        self.detach();
        // SAFETY: invariant 2 — the live wl_surface from the window handle,
        // read in this same update.
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), handles.surface as *mut _) };
        let Ok(surface) = id.and_then(|id| WlSurface::from_id(&self.connection, id)) else {
            return false;
        };
        let alpha = self.manager.get_surface(&surface, &self.queue, ());
        self.attached = Some(Attached { surface, alpha });
        true
    }

    pub fn attached(&self) -> bool {
        self.attached.is_some()
    }

    /// Show the surface at `opacity`, and ask for a frame callback carrying
    /// `serial` once the compositor has put it on screen.
    pub fn show(&mut self, opacity: f32, serial: u64) {
        let Some(a) = &self.attached else {
            return;
        };
        if !a.surface.is_alive() {
            // Invariant 3: the compositor took the surface first.
            self.forget();
            return;
        }
        a.alpha.set_multiplier(multiplier(opacity));
        a.surface.frame(&self.queue, serial);
        a.surface.commit();
        let _ = self.connection.flush();
    }

    /// Give the surface back at full opacity. Called before the layer surface
    /// is destroyed — invariant 3 — and on any instant close.
    pub fn detach(&mut self) {
        let Some(a) = self.attached.take() else {
            return;
        };
        if a.surface.is_alive() {
            a.alpha.destroy();
            let _ = self.connection.flush();
        }
    }

    /// Let go without sending anything: the surface is already gone, and
    /// every request on its alpha object would now be a protocol error.
    fn forget(&mut self) {
        self.attached = None;
    }
}

/// The protocol's fixed-point multiplier: 0 is invisible, `u32::MAX` opaque.
pub fn multiplier(opacity: f32) -> u32 {
    (f64::from(opacity.clamp(0.0, 1.0)) * f64::from(u32::MAX)).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_maps_onto_the_whole_fixed_point_range() {
        assert_eq!(multiplier(0.0), 0);
        assert_eq!(multiplier(1.0), u32::MAX);
        // Out of range clamps rather than wrapping round to transparent.
        assert_eq!(multiplier(1.5), u32::MAX);
        assert_eq!(multiplier(-0.2), 0);
        let half = multiplier(0.5);
        assert!(half.abs_diff(u32::MAX / 2) <= 1, "{half}");
    }
}
