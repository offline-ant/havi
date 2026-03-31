/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::f32;

use app_units::{Au, MAX_AU, MIN_AU};
use euclid::default::{Point2D as UntypedPoint2D, Rect as UntypedRect, Size2D as UntypedSize2D};
use euclid::{Box2D, Length, Point2D, Rect, Scale, SideOffsets2D, Size2D, Vector2D};
use std::borrow::Cow;

use euclid::Transform3D;
use malloc_size_of::malloc_size_of_is_0;
use malloc_size_of_derive::MallocSizeOf;
use webrender_api::units::{
    DeviceIntRect, DeviceIntSize, DevicePixel, FramebufferPixel, LayoutPixel, LayoutPoint,
    LayoutRect, LayoutSize,
};

// Units for use with euclid::length and euclid::scale_factor.

pub type FramebufferUintLength = Length<u32, FramebufferPixel>;

/// A normalized "pixel" at the default resolution for the display.
///
/// Like the CSS "px" unit, the exact physical size of this unit may vary between devices, but it
/// should approximate a device-independent reference length.  This unit corresponds to Android's
/// "density-independent pixel" (dip), Mac OS X's "point", and Windows "device-independent pixel."
///
/// The relationship between DevicePixel and DeviceIndependentPixel is defined by the OS.  On most low-dpi
/// screens, one DeviceIndependentPixel is equal to one DevicePixel.  But on high-density screens it can be
/// some larger number.  For example, by default on Apple "retina" displays, one DeviceIndependentPixel equals
/// two DevicePixels.  On Android "MDPI" displays, one DeviceIndependentPixel equals 1.5 device pixels.
///
/// The ratio between DeviceIndependentPixel and DevicePixel for a given display be found by calling
/// `servo::windowing::WindowMethods::hidpi_factor`.
#[derive(Clone, Copy, Debug, MallocSizeOf)]
pub enum DeviceIndependentPixel {}

pub type DeviceIndependentIntRect = Box2D<i32, DeviceIndependentPixel>;
pub type DeviceIndependentIntPoint = Point2D<i32, DeviceIndependentPixel>;
pub type DeviceIndependentIntSize = Size2D<i32, DeviceIndependentPixel>;
pub type DeviceIndependentIntLength = Length<i32, DeviceIndependentPixel>;
pub type DeviceIndependentIntSideOffsets = SideOffsets2D<i32, DeviceIndependentPixel>;
pub type DeviceIndependentIntVector2D = Vector2D<i32, DeviceIndependentPixel>;

pub type DeviceIndependentRect = Box2D<f32, DeviceIndependentPixel>;
pub type DeviceIndependentBox2D = Box2D<f32, DeviceIndependentPixel>;
pub type DeviceIndependentPoint = Point2D<f32, DeviceIndependentPixel>;
pub type DeviceIndependentVector2D = Vector2D<f32, DeviceIndependentPixel>;
pub type DeviceIndependentSize = Size2D<f32, DeviceIndependentPixel>;

pub type FastLayoutTransform = FastTransform<LayoutPixel, LayoutPixel>;

/// A cached 3D transform with fast-path for simple offsets.
/// Copied from webrender::util::FastTransform to remove the webrender dependency.
#[derive(serde::Serialize, serde::Deserialize)]
pub enum FastTransform<Src, Dst> {
    Offset(Vector2D<f32, Src>),
    Transform {
        transform: Transform3D<f32, Src, Dst>,
        inverse: Option<Transform3D<f32, Dst, Src>>,
        is_2d: bool,
    },
}

impl<Src, Dst> Clone for FastTransform<Src, Dst> {
    fn clone(&self) -> Self { *self }
}

impl<Src, Dst> Copy for FastTransform<Src, Dst> {}

