// SPDX-License-Identifier: AGPL-3.0-or-later

import { decodeEntry } from './';

describe('decodeEntry', () => {
  it('decodes an encoded entry', () => {
    const encodedEntry =
      '00' +
      'b443811e711fdbfcbeccaf655e0ebe7a1c83490cc28d1516c5920178bef416c5' +
      '0f' +
      '08' +
      '002034441bd15ac6c01ba5bb9f22b9a6d51d56e280cb3abcdb65216d45ddff74ae4b' +
      '0020d5c17b82ad475e2c9ec2d77b08737f7db967cd5f7d481bb4e15443a7d03b5327' +
      '03' +
      '00203ec19c37eaa225b9b811d0c30aa3f6994b05c78f630249d574c8824f191001d6' +
      '36ea3d6f735e388e4c257a3689030a28f60958c8bdb29e4039ed0bb0a3ede4c0' +
      'd3aed2095b1eb9a37ef065f20a2df90af0583da6081339a2689bc734dff0da04';

    const result = decodeEntry(encodedEntry);

    const publicKey =
      'b443811e711fdbfcbeccaf655e0ebe7a1c83490cc28d1516c5920178bef416c5';
    expect(result.publicKey).toBe(publicKey);
    expect(result.logId).toEqual(BigInt(15));
    expect(result.seqNum).toEqual(BigInt(8));
    expect(result.skiplink).toBe(
      '002034441bd15ac6c01ba5bb9f22b9a6d51d56e280cb3abcdb65216d45ddff74ae4b',
    );
    expect(result.backlink).toBe(
      '0020d5c17b82ad475e2c9ec2d77b08737f7db967cd5f7d481bb4e15443a7d03b5327',
    );
    expect(result.payloadSize).toEqual(BigInt(3));
    expect(result.payloadHash).toBe(
      '00203ec19c37eaa225b9b811d0c30aa3f6994b05c78f630249d574c8824f191001d6',
    );
    expect(result.signature).toBe(
      '36ea3d6f735e388e4c257a3689030a28f60958c8bdb29e4039ed0bb0a3ede4c0' +
        'd3aed2095b1eb9a37ef065f20a2df90af0583da6081339a2689bc734dff0da04',
    );
  });

  it('throws when decoding an invalid entry', () => {
    // Only `tag` and `publicKey` given
    expect(() => {
      decodeEntry(
        '00b443811e711fdbfcbeccaf655e0ebe7a1c83490cc28d1516c5920178bef416c5',
      );
    }).toThrow(
      'Could not decode entry: Could not decode log_id, error with varu64 encoding',
    );
  });
});
