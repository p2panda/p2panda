import initializeWasm from 'wasm-adapter';

import { P2Panda } from '~/index.d.ts';

// This promise makes sure we only load the p2panda-rs library once even when
// it was used multiple times (singleton). Also it sets the panic hook
// automatically for better debugging.
const wasm: Promise<P2Panda> = new Promise((resolve, reject) => {
  initializeWasm
    .then(({ setWasmPanicHook, KeyPair }) => {
      // Set panic hooks for better logging of wasm errors. See:
      // https://github.com/rustwasm/console_error_panic_hook
      setWasmPanicHook();

      resolve({
        KeyPair,
      });
    })
    .catch((err: Error) => {
      reject(err);
    });
});

export default wasm;
