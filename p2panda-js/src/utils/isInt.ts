// SPDX-License-Identifier: AGPL-3.0-or-later

/** Helper method to detect if value is an integer */
export function isInt(n: number) {
  return Number(n) === n && n % 1 === 0;
}
