use makepad_compositor::MpTransformStyle;
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::computed_values::transform_style::T as ComputedTransformStyle;
use style::properties::ComputedValues;
use style::values::computed::basic_shape::ClipPath;

pub(crate) fn compute_used_transform_style(style: &ComputedValues) -> MpTransformStyle {
    if style.get_box().transform_style != ComputedTransformStyle::Preserve3d {
        return MpTransformStyle::Flat;
    }
    if grouping_properties_flatten(style) {
        return MpTransformStyle::Flat;
    }
    MpTransformStyle::Preserve3D
}

pub(crate) fn grouping_properties_flatten(style: &ComputedValues) -> bool {
    let effects = style.get_effects();
    let overflow = style.get_box();
    effects.opacity != 1.0
        || !effects.filter.0.is_empty()
        || effects.mix_blend_mode != ComputedMixBlendMode::Normal
        || style.get_svg().clip_path != ClipPath::None
        || !matches!(overflow.overflow_x, ComputedOverflow::Visible)
        || !matches!(overflow.overflow_y, ComputedOverflow::Visible)
}
