// SPDX-License-Identifier: AGPL-3.0-or-later

// Defined by webpack.DefinePlugin
declare const BUILD_TARGET_WEB: boolean;

const wasmAdapter = BUILD_TARGET_WEB
  ? require('~/wasm/web')
  : require('~/wasm/node');

const { default: init, setWasmPanicHook, ...rest } = wasmAdapter;
const {
  KeyPair,
  OperationFields,
  decodeEntry,
  encodeCreateOperation,
  encodeDeleteOperation,
  encodeUpdateOperation,
  signEncodeEntry,
  verifySignature,
} = rest;

export default async () => {
  await init();

  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();

  return rest;
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
