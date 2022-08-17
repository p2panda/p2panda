// SPDX-License-Identifier: AGPL-3.0-or-later

import { isInt } from './';

describe('isInt', () => {
  it('detects if number is an integer', () => {
    expect(isInt(12)).toBe(true);
    expect(isInt(-1)).toBe(true);
    expect(isInt(124.4)).toBe(false);
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore
    expect(isInt('12')).toBe(false);
  });
});
