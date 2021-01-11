const wasm: Promise<typeof import('wasm-node') & { target: string }> = import(
  'wasm-node'
).then((lib) => {
  return { target: 'node', ...lib };
});

export default wasm;
