import { OperationFields } from 'wasm';

import { marshallResponseFields } from '~/utils';

import type {
  EncodedEntry,
  Entry,
  EntryArgs,
  FieldsTagged,
  Operation,
  OperationTagged,
  OperationValue,
} from '~/types';

import TEST_DATA from './test-data.json';

// Right now we only have one author `panda` who only has one schema log. This
// could be expanded in the future.
const encodedEntries = TEST_DATA.panda.logs[0].encodedEntries;
const nextEntryArgs = TEST_DATA.panda.logs[0].nextEntryArgs;

// Convert regular JavaScript object for operation fields into Map
// @TODO: I'm horribly lost here in TypeScript land AAAAAAAAAAAAH!
const decodedOperations = TEST_DATA.panda.logs[0].decodedOperations.map(
  (operation): OperationTagged => {
    if (operation.fields) {
      const fields: FieldsTagged = new Map();

      Object.keys(operation.fields).forEach((key: string) => {
        const value: OperationValue = operation.fields[
          'message'
        ] as OperationValue;
        fields.set(key, value);
      });

      operation.fields = fields;
    }

    return operation;
  },
);

export const schemaFixture = (): string => {
  return decodedOperations[0].schema;
};

export const documentIdFixture = (): string => {
  return encodedEntries[0].entryHash;
};

/**
 * Return an object with fields for an author's public and private key.
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
  if (decodedOperations[index].action !== 'delete') {
    fields = marshallResponseFields(
      decodedOperations[index].fields as FieldsTagged,
    );
  }

  const operation: Operation = {
    action: decodedOperations[index].action as Operation['action'],
    schema: decodedOperations[index].schema,
    fields: fields,
  };

  if (decodedOperations[index].previousOperations) {
    operation.previousOperations = decodedOperations[index].previousOperations;
  }

  const entry: Entry = {
    entryHashBacklink: nextEntryArgs[index].entryHashBacklink,
    entryHashSkiplink: nextEntryArgs[index].entryHashSkiplink,
    seqNum: nextEntryArgs[index].seqNum,
    logId: nextEntryArgs[index].logId,
    operation,
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
    entryBytes: encodedEntries[index].entryBytes,
    entryHash: encodedEntries[index].entryHash,
    logId: encodedEntries[index].logId,
    payloadBytes: encodedEntries[index].payloadBytes,
    payloadHash: encodedEntries[index].payloadHash,
    seqNum: encodedEntries[index].seqNum,
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
    entryHashBacklink: nextEntryArgs[index].entryHashBacklink as
      | string
      | undefined,
    entryHashSkiplink: nextEntryArgs[index].entryHashSkiplink as
      | string
      | undefined,
    seqNum: nextEntryArgs[index].seqNum,
    logId: nextEntryArgs[index].logId,
  };

  return entryArgs;
};
