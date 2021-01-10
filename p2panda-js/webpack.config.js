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
    entry: {
      index: getPath(PATH_SRC, 'index.ts'),
    },
    output: {
      path: getPath(PATH_DIST),
      libraryTarget: 'umd',
    },
    resolve: {
      alias: {
        '~': getPath(PATH_SRC),
        wasm: getPath(PATH_DIST_WASM),
      },
      extensions: ['.ts'],
    },
    module: {
      rules: [
        {
          test: /\.wasm/,
          exclude: /node_modules/,
          use: [
            {
              loader: 'url-loader',
              options: {
                generator: (content, mimetype, encoding) => {
                  return content.toString(encoding);
                },
              },
            },
          ],
        },
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
        extraArgs: '--target web',
        crateDirectory: getPath(PATH_SRC_WASM),
        outDir: getPath(PATH_DIST_WASM),
        pluginLogLevel: 'error',
      }),
    ],
    devtool: 'source-map',
    stats: 'minimal',
  };
};
