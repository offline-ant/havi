#!/usr/bin/env bun

/**
 * Generate HAVI TypeScript definitions from HAVI WebIDL.
 *
 * Outputs:
 *   - hppr-html.d.ts: general HPPR browser API surface (hppr:// pages)
 *   - havi.d.ts: HAVI-only augmentation (ring0 and related globals)
 *
 * Each file is written to:
 *   - havi/havi-protocols/src/js/hppr-html.d.ts
 *   - havi/hppr-html.d.ts
 *   - havi/havi.d.ts
 *
 * This script is invoked by havi-protocols/build.rs before running
 * `bun x tsc` on protocol page scripts.
 */

import { parse } from "webidl2";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const JS_DIR = resolve(import.meta.dir);
const HAVI_ROOT = resolve(JS_DIR, "../../..");
const WEBIDL_DIR = join(HAVI_ROOT, "components/script_bindings/webidls");

const JS_HPPR_HTML_DTS = join(JS_DIR, "hppr-html.d.ts");
const ROOT_HPPR_HTML_DTS = join(HAVI_ROOT, "hppr-html.d.ts");
const ROOT_HAVI_DTS = join(HAVI_ROOT, "havi.d.ts");

const IDL_FILES = [
  "HpprClient.webidl",
  "EnvelopeHpprClient.webidl",
  "HpprPacket.webidl",
  "WatchSocket.webidl",
  "StreamIn.webidl",
  "StreamOut.webidl",
  "URC.webidl",
  "Address.webidl",
  "HpprResult.webidl",
  "HpprError.webidl",
  "HpprRepoInfo.webidl",
  "H3.webidl",
];

const HPPR_HTML_PREAMBLE = `/**
 * AUTO-GENERATED FILE. DO NOT EDIT.
 *
 * General HPPR HTML API typings generated from HAVI WebIDL by
 * havi-protocols/src/js/gen-havi-dts.mjs.
 */

type QaValue = string | QaValue[] | { [key: string]: QaValue };
type Qa = { [key: string]: QaValue } | null;
\n`;

const HAVI_PREAMBLE = `/**
 * AUTO-GENERATED FILE. DO NOT EDIT.
 *
 * HAVI-specific typings layered on top of hppr-html.d.ts.
 * Exposes non-standard globals such as window.ring0.
 */\n\n`;

function mapType(idlType) {
  if (!idlType) return "void";
  if (typeof idlType === "string") return mapTypeName(idlType);

  if (idlType.union) {
    const union = idlType.idlType.map(mapType).join(" | ");
    return idlType.nullable ? `(${union}) | null` : union;
  }

  if (idlType.generic === "Promise") return `Promise<${mapType(idlType.idlType[0])}>`;
  if (idlType.generic === "sequence") return `${mapType(idlType.idlType[0])}[]`;
  if (idlType.generic === "record") return `Record<${mapType(idlType.idlType[0])}, ${mapType(idlType.idlType[1])}>`;

  let base = mapTypeName(idlType.idlType);
  if (idlType.nullable) base = `${base} | null`;
  return base;
}

function mapTypeName(name) {
  if (Array.isArray(name)) return mapType(name[0]);
  switch (name) {
    case "undefined": return "void";
    case "DOMString":
    case "USVString":
    case "ByteString":
    case "string": return "string";
    case "boolean": return "boolean";
    case "double":
    case "float":
    case "unrestricted double":
    case "unrestricted float":
    case "short":
    case "unsigned short":
    case "long":
    case "unsigned long":
    case "long long":
    case "unsigned long long":
    case "octet":
    case "byte": return "number";
    case "object": return "object";
    case "any": return "any";
    case "EventHandler": return "any";
    default: return name;
  }
}

function emitDictionary(def) {
  const lines = [`interface ${def.name} {`];
  for (const member of def.members || []) {
    lines.push(`  ${member.name}${member.required ? "" : "?"}: ${mapType(member.idlType)};`);
  }
  lines.push("}\n");
  return lines.join("\n");
}

