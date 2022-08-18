// SPDX-License-Identifier: AGPL-3.0-or-later

import { decodeOperation } from './';

describe('decodeOperation', () => {
  it('decodes an encoded operation', () => {
    const encodedOperation =
      '850101784e6d757368726f6f6d735f3030323062636265366238626430636532' +
      '3831346230353531643538376165653362643132346365633733616463376530' +
      '6362303533653731373966336433343765663082784430303230383933666463' +
      '3763303235623532346237313235646132373466656432326538343561336264' +
      '6435383563633831376462643935656134383037623636393632784430303230' +
      '3939323465383664623562363730343839386263393534663538616163353666' +
      '6166393535356637623764376361393033663335656135643563313636336237' +
      'a1646e616d6570416d616e697461206361657361726561';

    const result = decodeOperation(encodedOperation);

    expect(result.action).toBe('update');
    expect(result.version).toBe(1);
    expect(result.schemaId).toBe(
      'mushrooms_0020bcbe6b8bd0ce2814b0551d587aee3bd124cec73adc7e0cb053e7179f3d347ef0',
    );
    expect(result.previousOperations).toEqual([
      '0020893fdc7c025b524b7125da274fed22e845a3bdd585cc817dbd95ea4807b66962',
      '00209924e86db5b6704898bc954f58aac56faf9555f7b7d7ca903f35ea5d5c1663b7',
    ]);
    expect(result.fields?.get('name')).toBe('Amanita caesarea');
  });

  it('throws when decoding an invalid operation', () => {
    // Invalid schema id
    expect(() => {
      decodeOperation(
        '84010063626C61A1646E616D6570416D616E697461206361657361726561',
      );
    }).toThrow(
      "Could not decode operation: Error: malformed schema id `bla`: doesn't contain an underscore",
    );
  });
});
