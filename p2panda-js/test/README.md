# p2panda-js Testing

The `test-data.json` file in this directory contains values with valid cross-
references that can be used as fixtures for testing p2panda. Easy access to
these values is provided by the functions exported from `./fixtures.ts`.

## Example

```typescript
import { documentIdFixture } from './fixtures';

console.log(documentIdFixture());
// 00201c221b573b1e0c67c5e2c624a93419774cdf46b3d62414c44a698df1237b1c16
```
