import { WebAssembly } from 'wasm-adapter';

// WebAssembly code loaded dynamically from external file
const wasm: Promise<WebAssembly> = import('wasm').then(
  ({ KeyPair, setWasmPanicHook }) => {
    return {
      KeyPair,
      setWasmPanicHook,
    };
  },
);

export default wasm;
