import * as webpack from 'webpack';

import config, { tsRule } from './webpack.common';

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
    'wasm-node': './wasm',
    'node-fetch': 'commonjs2 node-fetch',
  },
  module: {
    rules: [tsRule('node')],
  },
  plugins: [
    new webpack.DefinePlugin({
      BUILD_TARGET_WEB: JSON.stringify(false),
    }),
  ],
  optimization: {
    minimize: false,
  },
};

export default configNode;
