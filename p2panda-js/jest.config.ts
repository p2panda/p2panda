import type { Config } from '@jest/types';

export default {
  preset: 'ts-jest',
  testEnvironment: 'node',
  modulePathIgnorePatterns: ['<rootDir>/wasm/web'],
  // Skip reporting coverage for auto-generated wasm module that should be
  // tested from p2panda-rs.
  coveragePathIgnorePatterns: ['<rootDir>/wasm'],
  globals: {
    // Set `BUILD_TARGET_WEB` to false to import the WebAssembly build for
    // NodeJS targets during testing. This is usually set via the Rollup
    // "define" plugin (see rollup configuration), but since Rollup is not used
    // during Jest testing we have to set it here as well.
    BUILD_TARGET_WEB: false,
  },
} as Config.InitialOptions;
