// SPDX-License-Identifier: AGPL-3.0-or-later

import { setWasmPanicHook } from '../wasm/node';

export async function initWebAssembly() {
  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
