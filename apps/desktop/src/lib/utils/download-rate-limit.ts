export const MIB = 1024 * 1024;
export function downloadRateBytes(value: number | undefined): number | null {
  if (value === undefined || !Number.isFinite(value) || value < 0 || value > 1024) {
    throw new Error("Enter a download limit between 0 and 1024 MiB/s");
  }
  if (value === 0) return null;
  const bytes = Math.round(value * MIB);
  if (bytes < 1024) throw new Error("The minimum limit is 1 KiB/s (0.0009765625 MiB/s)");
  return bytes;
}
