// SPDX-License-Identifier: AGPL-3.0-or-later

// This entry point is for NodeJS builds, where the "wasm" module is placed
// right next to the package inside a folder.
//
// Developers do not need to call "initWebAssembly" at all in NodeJS contexts.
// It is only optional to turn on the debugging features.
import { setWasmPanicHook } from './wasm';

/**
 * Depending on which `p2panda-js` build you chose to import into your project,
 * the WebAssembly code needs to be initialised in different ways:
 *
 * 1. NodeJS: No initialisation needed. You can optionally activate debug tools
 * for better error messages in WebAssembly code by calling `initWebAssembly`.
 *
 * 2. UMD, CJS and ESM builds with inlined WebAssembly code running in the
 * browser: WebAssembly needs to be decoded and initialised by calling
 * `initWebAssembly` once before all other methods. This will also implicitly
 * activate debug tools for better error messages in WebAssembly code.
 *
 * 3. CJS and ESM "slim" builds running in browser: WebAssembly needs to be
 * initialised by providing external "p2panda.wasm" file path as an input
 * when calling `initWebAssembly` methods. This will also implicitly activate
 * debug tools for better error messages in WebAssembly code.
 */
// Passing in an "input" is not required for NodeJS builds. Still we mention it
// here for our TypeDoc outputs, as some developers might want to know how to
// use this method in "slim" build contexts.
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
