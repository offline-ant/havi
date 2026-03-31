/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use dom_struct::dom_struct;
use euclid::default::Transform2D;
use html5ever::LocalName;

use crate::script::dom::bindings::codegen::Bindings::DOMMatrixBinding::DOMMatrix2DInit;
use crate::script::dom::bindings::codegen::GenericBindings::SVGLengthBinding::SVGLengthMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGLengthListBinding::SVGLengthListMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGNumberBinding::SVGNumberMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGNumberListBinding::SVGNumberListMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGPreserveAspectRatioBinding::SVGPreserveAspectRatioMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGTransformBinding::SVGTransformMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGTransformListBinding::SVGTransformListMethods;
use crate::script::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::script::dom::bindings::num::Finite;
use crate::script::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::script::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::dommatrix::DOMMatrix;
use crate::script::dom::dommatrixreadonly::dommatrix2dinit_to_matrix;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::dom::svg::svgelement::SVGElement;
use crate::script::script_runtime::CanGc;

use super::values::{
    SVGLengthValue, SVGPreserveAspectRatioValue, SVGTransformValue, SVG_LENGTHTYPE_NUMBER,
    SVG_LENGTHTYPE_UNKNOWN, SVG_MEETORSLICE_MEET, SVG_PRESERVEASPECTRATIO_XMIDYMID,
    SVG_TRANSFORM_MATRIX, compose_svg_transform_list, parse_svg_length, parse_svg_length_list,
    parse_svg_number_list, parse_svg_preserve_aspect_ratio,
    parse_svg_transform_list, serialize_svg_length, serialize_svg_length_list,
    serialize_svg_number_list, serialize_svg_preserve_aspect_ratio,
    serialize_svg_transform_list, set_svg_attribute_value, svg_attribute_value,
};

#[derive(Clone, JSTraceable, MallocSizeOf)]
enum SVGLengthSource {
    Attribute {
        owner: Dom<SVGElement>,
        #[no_trace]
        attribute: LocalName,
    },
    ListItem {
        owner: Dom<SVGElement>,
        #[no_trace]
        attribute: LocalName,
        index: u32,
    },
    Detached,
}

#[derive(Clone, JSTraceable, MallocSizeOf)]
enum SVGNumberSource {
    ListItem {
        owner: Dom<SVGElement>,
        #[no_trace]
        attribute: LocalName,
        index: u32,
    },
    Detached,
}

#[derive(Clone, JSTraceable, MallocSizeOf)]
enum SVGPreserveAspectRatioSource {
    Attribute {
        owner: Dom<SVGElement>,
        #[no_trace]
        attribute: LocalName,
    },
}

#[derive(Clone, JSTraceable, MallocSizeOf)]
enum SVGTransformSource {
    ListItem {
        owner: Dom<SVGElement>,
        #[no_trace]
        attribute: LocalName,
        index: u32,
    },
    Detached,
}

pub(crate) fn svg_matrix_from_transform(
    global: &GlobalScope,
    matrix: Transform2D<f32>,
    can_gc: CanGc,
) -> DomRoot<DOMMatrix> {
    DOMMatrix::new(
        global,
        true,
        euclid::default::Transform3D::new(
            matrix.m11 as f64,
            matrix.m12 as f64,
            0.0,
            0.0,
            matrix.m21 as f64,
            matrix.m22 as f64,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            matrix.m31 as f64,
            matrix.m32 as f64,
            0.0,
            1.0,
        ),
        can_gc,
    )
}

#[dom_struct]
pub(crate) struct SVGLength {
    reflector_: Reflector,
    source: SVGLengthSource,
    #[no_trace]
    #[ignore_malloc_size_of = "plain SVG value"]
    detached_value: Cell<SVGLengthValue>,
    read_only: bool,
}

impl SVGLength {
    fn new_inherited(source: SVGLengthSource, value: SVGLengthValue, read_only: bool) -> Self {
        Self {
            reflector_: Reflector::new(),
            source,
            detached_value: Cell::new(value),
            read_only,
        }
    }

