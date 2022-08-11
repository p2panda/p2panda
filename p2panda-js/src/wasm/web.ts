// SPDX-License-Identifier: AGPL-3.0-or-later

import init, {
  KeyPair,
  OperationFields,
  decodeEntry,
  encodeCreateOperation,
  encodeDeleteOperation,
  encodeUpdateOperation,
  setWasmPanicHook,
  signEncodeEntry,
  verifySignature,
} from 'wasm/web/index.js';
import wasmData from 'wasm/web/index_bg.wasm';

export default async () => {
  await init(wasmData);
};

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
