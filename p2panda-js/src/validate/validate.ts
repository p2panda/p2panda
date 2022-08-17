// SPDX-License-Identifier: AGPL-3.0-or-later

type Validation = {
  [key: string]: {
    // Value needs to be of this type, default is 'string'
    type?: 'object' | 'bigint' | 'string' | 'boolean' | 'number';

    // Value needs to have exact given length
    length?: number;

    // Number needs to have at least this value.
    min?: number;

    // Value need to be a valid hexadecimal string
    validHex?: boolean;

    // Value can be optional
    optional?: boolean;
  };
};

type ValidationValues = {
  [key: string]: object | bigint | string | boolean | number | undefined;
};

// Helper method to validate if hex string is correct
function isValidHexString(value: string): boolean {
  // Needs to be even number of characters
  if (value.length % 2 !== 0) {
    return false;
  }

  // Contains only valid characters (case insensitive)
  const hexRegEx = /^[0-9a-fA-F]+$/i;
  return hexRegEx.test(value);
}

// Helper method to validate user input
export function validate(values: ValidationValues, fields: Validation) {
  Object.keys(fields).forEach((key) => {
    const value = values[key];

    const optional = !!fields[key].optional;
    const type = fields[key].type || 'string';

    if (!optional) {
      if (typeof value === 'undefined' || value === null) {
        throw new Error(`"${key}" is required`);
      }
    } else {
      if (!value) {
        return;
      }
    }

    if (typeof value !== type) {
      throw new Error(`"${key}" needs to be a ${type}`);
    }

    if (type === 'string') {
      const stringValue = value as string;

      if ('length' in fields[key]) {
        const length = fields[key].length as number;
        if (stringValue.length !== length) {
          throw new Error(
            `"${key}" string expected length is ${fields[key].length} but received ${stringValue.length}`,
          );
        }
      }

      const validHex = !!fields[key].validHex;
      if (validHex) {
        if (!isValidHexString(stringValue)) {
          throw new Error(`"${key}" is not a valid hexadecimal string`);
        }
      }
    } else if (type === 'bigint') {
      const bigIntValue = value as bigint;

      if ('min' in fields[key]) {
        const min = fields[key].min as number;
        if (bigIntValue < BigInt(min)) {
          throw new Error(
            `"${key}" is smaller than the minimum value ${fields[key].min}`,
          );
        }
      }
    }
  });
}
