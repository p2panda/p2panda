// SPDX-License-Identifier: AGPL-3.0-or-later

// This entry point is for builds which inline the WebAssembly code as a base64
// string inside the bundled file.
//
// Calling `initWebAssembly` decodes the inline base64 string automatically
// into bytes and initialises WebAssembly.
//
// With this approach developers conveniently do not need to load the .wasm
// file from somewhere external, this is why the `initWebAssembly` method does
// not provide any arguments.
import init, { setWasmPanicHook } from '../wasm/web';
import wasmData from '../wasm/web/index_bg.wasm';

export async function initWebAssembly() {
  // "@rollup/plugin-wasm" changes the signature of wasmData during rollup
  // build process and turns it into a function which returns the decoded
  // bytes from base64 string.
  //
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  await init(wasmData());

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