    pub(crate) fn new_for_attribute(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let value = parse_svg_length(svg_attribute_value(owner, &attribute).as_deref());
        reflect_dom_object(
            Box::new(Self::new_inherited(
                SVGLengthSource::Attribute {
                    owner: Dom::from_ref(owner),
                    attribute,
                },
                value,
                read_only,
            )),
            global,
            can_gc,
        )
    }

    pub(crate) fn new_for_list_item(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        index: u32,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let value = parse_svg_length_list(svg_attribute_value(owner, &attribute).as_deref())
            .get(index as usize)
            .copied()
            .unwrap_or_default();
        reflect_dom_object(
            Box::new(Self::new_inherited(
                SVGLengthSource::ListItem {
                    owner: Dom::from_ref(owner),
                    attribute,
                    index,
                },
                value,
                read_only,
            )),
            global,
            can_gc,
        )
    }

    pub(crate) fn new_detached(
        global: &GlobalScope,
        value: SVGLengthValue,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(SVGLengthSource::Detached, value, false)),
            global,
            can_gc,
        )
    }

    pub(crate) fn current_value(&self) -> SVGLengthValue {
        match &self.source {
            SVGLengthSource::Attribute { owner, attribute } => {
                parse_svg_length(svg_attribute_value(owner, attribute).as_deref())
            },
            SVGLengthSource::ListItem {
                owner,
                attribute,
                index,
            } => parse_svg_length_list(svg_attribute_value(owner, attribute).as_deref())
                .get(*index as usize)
                .copied()
                .unwrap_or_default(),
            SVGLengthSource::Detached => self.detached_value.get(),
        }
    }

    fn set_value_internal(&self, value: SVGLengthValue) -> ErrorResult {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        match &self.source {
            SVGLengthSource::Attribute { owner, attribute } => {
                set_svg_attribute_value(
                    owner,
                    attribute,
                    Some(serialize_svg_length(value)),
                    CanGc::note(),
                );
            },
            SVGLengthSource::ListItem {
                owner,
                attribute,
                index,
            } => {
                let mut values =
                    parse_svg_length_list(svg_attribute_value(owner, attribute).as_deref());
                let index = *index as usize;
                if index >= values.len() {
                    return Err(Error::IndexSize(None));
                }
                values[index] = value;
                set_svg_attribute_value(
                    owner,
                    attribute,
                    serialize_svg_length_list(&values),
                    CanGc::note(),
                );
            },
            SVGLengthSource::Detached => self.detached_value.set(value),
        }
        Ok(())
    }
}

impl SVGLengthMethods<crate::DomTypeHolder> for SVGLength {
    fn UnitType(&self) -> u16 {
        self.current_value().unit_type
    }

    fn Value(&self) -> Finite<f32> {
        Finite::wrap(self.current_value().value)
    }

    fn SetValue(&self, value: Finite<f32>) -> ErrorResult {
        let mut current = self.current_value();
        if current.unit_type == SVG_LENGTHTYPE_UNKNOWN {
            current.unit_type = SVG_LENGTHTYPE_NUMBER;
        }
        current.value = *value;
        self.set_value_internal(current)
    }

    fn ValueInSpecifiedUnits(&self) -> Finite<f32> {
        Finite::wrap(self.current_value().value)
    }

    fn SetValueInSpecifiedUnits(&self, value: Finite<f32>) -> ErrorResult {
        let mut current = self.current_value();
        current.value = *value;
        self.set_value_internal(current)
    }

    fn ValueAsString(&self) -> DOMString {
        serialize_svg_length(self.current_value()).into()
    }

    fn SetValueAsString(&self, value: DOMString) -> ErrorResult {
        self.set_value_internal(parse_svg_length(Some(&value.str())))
    }

    fn NewValueSpecifiedUnits(&self, unit_type: u16, value_in_specified_units: Finite<f32>) -> Fallible<()> {
        self.set_value_internal(SVGLengthValue {
            unit_type,
            value: *value_in_specified_units,
        })
    }

    fn ConvertToSpecifiedUnits(&self, unit_type: u16) -> Fallible<()> {
        let mut current = self.current_value();
        current.unit_type = unit_type;
        self.set_value_internal(current)
    }
}

