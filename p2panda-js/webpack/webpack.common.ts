// Helper method to get absolute path of file or folder
import * as path from 'path';
import * as webpack from 'webpack';

import WasmPackPlugin from '@wasm-tool/wasm-pack-plugin';

export const PATH_DIST = '../lib';
export const PATH_DIST_WASM_WEB = '../wasm-web';
export const PATH_DIST_WASM_NODE = '../wasm-node';
export const PATH_SRC = '../src';
export const PATH_SRC_WASM = '../../p2panda-rs';

export function getPath(...args: Array<string>): string {
  return path.resolve(__dirname, ...args);
}

export const tsRule = {
  test: /\.ts/,
  exclude: /node_modules/,
  use: [
    {
      loader: 'babel-loader',
    },
    {
      loader: 'ts-loader',
      options: {
        onlyCompileBundledFiles: true,
        configFile: 'tsconfig.json',
      },
    },
    {
      loader: 'eslint-loader',
    },
  ],
};

// Returns WasmPackPlugin instance with configured target
export function getWasmPlugin(
  target = 'bundler',
): webpack.WebpackPluginInstance {
  return new WasmPackPlugin({
    extraArgs: `--target ${target} --mode normal`,
    crateDirectory: getPath(PATH_SRC_WASM),
    outDir:
      target === 'web'
        ? getPath(PATH_DIST_WASM_WEB)
        : getPath(PATH_DIST_WASM_NODE),
    pluginLogLevel: 'error',
  }) as webpack.WebpackPluginInstance;
}

// Base Webpack configuration
const config: webpack.Configuration = {
  entry: {
    index: getPath(PATH_SRC, 'index.ts'),
  },
  output: {
    path: getPath(PATH_DIST),
    libraryTarget: 'umd',
  },
  resolve: {
    extensions: ['.ts'],
    alias: {
      '~': getPath(PATH_SRC),
      'wasm-web': getPath(PATH_DIST_WASM_WEB),
      'wasm-node': getPath(PATH_DIST_WASM_NODE),
    },
  },
  module: {
    rules: [tsRule],
  },
  devtool: 'source-map',
  stats: 'minimal',
};

export default config;
