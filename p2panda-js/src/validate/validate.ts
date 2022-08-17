// SPDX-License-Identifier: AGPL-3.0-or-later

function isValidHexString(value: string): boolean {
  // Needs to be even number of characters
  if (value.length % 2 !== 0) {
    return false;
  }

  // Contains only valid characters (case insensitive)
  const hexRegEx = /^[0-9a-fA-F]+$/i;
  return hexRegEx.test(value);
}

type Validation = {
  [key: string]: {
    // Value needs to be of this type, default is 'string'
    type?: 'string' | 'boolean' | 'number';

    // Value needs to have exact given length
    length?: number;

    // Value need to be a valid hexadecimal string
    validHex?: boolean;

    // Value can be optional
    optional?: boolean;
  };
};

type ValidationValues = {
  [key: string]: string | boolean | number | undefined;
};

export function validate(values: ValidationValues, fields: Validation) {
  Object.keys(fields).forEach((key) => {
    const value = values[key];

    const optional = !!fields[key].optional;
    const type = fields[key].type || 'string';

    if (!optional) {
      if (!value) {
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
        if (stringValue.length !== fields[key].length) {
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
    }
  });
}
