import { Fields, FieldsTagged } from './types';

/**
 * Add type tags to message fields before sending to node
 */
export const marshallRequestFields = (fields: Fields): FieldsTagged => {
  const rv: FieldsTagged = {};
  Object.keys(fields).forEach((k) => {
    rv[k] = { Text: fields[k] };
  });
  return rv;
};

/**
 * Remove type tagging from mesasge fields on an entry received from node
 */
export const marshallResponseFields = (fieldsTagged: FieldsTagged): Fields => {
  const fields: Fields = {};
  Object.keys(fieldsTagged).forEach((k) => {
    fields[k] = fieldsTagged[k].Text;
  });
  return fields;
};
