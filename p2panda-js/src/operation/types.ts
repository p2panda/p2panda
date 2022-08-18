// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Operation actions mapping from strings to integers.
 */
export const OPERATION_ACTIONS = {
  create: 0,
  update: 1,
  delete: 2,
};

/**
 * Operation actions mapping from integers to strings.
 */
export const OPERATION_ACTIONS_INDEX: { [action: number]: string } =
  Object.fromEntries(Object.entries(OPERATION_ACTIONS).map(([k, v]) => [v, k]));

