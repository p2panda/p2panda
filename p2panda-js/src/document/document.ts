// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import {
  encodeDeleteOperation,
  encodeCreateOperation,
  encodeUpdateOperation,
} from '../wasm';
import { getOperationFields } from '../operation';
import { marshallRequestFields } from '../utils';
import { signPublishEntry } from '../entry';

import type { Context } from '../session';
import type { Fields } from '../types';

const log = debug('p2panda-js:document');

/**
 * Signs and publishes a CREATE operation for the given application data and
 * matching document id.
 *
 * Returns the encoded entry that was created.
 */
export const createDocument = async (
  fields: Fields,
  { keyPair, schema, session }: Context,
): Promise<string> => {
  log(`Creating document`, fields);

  const fieldsTagged = marshallRequestFields(fields);
  const operationFields = getOperationFields(fieldsTagged);
  const encodedOperation = encodeCreateOperation(schema, operationFields);

  const entryEncoded = signPublishEntry(encodedOperation, {
    keyPair,
    schema,
    session,
  });

  return entryEncoded;
};

/**
 * Signs and publishes an UPDATE operation for the given document id and
 * fields.
 *
 * Returns the encoded entry that was created.
 */
export const updateDocument = async (
  documentId: string,
  previousOperations: string[],
  fields: Fields,
  { keyPair, schema, session }: Context,
): Promise<string> => {
  log(`Updating document`, {
    document: documentId,
    previousOperations,
    fields,
  });

  const fieldsTagged = marshallRequestFields(fields);
  const operationFields = getOperationFields(fieldsTagged);

  const encodedOperation = encodeUpdateOperation(
    schema,
    previousOperations,
    operationFields,
  );

  const entryEncoded = await signPublishEntry(
    encodedOperation,
    {
      keyPair,
      schema,
      session,
    },
    documentId,
  );

  return entryEncoded;
};

/**
 * Signs and publishes a DELETE operation for the given document id.
 *
 * Returns the encoded entry that was created.
 */
export const deleteDocument = async (
  documentId: string,
  previousOperations: string[],
  { keyPair, schema, session }: Context,
): Promise<string> => {
  log('Deleting document', { document: documentId, previousOperations });

  const encodedOperation = encodeDeleteOperation(schema, previousOperations);

  const encodedEntry = await signPublishEntry(
    encodedOperation,
    {
      keyPair,
      schema,
      session,
    },
    documentId,
  );

  return encodedEntry;
};
