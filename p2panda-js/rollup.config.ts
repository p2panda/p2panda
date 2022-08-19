// SPDX-License-Identifier: AGPL-3.0-or-later

import fs from 'fs';
import path from 'path';

import pluginAlias from '@rollup/plugin-alias';
import pluginCommonJS from '@rollup/plugin-commonjs';
import pluginDefine from 'rollup-plugin-define';
import pluginDts from 'rollup-plugin-dts';
import pluginTypeScript from '@rollup/plugin-typescript';
import { wasm as pluginWasm } from '@rollup/plugin-wasm';
import { terser as pluginTerser } from 'rollup-plugin-terser';

import type {
  RollupOptions,
  Plugin,
  ModuleFormat,
  InputOption,
  OutputOptions,
} from 'rollup';

const PROJECT_NAME = 'p2panda';
const SRC_DIR = 'src';
const DIST_DIR = 'lib';
const BUILD_FILE_NAME = 'index';
const USE_SOURCEMAP = true;

// Helper method to copy a file from one place to another
function copyFile(from: string, to: string) {
  fs.copyFileSync(path.resolve(from), path.resolve(to));
}

// Helper method to create a folder if it does not exist yet
function createFolder(dirPath: string) {
  fs.mkdirSync(path.resolve(dirPath), {
    recursive: true,
  });
}

// Plugin for NodeJS builds, to copy everything from "./wasm/node" into the
// node destination folder, as this build is importing it locally from there
// (it is not inlined)
function pluginCopyWasm(): Plugin {
  return {
    name: 'copy-wasm-node',
    resolveImportMeta: () => `""`,
    generateBundle() {
      // This plugin is only used for NodeJS builds
      const dirName = getBuildName({ format: 'cjs', mode: 'node' });

      // Make sure the target folder exists
      createFolder(`./${DIST_DIR}/${dirName}/wasm`);

      // Copy all .wasm and TypeScript files
      ['index.d.ts', 'index_bg.wasm', 'index_bg.wasm.d.ts'].forEach(
        (fileName) => {
          copyFile(
            `./wasm/node/${fileName}`,
            `./${DIST_DIR}/${dirName}/wasm/${fileName}`,
          );
        },
      );

      // Do not forget to also copy the `index.js` file, but give it a `.cjs`
      // ending as NodeJS builds are CommonJS modules
      copyFile(
        './wasm/node/index.js',
        `./${DIST_DIR}/${dirName}/wasm/index.cjs`,
      );
    },
  };
}

type BuildMode = 'inline' | 'slim' | 'node';

type BuildName = string;

type Config = {
  format: ModuleFormat;
  mode: BuildMode;
};

// Returns the name of the sub-directory which will be created in the target
// folder for each build.
function getBuildName({ format, mode }: Config): BuildName {
  if (mode === 'node') {
    return 'node';
  } else if (mode === 'inline') {
    return format;
  } else {
    return `${format}-slim`;
  }
}

// Returns the `input` file based on the given `mode`.
//
// The `src` directory contains different entry points into the project,
// depending on the choosen `mode`. This method helps with picking the right
// one.
function getInput(mode: BuildMode): InputOption {
  if (mode === 'node') {
    return `./${SRC_DIR}/index.ts`;
  }

  return `./${SRC_DIR}/index.${mode}.ts`;
}

// Returns the output file options for each build.
function getOutputs({ format, mode }: Config): OutputOptions[] {
  const result: OutputOptions[] = [];

  const dirName = getBuildName({ format, mode });
  const sourcemap = USE_SOURCEMAP;

  // Determine suffix of output files. For CommonJS builds we choose `.cjs`.
  const ext = format === 'cjs' ? 'cjs' : 'js';

  result.push({
    name: PROJECT_NAME,
    file: `${DIST_DIR}/${dirName}/${BUILD_FILE_NAME}.${ext}`,
    format,
    sourcemap,
  });

  // Provide a minified version for non-NodeJS builds
  if (mode !== 'node') {
    result.push({
      name: PROJECT_NAME,
      file: `${DIST_DIR}/${dirName}/${BUILD_FILE_NAME}.min.js`,
      format,
      sourcemap,
      plugins: [pluginTerser()],
    });
  }

  return result;
}

function getPlugins({ mode }: Config): Plugin[] {
  const result: Plugin[] = [];

  // Set `BUILD_TARGET_WEB` flag in `./src/wasm.js` file to control if
  // we import from the './wasm/web' or './wasm/node' Rust build.
  //
  // These compiled versions are optimized for different environments (NodeJS
  // for speed, web for size).
  result.push(
    pluginDefine({
      replacements: {
        BUILD_TARGET_WEB: JSON.stringify(mode !== 'node'),
      },
    }),
  );

  if (mode === 'node') {
    // Treat Rust .wasm build as external so it is not affected by Rollup.
    //
    // Treating the WebAssembly module as external prevents a bug with Rollup
    // reformatting the generated code by `wasm-bindgen` and breaking it badly.
    //
    // This issue mostly hits the WebAssembly code of the `getrandom`
    // crate using dynamic `require` statements based on the environment
    // (Browser / NodeJS).
    //
    // Related issue: https://github.com/rust-random/getrandom/issues/224
    //
    // Through this workaround, there are a couple of things to take care of:
    //
    // 1. We treat `../wasm/node` as an external dependency.
    result.push(
      pluginAlias({
        entries: [{ find: '../wasm/node', replacement: './wasm/index.cjs' }],
      }),
    );

    // 2. Since this folder doesn't exist in the final build we copy it manually
    // over
    result.push(pluginCopyWasm());
  }

  // Inline WebAssembly as base64 strings for some builds
  if (mode === 'inline') {
    result.push(
      pluginWasm({
        targetEnv: 'auto-inline',
      }),
    );
  }

  // Compile TypeScript source code to JavaScript
  result.push(pluginTypeScript());

  // Convert CommonJS modules to ES6
  result.push(
    pluginCommonJS({
      extensions: ['.js', '.ts'],
    }),
  );

  return result;
}

function config({ format, mode }: Config): RollupOptions[] {
  const result: RollupOptions[] = [];

  // Determine entry point in `src`
  const input = getInput(mode);

  // Determine where files of this build get written to
  const output = getOutputs({ format, mode });

  // Determine plugins which will be used to process this build
  const plugins = getPlugins({ format, mode });

  // Package build
  result.push({
    input,
    output,
    plugins,
    // Treat wasm Rust module as external for NodeJS builds. Read comment in
    // `getPlugins` to understand why.
    external:
      mode === 'node'
        ? [
            // This is the "external" dependency we set via the "alias" plugin
            './wasm/index.cjs',
            // rollup falsly claims that this external dependency is missing,
            // we ignore it here:
            path.resolve(__dirname, 'src', 'wasm', 'index.cjs'),
          ]
        : [],
  });

  // Generate TypeScript definition file for each build
  const dirName = getBuildName({ format, mode });

  result.push({
    input,
    output: {
      file: `${DIST_DIR}/${dirName}/${BUILD_FILE_NAME}.d.ts`,
      format,
    },
    plugins: [pluginDts()],
  });

  return result;
}

export default [
  ...config({
    format: 'umd',
    mode: 'inline',
  }),
  ...config({
    format: 'cjs',
    mode: 'inline',
  }),
  ...config({
    format: 'cjs',
    mode: 'slim',
  }),
  ...config({
    format: 'esm',
    mode: 'inline',
  }),
  ...config({
    format: 'esm',
    mode: 'slim',
  }),
  ...config({
    format: 'cjs',
    mode: 'node',
  }),
];
