/**
 * AUTO-GENERATED FILE. DO NOT EDIT.
 *
 * General HPPR HTML API typings generated from HAVI WebIDL by
 * havi-protocols/src/js/gen-havi-dts.mjs.
 */

type QaValue = string | QaValue[] | { [key: string]: QaValue };
type Qa = { [key: string]: QaValue } | null;

interface HpprAddOptions {
  headers?: string | string[];
  data?: Blob | ArrayBuffer | ArrayBufferView | string;
}
interface HpprRepoOptions {
  role?: string;
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
interface StreamInOptions {
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
  home(options?: HpprRepoOptions): Promise<HpprClient>;
  connect(endpoint: string, identity?: string): Promise<HpprClient>;
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
  readonly repo: HpprRepoInfo | null;
  watch(urc: string): WatchSocket;
  streamIn(prefix: string, options?: StreamInOptions): StreamIn;
  streamOut(prefix: string): StreamOut;
}
declare var HpprClient: {
  prototype: HpprClient;
  home(options?: HpprRepoOptions): Promise<HpprClient>;
  connect(endpoint: string, identity?: string): Promise<HpprClient>;
};
interface EnvelopeHpprClient {
  home(): Promise<EnvelopeHpprClient>;
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
  readonly repo: HpprRepoInfo | null;
  watch(urc: string): WatchSocket;
  streamIn(prefix: string, options?: StreamInOptions): StreamIn;
  streamOut(prefix: string): StreamOut;
}
declare var EnvelopeHpprClient: {
  prototype: EnvelopeHpprClient;
  home(): Promise<EnvelopeHpprClient>;
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
interface StreamIn extends EventTarget {
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
interface StreamOut extends EventTarget {
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
  scheme: string;
  endpoint: string | null;
  readonly coordinate: string | null;
  readonly urc: URC;
  group: string | null;
  app: string | null;
  location: string | null;
  readonly isListing: boolean;
  readonly hasDirectEndpoint: boolean;
  qa: Qa;
  readonly fragment: string | null;
}
declare var Address: {
  prototype: Address;
  new (input: string): Address;
};
interface HpprResult {
  readonly value: any;
  readonly responseEnvelope: HpprPacket | null;
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
  readonly address: Address;
  readonly home: HpprClient;
  readonly route: HpprClient | null;
  readonly packet: HpprPacket | null;
}

interface Document {
  readonly packet: HpprPacket | null;
}
