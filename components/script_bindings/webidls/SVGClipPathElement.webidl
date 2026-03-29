/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface SVGClipPathElement : SVGElement {
  [SameObject] readonly attribute SVGAnimatedEnumeration clipPathUnits;
  [SameObject] readonly attribute SVGAnimatedTransformList transform;
};
