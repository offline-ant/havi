//! Legacy paint traversal entry point for lowering `RenderScene` into `MpScene`.
//!
//! This path remains only for `HAVI_DISABLE_BROWSER_SCENE=1` and direct-builder
//! fallback failures.

use makepad_widgets::*;

use crate::mp_scene_lowering;
use crate::{
    DrawBoxShadow, DrawGradient, DrawRoundedColor, DrawVideoYuv,
    FrameDrawListState, SelectionHighlight, TextureCache,
};

pub(crate) struct MakepadDrawState<'a> {
    pub draw_bg: &'a mut DrawColor,
    pub draw_text: &'a mut DrawText,
    pub draw_text_bold: &'a mut DrawText,
    pub draw_text_mono: &'a mut DrawText,
    pub draw_image: &'a mut DrawImage,
    pub texture_cache: &'a mut TextureCache,
    pub draw_rounded_bg: &'a mut DrawRoundedColor,
    pub draw_box_shadow: &'a mut DrawBoxShadow,
    pub draw_gradient: &'a mut DrawGradient,
    pub draw_video_yuv: &'a mut DrawVideoYuv,
    pub selection: Option<&'a SelectionHighlight>,
    pub frame_draw_lists: &'a mut FrameDrawListState,
    pub image_overrides: &'a havi_types::ImageOverrides,
}

pub(crate) fn paint_scene(
    cx: &mut Cx2d,
    scene: &crate::scene::RenderScene<'_>,
    webview_origin: DVec2,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
) {
    mp_scene_lowering::draw_render_scene(
        cx,
        scene,
        webview_origin,
        root_viewport_size,
        state,
    );
    crate::makepad_fragments::paint_selection_overlay(cx, state);
}
