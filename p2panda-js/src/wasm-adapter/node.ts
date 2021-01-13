type WasmNode = typeof import('wasm-node') & { target: string };

const wasm: Promise<WasmNode> = import('wasm-node').then((lib) => {
  return { target: 'node', ...lib };
});

export default wasm;
