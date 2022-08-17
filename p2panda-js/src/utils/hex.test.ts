// SPDX-License-Identifier: AGPL-3.0-or-later

import { hexToBytes, bytesToHex } from './';

describe('hexToBytes', () => {
  it('converts from hex string to bytes', () => {
    expect(hexToBytes('112233')).toEqual(new Uint8Array([17, 34, 51]));
  });

  it('throws when using an invalid hex string', () => {
    expect(() => {
      hexToBytes('gfhj');
    }).toThrow();
  });
});

describe('bytesToHex', () => {
  it('converts from bytes to hex string', () => {
    expect(bytesToHex(new Uint8Array([1, 2, 3]))).toEqual('010203');
  });
});
