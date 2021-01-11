// eslint-disable-next-line
// @ts-ignore
import wasmBase64 from 'wasm/index_bg.wasm';
import wasmInit, { setWasmPanicHook, KeyPair } from 'wasm';

import { WebAssembly } from 'wasm-adapter';

// The WebAssembly code is encoded to base64 and bundled by Webpack to be able
// to use this library directly in the browser without any further build steps.
const wasm: Promise<WebAssembly> = new Promise((resolve, reject) => {
  // Decode base64-encoded WebAssembly to bytes and initialize
  const bytes: BufferSource = Uint8Array.from(
    window
      .atob(wasmBase64)
      .split('')
      .map((char: string) => char.charCodeAt(0)),
  );

  // eslint-disable-next-line
  // @ts-ignore
  wasmInit(bytes)
    .then(() => {
      resolve({
        KeyPair,
        setWasmPanicHook,
      });
    })
    .catch((err: Error) => {
      reject(err);
    });
});

export default wasm;
