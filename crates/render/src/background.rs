
use style::computed_values::background_clip::single_value::T as Clip;
use style::computed_values::background_origin::single_value::T as Origin;
use style::properties::ComputedValues;
use style::values::computed::background::BackgroundSize;
use style::values::computed::LengthPercentage;
use style::values::specified::background::{
    BackgroundRepeat as RepeatXY, BackgroundRepeatKeyword as Repeat,
};

#[derive(Clone, Copy)]
pub(crate) struct BorderRadii {
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
}

impl BorderRadii {
    pub fn max(&self) -> f32 {
        self.tl.max(self.tr).max(self.br).max(self.bl)
    }
}

pub(crate) fn resolve_border_radii(computed: &ComputedValues) -> BorderRadii {
    let border = computed.get_border();
    let resolve = |r: &style::values::computed::LengthPercentage| -> f32 {
        r.to_length().map_or(0.0, |l| l.px())
    };
    BorderRadii {
        tl: resolve(&border.border_top_left_radius.0.width.0),
        tr: resolve(&border.border_top_right_radius.0.width.0),
        br: resolve(&border.border_bottom_right_radius.0.width.0),
        bl: resolve(&border.border_bottom_left_radius.0.width.0),
    }
}

pub(crate) struct BackgroundLayerGeom {
    pub bounds_x: f64,
    pub bounds_y: f64,
    pub bounds_w: f32,
    pub bounds_h: f32,
    pub tile_w: f32,
    pub tile_h: f32,
}

pub(crate) struct BoxInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

pub(crate) fn resolve_insets(computed: &ComputedValues) -> (BoxInsets, BoxInsets) {
    use style::values::specified::border::BorderStyle;

    let border = computed.get_border();
    let border_width = |style: BorderStyle, width: style::values::computed::BorderSideWidth| -> f32 {
        if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            0.0
        } else {
            width.0.to_f32_px().max(0.0)
        }
    };
    let border = BoxInsets {
        top: border_width(border.clone_border_top_style(), border.clone_border_top_width()),
        right: border_width(border.clone_border_right_style(), border.clone_border_right_width()),
        bottom: border_width(border.clone_border_bottom_style(), border.clone_border_bottom_width()),
        left: border_width(border.clone_border_left_style(), border.clone_border_left_width()),
    };

    let padding = computed.get_padding();
    let padding = BoxInsets {
        top: padding.padding_top.0.to_length().map_or(0.0, |length| length.px()),
        right: padding.padding_right.0.to_length().map_or(0.0, |length| length.px()),
        bottom: padding.padding_bottom.0.to_length().map_or(0.0, |length| length.px()),
        left: padding.padding_left.0.to_length().map_or(0.0, |length| length.px()),
    };

    (border, padding)
}

fn sub_rect(
    x: f64,
    y: f64,
    w: f32,
    h: f32,
    border: &BoxInsets,
    padding: &BoxInsets,
    which: Origin,
) -> (f64, f64, f32, f32) {
    match which {
        Origin::BorderBox => (x, y, w, h),
        Origin::PaddingBox => (
            x + border.left as f64,
            y + border.top as f64,
            (w - border.left - border.right).max(0.0),
            (h - border.top - border.bottom).max(0.0),
        ),
        Origin::ContentBox => (
            x + (border.left + padding.left) as f64,
            y + (border.top + padding.top) as f64,
            (w - border.left - border.right - padding.left - padding.right).max(0.0),
            (h - border.top - border.bottom - padding.top - padding.bottom).max(0.0),
        ),
    }
}

fn clip_to_origin(clip: Clip) -> Origin {
    match clip {
        Clip::BorderBox => Origin::BorderBox,
        Clip::PaddingBox => Origin::PaddingBox,
        Clip::ContentBox => Origin::ContentBox,
    }
}

fn get_cyclic<T>(values: &[T], index: usize) -> &T {
    &values[index % values.len()]
}

