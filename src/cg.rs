use crate::{Rect, SoftBufferError};
use core_graphics::base::{
    kCGBitmapByteOrder32Little, kCGImageAlphaNoneSkipFirst, kCGRenderingIntentDefault,
};
use core_graphics::color_space::CGColorSpace;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::image::CGImage;
#[cfg(target_os = "macos")]
use raw_window_handle::AppKitWindowHandle;
#[cfg(target_os = "ios")]
use raw_window_handle::UiKitWindowHandle;

#[cfg(target_os = "macos")]
use cocoa::appkit::{NSView, NSViewHeightSizable, NSViewWidthSizable, NSWindow};
#[cfg(target_os = "ios")]
use cocoa::uikit::{UIView, UIWindow};
use cocoa::base::{id, nil};
use cocoa::quartzcore::{transaction, CALayer, ContentsGravity};
use foreign_types::ForeignType;

use std::num::NonZeroU32;
use std::sync::Arc;

struct Buffer(Vec<u32>);

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        bytemuck::cast_slice(&self.0)
    }
}

pub struct CGImpl {
    layer: CALayer,
    window: id,
    color_space: CGColorSpace,
    size: Option<(NonZeroU32, NonZeroU32)>,
}

impl CGImpl {
    #[cfg(target_os = "ios")] pub unsafe fn new(handle: AppKitWindowHandle) -> Result<Self, SoftBufferError> {
        let window = handle.ui_window as id;
        let window: id = msg_send![window, retain];
        let view = handle.ui_view as id;
        let layer = CALayer::new();
        /*
        unsafe {
            let subview: id = UIView::alloc(nil).initWithFrame_(UIView::frame(view));
            layer.set_contents_gravity(ContentsGravity::TopLeft);
            layer.set_needs_display_on_bounds_change(false);
            subview.setLayer(layer.id());
            //subview.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);

            view.addSubview_(subview); // retains subview (+1) = 2
            let _: () = msg_send![subview, release]; // releases subview (-1) = 1
        }
        */
        let color_space = CGColorSpace::create_device_rgb();
        Ok(Self {
            layer,
            window,
            color_space,
            size: None,
        })
    }
    
    #[cfg(target_os = "macos")] pub unsafe fn new(handle: AppKitWindowHandle) -> Result<Self, SoftBufferError> {
        let window = handle.ns_window as id;
        let window: id = msg_send![window, retain];
        let view = handle.ns_view as id;
        let layer = CALayer::new();
        unsafe {
            let subview: id = NSView::alloc(nil).initWithFrame_(NSView::frame(view));
            layer.set_contents_gravity(ContentsGravity::TopLeft);
            layer.set_needs_display_on_bounds_change(false);
            subview.setLayer(layer.id());
            subview.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);

            view.addSubview_(subview); // retains subview (+1) = 2
            let _: () = msg_send![subview, release]; // releases subview (-1) = 1
        }
        let color_space = CGColorSpace::create_device_rgb();
        Ok(Self {
            layer,
            window,
            color_space,
            size: None,
        })
    }

    pub fn resize(&mut self, width: NonZeroU32, height: NonZeroU32) -> Result<(), SoftBufferError> {
        self.size = Some((width, height));
        Ok(())
    }

    pub fn buffer_mut(&mut self) -> Result<BufferImpl, SoftBufferError> {
        let (width, height) = self
            .size
            .expect("Must set size of surface before calling `buffer_mut()`");
        Ok(BufferImpl {
            buffer: vec![0; width.get() as usize * height.get() as usize],
            imp: self,
        })
    }

    /// Fetch the buffer from the window.
    pub fn fetch(&mut self) -> Result<Vec<u32>, SoftBufferError> {
        Err(SoftBufferError::Unimplemented)
    }
}

pub struct BufferImpl<'a> {
    imp: &'a mut CGImpl,
    buffer: Vec<u32>,
}

impl<'a> BufferImpl<'a> {
    #[inline]
    pub fn pixels(&self) -> &[u32] {
        &self.buffer
    }

    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.buffer
    }

    pub fn age(&self) -> u8 {
        0
    }

    pub fn present(self) -> Result<(), SoftBufferError> {
        let data_provider = CGDataProvider::from_buffer(Arc::new(Buffer(self.buffer)));
        let (width, height) = self.imp.size.unwrap();
        let image = CGImage::new(
            width.get() as usize,
            height.get() as usize,
            8,
            32,
            (width.get() * 4) as usize,
            &self.imp.color_space,
            kCGBitmapByteOrder32Little | kCGImageAlphaNoneSkipFirst,
            &data_provider,
            false,
            kCGRenderingIntentDefault,
        );

        // The CALayer has a default action associated with a change in the layer contents, causing
        // a quarter second fade transition to happen every time a new buffer is applied. This can
        // be mitigated by wrapping the operation in a transaction and disabling all actions.
        transaction::begin();
        transaction::set_disable_actions(true);

        #[cfg(target_os = "macos")] unsafe {
            self.imp
                .layer
                .set_contents_scale(self.imp.window.backingScaleFactor());
            self.imp.layer.set_contents(image.as_ptr() as id);
        };

        transaction::commit();

        Ok(())
    }

    pub fn present_with_damage(self, _damage: &[Rect]) -> Result<(), SoftBufferError> {
        self.present()
    }
}

impl Drop for CGImpl {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.window, release];
        }
    }
}
