/** @type {import('ts-jest/dist/types').InitialOptionsTsJest} */
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  moduleNameMapper: {
    // Use the same path alias for our codebase as we do in Webpack
    '^~(.*)$': '<rootDir>/src$1',
    // Map `wasm` imports to the build of `wasm-pack`
    'wasm': '<rootDir>/wasm',
  },
  globals: {
    // Set `BUILD_TARGET_WEB` to false to import the WebAssembly build for
    // NodeJS targets during testing. This is usually set via the Webpack
    // DefinePlugin (see webpack configuration), but since Webpack is not used
    // during Jest testing we have to set it here as well.
    BUILD_TARGET_WEB: false,
  },
};
