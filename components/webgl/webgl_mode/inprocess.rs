/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use canvas_traits::webgl::{WebGLContextId, WebGLMsg, WebGLThreads, webgl_channel};
use log::debug;
use paint_api::{CrossProcessPaintApi, PainterGlDetailsMap, WebRenderExternalImageIdManager};
use paint_api::gl_device::swap_chain::SwapChains;
#[cfg(feature = "webxr")]
use webxr::SurfmanGL as WebXRSurfman;
#[cfg(feature = "webxr")]
use webxr_api::LayerGrandManager as WebXRLayerGrandManager;

use crate::webgl_thread::{WebGLContextBusyMap, WebGLThread, WebGLThreadInit};

pub struct WebGLComm {
    pub webgl_threads: WebGLThreads,
    pub swap_chains: SwapChains<WebGLContextId>,
    pub busy_webgl_context_map: WebGLContextBusyMap,
    #[cfg(feature = "webxr")]
    pub webxr_layer_grand_manager: WebXRLayerGrandManager<WebXRSurfman>,
}

impl WebGLComm {
    /// Creates a new `WebGLComm` object.
    pub fn new(
        paint_api: CrossProcessPaintApi,
        external_image_id_manager: WebRenderExternalImageIdManager,
        painter_gl_details_map: PainterGlDetailsMap,
    ) -> WebGLComm {
        debug!("WebGLThreads::new()");
        let (sender, receiver) = webgl_channel::<WebGLMsg>().unwrap();
        let swap_chains = SwapChains::new();
        let busy_webgl_context_map = WebGLContextBusyMap::default();

        #[cfg(feature = "webxr")]
        let webxr_init = crate::webxr::WebXRBridgeInit::new(sender.clone());
        #[cfg(feature = "webxr")]
        let webxr_layer_grand_manager = webxr_init.layer_grand_manager();

        // This implementation creates a single `WebGLThread` for all the pipelines.
        let init = WebGLThreadInit {
            paint_api,
            external_image_id_manager,
            sender: sender.clone(),
            receiver,
            webrender_swap_chains: swap_chains.clone(),
            painter_gl_details_map,
            busy_webgl_context_map: busy_webgl_context_map.clone(),
            #[cfg(feature = "webxr")]
            webxr_init,
        };

        WebGLThread::run_on_own_thread(init);

        WebGLComm {
            webgl_threads: WebGLThreads(sender),
            swap_chains,
            busy_webgl_context_map,
            #[cfg(feature = "webxr")]
            webxr_layer_grand_manager,
        }
    }
}