#[dom_struct]
pub(crate) struct SVGLengthList {
    reflector_: Reflector,
    owner: Dom<SVGElement>,
    #[no_trace]
    attribute: LocalName,
    read_only: bool,
}

impl SVGLengthList {
    fn new_inherited(owner: &SVGElement, attribute: LocalName, read_only: bool) -> Self {
        Self {
            reflector_: Reflector::new(),
            owner: Dom::from_ref(owner),
            attribute,
            read_only,
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(owner, attribute, read_only)),
            global,
            can_gc,
        )
    }

    fn values(&self) -> Vec<SVGLengthValue> {
        parse_svg_length_list(svg_attribute_value(&self.owner, &self.attribute).as_deref())
    }

    fn set_values(&self, values: &[SVGLengthValue]) -> ErrorResult {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        set_svg_attribute_value(
            &self.owner,
            &self.attribute,
            serialize_svg_length_list(values),
            CanGc::note(),
        );
        Ok(())
    }

    fn item(&self, index: u32) -> Fallible<DomRoot<SVGLength>> {
        if index as usize >= self.values().len() {
            return Err(Error::IndexSize(None));
        }
        Ok(SVGLength::new_for_list_item(
            &self.global(),
            &self.owner,
            self.attribute.clone(),
            index,
            self.read_only,
            CanGc::note(),
        ))
    }
}

impl SVGLengthListMethods<crate::DomTypeHolder> for SVGLengthList {
    fn NumberOfItems(&self) -> u32 {
        self.values().len() as u32
    }

    fn Clear(&self) -> ErrorResult {
        self.set_values(&[])
    }

    fn Initialize(&self, new_item: &SVGLength) -> Fallible<DomRoot<SVGLength>> {
        let value = new_item.current_value();
        self.set_values(&[value])?;
        self.item(0)
    }

    fn GetItem(&self, index: u32) -> Fallible<DomRoot<SVGLength>> {
        self.item(index)
    }

    fn IndexedGetter(&self, index: u32) -> Fallible<Option<DomRoot<SVGLength>>> {
        Ok(self.item(index).ok())
    }

    fn InsertItemBefore(&self, _new_item: &SVGLength, _index: u32) -> Fallible<DomRoot<SVGLength>> {
        Err(Error::NotSupported(None))
    }

    fn ReplaceItem(&self, _new_item: &SVGLength, _index: u32) -> Fallible<DomRoot<SVGLength>> {
        Err(Error::NotSupported(None))
    }

    fn RemoveItem(&self, _index: u32) -> Fallible<DomRoot<SVGLength>> {
        Err(Error::NotSupported(None))
    }

    fn AppendItem(&self, _new_item: &SVGLength) -> Fallible<DomRoot<SVGLength>> {
        Err(Error::NotSupported(None))
    }

    fn IndexedSetter(&self, index: u32, new_item: &SVGLength) -> Fallible<()> {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        let mut values = self.values();
        let index = index as usize;
        if index >= values.len() {
            return Err(Error::IndexSize(None));
        }
        values[index] = new_item.current_value();
        self.set_values(&values)
    }

    fn Length(&self) -> u32 {
        self.NumberOfItems()
    }
}

#[dom_struct]
pub(crate) struct SVGNumber {
    reflector_: Reflector,
    source: SVGNumberSource,
    detached_value: Cell<f32>,
    read_only: bool,
}

impl SVGNumber {
    fn new_inherited(source: SVGNumberSource, value: f32, read_only: bool) -> Self {
        Self {
            reflector_: Reflector::new(),
            source,
            detached_value: Cell::new(value),
            read_only,
        }
    }

    pub(crate) fn new_for_list_item(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        index: u32,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let value = parse_svg_number_list(svg_attribute_value(owner, &attribute).as_deref())
            .get(index as usize)
            .copied()
            .unwrap_or(0.0);
        reflect_dom_object(
            Box::new(Self::new_inherited(
                SVGNumberSource::ListItem {
                    owner: Dom::from_ref(owner),
                    attribute,
                    index,
                },
                value,
                read_only,
            )),
            global,
            can_gc,
        )
    }

