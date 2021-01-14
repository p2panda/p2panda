import * as path from 'path';
import * as webpack from 'webpack';

export const PATH_DIST = '../lib';
export const PATH_DIST_WASM_WEB = '../wasm-web';
export const PATH_DIST_WASM_NODE = '../wasm-node';
export const PATH_SRC = '../src';
export const PATH_SRC_WASM = '../../p2panda-rs';

// Helper method to get absolute path of file or folder
export function getPath(...args: Array<string>): string {
  return path.resolve(__dirname, ...args);
}

// Helper method which builds a typescript module rule
export const tsRule = (target: 'node' | 'browser'): webpack.RuleSetRule => {
  return {
    test: /\.ts/,
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
          // Overwrite `wasm-node` path for NodeJS builds, otherwise TypeScript
          // will export declaration files with wrong import paths in library
          ...(target === 'node'
            ? {
                compilerOptions: {
                  paths: {
                    'wasm-node': ['./lib/wasm'],
                  },
                },
              }
            : {}),
        },
      },
      {
        loader: 'eslint-loader',
      },
    ],
  };
};

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
  devtool: 'source-map',
  stats: 'minimal',
};

export default config;
