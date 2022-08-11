// SPDX-License-Identifier: AGPL-3.0-or-later

import {
  KeyPair,
  OperationFields,
  decodeEntry,
  encodeCreateOperation,
  encodeDeleteOperation,
  encodeUpdateOperation,
  setWasmPanicHook,
  signEncodeEntry,
  verifySignature,
} from 'wasm/node/index.js';

export {
  KeyPair,
  OperationFields,
  decodeEntry,
  encodeCreateOperation,
  encodeDeleteOperation,
  encodeUpdateOperation,
  setWasmPanicHook,
  signEncodeEntry,
  verifySignature,
};

export default async () => {
  // No need to initialize any WebAssembly for NodeJS
  return;
};
