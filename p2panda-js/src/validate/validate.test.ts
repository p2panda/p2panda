// SPDX-License-Identifier: AGPL-3.0-or-later

import { validate } from './';

describe('validate', () => {
  it('checks if value has correct length', () => {
    expect(() => {
      validate(
        {
          value: '1234',
          title: 'This is a title',
        },
        {
          title: {
            length: 15,
          },
          value: {
            length: 4,
          },
        },
      );
    }).not.toThrow();

    expect(() => {
      validate(
        {
          value: '12',
        },
        {
          value: {
            length: 4,
          },
        },
      );
    }).toThrow('"value" string expected length is 4 but received 2');
  });

  it('checks if value has correct type', () => {
    expect(() => {
      validate(
        {
          value: true,
          age: 12,
        },
        {
          value: {
            type: 'boolean',
          },
          age: {
            type: 'number',
          },
        },
      );
    }).not.toThrow();

    expect(() => {
      validate(
        {
          value: 'tru',
        },
        {
          value: {
            type: 'boolean',
          },
        },
      );
    }).toThrow('"value" needs to be a boolean');
  });

  it('ignores optional values', () => {
    expect(() => {
      validate(
        {},
        {
          value: {
            optional: true,
            length: 3,
          },
        },
      );
    }).not.toThrow();

    expect(() => {
      validate(
        {
          value: 'abc',
        },
        {
          value: {
            optional: true,
            length: 3,
          },
        },
      );
    }).not.toThrow();

    expect(() => {
      validate(
        {},
        {
          value: {
            length: 3,
          },
        },
      );
    }).toThrow('"value" is required');
  });

  it('checks hexadecimal strings', () => {
    expect(() => {
      validate(
        {
          value: 'abcdef',
          title: 'Does not matter',
          publicKey: '0145abcdef',
        },
        {
          value: {
            validHex: true,
          },
          title: {
            validHex: false,
          },
          publicKey: {
            validHex: true,
          },
        },
      );
    }).not.toThrow();

    expect(() => {
      validate(
        {
          value: 'ghijkl',
        },
        {
          value: {
            validHex: true,
          },
        },
      );
    }).toThrow('"value" is not a valid hexadecimal string');
  });

  it('checks minimum numbers', () => {
    expect(() => {
      validate(
        {
          value: BigInt(12),
        },
        {
          value: {
            min: 12,
            type: 'bigint',
          },
        },
      );
    }).not.toThrow();

    expect(() => {
      validate(
        {
          value: BigInt(11),
        },
        {
          value: {
            min: 12,
            type: 'bigint',
          },
        },
      );
    }).toThrow('"value" is smaller than the minimum value 12');
  });
});
