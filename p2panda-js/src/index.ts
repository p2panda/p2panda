// SPDX-License-Identifier: AGPL-3.0-or-later

import fetch from 'node-fetch';
import Headers from 'fetch-headers';

import { setWasmPanicHook } from '../wasm/node';

if (!globalThis.fetch) {
  // @ts-expect-error we trust that `node-fetch` is a suitable replacement
  globalThis.fetch = fetch;
  globalThis.Headers = Headers;
}

export { createKeyPair, recoverKeyPair } from './identity';
export { Session } from './session';

export async function initWebAssembly() {
  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}