    pub(crate) fn new_detached(global: &GlobalScope, value: f32, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(SVGNumberSource::Detached, value, false)),
            global,
            can_gc,
        )
    }

    pub(crate) fn current_value(&self) -> f32 {
        match &self.source {
            SVGNumberSource::ListItem {
                owner,
                attribute,
                index,
            } => parse_svg_number_list(svg_attribute_value(owner, attribute).as_deref())
                .get(*index as usize)
                .copied()
                .unwrap_or(0.0),
            SVGNumberSource::Detached => self.detached_value.get(),
        }
    }

    fn set_value_internal(&self, value: f32) -> ErrorResult {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        match &self.source {
            SVGNumberSource::ListItem {
                owner,
                attribute,
                index,
            } => {
                let mut values =
                    parse_svg_number_list(svg_attribute_value(owner, attribute).as_deref());
                let index = *index as usize;
                if index >= values.len() {
                    return Err(Error::IndexSize(None));
                }
                values[index] = value;
                set_svg_attribute_value(
                    owner,
                    attribute,
                    serialize_svg_number_list(&values),
                    CanGc::note(),
                );
            },
            SVGNumberSource::Detached => self.detached_value.set(value),
        }
        Ok(())
    }
}

impl SVGNumberMethods<crate::DomTypeHolder> for SVGNumber {
    fn Value(&self) -> Finite<f32> {
        Finite::wrap(self.current_value())
    }

    fn SetValue(&self, value: Finite<f32>) -> ErrorResult {
        self.set_value_internal(*value)
    }
}

#[dom_struct]
pub(crate) struct SVGNumberList {
    reflector_: Reflector,
    owner: Dom<SVGElement>,
    #[no_trace]
    attribute: LocalName,
    read_only: bool,
}

impl SVGNumberList {
    fn new_inherited(owner: &SVGElement, attribute: LocalName, read_only: bool) -> Self {
        Self {
            reflector_: Reflector::new(),
            owner: Dom::from_ref(owner),
            attribute,
            read_only,
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(owner, attribute, read_only)),
            global,
            can_gc,
        )
    }

    fn values(&self) -> Vec<f32> {
        parse_svg_number_list(svg_attribute_value(&self.owner, &self.attribute).as_deref())
    }

    fn set_values(&self, values: &[f32]) -> ErrorResult {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        set_svg_attribute_value(
            &self.owner,
            &self.attribute,
            serialize_svg_number_list(values),
            CanGc::note(),
        );
        Ok(())
    }

    fn item(&self, index: u32) -> Fallible<DomRoot<SVGNumber>> {
        if index as usize >= self.values().len() {
            return Err(Error::IndexSize(None));
        }
        Ok(SVGNumber::new_for_list_item(
            &self.global(),
            &self.owner,
            self.attribute.clone(),
            index,
            self.read_only,
            CanGc::note(),
        ))
    }
}

impl SVGNumberListMethods<crate::DomTypeHolder> for SVGNumberList {
    fn NumberOfItems(&self) -> u32 {
        self.values().len() as u32
    }

    fn Clear(&self) -> ErrorResult {
        self.set_values(&[])
    }

    fn Initialize(&self, new_item: &SVGNumber) -> Fallible<DomRoot<SVGNumber>> {
        let value = new_item.current_value();
        self.set_values(&[value])?;
        self.item(0)
    }

    fn GetItem(&self, index: u32) -> Fallible<DomRoot<SVGNumber>> {
        self.item(index)
    }

    fn IndexedGetter(&self, index: u32) -> Fallible<Option<DomRoot<SVGNumber>>> {
        Ok(self.item(index).ok())
    }

    fn InsertItemBefore(&self, _new_item: &SVGNumber, _index: u32) -> Fallible<DomRoot<SVGNumber>> {
        Err(Error::NotSupported(None))
    }

    fn ReplaceItem(&self, _new_item: &SVGNumber, _index: u32) -> Fallible<DomRoot<SVGNumber>> {
        Err(Error::NotSupported(None))
    }

    fn RemoveItem(&self, _index: u32) -> Fallible<DomRoot<SVGNumber>> {
        Err(Error::NotSupported(None))
    }

