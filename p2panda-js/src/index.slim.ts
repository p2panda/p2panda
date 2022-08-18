// SPDX-License-Identifier: AGPL-3.0-or-later

// This entry point is for builds which import the WebAssembly code as an
// external file to allow for smaller file sizes and skipping the decoding
// step.
//
// Developers do need to specify an path of the '.wasm' file in the
// `initWebAssembly` method.
import init, { InitInput, setWasmPanicHook } from '../wasm/web';

export async function initWebAssembly(input: InitInput) {
  await init(input);

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
