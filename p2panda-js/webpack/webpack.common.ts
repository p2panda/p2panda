// SPDX-License-Identifier: AGPL-3.0-or-later

import * as path from 'path';

import webpack from 'webpack';

export const DIR_DIST = 'lib';
export const DIR_SRC = 'src';
export const DIR_WASM = 'wasm';
export const DIR_WASM_SRC = 'p2panda-rs';

// Helper method to get absolute path of file or folder
export function getPath(...args: Array<string>): string {
  return path.resolve(__dirname, '..', ...args);
}

export const tsRule: webpack.RuleSetRule = {
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
};

// Base Webpack configuration
const config: webpack.Configuration = {
  entry: getPath(DIR_SRC, 'index.ts'),
  output: {
    path: getPath(DIR_DIST),
    library: 'p2panda',
    libraryTarget: 'umd',
  },
  resolve: {
    extensions: ['.ts', '.js'],
  },
  devtool: 'source-map',
  stats: 'minimal',
  experiments: {
    asyncWebAssembly: true,
  },
  performance: {
    // We know that .wasm files are large and we can't do much about it ..
    hints: false,
  },
};

export default config;