    fn AppendItem(&self, _new_item: &SVGNumber) -> Fallible<DomRoot<SVGNumber>> {
        Err(Error::NotSupported(None))
    }

    fn Length(&self) -> u32 {
        self.NumberOfItems()
    }
}

#[dom_struct]
pub(crate) struct SVGPreserveAspectRatio {
    reflector_: Reflector,
    source: SVGPreserveAspectRatioSource,
    read_only: bool,
}

impl SVGPreserveAspectRatio {
    fn new_inherited(
        source: SVGPreserveAspectRatioSource,
        read_only: bool,
    ) -> Self {
        Self {
            reflector_: Reflector::new(),
            source,
            read_only,
        }
    }

    pub(crate) fn new_for_attribute(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(
                SVGPreserveAspectRatioSource::Attribute {
                    owner: Dom::from_ref(owner),
                    attribute,
                },
                read_only,
            )),
            global,
            can_gc,
        )
    }

    pub(crate) fn current_value(&self) -> SVGPreserveAspectRatioValue {
        let SVGPreserveAspectRatioSource::Attribute { owner, attribute } = &self.source;
        parse_svg_preserve_aspect_ratio(svg_attribute_value(owner, attribute).as_deref())
    }

    fn set_value_internal(&self, value: SVGPreserveAspectRatioValue) -> ErrorResult {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        let SVGPreserveAspectRatioSource::Attribute { owner, attribute } = &self.source;
        set_svg_attribute_value(
            owner,
            attribute,
            Some(serialize_svg_preserve_aspect_ratio(value)),
            CanGc::note(),
        );
        Ok(())
    }
}

impl SVGPreserveAspectRatioMethods<crate::DomTypeHolder> for SVGPreserveAspectRatio {
    fn Align(&self) -> u16 {
        self.current_value().align
    }

    fn SetAlign(&self, align: u16) -> ErrorResult {
        let mut current = self.current_value();
        current.align = if align == 0 {
            SVG_PRESERVEASPECTRATIO_XMIDYMID
        } else {
            align
        };
        self.set_value_internal(current)
    }

    fn MeetOrSlice(&self) -> u16 {
        self.current_value().meet_or_slice
    }

    fn SetMeetOrSlice(&self, meet_or_slice: u16) -> ErrorResult {
        let mut current = self.current_value();
        current.meet_or_slice = if meet_or_slice == 0 {
            SVG_MEETORSLICE_MEET
        } else {
            meet_or_slice
        };
        self.set_value_internal(current)
    }
}

#[dom_struct]
pub(crate) struct SVGTransform {
    reflector_: Reflector,
    source: SVGTransformSource,
    #[no_trace]
    #[ignore_malloc_size_of = "plain SVG value"]
    detached_value: Cell<SVGTransformValue>,
    read_only: bool,
    matrix_cache: MutNullableDom<DOMMatrix>,
}

impl SVGTransform {
    fn new_inherited(source: SVGTransformSource, value: SVGTransformValue, read_only: bool) -> Self {
        Self {
            reflector_: Reflector::new(),
            source,
            detached_value: Cell::new(value),
            read_only,
            matrix_cache: Default::default(),
        }
    }

    pub(crate) fn new_for_list_item(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        index: u32,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let value = parse_svg_transform_list(svg_attribute_value(owner, &attribute).as_deref())
            .get(index as usize)
            .copied()
            .unwrap_or_default();
        reflect_dom_object(
            Box::new(Self::new_inherited(
                SVGTransformSource::ListItem {
                    owner: Dom::from_ref(owner),
                    attribute,
                    index,
                },
                value,
                read_only,
            )),
            global,
            can_gc,
        )
    }

