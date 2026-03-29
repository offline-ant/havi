/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface SVGNumberList {
  readonly attribute unsigned long numberOfItems;
  [Throws] undefined clear();
  [Throws] SVGNumber initialize(SVGNumber newItem);
  [Throws] getter SVGNumber getItem(unsigned long index);
  [Throws] SVGNumber insertItemBefore(SVGNumber newItem, unsigned long index);
  [Throws] SVGNumber replaceItem(SVGNumber newItem, unsigned long index);
  [Throws] SVGNumber removeItem(unsigned long index);
  [Throws] SVGNumber appendItem(SVGNumber newItem);
  readonly attribute unsigned long length;
};
