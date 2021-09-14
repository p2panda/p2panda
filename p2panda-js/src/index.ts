// SPDX-License-Identifier: AGPL-3.0-or-later

import fetch from 'node-fetch';
import Headers from 'fetch-headers';

if (!globalThis.fetch) {
  // @ts-expect-error we trust that `node-fetch` is a suitable replacement
  globalThis.fetch = fetch;
  globalThis.Headers = Headers;
}

export { createKeyPair, recoverKeyPair } from './keyPair';
export { default as Session } from './session';
export { default as wasm } from './wasm';

export * from './types';
