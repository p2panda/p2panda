// SPDX-License-Identifier: AGPL-3.0-or-later

import type { Fields, FieldsTagged } from './types';

const FIELD_TYPE_MAPPING = {
  bigint: 'int',
  boolean: 'bool',
  number: 'int',
  string: 'str',
};

/**
 * Look up the type of a schema's fields.
 *
 * While we don't have proper schema support in the node this function just
 * guesses the schema's field type from a supplied fields record.
 *
 * @param fields assumed to be correct operation fields for an instance
 * @param field name of the field for which to look up the type
 * @returns field type
 */
const getFieldType = (
  fields: Fields,
  field: string,
): 'string' | 'bool' | 'int' => {
  const type = typeof fields[field];

  if (!Object.keys(FIELD_TYPE_MAPPING).includes(type)) {
    throw new Error(`Unsupported field type: ${typeof field}`);
  }

  // @ts-expect-error we have made sure that `type` is a key of `mapping`
  return FIELD_TYPE_MAPPING[type];
};

/**
 * Add type tags to operation fields before sending to node.
 */
export const marshallRequestFields = (fields: Fields): FieldsTagged => {
  const map: FieldsTagged = new Map();

  Object.keys(fields).forEach((key) => {
    const value = fields[key];

    switch (getFieldType(fields, key)) {
      case 'int':
        // "int" can be a BigInt instance or "number" which again can be a
        // float or integer type in the JavaScript world
        if (typeof value === 'number' && value.toString().includes('.')) {
          // This is a float number
          map.set(key, {
            value: value as number,
            type: 'float',
          });
        } else if (typeof value === 'bigint') {
          // Convert bigints into strings and store as "int"
          map.set(key, { value: value.toString(), type: 'int' });
        } else {
          // This is a regular integer, convert it to string and store as "int"
          map.set(key, {
            value: (value as number).toString(),
            type: 'int',
          });
        }

        break;
      case 'bool':
        map.set(key, { value: value as boolean, type: 'bool' });
        break;
      default:
        map.set(key, { value: value as string, type: 'str' });
    }
  });

  return map;
};

/**
 * Remove type tags from operation fields on an entry received from node.
 */
export const marshallResponseFields = (fieldsTagged: FieldsTagged): Fields => {
  const fields: Fields = {};

  for (const [key, fieldValue] of fieldsTagged.entries()) {
    const { type, value } = fieldValue;

    // Convert smaller integers to 'number', keep large ones as strings
    if (type === 'int' && BigInt(value) <= Number.MAX_SAFE_INTEGER) {
      fields[key] = parseInt(value as string, 10);
    } else {
      fields[key] = value;
    }
  }

  return fields;
};