impl<Src, Dst> std::fmt::Debug for FastTransform<Src, Dst> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FastTransform::Offset(v) => write!(f, "Offset({:?}, {:?})", v.x, v.y),
            FastTransform::Transform { transform, is_2d, .. } => {
                write!(f, "Transform(is_2d={}, {:?})", is_2d, transform)
            },
        }
    }
}

impl<Src, Dst> Default for FastTransform<Src, Dst> {
    fn default() -> Self { Self::identity() }
}

impl<Src, Dst> FastTransform<Src, Dst> {
    pub fn identity() -> Self {
        FastTransform::Offset(Vector2D::zero())
    }

    #[inline(always)]
    pub fn with_transform(transform: Transform3D<f32, Src, Dst>) -> Self {
        if transform.is_2d()
            && transform.m11 == 1.0
            && transform.m22 == 1.0
            && transform.m12 == 0.0
            && transform.m21 == 0.0
        {
            return FastTransform::Offset(Vector2D::new(transform.m41, transform.m42));
        }
        let inverse = transform.inverse();
        let is_2d = transform.is_2d();
        FastTransform::Transform { transform, inverse, is_2d }
    }

    pub fn to_transform(&self) -> Cow<'_, Transform3D<f32, Src, Dst>> {
        match *self {
            FastTransform::Offset(offset) => {
                Cow::Owned(Transform3D::translation(offset.x, offset.y, 0.0))
            },
            FastTransform::Transform { ref transform, .. } => Cow::Borrowed(transform),
        }
    }

    pub fn then<NewDst>(&self, other: &FastTransform<Dst, NewDst>) -> FastTransform<Src, NewDst> {
        match *self {
            FastTransform::Offset(offset) => match *other {
                FastTransform::Offset(other_offset) => {
                    FastTransform::Offset(offset + other_offset * Scale::<_, _, Src>::new(1.0))
                },
                FastTransform::Transform {
                    transform: ref other_transform,
                    ..
                } => FastTransform::with_transform(
                    other_transform
                        .with_source::<Src>()
                        .pre_translate(offset.to_3d()),
                ),
            },
            FastTransform::Transform {
                ref transform,
                ref inverse,
                is_2d,
            } => match *other {
                FastTransform::Offset(other_offset) => FastTransform::with_transform(
                    transform
                        .then_translate(other_offset.to_3d())
                        .with_destination::<NewDst>(),
                ),
                FastTransform::Transform {
                    transform: ref other_transform,
                    inverse: ref other_inverse,
                    is_2d: other_is_2d,
                } => FastTransform::Transform {
                    transform: transform.then(other_transform),
                    inverse: inverse
                        .as_ref()
                        .and_then(|self_inv| other_inverse.as_ref().map(|other_inv| other_inv.then(self_inv))),
                    is_2d: is_2d & other_is_2d,
                },
            },
        }
    }

    pub fn pre_translate(&self, other_offset: Vector2D<f32, Src>) -> Self {
        match *self {
            FastTransform::Offset(offset) => FastTransform::Offset(offset + other_offset),
            FastTransform::Transform { transform, .. } => {
                FastTransform::with_transform(transform.pre_translate(other_offset.to_3d()))
            },
        }
    }

    pub fn then_translate(&self, other_offset: Vector2D<f32, Dst>) -> Self {
        match *self {
            FastTransform::Offset(offset) => {
                FastTransform::Offset(offset + other_offset * Scale::<_, _, Src>::new(1.0))
            },
            FastTransform::Transform { ref transform, .. } => {
                FastTransform::with_transform(transform.then_translate(other_offset.to_3d()))
            },
        }
    }

    #[inline(always)]
    pub fn inverse(&self) -> Option<FastTransform<Dst, Src>> {
        match *self {
            FastTransform::Offset(offset) => {
                Some(FastTransform::Offset(Vector2D::new(-offset.x, -offset.y)))
            },
            FastTransform::Transform {
                transform,
                inverse: Some(inverse),
                is_2d,
            } => Some(FastTransform::Transform {
                transform: inverse,
                inverse: Some(transform),
                is_2d,
            }),
            FastTransform::Transform { inverse: None, .. } => None,
        }
    }

    #[inline(always)]
    pub fn transform_point2d(&self, point: Point2D<f32, Src>) -> Option<Point2D<f32, Dst>> {
        match *self {
            FastTransform::Offset(offset) => {
                let new_point = point + offset;
                Some(Point2D::from_untyped(new_point.to_untyped()))
            },
            FastTransform::Transform { ref transform, .. } => transform.transform_point2d(point),
        }
    }

    #[inline(always)]
    pub fn is_backface_visible(&self) -> bool {
        match *self {
            FastTransform::Offset(..) => false,
            FastTransform::Transform { inverse: None, .. } => false,
            FastTransform::Transform {
                inverse: Some(ref inverse),
                ..
            } => inverse.m33 < 0.0,
        }
    }
}

