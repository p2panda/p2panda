import { Entry, Message, FieldsTagged, EncodedEntry, EntryArgs } from '~/types';
import TEST_DATA from './test-data.json';
import { marshallResponseFields } from '~/utils';

// right now we only have one author `panda` who only has one schema log. This could be expanded in the future.
const PANDA_LOG = TEST_DATA.panda.logs[0];

export const schemaFixture = (): string => {
  return PANDA_LOG.decodedMessages[0].schema;
};

/**
 * Return an object with fields for an author's public and
 * private key.
 */
export const authorFixture = (): { publicKey: string; privateKey: string } => {
  const author = {
    publicKey: TEST_DATA.panda.publicKey,
    privateKey: TEST_DATA.panda.privateKey,
  };
  return author;
};

/**
 * Return an Entry given a sequence number in the testing log.
 */
export const entryFixture = (seqNum: number): Entry => {
  const index = seqNum - 1;

  let fields = undefined;
  if (PANDA_LOG.decodedMessages[index].action !== 'delete') {
    fields = marshallResponseFields(
      PANDA_LOG.decodedMessages[index].fields as FieldsTagged,
    );
  }

  const message: Message = {
    action: PANDA_LOG.decodedMessages[index].action as Message['action'],
    schema: PANDA_LOG.decodedMessages[index].schema,
    fields: fields,
  };

  if (PANDA_LOG.decodedMessages[index].id != null) {
    message.id = PANDA_LOG.decodedMessages[index].id;
  }

  const entry: Entry = {
    entryHashBacklink: PANDA_LOG.nextEntryArgs[index].entryHashBacklink,
    entryHashSkiplink: PANDA_LOG.nextEntryArgs[index].entryHashSkiplink,
    seqNum: PANDA_LOG.nextEntryArgs[index].seqNum,
    logId: PANDA_LOG.nextEntryArgs[index].logId,
    message,
  };

  return entry;
};

/**
 * Return an encoded entry given a sequence number on the mock log.
 */
export const encodedEntryFixture = (seqNum: number): EncodedEntry => {
  const index = seqNum - 1;

  const encodedEntry: EncodedEntry = {
    author: TEST_DATA.panda.publicKey,
    entryBytes: PANDA_LOG.encodedEntries[index].entryBytes,
    entryHash: PANDA_LOG.encodedEntries[index].entryHash,
    logId: PANDA_LOG.encodedEntries[index].logId,
    payloadBytes: PANDA_LOG.encodedEntries[index].payloadBytes,
    payloadHash: PANDA_LOG.encodedEntries[index].payloadHash,
    seqNum: PANDA_LOG.encodedEntries[index].seqNum,
  };

  return encodedEntry;
};

/**
 * Return arguments for creating an entry.
 *
 * Takes a `seqNum` parameter, which is the sequence number of
 * the entry preceding the one we want arguments for.
 */
export const entryArgsFixture = (seqNum: number): EntryArgs => {
  const index = seqNum - 1;

  const entryArgs: EntryArgs = {
    entryHashBacklink: PANDA_LOG.nextEntryArgs[index].entryHashBacklink as
      | string
      | undefined,
    entryHashSkiplink: PANDA_LOG.nextEntryArgs[index].entryHashSkiplink as
      | string
      | undefined,
    seqNum: PANDA_LOG.nextEntryArgs[index].seqNum,
    logId: PANDA_LOG.nextEntryArgs[index].logId,
  };

  return entryArgs;
};
