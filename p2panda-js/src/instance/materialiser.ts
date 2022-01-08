// SPDX-License-Identifier: AGPL-3.0-or-later

import debug from 'debug';

import type { EntryRecord, InstanceRecord } from '~/types';

const log = debug('p2panda-js:operation');

/**
 * Create a record of data instances by parsing a series of p2panda log
 * entries.
 *
 * @param entries entry records from node
 * @returns records of the instance's data and metadata
 */
export const materializeEntries = (
  entries: EntryRecord[],
): { [instanceId: string]: InstanceRecord } => {
  const instances: { [instanceId: string]: InstanceRecord } = {};

  log(`Materialising ${entries.length} entries`);

  // Initiate all instances from their create operation.
  entries.forEach((entry) => {
    if (entry.operation && entry.operation.action == 'create') {
      const instanceId = entry.encoded.entryHash;
      instances[instanceId] = {
        ...entry.operation.fields,
        _meta: {
          id: instanceId,
          author: entry.encoded.author,
          deleted: false,
          edited: false,
          entries: [entry],
          schema: entry.operation.schema,
          last_operation: entry.encoded.entryHash,
        },
      };
    }
  });

  for (const instanceId in instances) {
    // Find and apply update or delete operations until this instance is.
    while (true) {
      // Find the next entry by matching previousEntries against the instance's last_operation.
      const nextEntry = entries.find((entry) => {
        if (entry.operation && entry.operation.previousOperations) {
          return entry.operation.previousOperations.includes(
            instances[instanceId]._meta.last_operation,
          );
        }
      });

      // If there are no more entries for this instance, we break here.
      if (!nextEntry) break;

      // If there is an entry, but it's operation was deleted, we only update some meta values, then continue.
      if (!nextEntry.operation) {
        instances[instanceId]._meta.entries.push(nextEntry);
        instances[instanceId]._meta.last_operation =
          nextEntry.encoded.entryHash;
        continue;
      }

      // Apply update or delete operations as usual.
      let updated: InstanceRecord;

      switch (nextEntry.operation.action) {
        case 'update':
          updated = {
            ...instances[instanceId],
            ...nextEntry.operation.fields,
          };
          updated._meta.edited = true;
          updated._meta.last_operation = nextEntry.encoded.entryHash;
          updated._meta.entries.push(nextEntry);
          instances[instanceId] = updated;
          continue;

        case 'delete':
          updated = { _meta: instances[instanceId]._meta };
          updated._meta.deleted = true;
          updated._meta.last_operation = nextEntry.encoded.entryHash;
          updated._meta.entries.push(nextEntry);
          instances[instanceId] = updated;
          break;
        default:
          throw new Error('Unhandled mesage action');
      }
    }
  }

  log(`Materialisation yields ${Object.keys(instances).length} instances`);
  return instances;
};
