// Defined by WebpackDefinePlugin
declare const BUILD_TARGET_WEB: boolean;

const adapter = BUILD_TARGET_WEB
  ? require('~/wasm-adapter/browser')
  : require('~/wasm-adapter/node');

export default adapter.default;
