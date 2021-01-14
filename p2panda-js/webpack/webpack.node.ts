import * as webpack from 'webpack';
import CopyWebpackPlugin from 'copy-webpack-plugin';

import config, {
  PATH_DIST,
  PATH_DIST_WASM_NODE,
  getPath,
} from './webpack.common';

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
  },
  plugins: [
    new CopyWebpackPlugin({
      patterns: [
        // Copy exported wasm package into library folder where it gets imported as
        // an external module
        {
          from: getPath(PATH_DIST_WASM_NODE),
          to: getPath(PATH_DIST, 'wasm'),
          globOptions: {
            ignore: ['**/*.json', '**/*.md', '**/.gitignore', '**/LICENSE'],
          },
        },
      ],
    }),
    new webpack.DefinePlugin({
      BUILD_TARGET_WEB: JSON.stringify(false),
    }),
  ],
  optimization: {
    minimize: false,
  },
};

export default configNode;
