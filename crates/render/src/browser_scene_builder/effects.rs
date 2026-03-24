use havi_types::fragment_tree::BoxFragment;
use makepad_browser_scene::{MpBlendMode, MpEffectNode, MpFilter, MpIsolation};
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::values::computed::basic_shape::ClipPath;
use style::values::computed::effects::Filter as ComputedFilter;

pub(super) fn lower_box_effect_node(
    bf: &BoxFragment,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
) -> Result<Option<MpEffectNode>, String> {
    let style = bf.style();
    let effects = style.get_effects();
    let svg = style.get_svg();
    if svg.clip_path != ClipPath::None {
        return Err("direct browser-scene builder does not lower clip-path masks yet".to_string());
    }
    let opacity = effects.opacity;
    let filters = lower_box_effect_filters(&effects.filter.0)?;
    let blend_mode = if effects.mix_blend_mode == ComputedMixBlendMode::Normal {
        MpBlendMode::Normal
    } else {
        MpBlendMode::Named(format!("{:?}", effects.mix_blend_mode))
    };
    let isolated = opacity != 1.0 || !filters.is_empty() || !matches!(blend_mode, MpBlendMode::Normal);
    if !isolated {
        return Ok(None);
    }
    Ok(Some(MpEffectNode {
        spatial_id,
        clip_chain_id,
        opacity,
        filters,
        blend_mode,
        isolation: MpIsolation::Isolate,
        mask: None,
    }))
}

pub(super) fn lower_box_effect_filters(filters: &[ComputedFilter]) -> Result<Vec<MpFilter>, String> {
    let mut lowered = Vec::new();
    for filter in filters {
        match filter {
            ComputedFilter::Blur(radius) => lowered.push(MpFilter::Blur(radius.0.px().max(0.0))),
            ComputedFilter::Opacity(opacity) => {
                lowered.push(MpFilter::Opacity(opacity.0.clamp(0.0, 1.0)))
            }
            other => {
                return Err(format!(
                    "direct browser-scene builder does not lower filter yet: {other:?}"
                ))
            }
        }
    }
    Ok(lowered)
}
