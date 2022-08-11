// SPDX-License-Identifier: AGPL-3.0-or-later

type Wasm = typeof import('wasm/node');

type WasmAdapter = {
  default: () => Promise<void>,
} & Wasm;

// Defined by webpack.DefinePlugin
declare const BUILD_TARGET_WEB: boolean;

const wasmAdapter: WasmAdapter = BUILD_TARGET_WEB
  ? require('~/wasm/web')
  : require('~/wasm/node');

const {
  default: init,
  setWasmPanicHook,
  ...rest
} = wasmAdapter;

export default async () => {
  await init();

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
};

module.exports = rest;
