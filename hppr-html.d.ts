/**
 * AUTO-GENERATED FILE. DO NOT EDIT.
 *
 * General HPPR HTML API typings generated from HAVI WebIDL by
 * the libhavi::hppr TypeScript generator.
 */

type QaValue = string | QaValue[] | { [key: string]: QaValue };
type Qa = { [key: string]: QaValue } | null;

interface HpprAddOptions {
  headers?: string | string[];
  data?: Blob | ArrayBuffer | ArrayBufferView | string;
}
interface HpprGreeting {
  repoName: string;
  sessionId: string;
  verifyingKey: string;
  phc?: string | null;
  format: string;
  commands: string[];
  status?: string | null;
  uptime?: string | null;
  version?: string | null;
  backend?: string | null;
}
interface StreamPubOptions {
  key?: string;
  headers?: Record<string, string>;
  maxSegmentSize?: number;
  flushSeq?: string;
}
interface URCSelector {
  type?: string;
  verifyingKey?: string | null;
  tai?: string | null;
  hash?: string | null;
}
interface HpprKeyPair {
  signingKey: string;
  verifyingKey: string;
}
interface HpprClient {
  connect(endpoint: string, identity?: string): Promise<HpprClient>;
  connectRing2Password(endpoint: string, group: string, username: string, password: string): Promise<HpprClient>;
  named(name: string): Promise<HpprClient>;
  envelope(): EnvelopeHpprClient;
  readonly endpoint: string;
  readonly account: string | null;
  readonly group: string | null;
  readonly ring1Name: string | null;
  get(urc: string): Promise<HpprPacket>;
  list(urc: string): Promise<string[]>;
  headers(urc: string): Promise<string[]>;
  tips(urc: string): Promise<string[]>;
  members(urc: string): Promise<string[]>;
  store(packet: HpprPacket): Promise<string[]>;
  detach(hash: string): Promise<void>;
  add(options?: HpprAddOptions): Promise<string[]>;
  hello(): Promise<HpprGreeting>;
  watch(urc: string): WatchSocket;
  streamPub(prefix: string, options?: StreamPubOptions): StreamPub;
  streamSub(prefix: string): StreamSub;
}
declare var HpprClient: {
  prototype: HpprClient;
  connect(endpoint: string, identity?: string): Promise<HpprClient>;
  connectRing2Password(endpoint: string, group: string, username: string, password: string): Promise<HpprClient>;
  named(name: string): Promise<HpprClient>;
};
interface EnvelopeHpprClient {
  connect(endpoint: string, identity?: string): Promise<EnvelopeHpprClient>;
  unpack(): HpprClient;
  readonly endpoint: string;
  readonly account: string | null;
  readonly group: string | null;
  get(urc: string): Promise<HpprResult>;
  list(urc: string): Promise<HpprResult>;
  headers(urc: string): Promise<HpprResult>;
  tips(urc: string): Promise<HpprResult>;
  members(urc: string): Promise<HpprResult>;
  store(packet: HpprPacket): Promise<HpprResult>;
  detach(hash: string): Promise<HpprResult>;
  add(options?: HpprAddOptions): Promise<HpprResult>;
  hello(): Promise<HpprResult>;
  watch(urc: string): WatchSocket;
  streamPub(prefix: string, options?: StreamPubOptions): StreamPub;
  streamSub(prefix: string): StreamSub;
}
declare var EnvelopeHpprClient: {
  prototype: EnvelopeHpprClient;
  connect(endpoint: string, identity?: string): Promise<EnvelopeHpprClient>;
};
interface HpprPacket {
  readonly hash: string;
  readonly type: string;
  getHeader(name: string): string | null;
  getHeaders(name: string): string[];
  headers(): string[];
  customHeaders(): string[];
  readonly group: string | null;
  readonly app: string | null;
  readonly location: string | null;
  readonly tai: string | null;
  taiDate(): object | null;
  readonly coordinate: string | null;
  readonly sealBy: string | null;
  readonly dataLength: number;
  arrayBuffer(): ArrayBuffer;
  blob(): Blob;
  text(): string;
  json(): any;
  raw(): ArrayBuffer;
}
interface HpprResolveResult {
  readonly packet: HpprPacket;
  readonly kind: string;
  readonly contentAuthority: string | null;
}
interface HpprSource {
  readonly client: HpprClient;
  readonly kind: string;
  readonly authority: string | null;
}
interface WatchSocket extends EventTarget {
  readonly CONNECTING: number;
  readonly OPEN: number;
  readonly CLOSING: number;
  readonly CLOSED: number;
  readonly readyState: number;
  readonly urc: string;
  onopen: any;
  onmessage: any;
  onerror: any;
  onclose: any;
  close(): void;
}
interface StreamPub extends EventTarget {
  readonly CONNECTING: number;
  readonly OPEN: number;
  readonly CLOSING: number;
  readonly CLOSED: number;
  readonly readyState: number;
  readonly prefix: string;
  onopen: any;
  onerror: any;
  onclose: any;
  onpacket: any;
  write(data: BufferSource): Promise<void>;
  finishSegment(): void;
  close(): void;
}
interface StreamSub extends EventTarget {
  readonly CONNECTING: number;
  readonly OPEN: number;
  readonly CLOSING: number;
  readonly CLOSED: number;
  readonly readyState: number;
  readonly prefix: string;
  readonly stream: ReadableStream;
  onopen: any;
  onerror: any;
  onclose: any;
  onpacket: any;
  close(): void;
}
interface URC {
  readonly href: string;
  readonly method: string;
  readonly group: string | null;
  readonly app: string | null;
  readonly location: string | null;
  readonly coordinate: string | null;
  readonly isListing: boolean;
  getSelector(): URCSelector | null;
  qa: Qa;
  readonly fragment: string | null;
  join(coordinate: string): URC;
  setListing(isListing: boolean): URC;
}
declare var URC: {
  prototype: URC;
  new (input: string): URC;
};
interface Address {
  href: string;
  readonly scheme: string;
  readonly coordinate: string | null;
  readonly urc: URC;
  group: string | null;
  app: string | null;
  location: string | null;
  readonly isListing: boolean;
  qa: Qa;
  readonly fragment: string | null;
}
declare var Address: {
  prototype: Address;
  new (input: string): Address;
};
interface WindowAddress {
  href: string;
  readonly scheme: string;
  readonly qa: Qa;
  readonly fragment: string | null;
  readonly isListing: boolean;
}
interface HpprWindowAddress extends WindowAddress {
  readonly coordinate: string | null;
  readonly urc: URC;
  group: string | null;
  app: string | null;
  location: string | null;
}
interface FileWindowAddress extends WindowAddress {
  pathname: string;
}
interface HpprResult {
  readonly value: any;
}
interface HpprError extends DOMException {
  readonly type: string;
  readonly detail: string;
  readonly fatal: boolean;
}
declare var HpprError: {
  prototype: HpprError;
  new (errorType: string, detail?: string): HpprError;
};
interface HpprRepoInfo {
  port(): Promise<number>;
  repoPath(): Promise<string>;
  status(): Promise<string>;
}
declare namespace H3 {
  function deriveKeyPair(password: string, name: string, domainKey: string, phc?: string): HpprKeyPair;
  function hash(data: ArrayBuffer | ArrayBufferView | string): string;
  function sign(hash: string, signingKey: string): string;
  function verify(hash: string, signature: string, verifyingKey: string): boolean;
  function generateKey(): HpprKeyPair;
}
interface Window {
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
