/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface MediaSource : EventTarget {
  constructor();

  static boolean isTypeSupported(DOMString type);

  readonly attribute SourceBufferList sourceBuffers;
  readonly attribute SourceBufferList activeSourceBuffers;
  readonly attribute DOMString readyState;
  [Throws] attribute unrestricted double duration;

  [Throws] SourceBuffer addSourceBuffer(DOMString type);
  [Throws] undefined endOfStream();

  attribute EventHandler onsourceopen;
  attribute EventHandler onsourceended;
  attribute EventHandler onsourceclose;
};
