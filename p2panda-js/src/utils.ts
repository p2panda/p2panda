import { Fields, FieldsTagged } from './types';

/**
 * Lookup the type of a schema's fields
 *
 * While we don't have proper schema support in the node this function
 * just guesses the schema's field type from a supplied fields record.
 *
 * @param fields assumed to be correct message fields for an instance
 * @param field name of the field for which to look up the type
 * @returns field type
 */
const getFieldType = (
  fields: Fields,
  field: string,
): 'string' | 'bool' | 'int' => {
  const mapping = {
    string: 'str',
    boolean: 'bool',
    number: 'int',
  };
  const type = typeof fields[field];
  if (!Object.keys(mapping).includes(type)) {
    throw new Error(`Unsupported field type: ${typeof field}`);
  }
  return mapping[type];
};

/**
 * Add type tags to message fields before sending to node
 */
export const marshallRequestFields = (fields: Fields): FieldsTagged => {
  const rv: FieldsTagged = {};
  Object.keys(fields).forEach((k) => {
    switch (getFieldType(fields, k)) {
      case 'int':
        // this case is entered for any `number` type so the value is rounded
        // to get an integer
        rv[k] = { value: Math.round(fields[k] as number), type: 'int' };
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
 * Remove type tagging from message fields on an entry received from node
 */
export const marshallResponseFields = (fieldsTagged: FieldsTagged): Fields => {
  const fields: Fields = {};
  Object.keys(fieldsTagged).forEach((k) => {
    fields[k] = fieldsTagged[k].value;
  });
  return fields;
};
