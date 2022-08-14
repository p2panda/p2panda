// SPDX-License-Identifier: AGPL-3.0-or-later

import init, { setWasmPanicHook } from '../wasm/web';
import wasmData from '../wasm/web/index_bg.wasm';

export async function initWebAssembly() {
  await init(wasmData);

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
