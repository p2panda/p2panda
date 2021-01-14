import { initializeWasm } from '~/wasm-adapter';

// Helper to extract resolved promise
type Resolved<T> = T extends PromiseLike<infer U> ? Resolved<U> : T;

// p2panda is exported without web assembly utilities
type P2Panda = Omit<
  Resolved<typeof initializeWasm>,
  'setWasmPanicHook' | 'init' | 'default'
>;

// This promise makes sure we only load the p2panda-rs library once even when
// it was used multiple times (singleton). Also it sets the panic hook
// automatically for better debugging.
const wasm = new Promise<P2Panda>((resolve, reject) => {
  initializeWasm
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
