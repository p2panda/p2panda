// SPDX-License-Identifier: AGPL-3.0-or-later

type WasmNode = typeof import('wasm-node');

const wasm: Promise<WasmNode> = import('wasm-node').then((lib) => {
  return lib;
});

export default wasm;
