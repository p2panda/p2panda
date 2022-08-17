// SPDX-License-Identifier: AGPL-3.0-or-later

import { signAndEncodeEntry } from './';
import { KeyPair } from '../identity';

describe('signAndEncodeEntry', () => {
  it('signs and encodes a new entry', () => {
    const keyPair = new KeyPair(
      'ec6e84f8255cbd7243599545364fd7364a83830774d4c873aaba9bd5d60c773e',
    );

    const entry = {
      payload: '112233',
    };

    const result = signAndEncodeEntry(entry, keyPair);
    expect(result).toBe(
      '00b443811e711fdbfcbeccaf655e0ebe7a1c83490cc28d1516c5920178bef416c500010' +
        '300203ec19c37eaa225b9b811d0c30aa3f6994b05c78f630249d574c8824f191001d6a7' +
        'ffdec790e40c37f07509e7343cc2b7c6b66b1f34e372ac4ea5163a0e684bed88b28b0cb' +
        'd52fd47198f7f54041058f40c8212a1f0e0ac55d4bc35904b8b860d',
    );
  });

  it('allows different types for logId and seqNum', () => {
    const keyPair = new KeyPair(
      'ec6e84f8255cbd7243599545364fd7364a83830774d4c873aaba9bd5d60c773e',
    );

    const entry = {
      payload: '112233',
      logId: 15,
      seqNum: '2331',
      backlink:
        '00206531fcd5480129b33095a1ea6eff21120e6fd85c6caf67fa496b73ffd5ca1c8c',
    };

    expect(() => {
      signAndEncodeEntry(entry, keyPair);
    }).not.toThrow();
  });

  it('throws an error on invalid parameters', () => {
    const keyPair = new KeyPair();

    expect(() => {
      signAndEncodeEntry(
        {
          logId: BigInt(-12),
          payload: '1234',
        },
        keyPair,
      );
    }).toThrow();

    expect(() => {
      signAndEncodeEntry(
        {
          seqNum: BigInt(0),
          payload: '1234',
        },
        keyPair,
      );
    }).toThrow();
  });

  it('throws when trying to create wrong entry', () => {
    const keyPair = new KeyPair();

    // Backlink is missing
    expect(() => {
      signAndEncodeEntry(
        {
          seqNum: BigInt(2),
          payload: '1234',
        },
        keyPair,
      );
    }).toThrow(
      'Could not sign and encode entry: Error: backlink and skiplink not valid for this sequence number',
    );
  });
});
