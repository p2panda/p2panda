const path = require('path');
const WasmPackPlugin = require('@wasm-tool/wasm-pack-plugin');

// const PATH_DIST = "./build";
const PATH_DIST_WASM = './wasm';
// const PATH_SRC = "./src";
const PATH_SRC_WASM = '../p2panda-rs';

module.exports = () => {
  return {
    entry: {
      app: path.resolve(__dirname, 'src', 'index.ts'),
    },
    output: {
      filename: 'p2panda.js',
    },
    resolve: {
      alias: {
        '~': path.resolve(__dirname, 'src'),
        wasm: path.resolve(__dirname, 'wasm'),
      },
      extensions: ['.js', '.ts', '.tsx'],
    },
    experiments: {
      // Support the new WebAssembly according to the updated specification, it
      // makes a WebAssembly module an async module.
      // See: https://webpack.js.org/configuration/experiments/
      asyncWebAssembly: true,
    },
    module: {
      rules: [
        {
          test: /\.tsx?/,
          exclude: /node_modules/,
          use: [
            {
              loader: 'babel-loader',
            },
            {
              loader: 'eslint-loader',
            },
            {
              loader: 'ts-loader',
            },
          ],
        },
      ],
    },
    plugins: [
      new WasmPackPlugin({
        crateDirectory: path.resolve(__dirname, PATH_SRC_WASM),
        outDir: path.resolve(__dirname, PATH_DIST_WASM),
      }),
    ],
    devtool: 'source-map',
  };
};
