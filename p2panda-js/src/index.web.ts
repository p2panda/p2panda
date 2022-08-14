// SPDX-License-Identifier: AGPL-3.0-or-later

import init, { InitInput, setWasmPanicHook } from '../wasm/web';

export async function initWebAssembly(input: InitInput) {
  await init(input);

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
}

export * from './p2panda';