impl<Src, Dst> From<Transform3D<f32, Src, Dst>> for FastTransform<Src, Dst> {
    fn from(transform: Transform3D<f32, Src, Dst>) -> Self {
        FastTransform::with_transform(transform)
    }
}

impl<Src, Dst> From<Vector2D<f32, Src>> for FastTransform<Src, Dst> {
    fn from(vector: Vector2D<f32, Src>) -> Self {
        FastTransform::Offset(vector)
    }
}

malloc_size_of_is_0!(FastLayoutTransform);

// An Au is an "App Unit" and represents 1/60th of a CSS pixel.  It was
// originally proposed in 2002 as a standard unit of measure in Gecko.
// See https://bugzilla.mozilla.org/show_bug.cgi?id=177805 for more info.

pub trait MaxRect {
    fn max_rect() -> Self;
}

/// A helper function to convert a Device rect to CSS pixels.
pub fn convert_rect_to_css_pixel(
    rect: DeviceIntRect,
    scale: Scale<f32, DeviceIndependentPixel, DevicePixel>,
) -> DeviceIndependentIntRect {
    (rect.to_f32() / scale).round().to_i32()
}

/// A helper function to convert a Device size to CSS pixels.
pub fn convert_size_to_css_pixel(
    size: DeviceIntSize,
    scale: Scale<f32, DeviceIndependentPixel, DevicePixel>,
) -> DeviceIndependentIntSize {
    (size.to_f32() / scale).round().to_i32()
}

impl MaxRect for UntypedRect<Au> {
    #[inline]
    fn max_rect() -> Self {
        Self::new(
            UntypedPoint2D::new(MIN_AU / 2, MIN_AU / 2),
            UntypedSize2D::new(MAX_AU, MAX_AU),
        )
    }
}

impl MaxRect for LayoutRect {
    #[inline]
    fn max_rect() -> Self {
        Self::from_origin_and_size(
            LayoutPoint::new(f32::MIN / 2.0, f32::MIN / 2.0),
            LayoutSize::new(f32::MAX, f32::MAX),
        )
    }
}

/// A helper function to convert a rect of `f32` pixels to a rect of app units.
pub fn f32_rect_to_au_rect<T>(rect: Rect<f32, T>) -> Rect<Au, T> {
    Rect::new(
        Point2D::new(
            Au::from_f32_px(rect.origin.x),
            Au::from_f32_px(rect.origin.y),
        ),
        Size2D::new(
            Au::from_f32_px(rect.size.width),
            Au::from_f32_px(rect.size.height),
        ),
    )
}

/// A helper function to convert a rect of `Au` pixels to a rect of f32 units.
pub fn au_rect_to_f32_rect<T>(rect: Rect<Au, T>) -> Rect<f32, T> {
    Rect::new(
        Point2D::new(rect.origin.x.to_f32_px(), rect.origin.y.to_f32_px()),
        Size2D::new(rect.size.width.to_f32_px(), rect.size.height.to_f32_px()),
    )
}
