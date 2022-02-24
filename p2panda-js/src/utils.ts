import type { Fields, FieldsTagged, Relation } from './types';

const FIELD_TYPE_MAPPING = {
  bigint: 'int',
  boolean: 'bool',
  number: 'int',
  string: 'str',
};

/**
 * Helper method to validate if required fields are given in relation object.
 *
 * @param relation object
 * @returns boolean
 */
const validateRelation = (relation: object): boolean => {
  if (!('document' in relation)) {
    throw new Error('`document` field is missing in relation');
  }

  // `document_view` is optional but if set it needs at least one item inside
  if ('document_view' in relation) {
    const documentView = (relation as Relation)['document_view'];

    if (!Array.isArray(documentView)) {
      throw new Error('`document_view` is not an array');
    } else if (documentView.length === 0) {
      throw new Error('`document_view` array can not be empty ');
    }
  }

  return true;
};

/**
 * Look up the type of a schema's fields.
 *
 * While we don't have proper schema support in the node this function just
 * guesses the schema's field type from a supplied fields record.
 *
 * Returns null when a key has no value.
 *
 * @param fields assumed to be correct operation fields for an instance
 * @param key name of the field for which to look up the type
 * @returns field type
 */
const getFieldType = (
  fields: Fields,
  key: string,
): 'str' | 'bool' | 'int' | 'relation' | 'relation_list' | null => {
  const type = typeof fields[key];

  if (type === 'undefined') {
    // Return null if a key has no value
    return null;
  }

  if (type === 'object') {
    // Value is probably a relation or relation_list
    if (Array.isArray(fields[key])) {
      const list = fields[key] as Array<Relation>;

      if (list.length === 0) {
        throw new Error('Empty array found');
      }

      list.forEach((relation) => {
        validateRelation(relation as object);
      });

      return 'relation_list';
    } else {
      validateRelation(fields[key] as object);
      return 'relation';
    }
  }

  if (!Object.keys(FIELD_TYPE_MAPPING).includes(type)) {
    throw new Error(`Unsupported field type: ${type}`);
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
      case 'str':
        map.set(key, { value: value as string, type: 'str' });
        break;
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
      case 'relation':
        map.set(key, {
          value: value as Relation,
          type: 'relation',
        });
        break;
      case 'relation_list':
        map.set(key, {
          value: value as Array<Relation>,
          type: 'relation_list',
        });
        break;
      case null:
        // Skip fields that have no value
        break;
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
    if (type === 'int' && BigInt(value as string) <= Number.MAX_SAFE_INTEGER) {
      fields[key] = parseInt(value as string, 10);
    } else {
      fields[key] = value;
    }
  }

  return fields;
};
