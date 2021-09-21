<h1 align="center">p2panda-js tests</h1>

<div align="center">
  <strong>All the things a testing panda needs (for JavaScript)</strong>
</div>

<br />

<div align="center">
  <!-- CI status -->
  <a href="https://github.com/p2panda/p2panda/actions">
    <img src="https://img.shields.io/github/workflow/status/p2panda/p2panda/Build%20and%20test?style=flat-square" alt="CI Status" />
  </a>
  <!-- Crates version -->
  <a href="https://crates.io/crates/p2panda-rs">
    <img src="https://img.shields.io/crates/v/p2panda-rs.svg?style=flat-square" alt="Crates.io version" />
  </a>
  <!-- NPM version -->
  <a href="https://www.npmjs.com/package/p2panda-js">
    <img src="https://img.shields.io/npm/v/p2panda-js?style=flat-square" alt="NPM version" />
  </a>
</div>

<div align="center">
  <h3>
    <a href="https://github.com/p2panda/p2panda">
      Installation
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/design-document#how-to-contribute">
      Contributing
    </a>
  </h3>
</div>

<br />

The following is an explanation of the tesing setup for the `p2panda-js` library. An `open-RPC` specificastion defines the methods available when interacting with a `p2panda` node and a mock server is implemented according to this specification for mocking requests and responses. Test data is contained in `test-data.json` which is consumed by both a `JSON Schema` templating process, which outputs the actual `open-RPC` definition file, as well as the test suite itself.  

## Setup

Test data can be found in `test-data.json`. This already contains valid testing values, so if you just want to run the tests you can skip this step. The values in this file are injected into `openrpc-template.json` via the `test-setup.ts` script. The final `openrpc.json` file is output into the `p2panda-js` folder. This can be accomplished with the following command:

```bash 
$ npm run test:template-openrpc
```

## Run Tests

To run the tests:

```bash 
$ npm run test
```