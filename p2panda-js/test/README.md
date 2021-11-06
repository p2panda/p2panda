# p2panda-js Test Setup

## Setup

* [OpenRPC](https://open-rpc.org/) specification defining the RPC methods available when interacting with a p2panda node.
* Mock server according to this specification for testing requests and responses.
* `test-data.json` which is consumed by both a JSON schema templating process, which outputs the actual OpenRPC definition file, as well as the test suite itself.

## Scripts

### Generate template

The `test-data.json` contains valid values which you can use as fixtures or test vectors. The values in this file are injected into `openrpc-template.json` via the `generate-openrpc-spec.ts` script. The final `openrpc.json` file is output into the `p2panda-js` folder. This can be accomplished with the following command:

```bash
npm run test:template-openrpc
```

### Run tests

To run the tests:

```bash
npm test
npm run test:watch
```
