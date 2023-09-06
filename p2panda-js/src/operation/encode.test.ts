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

  it('convert from string representation of `previous` field', () => {
    const encodedOperation = encodeOperation({
      action: 'update',
      schemaId: 'schema_field_definition_v1',
      previous:
        '00200be56c7f138e11568acec1a25cf4122980d452c86e4cb9112f80302692e95b3b_00204e8b90414abd47af7e8538a5e2b1bd12a49dc05ba0a5a0e79012dbb8bc88867e',
      fields: {
        name: 'venue',
      },
    });

    expect(encodedOperation).toEqual(
      '850101781a736368656d615f6669656c645f646566696e6974696f6e5f7631827844' +
        '303032303062653536633766313338653131353638616365633161323563663431' +
        '323239383064343532633836653463623931313266383033303236393265393562' +
        '336278443030323034653862393034313461626434376166376538353338613565' +
        '326231626431326134396463303562613061356130653739303132646262386263' +
        '383838363765a1646e616d656576656e7565',
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
