import fs from 'fs';
import path from 'path';

import pluginAlias from '@rollup/plugin-alias';
import pluginCommonJS from '@rollup/plugin-commonjs';
import pluginDefine from 'rollup-plugin-define';
import pluginDts from 'rollup-plugin-dts';
import pluginTypeScript from '@rollup/plugin-typescript';
import { wasm as pluginWasm } from '@rollup/plugin-wasm';
import { terser as pluginTerser } from 'rollup-plugin-terser';

import pkg from './package.json';

const SRC_DIR = 'src';
const DIST_DIR = 'lib';

function pluginCopy({ name }) {
  return {
    name: 'copy-wasm-node',
    resolveImportMeta: () => `""`,
    generateBundle() {
      // Copy everything from "wasm/node" into the node destination folder, as
      // this build is importing it locally from there (it is not inlined)
      fs.mkdirSync(path.resolve(`./${DIST_DIR}/${name}/wasm`), {
        recursive: true,
      });

      ['index.js', 'index.d.ts', 'index_bg.wasm', 'index_bg.wasm.d.ts'].forEach(
        (fileName) => {
          fs.copyFileSync(
            path.resolve(`./wasm/node/${fileName}`),
            path.resolve(`./${DIST_DIR}/${name}/wasm/${fileName}`),
          );
        },
      );

      // Copy .wasm file into root of destination, "slim" versions can import
      // it from there
      fs.copyFileSync(
        path.resolve('./wasm/web/index_bg.wasm'),
        path.resolve(`./${DIST_DIR}/p2panda.wasm`),
      );
    },
  };
}

function config({
  input,
  format = 'esm',
  isNode = false,
  isSlim = false,
  name = 'esm',
}) {
  return [
    // Build package
    {
      input,
      output: [
        {
          name: pkg.name,
          file: `${DIST_DIR}/${name}/index.js`,
          format,
          sourcemap: true,
        },
        // Provide a minified version for non-NodeJS builds
        !isNode && {
          name: pkg.name,
          file: `${DIST_DIR}/${name}/index.min.js`,
          format,
          sourcemap: true,
          plugins: [pluginTerser()],
        },
      ],
      plugins: [
        // Copy .wasm files around once
        isNode && pluginCopy({ name }),
        // Set `BUILD_TARGET_WEB` flag in code so we can control if we import
        // from the wasm 'web' or 'node' build
        pluginDefine({
          replacements: {
            BUILD_TARGET_WEB: JSON.stringify(!isNode),
          },
        }),
        // Treat wasm module as external for NodeJS builds
        isNode &&
          pluginAlias({
            entries: [{ find: '../wasm/node', replacement: './wasm/index.js' }],
          }),
        // Inline WebAssembly as base64 strings for some builds
        !isNode &&
          !isSlim &&
          pluginWasm({
            targetEnv: 'auto-inline',
          }),
        pluginTypeScript(),
        pluginCommonJS({
          extensions: ['.js', '.ts'],
        }),
      ],
      // Treat wasm module as external for NodeJS builds
      external: isNode ? ['./wasm/index.js'] : [],
    },
    // Build TypeScript definitions
    {
      input,
      output: {
        file: `${DIST_DIR}/${name}/index.d.ts`,
        format,
      },
      plugins: [pluginDts()],
    },
  ];
}

export default [
  ...config({
    input: `./${SRC_DIR}/index.inline.ts`,
    format: 'umd',
    isNode: false,
    name: 'umd',
  }),
  ...config({
    input: `./${SRC_DIR}/index.inline.ts`,
    format: 'cjs',
    isNode: false,
    name: 'cjs',
  }),
  ...config({
    input: `./${SRC_DIR}/index.slim.ts`,
    format: 'cjs',
    isNode: false,
    isSlim: true,
    name: 'cjs-slim',
  }),
  ...config({
    input: `./${SRC_DIR}/index.inline.ts`,
    format: 'esm',
    isNode: false,
    name: 'esm',
  }),
  ...config({
    input: `./${SRC_DIR}/index.slim.ts`,
    format: 'esm',
    isNode: false,
    isSlim: true,
    name: 'esm-slim',
  }),
  ...config({
    input: `./${SRC_DIR}/index.ts`,
    format: 'cjs',
    isNode: true,
    name: 'node',
  }),
];
