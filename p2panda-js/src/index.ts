// SPDX-License-Identifier: AGPL-3.0-or-later

// This entry point is for NodeJS builds, where the "wasm" module is placed
// right next to the package inside a folder.
//
// Developers do not need to call "initWebAssembly" at all in NodeJS. It is
// only optional to turn on the debugging features.
import { setWasmPanicHook } from './wasm';

/**
 * Depending on the import path and context the WebAssembly code can be
 * initialised here in various ways:
 *
 * 1. NodeJS: No initialisation needed, optionally debug tools get activated
 * for better error messages in wasm code
 *
 * 2. Inline: Initialisation required by calling method, debug tools get
 * activated
 *
 * 3. Slim: Initialisation required by calling method and passing path to
 * '.wasm' file, debug tools get activated
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
