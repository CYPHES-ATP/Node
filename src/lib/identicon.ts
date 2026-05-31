export interface IdenticonCell {
  x: number;
  y: number;
  active: boolean;
  color: string;
}

const colors = ["var(--cyan)", "var(--cyan-soft)", "var(--green)", "var(--blue)", "var(--violet)"];

export function hashString(input: string) {
  let hash = 2166136261;

  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }

  return hash >>> 0;
}

export function makeIdenticon(seed: string, size = 5): IdenticonCell[] {
  const hash = hashString(seed);
  const cells: IdenticonCell[] = [];

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < Math.ceil(size / 2); x += 1) {
      const bitIndex = y * size + x;
      const active = ((hash >> (bitIndex % 24)) & 1) === 1 || (x === 2 && y === 2);
      const color = colors[(hash + x + y * 2) % colors.length];
      cells.push({ x, y, active, color });

      const mirrorX = size - x - 1;
      if (mirrorX !== x) {
        cells.push({ x: mirrorX, y, active, color });
      }
    }
  }

  return cells;
}
