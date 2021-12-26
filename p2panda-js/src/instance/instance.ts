// SPDX-License-Identifier: AGPL-3.0-or-later

import { materializeEntries } from './materialiser';

import type { InstanceRecord } from '~/types';
import type { Context } from '~/session';

export const queryInstances = async ({
  schema,
  session,
}: Pick<Context, 'schema' | 'session'>): Promise<InstanceRecord[]> => {
  const entries = await session.queryEntries(schema);
  const instances = Object.values(materializeEntries(entries));
  return instances;
};
