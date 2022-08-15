// SPDX-License-Identifier: AGPL-3.0-or-later

import fetch from 'node-fetch';
import Headers from 'fetch-headers';

import { setWasmPanicHook } from '../wasm/node';

// @TODO: Remove `fetch` together with `apollo/client`
// https://github.com/p2panda/p2panda/issues/433
if (!globalThis.fetch) {
  // @ts-expect-error we trust that `node-fetch` is a suitable replacement
  globalThis.fetch = fetch;
  globalThis.Headers = Headers;
}

export async function initWebAssembly() {
  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
