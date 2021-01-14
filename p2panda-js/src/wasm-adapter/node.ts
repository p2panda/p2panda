type WasmNode = typeof import('wasm-node');

const wasm: Promise<WasmNode> = import('wasm-node').then((lib) => {
  return lib;
});

export default wasm;
