import debug from 'debug';

import { Session } from '~/index';
import { Fields, FieldsTagged } from '~/types';
import { marshallRequestFields } from '~/utils';

import type { Resolved } from '~/index';

type InstanceArgs = {
  // @ts-expect requires types exported from rust
  keyPair: Resolved<Session['p2panda']['KeyPair']>;
  schema: string;
  session: Session;
};

const log = debug('p2panda-api:instance');

/**
 * Returns a message fields instance for the given field contents and schema
 */
const getMessageFields = async (
  session: Session,
  _schema: string,
  fields: FieldsTagged,
) => {
  const { MessageFields } = await session.loadWasm();

  const messageFields = new MessageFields();
  for (const fieldName of Object.keys(fields)) {
    const fieldType = Object.keys(fields[fieldName])[0];
    messageFields.add(fieldName, fields[fieldName][fieldType]);
  }
  log('getMessageFields', messageFields.toString());
  return messageFields;
};

/**
 * Sign and publish an entry given a prepared `Message`, `KeyPair` and `Session`
 */
const signPublishEntry = async (
  messageEncoded,
  { keyPair, schema, session },
) => {
  const { signEncodeEntry } = await session.loadWasm();

  const entryArgs = await session.getNextEntryArgs(keyPair.publicKey(), schema);

  // If lastSeqNum is null don't try and convert to BigInt
  // Can this be handled better in the wasm code?
  const lastSeqNum = entryArgs.lastSeqNum
    ? BigInt(entryArgs.lastSeqNum)
    : entryArgs.lastSeqNum;

  // Sign and encode entry passing in copy of keyPair
  const { entryEncoded } = signEncodeEntry(
    keyPair,
    messageEncoded,
    entryArgs.entryHashSkiplink,
    entryArgs.entryHashBacklink,
    lastSeqNum,
    BigInt(entryArgs.logId),
  );

  // Publish entry and store returned entryArgs for next entry
  const nextEntryArgs = await session.publishEntry(
    entryEncoded,
    messageEncoded,
  );

  // Cache next entry args for next publish
  session.setNextEntryArgs(keyPair.publicKey(), schema, nextEntryArgs);
};

/**
 * Signs and publishes a `create` entry for the given user data and matching schema.
 */
const create = async (
  fields: Fields,
  { keyPair, schema, session }: InstanceArgs,
): Promise<void> => {
  const { encodeCreateMessage } = await session.loadWasm();

  // Create message
  const fieldsTagged = marshallRequestFields(fields);
  const messageFields = await getMessageFields(session, schema, fieldsTagged);
  const encodedMessage = await encodeCreateMessage(schema, messageFields);
  await signPublishEntry(encodedMessage, { keyPair, schema, session });
};

export default { create };
