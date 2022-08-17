// SPDX-License-Identifier: AGPL-3.0-or-later

// Helper method to convert different inputs to BigInt
export function toBigInt(
  value: string | number | bigint | undefined | null,
  defaultValue: bigint,
): bigint {
  if (typeof value === 'undefined' || value === null) {
    return BigInt(defaultValue);
  } else {
    return BigInt(value);
  }
}