function emitInterface(def) {
  const extendsPart = def.inheritance ? ` extends ${def.inheritance}` : "";
  const lines = [`interface ${def.name}${extendsPart} {`];

  for (const member of def.members || []) {
    if (member.type === "const") {
      lines.push(`  readonly ${member.name}: ${mapType(member.idlType)};`);
    } else if (member.type === "attribute") {
      const qaOverride = (def.name === "URC" || def.name === "Address") && member.name === "qa";
      const memberType = qaOverride ? "Qa" : mapType(member.idlType);
      lines.push(`  ${member.readonly ? "readonly " : ""}${member.name}: ${memberType};`);
    } else if (member.type === "operation") {
      const returnOverride = def.name === "HpprClient" && {
        get: "Promise<HpprPacket>",
        list: "Promise<string[]>",
        headers: "Promise<string[]>",
        tips: "Promise<string[]>",
        members: "Promise<string[]>",
        store: "Promise<string[]>",
        add: "Promise<string[]>",
        hello: "Promise<HpprGreeting>",
        detach: "Promise<void>",
      }[member.name || ""];
      const ret = returnOverride || mapType(member.idlType);
      const args = (member.arguments || []).map(a => `${a.name}${a.optional ? "?" : ""}: ${mapType(a.idlType)}`).join(", ");
      const name = member.name || "__call";
      lines.push(`  ${name}(${args}): ${ret};`);
    }
  }

  lines.push("}\n");
  return lines.join("\n");
}

function emitCtorAndStatics(def) {
  const ctors = (def.members || []).filter(m => m.type === "constructor");
  const statics = (def.members || []).filter(m => m.type === "operation" && m.special === "static");
  if (ctors.length === 0 && statics.length === 0) return "";

  const lines = [`declare var ${def.name}: {`, `  prototype: ${def.name};`];
  for (const ctor of ctors) {
    const args = (ctor.arguments || []).map(a => `${a.name}${a.optional ? "?" : ""}: ${mapType(a.idlType)}`).join(", ");
    lines.push(`  new (${args}): ${def.name};`);
  }
  for (const s of statics) {
    const args = (s.arguments || []).map(a => `${a.name}${a.optional ? "?" : ""}: ${mapType(a.idlType)}`).join(", ");
    lines.push(`  ${s.name}(${args}): ${mapType(s.idlType)};`);
  }
  lines.push("};\n");
  return lines.join("\n");
}

function emitNamespace(def) {
  const lines = [`declare namespace ${def.name} {`];
  for (const member of def.members || []) {
    if (member.type === "operation") {
      const args = (member.arguments || []).map(a => `${a.name}${a.optional ? "?" : ""}: ${mapType(a.idlType)}`).join(", ");
      lines.push(`  function ${member.name}(${args}): ${mapType(member.idlType)};`);
    }
  }
  lines.push("}\n");
  return lines.join("\n");
}

const allDefs = [];
for (const file of IDL_FILES) {
  const src = readFileSync(join(WEBIDL_DIR, file), "utf8");
  const defs = parse(src, { sourceName: file });
  for (const d of defs) allDefs.push(d);
}

const wanted = new Set([
  "HpprRepoOptions",
  "HpprAddOptions",
  "HpprGreeting",
  "StreamInOptions",
  "URCSelector",
  "HpprClient",
  "EnvelopeHpprClient",
  "HpprPacket",
  "WatchSocket",
  "StreamIn",
  "StreamOut",

  "URC",
  "Address",
  "HpprResult",
  "HpprError",
  "HpprRepoInfo",
  "HpprKeyPair",
  "H3",
]);

let hpprHtml = HPPR_HTML_PREAMBLE;
for (const def of allDefs) {
  if (def.type === "dictionary" && wanted.has(def.name)) hpprHtml += emitDictionary(def);
}
for (const def of allDefs) {
  if (def.type === "interface" && wanted.has(def.name)) {
    hpprHtml += emitInterface(def);
    hpprHtml += emitCtorAndStatics(def);
  }
}

for (const def of allDefs) {
  if (def.type === "namespace" && wanted.has(def.name)) hpprHtml += emitNamespace(def);
}

// General hppr:// globals.
// General hppr:// globals.
hpprHtml += `interface Window {
  readonly address: Address;
  readonly home: HpprClient;
  readonly route: HpprClient | null;
  readonly packet: HpprPacket | null;
}

interface Document {
  readonly packet: HpprPacket | null;
}
`;

// HAVI layer: only non-standard global augmentations.
const havi = `${HAVI_PREAMBLE}/// <reference path="./hppr-html.d.ts" />

interface Window {
  readonly ring0: HpprClient | null;
}
`;

/** Write file only if content differs (preserves mtime for cargo caching). */
function writeIfChanged(path, content) {
  mkdirSync(dirname(path), { recursive: true });
  try {
    if (readFileSync(path, "utf8") === content) return false;
  } catch {}
  writeFileSync(path, content);
  return true;
}

writeIfChanged(JS_HPPR_HTML_DTS, hpprHtml);
writeIfChanged(ROOT_HPPR_HTML_DTS, hpprHtml);
writeIfChanged(ROOT_HAVI_DTS, havi);

console.log(`generated ${JS_HPPR_HTML_DTS}`);
console.log(`generated ${ROOT_HPPR_HTML_DTS}`);
console.log(`generated ${ROOT_HAVI_DTS}`);
