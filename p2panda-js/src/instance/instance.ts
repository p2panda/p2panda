// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';
import { Fields, InstanceRecord } from '~/types';
import { getOperationFields } from '~/operation';
import { marshallRequestFields } from '~/utils';
import { signPublishEntry } from '~/entry';

import { materializeEntries } from './materialiser';

import type { Context } from '~/session';

/**
 * Signs and publishes a `create` entry for the given user data and matching
 * schema.
 *
 * Returns the encoded entry that was created.
 */
export const createInstance = async (
  fields: Fields,
  { keyPair, schema, session }: Context,
): Promise<string> => {
  const { encodeCreateOperation } = await wasm;

  // Create operation
  const fieldsTagged = marshallRequestFields(fields);
  const operationFields = await getOperationFields(session, fieldsTagged);
  const encodedOperation = encodeCreateOperation(schema, operationFields);
  const entryEncoded = await signPublishEntry(encodedOperation, {
    keyPair,
    schema,
    session,
  });

  return entryEncoded;
};

/**
 * Signs and publishes an `update` entry for the given instance id and fields
 *
 * Returns the encoded entry that was created.
 */
export const updateInstance = async (
  id: string,
  fields: Fields,
  { keyPair, schema, session }: Context,
): Promise<string> => {
  const { encodeUpdateOperation } = await wasm;

  // Create operation
  const fieldsTagged = marshallRequestFields(fields);
  const operationFields = await getOperationFields(session, fieldsTagged);
  const encodedOperation = encodeUpdateOperation(id, schema, operationFields);
  const entryEncoded = await signPublishEntry(encodedOperation, {
    keyPair,
    schema,
    session,
  });

  return entryEncoded;
};

/**
 * Signs and publishes a `delete` entry for the given instance id
 *
 * Returns the encoded entry that was created.
 */
export const deleteInstance = async (
  id: string,
  { keyPair, schema, session }: Context,
): Promise<string> => {
  const { encodeDeleteOperation } = await wasm;

  // Create operation
  const encodedOperation = encodeDeleteOperation(id, schema);
  const encodedEntry = await signPublishEntry(encodedOperation, {
    keyPair,
    schema,
    session,
  });

  return encodedEntry;
};

export const queryInstances = async ({
  schema,
  session,
}: Pick<Context, 'schema' | 'session'>): Promise<InstanceRecord[]> => {
  const entries = await session.queryEntries(schema);
  const instances = Object.values(materializeEntries(entries));
  return instances;
};
