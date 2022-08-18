// SPDX-License-Identifier: AGPL-3.0-or-later

import { generateHash } from './';

describe('generateHash', () => {
  it('generates a hash for any value', () => {
    expect(generateHash('aabbcc')).toBe(
      '0020d5f1c831db4153ae65d0bd3edf6b88eb8f5c9985e35d5192e371f5b469eeb4c4',
    );
  });

  it('throws when using an invalid hex string', () => {
    expect(() => {
      generateHash('gfhj');
    }).toThrow();
  });
});
