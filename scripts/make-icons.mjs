// Builds app icon (rounded dark tile) and a transparent white logo mark from assets/logo-source.webp.
// Then run: npx tauri icon assets/app-icon.png
import sharp from "sharp";

const SRC = "assets/logo-source.webp";
const SIZE = 1024;
const RADIUS = Math.round(SIZE * 0.22);

// 1) Transparent white mark: luminance -> alpha.
const { data, info } = await sharp(SRC)
  .greyscale()
  .raw()
  .toBuffer({ resolveWithObject: true });
const rgba = Buffer.alloc(info.width * info.height * 4);
for (let i = 0; i < info.width * info.height; i++) {
  const v = data[i];
  const a = v < 40 ? 0 : v > 215 ? 255 : Math.round(((v - 40) / 175) * 255);
  rgba[i * 4] = 255;
  rgba[i * 4 + 1] = 255;
  rgba[i * 4 + 2] = 255;
  rgba[i * 4 + 3] = a;
}
const mark = await sharp(rgba, { raw: { width: info.width, height: info.height, channels: 4 } })
  .trim()
  .png()
  .toBuffer();
await sharp(mark).resize(256, 256, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } }).png().toFile("public/logo-mark.png");

// 2) App icon: dark rounded tile + centered mark (~58% of tile).
const markSize = Math.round(SIZE * 0.58);
const markResized = await sharp(mark)
  .resize(markSize, markSize, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .png()
  .toBuffer();
const tile = Buffer.from(
  `<svg width="${SIZE}" height="${SIZE}" xmlns="http://www.w3.org/2000/svg">
     <rect x="0" y="0" width="${SIZE}" height="${SIZE}" rx="${RADIUS}" ry="${RADIUS}" fill="#0B0B0C"/>
     <rect x="4" y="4" width="${SIZE - 8}" height="${SIZE - 8}" rx="${RADIUS - 4}" ry="${RADIUS - 4}" fill="none" stroke="#2A2A2E" stroke-width="6"/>
   </svg>`,
);
await sharp(tile)
  .composite([{ input: markResized, gravity: "center" }])
  .png()
  .toFile("assets/app-icon.png");

console.log("icons ready: assets/app-icon.png, public/logo-mark.png");
