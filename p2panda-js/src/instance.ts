// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import { Session } from '~/index';
import { EntryRecord, Fields, FieldsTagged, InstanceRecord } from '~/types';
import { marshallRequestFields } from '~/utils';
import p2panda from '~/wasm';

import { KeyPair, MessageFields } from 'wasm-web';

export type Context = {
  keyPair: KeyPair;
  schema: string;
  session: Session;
};

const log = debug('p2panda-api:instance');

/**
 * Returns a message fields instance for the given field contents and schema.
 */
export const getMessageFields = async (
  session: Session,
  fields: FieldsTagged,
): Promise<MessageFields> => {
  const { MessageFields } = await p2panda;

  const messageFields = new MessageFields();
  for (const k of Object.keys(fields)) {
    messageFields.add(k, fields[k]['type'], fields[k]['value']);
  }
  log('getMessageFields', messageFields.toString());
  return messageFields;
};

/**
 * Sign and publish an entry given a prepared `Message`, `KeyPair` and
 * `Session`.
 */
const signPublishEntry = async (
  messageEncoded: string,
  { keyPair, schema, session }: Context,
) => {
  const { signEncodeEntry } = await p2panda;

  const entryArgs = await session.getNextEntryArgs(keyPair.publicKey(), schema);

  const { entryEncoded } = signEncodeEntry(
    keyPair,
    messageEncoded,
    entryArgs.entryHashSkiplink,
    entryArgs.entryHashBacklink,
    entryArgs.seqNum,
    entryArgs.logId,
  );

  const nextEntryArgs = await session.publishEntry(
    entryEncoded,
    messageEncoded,
  );

  // Cache next entry args for next publish
  session.setNextEntryArgs(keyPair.publicKey(), schema, nextEntryArgs);
};

/**
 * Signs and publishes a `create` entry for the given user data and matching
 * schema.
 */
const create = async (
  fields: Fields,
  { keyPair, schema, session }: Context,
): Promise<void> => {
  const { encodeCreateMessage } = await p2panda;

  // Create message
  const fieldsTagged = marshallRequestFields(fields);
  const messageFields = await getMessageFields(session, fieldsTagged);
  const encodedMessage = encodeCreateMessage(schema, messageFields);
  await signPublishEntry(encodedMessage, { keyPair, schema, session });
};

/**
 * Create a record of data instances by parsing a series of p2panda log entries
 *
 * @param entries entry records from node
 * @returns records of the instance's data and metadata
 */
const materializeEntries = (
  entries: EntryRecord[],
): { [instanceId: string]: InstanceRecord } => {
  const instances: { [instanceId: string]: InstanceRecord } = {};
  entries.sort((a, b) => a.seqNum - b.seqNum);
  for (const entry of entries) {
    if (entry.message == null) continue;

    const entryHash = entry.encoded.entryHash;
    const author = entry.encoded.author;
    const schema = entry.message.schema;

    if (instances[entryHash] && instances[entryHash].deleted) continue;

    let updated: InstanceRecord;
    switch (entry.message.action) {
      case 'create':
        instances[entryHash] = {
          ...entry.message.fields,
          _meta: {
            author,
            deleted: false,
            edited: false,
            entries: [entry],
            hash: entryHash,
            schema,
          },
        };
        break;

      case 'update':
        updated = {
          ...instances[entryHash],
          ...entry.message.fields,
        };
        updated._meta.edited = true;
        updated._meta.entries.push(entry);
        instances[entryHash] = updated;
        break;

      case 'delete':
        updated = { _meta: instances[entryHash]._meta };
        updated._meta.deleted = true;
        updated._meta.entries.push(entry);
        instances[entryHash] = updated;
        break;
      default:
        throw new Error('Unhandled mesage action');
    }
  }
  return instances;
};

const query = async ({
  schema,
  session,
}: Pick<Context, 'schema' | 'session'>): Promise<InstanceRecord[]> => {
  const entries = await session.queryEntries(schema);
  const instances = Object.values(materializeEntries(entries));
  return instances;
};

export default { create, query };
