import fetch from 'node-fetch';
import Headers from 'fetch-headers';

export type Resolved<T> = T extends PromiseLike<infer U> ? Resolved<U> : T;

if (!globalThis.fetch) {
  globalThis.fetch = fetch;
  globalThis.Headers = Headers;
}

export { default as Session } from './session';
export { default as wasm } from './wasm';