    pub(crate) fn new_detached(global: &GlobalScope, value: SVGTransformValue, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(SVGTransformSource::Detached, value, false)),
            global,
            can_gc,
        )
    }

    pub(crate) fn current_value(&self) -> SVGTransformValue {
        match &self.source {
            SVGTransformSource::ListItem {
                owner,
                attribute,
                index,
            } => parse_svg_transform_list(svg_attribute_value(owner, attribute).as_deref())
                .get(*index as usize)
                .copied()
                .unwrap_or_default(),
            SVGTransformSource::Detached => self.detached_value.get(),
        }
    }

    fn set_value_internal(&self, value: SVGTransformValue) -> ErrorResult {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        match &self.source {
            SVGTransformSource::ListItem {
                owner,
                attribute,
                index,
            } => {
                let mut values =
                    parse_svg_transform_list(svg_attribute_value(owner, attribute).as_deref());
                let index = *index as usize;
                if index >= values.len() {
                    return Err(Error::IndexSize(None));
                }
                values[index] = value;
                set_svg_attribute_value(
                    owner,
                    attribute,
                    serialize_svg_transform_list(&values),
                    CanGc::note(),
                );
            },
            SVGTransformSource::Detached => self.detached_value.set(value),
        }
        Ok(())
    }

    fn transform_for_matrix(matrix: Transform2D<f32>) -> SVGTransformValue {
        SVGTransformValue {
            transform_type: SVG_TRANSFORM_MATRIX,
            matrix,
            angle: 0.0,
        }
    }

    pub(crate) fn set_matrix_direct(&self, matrix: Transform2D<f32>) -> ErrorResult {
        self.set_value_internal(Self::transform_for_matrix(matrix))
    }
}

impl SVGTransformMethods<crate::DomTypeHolder> for SVGTransform {
    fn Type(&self) -> u16 {
        self.current_value().transform_type
    }

    fn Matrix(&self) -> DomRoot<DOMMatrix> {
        self.matrix_cache.or_init(|| {
            svg_matrix_from_transform(&self.global(), self.current_value().matrix, CanGc::note())
        })
    }

    fn Angle(&self) -> Finite<f32> {
        Finite::wrap(self.current_value().angle)
    }

    fn SetMatrix(&self, matrix: &DOMMatrix2DInit) -> ErrorResult {
        let matrix = dommatrix2dinit_to_matrix(matrix)?;
        self.set_matrix_direct(Transform2D::new(
            matrix.m11 as f32,
            matrix.m12 as f32,
            matrix.m21 as f32,
            matrix.m22 as f32,
            matrix.m31 as f32,
            matrix.m32 as f32,
        ))
    }

    fn SetTranslate(&self, tx: Finite<f32>, ty: Finite<f32>) -> Fallible<()> {
        self.set_value_internal(SVGTransformValue {
            transform_type: super::values::SVG_TRANSFORM_TRANSLATE,
            matrix: Transform2D::translation(*tx, *ty),
            angle: 0.0,
        })
    }

    fn SetScale(&self, sx: Finite<f32>, sy: Finite<f32>) -> Fallible<()> {
        self.set_value_internal(SVGTransformValue {
            transform_type: super::values::SVG_TRANSFORM_SCALE,
            matrix: Transform2D::scale(*sx, *sy),
            angle: 0.0,
        })
    }

    fn SetRotate(&self, angle: Finite<f32>, cx: Finite<f32>, cy: Finite<f32>) -> Fallible<()> {
        let matrix = Transform2D::translation(*cx, *cy)
            .then_rotate(euclid::Angle::degrees(*angle))
            .then_translate(euclid::vec2(-*cx, -*cy));
        self.set_value_internal(SVGTransformValue {
            transform_type: super::values::SVG_TRANSFORM_ROTATE,
            matrix,
            angle: *angle,
        })
    }

    fn SetSkewX(&self, angle: Finite<f32>) -> Fallible<()> {
        self.set_value_internal(SVGTransformValue {
            transform_type: super::values::SVG_TRANSFORM_SKEWX,
            matrix: Transform2D::new(1.0, 0.0, angle.to_radians().tan(), 1.0, 0.0, 0.0),
            angle: *angle,
        })
    }

    fn SetSkewY(&self, angle: Finite<f32>) -> Fallible<()> {
        self.set_value_internal(SVGTransformValue {
            transform_type: super::values::SVG_TRANSFORM_SKEWY,
            matrix: Transform2D::new(1.0, angle.to_radians().tan(), 0.0, 1.0, 0.0, 0.0),
            angle: *angle,
        })
    }
}

#[dom_struct]
pub(crate) struct SVGTransformList {
    reflector_: Reflector,
    owner: Dom<SVGElement>,
    #[no_trace]
    attribute: LocalName,
    read_only: bool,
}

