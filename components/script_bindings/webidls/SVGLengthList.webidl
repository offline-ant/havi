/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface SVGLengthList {
  readonly attribute unsigned long numberOfItems;
  [Throws] undefined clear();
  [Throws] SVGLength initialize(SVGLength newItem);
  [Throws] getter SVGLength getItem(unsigned long index);
  [Throws] SVGLength insertItemBefore(SVGLength newItem, unsigned long index);
  [Throws] SVGLength replaceItem(SVGLength newItem, unsigned long index);
  [Throws] SVGLength removeItem(unsigned long index);
  [Throws] SVGLength appendItem(SVGLength newItem);
  [Throws] setter undefined (unsigned long index, SVGLength newItem);

  readonly attribute unsigned long length;
};
