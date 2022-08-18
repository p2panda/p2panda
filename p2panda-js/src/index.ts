// SPDX-License-Identifier: AGPL-3.0-or-later

import { setWasmPanicHook } from '../wasm/node';

/**
 * Depending on the setup the WebAssembly code can be initialised here in
 * various ways:
 *
 * 1. NodeJS: No initialisation needed, but debug tools get activated for
 * better error messages in wasm code
 * 2. Browser: Initialisation required before using library by calling
 * method, debug tools get activated
 * 3. Browser (slim): Initialisation required before using library by
 * calling method and passing in path to '.wasm' file, debug tools get
 * activated
 */
// We mention the input here for nicer typedoc outputs, as some developers
// might want to know how they can use this method in browser contexts
export async function initWebAssembly(input?: URL | WebAssembly.Module) {
  if (input) {
    console.warn(
      'No input needs to be passed to `initWebAssembly` when in NodeJS contexts',
    );
  }

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
