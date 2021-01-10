// eslint-disable-next-line
// @ts-ignore
import wasmBase64 from 'wasm/index_bg.wasm';
import wasmInit, { setWasmPanicHook, KeyPair } from 'wasm';

// This method makes sure we only load the p2panda-rs library once even when it
// was used multiple times (singleton). Also it sets the panic hook
// automatically for better debugging.
//
// The WebAssembly code is encoded to base64 and bundled by Webpack to be able
// to use this library straight from the browser.
export default new Promise((resolve) => {
  // Decode base64-encoded WebAssembly to bytes and initialize
  const bytes = Uint8Array.from(atob(wasmBase64), (c) => c.charCodeAt(0));

  wasmInit(bytes).then(() => {
    // Set panic hooks for better logging of wasm errors. See:
    // https://github.com/rustwasm/console_error_panic_hook
    setWasmPanicHook();

    resolve({
      KeyPair,
    });
  });
});
