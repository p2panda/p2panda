// SPDX-License-Identifier: AGPL-3.0-or-later

import * as path from 'path';

import * as webpack from 'webpack';
import WasmPackPlugin from '@wasm-tool/wasm-pack-plugin';

export const DIR_DIST = 'lib';
export const DIR_SRC = 'src';
export const DIR_WASM = 'wasm';
export const DIR_WASM_SRC = 'p2panda-rs';

// Helper method to get absolute path of file or folder
export function getPath(...args: Array<string>): string {
  return path.resolve(__dirname, '..', ...args);
}

// Helper method to create a `wasm-pack` plugin instance
export function getWasmPlugin(
  target: 'nodejs' | 'web' | 'bundler',
): WasmPackPlugin {
  return new WasmPackPlugin({
    crateDirectory: getPath('..', DIR_WASM_SRC),
    outDir: getPath(DIR_WASM),
    extraArgs: `--target ${target}`,
    pluginLogLevel: 'error',
  });
}

// Base Webpack configuration
const config: webpack.Configuration = {
  entry: {
    index: getPath(DIR_SRC, 'index.ts'),
  },
  output: {
    path: getPath(DIR_DIST),
    library: {
      name: 'p2panda',
      type: 'umd',
    },
  },
  resolve: {
    extensions: ['.ts', '.js'],
    alias: {
      '~': getPath(DIR_SRC),
      wasm: getPath(DIR_WASM),
    },
  },
  module: {
    rules: [
      {
        test: /\.ts$/,
        exclude: /node_modules/,
        use: [
          {
            loader: 'babel-loader',
          },
          {
            loader: 'ts-loader',
            options: {
              configFile: 'tsconfig.json',
              onlyCompileBundledFiles: true,
            },
          },
        ],
      },
    ],
  },
  devtool: 'source-map',
  stats: 'minimal',
  experiments: {
    asyncWebAssembly: true,
  },
};

export default config;
