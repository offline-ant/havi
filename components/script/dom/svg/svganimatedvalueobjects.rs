/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::LocalName;

use crate::dom::bindings::codegen::Bindings::DOMRectBinding::DOMRectMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedEnumerationBinding::SVGAnimatedEnumerationMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedLengthBinding::SVGAnimatedLengthMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedLengthListBinding::SVGAnimatedLengthListMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedNumberBinding::SVGAnimatedNumberMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedNumberListBinding::SVGAnimatedNumberListMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedPreserveAspectRatioBinding::SVGAnimatedPreserveAspectRatioMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedRectBinding::SVGAnimatedRectMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedStringBinding::SVGAnimatedStringMethods;
use crate::dom::bindings::codegen::Bindings::SVGAnimatedTransformListBinding::SVGAnimatedTransformListMethods;
use crate::dom::bindings::num::Finite;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::domrect::DOMRect;
use crate::dom::globalscope::GlobalScope;
use crate::dom::svg::svgelement::SVGElement;
use crate::script_runtime::CanGc;

use super::svgvalueobjects::{
    SVGLength, SVGLengthList, SVGNumberList, SVGPreserveAspectRatio, SVGTransformList,
};
use super::values::{
    parse_svg_number, parse_svg_view_box, set_svg_attribute_value, svg_attribute_value,
};

#[dom_struct]
pub(crate) struct SVGAnimatedString {
    reflector_: Reflector,
    owner: Dom<SVGElement>,
    #[no_trace]
    attribute: LocalName,
}

impl SVGAnimatedString {
    fn new_inherited(owner: &SVGElement, attribute: LocalName) -> Self {
        Self {
            reflector_: Reflector::new(),
            owner: Dom::from_ref(owner),
            attribute,
        }
    }

    pub(crate) fn new(global: &GlobalScope, owner: &SVGElement, attribute: LocalName, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(owner, attribute)), global, can_gc)
    }
}

impl SVGAnimatedStringMethods<crate::DomTypeHolder> for SVGAnimatedString {
    fn BaseVal(&self) -> DOMString {
        svg_attribute_value(&self.owner, &self.attribute).unwrap_or_default().into()
    }

    fn SetBaseVal(&self, value: DOMString) {
        set_svg_attribute_value(&self.owner, &self.attribute, Some(value.to_string()), CanGc::note());
    }

    fn AnimVal(&self) -> DOMString {
        self.BaseVal()
    }
}

macro_rules! animated_wrapper {
    ($name:ident, $methods:ident, $inner:ident, $base_method:ident, $anim_method:ident, $ctor:ident) => {
        #[dom_struct]
        pub(crate) struct $name {
            reflector_: Reflector,
            owner: Dom<SVGElement>,
            #[no_trace]
            attribute: LocalName,
            base: MutNullableDom<$inner>,
            anim: MutNullableDom<$inner>,
        }

        impl $name {
            fn new_inherited(owner: &SVGElement, attribute: LocalName) -> Self {
                Self {
                    reflector_: Reflector::new(),
                    owner: Dom::from_ref(owner),
                    attribute,
                    base: Default::default(),
                    anim: Default::default(),
                }
            }

            pub(crate) fn new(global: &GlobalScope, owner: &SVGElement, attribute: LocalName, can_gc: CanGc) -> DomRoot<Self> {
                reflect_dom_object(Box::new(Self::new_inherited(owner, attribute)), global, can_gc)
            }
        }

        impl $methods<crate::DomTypeHolder> for $name {
            fn $base_method(&self) -> DomRoot<$inner> {
                self.base.or_init(|| {
                    $inner::$ctor(&self.global(), &self.owner, self.attribute.clone(), false, CanGc::note())
                })
            }

            fn $anim_method(&self) -> DomRoot<$inner> {
                self.anim.or_init(|| {
                    $inner::$ctor(&self.global(), &self.owner, self.attribute.clone(), true, CanGc::note())
                })
            }
        }
    };
}

animated_wrapper!(SVGAnimatedLength, SVGAnimatedLengthMethods, SVGLength, BaseVal, AnimVal, new_for_attribute);
animated_wrapper!(SVGAnimatedLengthList, SVGAnimatedLengthListMethods, SVGLengthList, BaseVal, AnimVal, new);
animated_wrapper!(SVGAnimatedNumberList, SVGAnimatedNumberListMethods, SVGNumberList, BaseVal, AnimVal, new);
animated_wrapper!(SVGAnimatedTransformList, SVGAnimatedTransformListMethods, SVGTransformList, BaseVal, AnimVal, new);
animated_wrapper!(SVGAnimatedPreserveAspectRatio, SVGAnimatedPreserveAspectRatioMethods, SVGPreserveAspectRatio, BaseVal, AnimVal, new_for_attribute);

