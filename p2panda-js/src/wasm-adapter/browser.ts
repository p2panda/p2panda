// eslint-disable-next-line
// @ts-ignore
import wasmBase64 from 'wasm-web/index_bg.wasm';
import wasmInit, * as wasmLib from 'wasm-web';

// The WebAssembly code is encoded to base64 and bundled by Webpack to be able
// to use this library directly in the browser without any further build steps.
export default new Promise<typeof wasmLib>((resolve, reject) => {
  // Decode base64-encoded WebAssembly to bytes and initialize
  const bytes = Uint8Array.from(
    window
      .atob(wasmBase64)
      .split('')
      .map((char) => char.charCodeAt(0)),
  );

  wasmInit(bytes)
    .then(() => {
      resolve(wasmLib);
    })
    .catch((err: Error) => {
      reject(err);
    });
});