impl SVGTransformList {
    fn new_inherited(owner: &SVGElement, attribute: LocalName, read_only: bool) -> Self {
        Self {
            reflector_: Reflector::new(),
            owner: Dom::from_ref(owner),
            attribute,
            read_only,
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        owner: &SVGElement,
        attribute: LocalName,
        read_only: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(owner, attribute, read_only)),
            global,
            can_gc,
        )
    }

    fn values(&self) -> Vec<SVGTransformValue> {
        parse_svg_transform_list(svg_attribute_value(&self.owner, &self.attribute).as_deref())
    }

    fn set_values(&self, values: &[SVGTransformValue]) -> ErrorResult {
        if self.read_only {
            return Err(Error::NoModificationAllowed(None));
        }
        set_svg_attribute_value(
            &self.owner,
            &self.attribute,
            serialize_svg_transform_list(values),
            CanGc::note(),
        );
        Ok(())
    }

    fn item(&self, index: u32) -> Fallible<DomRoot<SVGTransform>> {
        if index as usize >= self.values().len() {
            return Err(Error::IndexSize(None));
        }
        Ok(SVGTransform::new_for_list_item(
            &self.global(),
            &self.owner,
            self.attribute.clone(),
            index,
            self.read_only,
            CanGc::note(),
        ))
    }
}

impl SVGTransformListMethods<crate::DomTypeHolder> for SVGTransformList {
    fn NumberOfItems(&self) -> u32 {
        self.values().len() as u32
    }

    fn Clear(&self) -> ErrorResult {
        self.set_values(&[])
    }

    fn Initialize(&self, new_item: &SVGTransform) -> Fallible<DomRoot<SVGTransform>> {
        let value = new_item.current_value();
        self.set_values(&[value])?;
        self.item(0)
    }

    fn GetItem(&self, index: u32) -> Fallible<DomRoot<SVGTransform>> {
        self.item(index)
    }

    fn IndexedGetter(&self, index: u32) -> Fallible<Option<DomRoot<SVGTransform>>> {
        Ok(self.item(index).ok())
    }

    fn InsertItemBefore(&self, _new_item: &SVGTransform, _index: u32) -> Fallible<DomRoot<SVGTransform>> {
        Err(Error::NotSupported(None))
    }

    fn ReplaceItem(&self, _new_item: &SVGTransform, _index: u32) -> Fallible<DomRoot<SVGTransform>> {
        Err(Error::NotSupported(None))
    }

    fn RemoveItem(&self, _index: u32) -> Fallible<DomRoot<SVGTransform>> {
        Err(Error::NotSupported(None))
    }

    fn AppendItem(&self, _new_item: &SVGTransform) -> Fallible<DomRoot<SVGTransform>> {
        Err(Error::NotSupported(None))
    }

    fn CreateSVGTransformFromMatrix(&self, matrix: &DOMMatrix2DInit) -> Fallible<DomRoot<SVGTransform>> {
        let matrix = dommatrix2dinit_to_matrix(matrix)?;
        Ok(SVGTransform::new_detached(
            &self.global(),
            SVGTransformValue {
                transform_type: SVG_TRANSFORM_MATRIX,
                matrix: Transform2D::new(
                    matrix.m11 as f32,
                    matrix.m12 as f32,
                    matrix.m21 as f32,
                    matrix.m22 as f32,
                    matrix.m31 as f32,
                    matrix.m32 as f32,
                ),
                angle: 0.0,
            },
            CanGc::note(),
        ))
    }

    fn Consolidate(&self) -> Fallible<Option<DomRoot<SVGTransform>>> {
        let values = self.values();
        if values.is_empty() {
            return Ok(None);
        }
        let matrix = compose_svg_transform_list(&values);
        Ok(Some(SVGTransform::new_detached(
            &self.global(),
            SVGTransformValue {
                transform_type: SVG_TRANSFORM_MATRIX,
                matrix,
                angle: 0.0,
            },
            CanGc::note(),
        )))
    }

    fn Length(&self) -> u32 {
        self.NumberOfItems()
    }
}