#[dom_struct]
pub(crate) struct SVGAnimatedRect {
    reflector_: Reflector,
    owner: Dom<SVGElement>,
    #[no_trace]
    attribute: LocalName,
    base: MutNullableDom<DOMRect>,
    anim: MutNullableDom<DOMRect>,
}

impl SVGAnimatedRect {
    fn new_inherited(owner: &SVGElement, attribute: LocalName) -> Self {
        Self {
            reflector_: Reflector::new(),
            owner: Dom::from_ref(owner),
            attribute,
            base: Default::default(),
            anim: Default::default(),
        }
    }

    pub(crate) fn new(global: &GlobalScope, owner: &SVGElement, attribute: LocalName, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(owner, attribute)), global, can_gc)
    }

    fn sync_rect(&self, rect: &DOMRect) {
        let value = parse_svg_view_box(svg_attribute_value(&self.owner, &self.attribute).as_deref());
        rect.SetX(value.x as f64);
        rect.SetY(value.y as f64);
        rect.SetWidth(value.width as f64);
        rect.SetHeight(value.height as f64);
    }
}

impl SVGAnimatedRectMethods<crate::DomTypeHolder> for SVGAnimatedRect {
    fn BaseVal(&self) -> DomRoot<DOMRect> {
        let rect = self.base.or_init(|| DOMRect::new(&self.global(), 0.0, 0.0, 0.0, 0.0, CanGc::note()));
        self.sync_rect(&rect);
        rect
    }

    fn AnimVal(&self) -> DomRoot<DOMRect> {
        let rect = self.anim.or_init(|| DOMRect::new(&self.global(), 0.0, 0.0, 0.0, 0.0, CanGc::note()));
        self.sync_rect(&rect);
        rect
    }
}

#[dom_struct]
pub(crate) struct SVGAnimatedNumber {
    reflector_: Reflector,
    owner: Dom<SVGElement>,
    #[no_trace]
    attribute: LocalName,
}

impl SVGAnimatedNumber {
    fn new_inherited(owner: &SVGElement, attribute: LocalName) -> Self {
        Self {
            reflector_: Reflector::new(),
            owner: Dom::from_ref(owner),
            attribute,
        }
    }

    pub(crate) fn new(global: &GlobalScope, owner: &SVGElement, attribute: LocalName, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(owner, attribute)), global, can_gc)
    }
}

impl SVGAnimatedNumberMethods<crate::DomTypeHolder> for SVGAnimatedNumber {
    fn BaseVal(&self) -> Finite<f32> {
        Finite::wrap(parse_svg_number(svg_attribute_value(&self.owner, &self.attribute).as_deref()))
    }

    fn SetBaseVal(&self, value: Finite<f32>) {
        set_svg_attribute_value(&self.owner, &self.attribute, Some((*value).to_string()), CanGc::note());
    }

    fn AnimVal(&self) -> Finite<f32> {
        self.BaseVal()
    }
}

#[dom_struct]
pub(crate) struct SVGAnimatedEnumeration {
    reflector_: Reflector,
    owner: Dom<SVGElement>,
    #[no_trace]
    attribute: LocalName,
    #[no_trace]
    #[ignore_malloc_size_of = "static table"]
    mapping: &'static [(&'static str, u16)],
}

impl SVGAnimatedEnumeration {
    fn new_inherited(owner: &SVGElement, attribute: LocalName, mapping: &'static [(&'static str, u16)]) -> Self {
        Self {
            reflector_: Reflector::new(),
            owner: Dom::from_ref(owner),
            attribute,
            mapping,
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        mapping: &'static [(&'static str, u16)],
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(owner, attribute, mapping)), global, can_gc)
    }

    fn current_value(&self) -> u16 {
        let raw = svg_attribute_value(&self.owner, &self.attribute).unwrap_or_default();
        self.mapping
            .iter()
            .find_map(|(name, value)| raw.trim().eq(*name).then_some(*value))
            .unwrap_or(0)
    }
}

impl SVGAnimatedEnumerationMethods<crate::DomTypeHolder> for SVGAnimatedEnumeration {
    fn BaseVal(&self) -> u16 {
        self.current_value()
    }

    fn SetBaseVal(&self, value: u16) {
        if let Some((name, _)) = self.mapping.iter().find(|(_, mapped)| *mapped == value) {
            set_svg_attribute_value(&self.owner, &self.attribute, Some((*name).to_owned()), CanGc::note());
        }
    }

    fn AnimVal(&self) -> u16 {
        self.current_value()
    }
}
