use crate::{error::unwrap, GraphicsContextImpl, SoftBufferError};
use raw_window_handle::{HasRawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use smithay_client_toolkit::shm::AutoMemPool;
use std::ops::Deref;
use std::{
    fs::File,
    io::Write,
    os::unix::prelude::{AsRawFd, FileExt},
};
use bytemuck::cast_slice;
use wayland_client::{
    protocol::{wl_buffer::WlBuffer, wl_shm::WlShm, wl_surface::WlSurface},
    sys::client::wl_display,
    Attached, Display, EventQueue, GlobalManager, Main, Proxy,
};

pub struct WaylandImpl {
    //event_queue: EventQueue,
    pool: AutoMemPool,
    surface: WlSurface,
}

impl WaylandImpl {
    pub unsafe fn new<W: HasRawWindowHandle>(
        window_handle: WaylandWindowHandle,
        display_handle: WaylandDisplayHandle,
    ) -> Result<Self, SoftBufferError<W>> {
        let display = Display::from_external_display(display_handle.display as *mut wl_display);
        let surface: WlSurface = Proxy::from_c_ptr(window_handle.surface as _).into();
        Self::new_safe(display, surface)
    }

    fn new_safe<W: HasRawWindowHandle>(display: Display, surface: WlSurface) -> Result<Self, SoftBufferError<W>>{
        let mut event_queue = display.create_event_queue();
        let attached_display = display.attach(event_queue.token());
        let globals = GlobalManager::new(&attached_display);
        unwrap(
            event_queue.sync_roundtrip(&mut (), |_, _, _| unreachable!()),
            "Failed ot make round trip to Wayland server.",
        )?;
        let shm: Main<WlShm> = unwrap(
            globals.instantiate_exact::<WlShm>(1),
            "Failed to instantiate Wayland shm.",
        )?;
        let shm_attached: Attached<WlShm> = shm.into();
        let pool = unwrap(
            AutoMemPool::new(shm_attached),
            "Failed to create AutoMemPool from smithay client toolkit",
        )?;

        Ok(Self { pool, surface })
    }

    fn set_buffer_safe(&mut self, buffer: &[u32], width: u16, height: u16){
        let (canvas, new_buffer) = self
            .pool
            .buffer(
                width as i32,
                height as i32,
                4 * (width as i32),
                wayland_client::protocol::wl_shm::Format::Xrgb8888,
            )
            .unwrap();

        assert_eq!(canvas.len(), buffer.len()*4);
        canvas.copy_from_slice(cast_slice(buffer));

        self.surface.attach(Some(&new_buffer), 0, 0);

        if self.surface.as_ref().version() >= 4 {
            // If our server is recent enough and supports at least version 4 of the
            // wl_surface interface, we can specify the damage in buffer coordinates.
            // This is obviously the best and do that if possible.
            self.surface.damage_buffer(0, 0, width as i32, width as i32);
        } else {
            // Otherwise, we fallback to compatibility mode. Here we specify damage
            // in surface coordinates, which would have been different if we had drawn
            // our buffer at HiDPI resolution. We didn't though, so it is ok.
            // Using `damage_buffer` in general is better though.
            self.surface.damage(0, 0, width as i32, width as i32);
        }

        self.surface.commit();
        //self.event_queue.dispatch(&mut next_action, |_, _, _| {}).unwrap();
    }

}

impl GraphicsContextImpl for WaylandImpl {

    unsafe fn set_buffer(&mut self, buffer: &[u32], width: u16, height: u16) {
        self.set_buffer_safe(buffer, width, height);
    }

}
