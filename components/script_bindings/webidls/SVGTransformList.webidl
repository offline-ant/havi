/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface SVGTransformList {
  readonly attribute unsigned long numberOfItems;
  [Throws] undefined clear();
  [Throws] SVGTransform initialize(SVGTransform newItem);
  [Throws] getter SVGTransform getItem(unsigned long index);
  [Throws] SVGTransform insertItemBefore(SVGTransform newItem, unsigned long index);
  [Throws] SVGTransform replaceItem(SVGTransform newItem, unsigned long index);
  [Throws] SVGTransform removeItem(unsigned long index);
  [Throws] SVGTransform appendItem(SVGTransform newItem);
  [Throws] SVGTransform createSVGTransformFromMatrix(optional DOMMatrix2DInit matrix = {});
  [Throws] SVGTransform? consolidate();
  readonly attribute unsigned long length;
};
