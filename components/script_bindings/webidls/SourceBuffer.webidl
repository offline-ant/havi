/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window]
interface SourceBuffer : EventTarget {
  readonly attribute boolean updating;
  readonly attribute TimeRanges buffered;

  [Throws] undefined appendBuffer(BufferSource data);
  [Throws] undefined remove(double start, double end);
  [Throws] undefined abort();

  attribute EventHandler onupdatestart;
  attribute EventHandler onupdate;
  attribute EventHandler onupdateend;
  attribute EventHandler onerror;
};