pub(crate) fn layout_background_layer(
    computed: &ComputedValues,
    layer_index: usize,
    x: f64,
    y: f64,
    w: f32,
    h: f32,
    border: &BoxInsets,
    padding: &BoxInsets,
    natural_w: Option<f32>,
    natural_h: Option<f32>,
) -> Option<BackgroundLayerGeom> {
    let background = computed.get_background();

    let origin = *get_cyclic(&background.background_origin.0, layer_index);
    let clip = *get_cyclic(&background.background_clip.0, layer_index);

    let (position_x, position_y, position_w, position_h) = sub_rect(x, y, w, h, border, padding, origin);
    let (paint_x, paint_y, paint_w, paint_h) = sub_rect(x, y, w, h, border, padding, clip_to_origin(clip));

    let mut tile_w;
    let mut tile_h;
    match get_cyclic(&background.background_size.0, layer_index) {
        BackgroundSize::Contain | BackgroundSize::Cover => {
            tile_w = position_w;
            tile_h = position_h;
            if let (Some(natural_w), Some(natural_h)) = (natural_w, natural_h) {
                if natural_w > 0.0 && natural_h > 0.0 {
                    let natural_ratio = natural_w / natural_h;
                    let position_ratio = position_w / position_h;
                    let fit_width = match get_cyclic(&background.background_size.0, layer_index) {
                        BackgroundSize::Contain => position_ratio <= natural_ratio,
                        BackgroundSize::Cover => position_ratio > natural_ratio,
                        BackgroundSize::ExplicitSize { .. } => unreachable!(),
                    };
                    if fit_width {
                        tile_h = tile_w / natural_ratio;
                    } else {
                        tile_w = tile_h * natural_ratio;
                    }
                }
            }
        }
        BackgroundSize::ExplicitSize { width, height } => {
            let mut explicit_w = width.non_auto().map(|value| {
                value.0.to_used_value(app_units::Au::from_f32_px(position_w)).to_f32_px()
            });
            let mut explicit_h = height.non_auto().map(|value| {
                value.0.to_used_value(app_units::Au::from_f32_px(position_h)).to_f32_px()
            });
            if explicit_w.is_none() && explicit_h.is_none() {
                explicit_w = natural_w;
                explicit_h = natural_h;
            }
            match (explicit_w, explicit_h) {
                (Some(tile_width), Some(tile_height)) => {
                    tile_w = tile_width;
                    tile_h = tile_height;
                }
                (Some(tile_width), None) => {
                    tile_w = tile_width;
                    tile_h = if let (Some(natural_w), Some(natural_h)) = (natural_w, natural_h) {
                        if natural_w > 0.0 {
                            tile_width * natural_h / natural_w
                        } else {
                            position_h
                        }
                    } else {
                        natural_h.unwrap_or(position_h)
                    };
                }
                (None, Some(tile_height)) => {
                    tile_h = tile_height;
                    tile_w = if let (Some(natural_w), Some(natural_h)) = (natural_w, natural_h) {
                        if natural_h > 0.0 {
                            tile_height * natural_w / natural_h
                        } else {
                            position_w
                        }
                    } else {
                        natural_w.unwrap_or(position_w)
                    };
                }
                (None, None) => {
                    tile_w = position_w;
                    tile_h = position_h;
                }
            }
        }
    }

    if tile_w <= 0.0 || tile_h <= 0.0 {
        return None;
    }

    let RepeatXY(repeat_x, repeat_y) = *get_cyclic(&background.background_repeat.0, layer_index);
    let layout_x = layout_1d(
        &mut tile_w,
        repeat_x,
        get_cyclic(&background.background_position_x.0, layer_index),
        paint_x as f32 - position_x as f32,
        paint_w,
        position_w,
    );
    let layout_y = layout_1d(
        &mut tile_h,
        repeat_y,
        get_cyclic(&background.background_position_y.0, layer_index),
        paint_y as f32 - position_y as f32,
        paint_h,
        position_h,
    );

    Some(BackgroundLayerGeom {
        bounds_x: position_x + layout_x.origin as f64,
        bounds_y: position_y + layout_y.origin as f64,
        bounds_w: layout_x.size,
        bounds_h: layout_y.size,
        tile_w,
        tile_h,
    })
}

struct Layout1DResult {
    origin: f32,
    size: f32,
}

fn layout_1d(
    tile_size: &mut f32,
    mut repeat: Repeat,
    position: &LengthPercentage,
    painting_area_origin: f32,
    painting_area_size: f32,
    positioning_area_size: f32,
) -> Layout1DResult {
    if let Repeat::Round = repeat {
        if positioning_area_size > 0.0 {
            *tile_size = positioning_area_size / (positioning_area_size / *tile_size).round().max(1.0);
        }
    }

    let mut origin = position
        .to_used_value(app_units::Au::from_f32_px(positioning_area_size - *tile_size))
        .to_f32_px();
    let mut spacing = 0.0;
    if let Repeat::Space = repeat {
        let count = (positioning_area_size / *tile_size).floor();
        if count >= 2.0 {
            origin = 0.0;
            spacing = (positioning_area_size - *tile_size * count) / (count - 1.0);
        } else {
            repeat = Repeat::NoRepeat;
        }
    }

    match repeat {
        Repeat::Repeat | Repeat::Round | Repeat::Space => {
            let stride = *tile_size + spacing;
            let offset = origin - painting_area_origin;
            let origin = origin - stride * (offset / stride).ceil();
            let end = painting_area_origin + painting_area_size;
            Layout1DResult {
                origin,
                size: end - origin,
            }
        }
        Repeat::NoRepeat => Layout1DResult {
            origin,
            size: *tile_size,
        },
    }
}
