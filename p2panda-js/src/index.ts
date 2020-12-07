const wasm = import('wasm').then(({ setWasmPanicHook, ...rest }) => {
  // Set panic hooks for better logging of wasm errors. See:
  // https://github.com/rustwasm/console_error_panic_hook
  setWasmPanicHook();

  return rest;
});

// This method makes sure we only load the p2panda-rs library once even when it was
// used multiple times (singleton). Also it sets the panic hook automatically
// for better debugging.
wasm.then(({ KeyPair }) => {
  console.log(new KeyPair());
});
