import * as webpack from 'webpack';
import config, { getWasmPlugin, tsRule } from './webpack.common';

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
      tsRule,
    ],
  },
  plugins: [
    getWasmPlugin('web'),
    new webpack.IgnorePlugin({
      resourceRegExp: /wasm-node/,
    }),
    new webpack.DefinePlugin({
      BUILD_TARGET_WEB: JSON.stringify(true),
    }),
  ],
};

export default configBrowser;
