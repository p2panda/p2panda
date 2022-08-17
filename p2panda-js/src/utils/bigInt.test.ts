// SPDX-License-Identifier: AGPL-3.0-or-later

import { toBigInt } from './';

describe('toBigInt', () => {
  it('converts to BigInt', () => {
    expect(toBigInt(5, BigInt(0))).toEqual(BigInt(5));
    expect(toBigInt('5', BigInt(0))).toEqual(BigInt(5));
    expect(toBigInt(BigInt(5), BigInt(0))).toEqual(BigInt(5));
  });

  it('sets a default value', () => {
    expect(toBigInt(undefined, BigInt(5))).toEqual(BigInt(5));
    expect(toBigInt(null, BigInt(5))).toEqual(BigInt(5));
    expect(toBigInt(0, BigInt(5))).toEqual(BigInt(0));
  });
});
