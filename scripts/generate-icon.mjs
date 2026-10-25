/**
 * 生成 CheckIt 应用图标源图（1024×1024 PNG）。
 * 纯 Node 实现（zlib + 手写 PNG 编码），无第三方依赖。
 * 生成后运行 `npx tauri icon` 产出全平台图标。
 */
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const SIZE = 1024;

// —— 简易 RGBA 画布 ——
const buf = Buffer.alloc(SIZE * SIZE * 4, 0);

function setPixel(x, y, r, g, b, a) {
  if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
  const i = (y * SIZE + x) * 4;
  buf[i] = r;
  buf[i + 1] = g;
  buf[i + 2] = b;
  buf[i + 3] = a;
}

/** 圆角矩形内判定 */
function insideRoundedRect(x, y, x0, y0, x1, y1, radius) {
  if (x < x0 || x > x1 || y < y0 || y > y1) return false;
  const cx = Math.max(x0 + radius, Math.min(x, x1 - radius));
  const cy = Math.max(y0 + radius, Math.min(y, y1 - radius));
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= radius * radius || (x >= x0 + radius && x <= x1 - radius) || (y >= y0 + radius && y <= y1 - radius)
    ? x >= x0 && x <= x1 && y >= y0 && y <= y1 &&
      !(
        (x < x0 + radius && y < y0 + radius &&
          (x - (x0 + radius)) ** 2 + (y - (y0 + radius)) ** 2 > radius ** 2) ||
        (x > x1 - radius && y < y0 + radius &&
          (x - (x1 - radius)) ** 2 + (y - (y0 + radius)) ** 2 > radius ** 2) ||
        (x < x0 + radius && y > y1 - radius &&
          (x - (x0 + radius)) ** 2 + (y - (y1 - radius)) ** 2 > radius ** 2) ||
        (x > x1 - radius && y > y1 - radius &&
          (x - (x1 - radius)) ** 2 + (y - (y1 - radius)) ** 2 > radius ** 2)
      )
    : false;
}

/** 三次贝塞尔点 */
function bezier(p0, p1, p2, p3, t) {
  const u = 1 - t;
  return {
    x: u ** 3 * p0.x + 3 * u * u * t * p1.x + 3 * u * t * t * p2.x + t ** 3 * p3.x,
    y: u ** 3 * p0.y + 3 * u * u * t * p1.y + 3 * u * t * t * p2.y + t ** 3 * p3.y,
  };
}

// 1. 背景：圆角矩形 + 对角线性渐变 #6366F1 → #8B5CF6
const M = 64; // 边距
const R = 224; // 圆角半径
const c0 = [0x63, 0x66, 0xf1];
const c1 = [0x8b, 0x5c, 0xf6];

for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    if (!insideRoundedRect(x, y, M, M, SIZE - M, SIZE - M, R)) continue;
    const t = (x + y) / (2 * SIZE);
    const r = Math.round(c0[0] + (c1[0] - c0[0]) * t);
    const g = Math.round(c0[1] + (c1[1] - c0[1]) * t);
    const b = Math.round(c0[2] + (c1[2] - c0[2]) * t);
    setPixel(x, y, r, g, b, 255);
  }
}

// 2. 白色对勾（粗描边，圆角端点）
const CHECK = [
  { x: 296, y: 528 },
  { x: 448, y: 676 },
  { x: 448, y: 676 },
  { x: 560, y: 560 },
];
const CHECK2 = [
  { x: 560, y: 560 },
  { x: 672, y: 444 },
  { x: 672, y: 444 },
  { x: 736, y: 364 },
];
const STROKE = 96;

function drawSegment(pts) {
  // 采样贝塞尔并给每个采样点画圆（近似粗描边）
  const steps = 400;
  for (let i = 0; i <= steps; i++) {
    const p = bezier(pts[0], pts[1], pts[2], pts[3], i / steps);
    const half = STROKE / 2;
    for (let dy = -half - 2; dy <= half + 2; dy++) {
      for (let dx = -half - 2; dx <= half + 2; dx++) {
        const d2 = dx * dx + dy * dy;
        if (d2 <= half * half) {
          setPixel(Math.round(p.x + dx), Math.round(p.y + dy), 255, 255, 255, 255);
        } else if (d2 <= (half + 2) ** 2) {
          // 抗锯齿边缘
          const a = Math.max(0, Math.min(255, Math.round(255 * (1 - (Math.sqrt(d2) - half) / 2))));
          setPixel(Math.round(p.x + dx), Math.round(p.y + dy), 255, 255, 255, a);
        }
      }
    }
  }
}

drawSegment(CHECK);
drawSegment(CHECK2);

// —— PNG 编码 ——
function crc32(data) {
  let c;
  const table = [];
  for (let n = 0; n < 256; n++) {
    c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  let crc = 0xffffffff;
  for (const byte of data) crc = table[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const typeBuf = Buffer.from(type, "ascii");
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])));
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type RGBA
// raw scanlines with filter byte 0
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0;
  buf.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}
const idat = deflateSync(raw, { level: 9 });

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", idat),
  chunk("IEND", Buffer.alloc(0)),
]);

writeFileSync(join(dirname(fileURLToPath(import.meta.url)), "app-icon.png"), png);
console.log("app-icon.png generated:", SIZE, "x", SIZE);
