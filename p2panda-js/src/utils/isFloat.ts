// SPDX-License-Identifier: AGPL-3.0-or-later

/** Helper method to detect if value is a float */
export function isFloat(n: number) {
  return Number(n) === n && n % 1 !== 0;
}
