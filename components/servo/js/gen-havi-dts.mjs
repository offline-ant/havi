#!/usr/bin/env bun

/**
 * Generate HAVI TypeScript definitions from HAVI WebIDL.
 *
 * Outputs:
 *   - hppr-html.d.ts: libhavi::hppr browser API surface (ordinary pages)
 *   - havi.d.ts: HAVI internal helper augmentations
 *
 * Each file is written to:
 *   - havi/components/servo/js/hppr-html.d.ts
 *   - havi/hppr-html.d.ts
 *   - havi/havi.d.ts
 *
 * This script is invoked by the libhavi build before running
 * `bun x tsc` on libhavi::pages scripts.
 */

import { parse } from "webidl2";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const JS_DIR = resolve(import.meta.dir);
const HAVI_ROOT = resolve(JS_DIR, "../../..");
const WEBIDL_DIR = join(HAVI_ROOT, "components/script_bindings/webidls");

const JS_HPPR_HTML_DTS = join(JS_DIR, "hppr-html.d.ts");
const JS_HAVI_DTS = join(JS_DIR, "havi.d.ts");
const ROOT_HPPR_HTML_DTS = join(HAVI_ROOT, "hppr-html.d.ts");
const ROOT_HAVI_DTS = join(HAVI_ROOT, "havi.d.ts");

const IDL_FILES = [
  "HpprClient.webidl",
  "EnvelopeHpprClient.webidl",
  "HpprPacket.webidl",
  "HpprResolveResult.webidl",
  "HpprSource.webidl",
  "WatchSocket.webidl",
  "StreamPub.webidl",
  "StreamSub.webidl",
  "URC.webidl",
  "Address.webidl",
  "WindowAddress.webidl",
  "HpprWindowAddress.webidl",
  "FileWindowAddress.webidl",
  "HpprResult.webidl",
  "HpprError.webidl",
  "H3.webidl",
];

const HPPR_HTML_PREAMBLE = `/**
 * AUTO-GENERATED FILE. DO NOT EDIT.
 *
 * General HPPR HTML API typings generated from HAVI WebIDL by
 * the libhavi::hppr TypeScript generator.
 */

type QaValue = string | QaValue[] | { [key: string]: QaValue };
type Qa = { [key: string]: QaValue } | null;
\n`;

const HAVI_PREAMBLE = `/**
 * AUTO-GENERATED FILE. DO NOT EDIT.
 *
 * HAVI-specific typings layered on top of hppr-html.d.ts.
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
      const qaOverride =
        (def.name === "URC" || def.name === "Address" || def.name === "WindowAddress") &&
        member.name === "qa";
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
      const args = (member.arguments || [])
        .map(a => `${a.name}${a.optional ? "?" : ""}: ${mapType(a.idlType)}`)
        .join(", ");
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

const hpprHtmlWanted = new Set([
  "HpprAddOptions",
  "HpprGreeting",
  "StreamPubOptions",
  "URCSelector",
  "HpprClient",
  "EnvelopeHpprClient",
  "HpprPacket",
  "HpprResolveResult",
  "HpprSource",
  "WatchSocket",
  "StreamPub",
  "StreamSub",
  "URC",
  "Address",
  "WindowAddress",
  "HpprWindowAddress",
  "FileWindowAddress",
  "HpprResult",
  "HpprError",
  "HpprKeyPair",
  "H3",
]);

let hpprHtml = HPPR_HTML_PREAMBLE;
for (const def of allDefs) {
  if (def.type === "dictionary" && hpprHtmlWanted.has(def.name)) hpprHtml += emitDictionary(def);
}
for (const def of allDefs) {
  if (def.type === "interface" && hpprHtmlWanted.has(def.name)) {
    hpprHtml += emitInterface(def);
    hpprHtml += emitCtorAndStatics(def);
  }
}
for (const def of allDefs) {
  if (def.type === "namespace" && hpprHtmlWanted.has(def.name)) hpprHtml += emitNamespace(def);
}

hpprHtml += `interface Window {
  readonly address: WindowAddress | null;
  readonly source: HpprSource | null;
  readonly packet: HpprPacket | null;
  resolve(input: string): Promise<HpprResolveResult>;
}

interface Document {
  readonly packet: HpprPacket | null;
  readonly URC: string | null;
  readonly URL: string;
  readonly documentURI: string;
}
`;

let havi = `${HAVI_PREAMBLE}/// <reference path="./hppr-html.d.ts" />
`;

/** Write file only if content differs (preserves mtime for cargo caching). */
function writeIfChanged(path, content) {
  mkdirSync(dirname(path), { recursive: true });

  let previous = null;
  try {
    previous = readFileSync(path, "utf8");
  } catch {
    // missing file is fine
  }
  if (previous !== content) {
    writeFileSync(path, content);
  }
}

writeIfChanged(JS_HPPR_HTML_DTS, hpprHtml);
writeIfChanged(JS_HAVI_DTS, havi);
writeIfChanged(ROOT_HPPR_HTML_DTS, hpprHtml);
writeIfChanged(ROOT_HAVI_DTS, havi);
