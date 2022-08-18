// SPDX-License-Identifier: AGPL-3.0-or-later

// This entry point is for builds which inline the WebAssembly code as a base64
// string inside the file.
//
// Developers do not need to load the .wasm file from somewhere external like
// this, this is why the `initWebAssembly` method does not provide any
// arguments.
import init, { setWasmPanicHook } from '../wasm/web';
import wasmData from '../wasm/web/index_bg.wasm';

export async function initWebAssembly() {
  await init(wasmData);

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
