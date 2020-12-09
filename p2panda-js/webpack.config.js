const path = require('path');
const WasmPackPlugin = require('@wasm-tool/wasm-pack-plugin');

const PATH_DIST = './lib';
const PATH_DIST_WASM = './wasm';
const PATH_SRC = './src';
const PATH_SRC_WASM = '../p2panda-rs';

function getPath(...args) {
  return path.resolve(__dirname, ...args);
}

module.exports = () => {
  return {
    entry: getPath(PATH_SRC, 'index.ts'),
    output: {
      filename: 'index.js',
      path: getPath(PATH_DIST),
    },
    resolve: {
      alias: {
        '~': getPath(PATH_SRC),
        wasm: getPath(PATH_DIST_WASM),
      },
      extensions: ['.ts'],
    },
    experiments: {
      // Use `syncWebAssembly` for now as the new WebAssembly
      // `asyncWebAssembly` import does not work due to a bug in wasm-bindgen
      // builds. See: https://github.com/rustwasm/wasm-bindgen/issues/2343 and
      // https://webpack.js.org/configuration/experiments/
      syncWebAssembly: true,
    },
    module: {
      rules: [
        {
          test: /\.ts/,
          exclude: /node_modules/,
          use: [
            {
              loader: 'babel-loader',
            },
            {
              loader: 'ts-loader',
            },
            {
              loader: 'eslint-loader',
            },
          ],
        },
      ],
    },
    plugins: [
      new WasmPackPlugin({
        crateDirectory: getPath(PATH_SRC_WASM),
        outDir: getPath(PATH_DIST_WASM),
        pluginLogLevel: 'error',
      }),
    ],
    devtool: 'source-map',
    stats: 'minimal',
  };
};
