// SPDX-License-Identifier: AGPL-3.0-or-later

// Defined by webpack.DefinePlugin
declare const BUILD_TARGET_WEB: boolean;

const wasmAdapter = BUILD_TARGET_WEB
  ? require('~/wasm/web')
  : require('~/wasm/node');

const {
  KeyPair,
  OperationFields,
  decodeEntry,
  default: init,
  encodeCreateOperation,
  encodeDeleteOperation,
  encodeUpdateOperation,
  setWasmPanicHook,
  signEncodeEntry,
  verifySignature,
} = wasmAdapter;

export default async () => {
  await init();

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();
};

export {
  KeyPair,
  OperationFields,
  decodeEntry,
  encodeCreateOperation,
  encodeDeleteOperation,
  encodeUpdateOperation,
  signEncodeEntry,
  verifySignature,
};
