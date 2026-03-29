/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface SVGMaskElement : SVGElement {
  const unsigned short SVG_MASKTYPE_LUMINANCE = 0;
  const unsigned short SVG_MASKTYPE_ALPHA = 1;

  [SameObject] readonly attribute SVGAnimatedEnumeration maskUnits;
  [SameObject] readonly attribute SVGAnimatedEnumeration maskContentUnits;
  [SameObject] readonly attribute SVGAnimatedLength x;
  [SameObject] readonly attribute SVGAnimatedLength y;
  [SameObject] readonly attribute SVGAnimatedLength width;
  [SameObject] readonly attribute SVGAnimatedLength height;
};
