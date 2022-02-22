// SPDX-License-Identifier: AGPL-3.0-or-later

import { createKeyPair } from '~/identity';
import { getOperationFields } from '~/operation';
import { marshallRequestFields, marshallResponseFields } from '~/utils';
import wasm from '~/wasm';

describe('operation', () => {
  describe('getOperationFields', () => {
    it('creates a WebAssembly OperationField', async () => {
      const fields = marshallRequestFields({
        channel: 5,
        temperature: 12.921,
        message: 'chin chin',
        serious: false,
      });

      const operationFields = await getOperationFields(fields);

      const outputRepresentation =
        'OperationFields(OperationFields({"channel": Integer(5), "message": ' +
        'Text("chin chin"), "serious": Boolean(false), "temperature": Float(12.921)}))';
      expect(operationFields.toString()).toEqual(outputRepresentation);
    });
  });

  it('correctly encodes and decodes an operation with relation fields', async () => {
    const { signEncodeEntry, encodeCreateOperation, decodeEntry } = await wasm;

    const keyPair = await createKeyPair();
    const schema =
      '002023261605de96605e5d802c48be16a3ba157049754e3bba6b8b13d788cd082434';

    const fields = {
      location: {
        document:
          '0020bdf61b3f79760cd43bef2de447a6e0cca548e8e50036d4c5c5b60cca433ab67f',
        document_view: [
          '00202c3304ca4ffd4d25b53d33c4ab30f5a6e8fb1e56768c6697c047d896ed644512',
        ],
      },
      profiles: [
        {
          document:
            '0020dbeef88ee89b3abcdc62bf5eccb279b2a2a319632739a4769f67ae9e0eb78da8',
          document_view: [
            '0020d0951b0560d3af5854e01407c6d7e927d332641c501d3f9e50d44d445f6c336c',
          ],
        },
        {
          document:
            '00200fe5b7163e5e6eddca0682186abe852987c2ca525cbe58e788ba25a14d3cf21e',
        },
      ],
    };

    // Encode entry and operation
    const fieldsTagged = marshallRequestFields(fields);
    const operationFields = await getOperationFields(fieldsTagged);
    const operationEncoded = encodeCreateOperation(schema, operationFields);
    const { entryEncoded } = signEncodeEntry(
      keyPair,
      operationEncoded,
      undefined,
      undefined,
      BigInt(1),
      BigInt(1),
    );

    // Decode it again and compare fields
    const { operation } = decodeEntry(entryEncoded, operationEncoded);
    const fieldsDecoded = marshallResponseFields(operation.fields);
    expect(fieldsDecoded).toEqual(fields);
  });
});
