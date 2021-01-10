import * as path from 'path';
import * as webpack from 'webpack';

import WasmPackPlugin from '@wasm-tool/wasm-pack-plugin';
import CopyWebpackPlugin from 'copy-webpack-plugin';

const PATH_DIST = './lib';
const PATH_DIST_WASM = './wasm';
const PATH_SRC = './src';
const PATH_SRC_WASM = '../p2panda-rs';

// Helper method to get absolute path of file or folder
function getPath(...args) {
  return path.resolve(__dirname, ...args);
}

// Returns WasmPackPlugin instance with configured target
function getWasmPlugin(target = 'bundler') {
  return new WasmPackPlugin({
    extraArgs: `--target ${target} --mode normal`,
    crateDirectory: getPath(PATH_SRC_WASM),
    outDir: getPath(PATH_DIST_WASM),
    pluginLogLevel: 'error',
  });
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
      wasm: getPath(PATH_DIST_WASM),
    },
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
  devtool: 'source-map',
  stats: 'minimal',
};

/*
 * Extended configuration to build library targeting modern browsers:
 *
 * - Output is minified for smaller library size
 * - Rust compiles wasm with `web` target
 * - Wasm converted to base64 (via url-loader) string and embedded inline
 */
const configBrowser: webpack.Configuration = {
  ...config,
  name: 'browser',
  output: {
    ...config.output,
    filename: '[name].min.js',
  },
  target: 'web',
  resolve: {
    ...config.resolve,
    alias: {
      ...config.resolve.alias,
      // Use browser adapter to load embedded base64 wasm string
      'wasm-init-adapter': getPath(PATH_SRC, 'wasm', 'browser.ts'),
    },
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
              generator: (
                content: Buffer,
                mimetype: string,
                encoding: BufferEncoding,
              ): string => {
                // Remove `mime` and `encoding` string from result, we are only
                // interested in the base64 encoded content
                return content.toString(encoding);
              },
            },
          },
        ],
      },
      ...config.module.rules,
    ],
  },
  // eslint-disable-next-line
  // @ts-ignore
  plugins: [getWasmPlugin('web')],
};

/*
 * Extended configuration to build library targeting node applications:
 *
 * - Output is not minified
 * - Rust compiles wasm with `nodejs` target
 * - Copy compiled wasm to library folder and treat it as external module
 */
const configNode: webpack.Configuration = {
  ...config,
  name: 'node',
  output: {
    ...config.output,
    filename: '[name].js',
  },
  target: 'node',
  externals: {
    // Treat exported wasm as external module
    wasm: './wasm',
  },
  resolve: {
    ...config.resolve,
    alias: {
      ...config.resolve.alias,
      // Use dynamically imported wasm file
      'wasm-init-adapter': getPath(PATH_SRC, 'wasm', 'node.ts'),
    },
  },
  plugins: [
    getWasmPlugin('nodejs'),
    // Copy exported wasm package into library folder where it gets imported as
    // an external module
    new CopyWebpackPlugin({
      patterns: [
        {
          from: getPath(PATH_DIST_WASM),
          to: getPath(PATH_DIST, 'wasm'),
          globOptions: {
            ignore: ['**/*.json', '**/*.ts', '**/*.md'],
          },
        },
      ],
    }),
  ],
  optimization: {
    minimize: false,
  },
};

export default [configNode, configBrowser];
