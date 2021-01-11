import { KeyPair as BumBum, setWasmPanicHook } from 'wasm';

declare module 'wasm-adapter' {
  export type KeyPair = typeof BumBum;

  export interface WebAssembly {
    KeyPair: BumBum;
    setWasmPanicHook: setWasmPanicHook;
  }

  declare const promisedWasm: Promise<WebAssembly>;

  export default promisedWasm;
}
