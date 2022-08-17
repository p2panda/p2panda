// SPDX-License-Identifier: AGPL-3.0-or-later

import { isFloat } from './';

describe('isFloat', () => {
  it('detects if number is a float', () => {
    expect(isFloat(12.2)).toBe(true);
    expect(isFloat(-1.24)).toBe(true);
    expect(isFloat(124)).toBe(false);
    expect(isFloat(0)).toBe(false);
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore
    expect(isFloat('122.3')).toBe(false);
  });
});
