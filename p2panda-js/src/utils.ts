// SPDX-License-Identifier: AGPL-3.0-or-later

import type { Fields, FieldsTagged } from './types';

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
  const mapping = {
    bigint: 'int',
    boolean: 'bool',
    number: 'int',
    string: 'str',
  };
  const type = typeof fields[field];
  if (!Object.keys(mapping).includes(type)) {
    throw new Error(`Unsupported field type: ${typeof field}`);
  }
  // @ts-expect-error we have made sure that `type` is a key of `mapping`
  return mapping[type];
};

/**
 * Add type tags to operation fields before sending to node.
 */
export const marshallRequestFields = (fields: Fields): FieldsTagged => {
  const rv: FieldsTagged = {};
  Object.keys(fields).forEach((k) => {
    switch (getFieldType(fields, k)) {
      case 'int':
        if (typeof fields[k] === 'number') {
          // Round the number in case we passed a float here and store as
          // string
          rv[k] = {
            value: Math.round(fields[k] as number).toString(),
            type: 'int',
          };
        } else if (typeof fields[k] === 'bigint') {
          // Convert bigints into strings
          rv[k] = { value: fields[k].toString(), type: 'int' };
        } else {
          throw new Error('Invalid integer type');
        }

        break;
      case 'bool':
        rv[k] = { value: fields[k] as boolean, type: 'bool' };
        break;
      default:
        rv[k] = { value: fields[k] as string, type: 'str' };
    }
  });
  return rv;
};

/**
 * Remove type tagging from operation fields on an entry received from node.
 */
export const marshallResponseFields = (fieldsTagged: FieldsTagged): Fields => {
  return Object.keys(fieldsTagged).reduce((acc: Fields, key) => {
    const { value, type } = fieldsTagged[key];

    // Convert smaller integers to 'number', keep large ones as strings
    if (type === 'int' && BigInt(value) <= Number.MAX_SAFE_INTEGER) {
      acc[key] = parseInt(value as string, 10);
    } else {
      acc[key] = value;
    }

    return acc;
  }, {});
};
