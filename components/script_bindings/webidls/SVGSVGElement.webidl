/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// https://svgwg.org/svg2-draft/struct.html#InterfaceSVGSVGElement
[Exposed=Window]
interface SVGSVGElement : SVGGraphicsElement {
  [SameObject] readonly attribute SVGAnimatedLength x;
  [SameObject] readonly attribute SVGAnimatedLength y;
  [SameObject] readonly attribute SVGAnimatedLength width;
  [SameObject] readonly attribute SVGAnimatedLength height;

  [SameObject] readonly attribute DOMPoint currentTranslate;

  SVGNumber createSVGNumber();
  SVGLength createSVGLength();
  DOMPoint createSVGPoint();
  DOMMatrix createSVGMatrix();
  DOMRect createSVGRect();
  SVGTransform createSVGTransform();
  SVGTransform createSVGTransformFromMatrix(optional DOMMatrix2DInit matrix = {});

  Element? getElementById(DOMString elementId);
};

SVGSVGElement includes SVGFitToViewBox;
