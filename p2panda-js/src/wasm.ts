// SPDX-License-Identifier: AGPL-3.0-or-later

type WebAssembly = typeof import('wasm');

// Helper to extract resolved promise
type Resolved<T> = T extends PromiseLike<infer U> ? Resolved<U> : T;

// p2panda is exported without WebAssembly utilities
export type P2Panda = Omit<
  Resolved<WebAssembly>,
  'setWasmPanicHook' | 'init' | 'default'
>;

// This promise makes sure we only load the p2panda-rs library once even when
// it was used multiple times (singleton). Also it sets the panic hook
// automatically for better debugging.
const wasm = new Promise<P2Panda>((resolve, reject) => {
  import('wasm')
    .then((lib) => lib.default)
    .then(({ setWasmPanicHook, ...rest }) => {
      // Set panic hooks for better logging of wasm errors. See:
      // https://github.com/rustwasm/console_error_panic_hook
      setWasmPanicHook();

      resolve(rest);
    })
    .catch((err: Error) => {
      reject(err);
    });
});

export default wasm;
