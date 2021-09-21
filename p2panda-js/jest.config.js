/** @type {import('ts-jest/dist/types').InitialOptionsTsJest} */
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  moduleNameMapper: {
    '^~(.*)$': '<rootDir>/src$1',
    'wasm-node': '<rootDir>/wasm-node',
  },
};
