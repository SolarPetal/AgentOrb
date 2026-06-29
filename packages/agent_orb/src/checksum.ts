import crypto from 'node:crypto';
import fs from 'node:fs';

export function sha256File(filePath: string): string {
  const hash = crypto.createHash('sha256');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('hex');
}

export function parseChecksums(text: string): Map<string, string> {
  const checksums = new Map<string, string>();

  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;

    const match = line.match(/^([a-fA-F0-9]{64})\s+\*?(.+)$/);
    if (!match) continue;

    const [, checksum, filename] = match;
    checksums.set(filename.trim(), checksum.toLowerCase());
  }

  return checksums;
}

export function verifyChecksum(filePath: string, expectedSha256: string): void {
  const actual = sha256File(filePath);
  if (actual !== expectedSha256.toLowerCase()) {
    throw new Error(`Checksum mismatch for ${filePath}\nexpected: ${expectedSha256}\nactual:   ${actual}`);
  }
}
