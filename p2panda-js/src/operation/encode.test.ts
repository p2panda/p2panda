// SPDX-License-Identifier: AGPL-3.0-or-later

import { encodeOperation, OperationFields } from './';

describe('encodeOperation', () => {
  it('encodes an operation', () => {
    const encodedOperation = encodeOperation({
      action: 'create',
      schemaId: 'schema_field_definition_v1',
      fields: new OperationFields({
        name: 'venue',
        type: 'str',
      }),
    });

    expect(encodedOperation).toEqual(
      '840100781a736368656d615f6669656c645f646566696e6974696f' +
        '6e5f7631a2646e616d656576656e7565647479706563737472',
    );
  });

  it('encodes an operation using easy fields', () => {
    const encodedOperation = encodeOperation({
      action: 'create',
      schemaId: 'schema_field_definition_v1',
      fields: {
        name: 'venue',
        type: 'str',
      },
    });

    expect(encodedOperation).toEqual(
      '840100781a736368656d615f6669656c645f646566696e6974696f' +
        '6e5f7631a2646e616d656576656e7565647479706563737472',
    );
  });

  it('throws an error when creating an invalid operation', () => {
    // Fields and previous operations missing
    expect(() => {
      encodeOperation({
        action: 'update',
        schemaId: 'schema_field_definition_v1',
      });
    }).toThrow(
      "Could not encode operation: expected 'fields' in CREATE or UPDATE operation",
    );
  });
});
