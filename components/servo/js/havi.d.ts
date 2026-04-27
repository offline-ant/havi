/**
 * AUTO-GENERATED FILE. DO NOT EDIT.
 *
 * HAVI-specific typings layered on top of hppr-html.d.ts.
 * Exposes internal helper globals such as window.havi.
 */

/// <reference path="./hppr-html.d.ts" />

interface HaviAdmin {
  readonly client: HpprClient;
  readonly repo: HpprRepoInfo;
}
interface HaviInternal {
  readonly admin: HaviAdmin | null;
}
interface Window {
  readonly havi: HaviInternal | null;
}
