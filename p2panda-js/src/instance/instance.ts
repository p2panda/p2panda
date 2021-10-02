// SPDX-License-Identifier: AGPL-3.0-or-later

import wasm from '~/wasm';
import { Fields, InstanceRecord } from '~/types';
import { getMessageFields } from '~/message';
import { marshallRequestFields } from '~/utils';
import { signPublishEntry } from '~/entry';

import { materializeEntries } from './materialiser';

import type { Context } from '~/session';

/**
 * Signs and publishes a `create` entry for the given user data and matching
 * schema.
 */
export const createInstance = async (
  fields: Fields,
  { keyPair, schema, session }: Context,
): Promise<void> => {
  const { encodeCreateMessage } = await wasm;

  // Create message
  const fieldsTagged = marshallRequestFields(fields);
  const messageFields = await getMessageFields(session, fieldsTagged);
  const encodedMessage = encodeCreateMessage(schema, messageFields);
  await signPublishEntry(encodedMessage, { keyPair, schema, session });
};

export const queryInstances = async ({
  schema,
  session,
}: Pick<Context, 'schema' | 'session'>): Promise<InstanceRecord[]> => {
  const entries = await session.queryEntries(schema);
  const instances = Object.values(materializeEntries(entries));
  return instances;
};
