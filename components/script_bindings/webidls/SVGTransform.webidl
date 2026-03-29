/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface SVGTransform {
  const unsigned short SVG_TRANSFORM_UNKNOWN = 0;
  const unsigned short SVG_TRANSFORM_MATRIX = 1;
  const unsigned short SVG_TRANSFORM_TRANSLATE = 2;
  const unsigned short SVG_TRANSFORM_SCALE = 3;
  const unsigned short SVG_TRANSFORM_ROTATE = 4;
  const unsigned short SVG_TRANSFORM_SKEWX = 5;
  const unsigned short SVG_TRANSFORM_SKEWY = 6;

  readonly attribute unsigned short type;
  [SameObject] readonly attribute DOMMatrix matrix;
  readonly attribute float angle;

  [Throws] undefined setMatrix(optional DOMMatrix2DInit matrix = {});
  [Throws] undefined setTranslate(float tx, float ty);
  [Throws] undefined setScale(float sx, float sy);
  [Throws] undefined setRotate(float angle, float cx, float cy);
  [Throws] undefined setSkewX(float angle);
  [Throws] undefined setSkewY(float angle);
};
